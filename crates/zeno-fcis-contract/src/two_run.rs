//! Advisory finite two-safety checks by self-composition through the Authority.
//! All mutation is invocation-owned scratch; neither policy nor inputs mutate.
use crate::{
    ContractError,
    internals::{
        declarations::{Declarations, Source},
        expr,
        graph::{Graph, ScalarGraph},
        model::Contract,
        policy,
        review::{
            domain::{self, LeafDomain, Position},
            evaluate::{self, Framer, Outcome},
        },
        rules::{Class, Rules},
        schema, schema_commitment,
    },
};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use zeno_fcis_synthesis::finite::v2_authority::Authority;
use zeno_fcis_synthesis::finite::{Domain, Program};

/// Existing finite judge tuple ceiling.
pub const MAX_TUPLES: u64 = 1 << 20;
/// Exact scope retained in every packet, including UNKNOWN.
pub const SCOPE: &str = "two runs from every admissible state over the declared finite domain; one step per side (noninterference) or two (order independence); longer interleavings are not claimed";
/// Requested property. State paths use `pre.100.FIELD` or `post.100.FIELD`.
#[derive(Clone, Debug)]
pub enum Property {
    /// Match each input's class across its two positions; compare final fields.
    Commute {
        /// Complete-input selector for the first input.
        first: String,
        /// Complete-input selector for the second input.
        second: String,
        /// Final state fields to compare.
        observe: Vec<String>,
        /// Maximum absolute integer difference, inclusive.
        within: u128,
    },
    /// Compare outcomes for states differing only in the secret fields.
    Noninterference {
        /// State fields omitted from the grouping key.
        secret: Vec<String>,
        /// Successor fields whose equality is required.
        public: Vec<String>,
    },
}
/// Exhaustive finite verdict; UNKNOWN never supplies a positive claim.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    /// Every required comparison completed.
    Checked,
    /// A replayable pair differs.
    Refuted,
    /// The full requested claim could not be checked.
    Unknown,
}
impl Verdict {
    /// Stable packet spelling.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Checked => "checked",
            Self::Refuted => "refuted",
            Self::Unknown => "UNKNOWN",
        }
    }
}
/// Owned result, exposed only through immutable accessors. Advisory only.
#[derive(Clone, Debug)]
pub struct CheckResult {
    verdict: Verdict,
    packet: Value,
}
impl CheckResult {
    /// Exhaustive verdict or UNKNOWN.
    pub const fn verdict(&self) -> Verdict {
        self.verdict
    }
    /// Source-bound packet with scope, work and a replayable counterexample.
    pub const fn packet(&self) -> &Value {
        &self.packet
    }
}
#[derive(Default)]
struct Measured {
    states: u64,
    inputs: u64,
    invocations: u64,
    comparisons: u64,
}
fn finish(mut packet: Value, m: &Measured, verdict: Verdict, result: Value) -> CheckResult {
    packet["status"] = json!(verdict.name());
    packet["claim"] = if verdict == Verdict::Checked {
        json!("exhaustively checked over the declared finite domain")
    } else {
        Value::Null
    };
    packet["measured"] = json!({"admissible_states":m.states,"complete_inputs":m.inputs,"authority_invocations":m.invocations,"comparisons":m.comparisons});
    packet["result"] = result;
    CheckResult { verdict, packet }
}
fn invalid(reason: impl Into<String>) -> ContractError {
    ContractError::new("two-run", reason)
}
fn fields(paths: &[String], positions: &[Position]) -> Result<Vec<usize>, ContractError> {
    if paths.is_empty() {
        return Err(invalid("at least one state field is required"));
    }
    let mut seen = BTreeSet::new();
    paths
        .iter()
        .map(|path| {
            let name = path.replacen("post.", "pre.", 1);
            let index = positions
                .iter()
                .position(|p| p.source == Source::State && p.name == name)
                .ok_or_else(|| invalid(format!("unknown state field: {path}")))?;
            if !seen.insert(index) {
                return Err(invalid(format!("duplicate state field: {path}")));
            }
            Ok(index)
        })
        .collect()
}
struct Selector {
    program: Program,
}
impl Selector {
    fn compile(
        text: &str,
        d: &Declarations,
        r: &Rules,
        positions: &[Position],
    ) -> Result<Self, ContractError> {
        if text.len() > 4096 {
            return Err(invalid("selector exceeds 4096 bytes"));
        }
        let ast = expr::parse(text)
            .and_then(|a| expr::expand_with_limits(&a, &r.variables, true))
            .map_err(invalid)?;
        // No state input is supplied to the compiler, even through a variable.
        let inputs = d
            .inputs()?
            .into_iter()
            .filter(|i| i.source != Source::State)
            .collect::<Vec<_>>();
        let mut graph = ScalarGraph::new(d, &inputs).with_strict(r.unsigned);
        let root = graph.compile_with_limits(&ast, true).map_err(invalid)?;
        let root = graph.boolean(root).map_err(invalid)?;
        let domains = positions
            .iter()
            .filter(|p| p.source != Source::State)
            .map(|p| match &p.domain {
                LeafDomain::Bool => Domain::Bool,
                LeafDomain::Int { min, max } => Domain::Int {
                    min: *min,
                    max: *max,
                },
                LeafDomain::Sum { variants, .. } => Domain::Int {
                    min: i128::from(*variants.iter().min().unwrap_or(&0)),
                    max: i128::from(*variants.iter().max().unwrap_or(&0)),
                },
            })
            .collect();
        let program = Program::try_new_extended(
            domains,
            vec![Domain::Bool],
            graph
                .table
                .nodes
                .iter()
                .map(policy::scalar_op)
                .collect::<Result<_, _>>()?,
            vec![policy::small(root.index)?],
        )
        .map_err(|e| invalid(format!("selector binding: {e:?}")))?;
        Ok(Self { program })
    }
    fn matches(&self, input: &[i128]) -> Result<bool, String> {
        self.program
            .evaluate(input)
            .map(|v| v[0] == 1)
            .map_err(|e| format!("undefined selector: {e:?}"))
    }
}
// Only collect a factor after the complete product has passed the tuple cap.
fn assignments(positions: &[Position], indices: &[usize]) -> Vec<Vec<i128>> {
    let mut rows = vec![Vec::new()];
    for i in indices {
        let values = positions[*i]
            .domain
            .values_within(MAX_TUPLES)
            .unwrap_or_default();
        rows = rows
            .into_iter()
            .flat_map(|row| {
                values.iter().map(move |v| {
                    let mut next = row.clone();
                    next.push(*v);
                    next
                })
            })
            .collect();
    }
    rows
}
fn tuple(state: &[i128], input: &[i128]) -> Vec<i128> {
    state.iter().chain(input).copied().collect()
}
fn invoke(
    a: &Authority<'_>,
    f: &Framer,
    state: &[i128],
    input: &[i128],
    m: &mut Measured,
) -> Outcome {
    m.invocations += 1;
    evaluate::evaluate(a, f, &tuple(state, input))
}
fn after(state: &[i128], outcome: &Outcome) -> Vec<i128> {
    match outcome {
        Outcome::Decision(d) if d.class != Class::Reject => {
            d.post.iter().filter_map(|(_, v)| v.number()).collect()
        }
        _ => state.to_vec(),
    }
}
fn class(o: &Outcome) -> &'static str {
    match o {
        Outcome::Decision(d) => d.class.name(),
        Outcome::Refused(_) => "Refused",
    }
}
fn outcome_json(o: &Outcome) -> Value {
    match o {
        Outcome::Refused(r) => json!({"class":"Refused","reason":r.text,"law":r.law}),
        Outcome::Decision(d) => {
            json!({"class":d.class.name(),"reason":d.reason,"post":d.post.iter().map(|(id,v)|(id.to_string(),v.json())).collect::<serde_json::Map<_,_>>(),
            "outbox":d.outbox.iter().map(|v|json!({"ordinal":v.ordinal,"channel":v.channel,"destination":v.destination.json(),"payload":v.payload.iter().map(|(id,n)|(id.to_string(),n.json())).collect::<serde_json::Map<_,_>>(),"idempotency":v.idempotency.json()})).collect::<Vec<_>>() })
        }
    }
}
fn coordinates(positions: &[Position], values: &[i128]) -> Value {
    positions
        .iter()
        .zip(values)
        .map(|(p, n)| (p.name.clone(), json!(n.to_string())))
        .collect()
}
fn observed(positions: &[Position], indices: &[usize], state: &[i128]) -> Value {
    indices
        .iter()
        .map(|i| {
            (
                positions[*i].name.replacen("pre.", "post.", 1),
                json!(state[*i].to_string()),
            )
        })
        .collect()
}
struct Run {
    outcomes: [Outcome; 2],
    final_state: Vec<i128>,
}
fn run(
    a: &Authority<'_>,
    f: &Framer,
    state: &[i128],
    x: &[i128],
    y: &[i128],
    m: &mut Measured,
) -> Run {
    let one = invoke(a, f, state, x, m);
    let intermediate = after(state, &one);
    let two = invoke(a, f, &intermediate, y, m);
    let final_state = after(&intermediate, &two);
    Run {
        outcomes: [one, two],
        final_state,
    }
}
fn difference(
    left: &Run,
    right: &Run,
    observe: &[usize],
    within: u128,
    positions: &[Position],
) -> Option<Value> {
    // Match the same input in the other order, including refused invocations.
    for i in 0..2 {
        if class(&left.outcomes[i]) != class(&right.outcomes[1 - i]) {
            return Some(
                json!({"kind":"decision_class","input":i+1,"left":class(&left.outcomes[i]),"right":class(&right.outcomes[1-i])}),
            );
        }
    }
    for index in observe {
        let l = left.final_state[*index];
        let r = right.final_state[*index];
        if l.abs_diff(r) > within {
            return Some(
                json!({"kind":"observed_field","path":positions[*index].name.replacen("pre.", "post.", 1),"left":l.to_string(),"right":r.to_string(),"distance":l.abs_diff(r).to_string(),"within":within.to_string()}),
            );
        }
    }
    None
}
fn run_json(run: &Run, positions: &[Position], observe: &[usize]) -> Value {
    json!({"decisions":run.outcomes.iter().map(outcome_json).collect::<Vec<_>>(),"observed":observed(positions,observe,&run.final_state),"final_state":run.final_state.iter().map(ToString::to_string).collect::<Vec<_>>()})
}
fn ni_difference(
    l: &Outcome,
    r: &Outcome,
    public: &[usize],
    positions: &[Position],
) -> Option<Value> {
    if class(l) != class(r) {
        return Some(json!({"kind":"decision_class","left":class(l),"right":class(r)}));
    }
    match (l, r) {
        (Outcome::Refused(a), Outcome::Refused(b)) => (a != b)
            .then(|| json!({"kind":"reason","left":outcome_json(l),"right":outcome_json(r)})),
        (Outcome::Decision(a), Outcome::Decision(b)) => {
            if a.reason != b.reason {
                return Some(json!({"kind":"reason","left":a.reason,"right":b.reason}));
            }
            for i in public {
                let id = positions[*i].field;
                let x = a.post.iter().find(|(f, _)| Some(*f) == id);
                let y = b.post.iter().find(|(f, _)| Some(*f) == id);
                if x != y {
                    return Some(
                        json!({"kind":"public_field","path":positions[*i].name.replacen("pre.", "post.", 1)}),
                    );
                }
            }
            (a.outbox != b.outbox).then(|| json!({"kind":"outbox"}))
        }
        _ => None,
    }
}
/// Check a fixed property through the contract's own bound Authority.
/// Source errors are refused; incomplete/unsupported domains or undefined
/// selectors yield UNKNOWN. No boundary sampling can return Checked.
pub fn check(
    project: &str,
    rules_source: &str,
    property: &Property,
    max_tuples: u64,
) -> Result<CheckResult, ContractError> {
    let rules = Rules::read(rules_source)?;
    let declarations = Declarations::read(project, &rules.leaf_bindings)?;
    let sha = |s: &str| evaluate::hex(&evaluate::digest(s.as_bytes()));
    let mut packet = json!({"schema":"zeno-fcis/two-run/1","authority":"none","scope":SCOPE,
        "sources":{"project_sha256":sha(project),"rules_sha256":sha(rules_source)},
        "checker":crate::checked_composition::identity::Identity::current().json(),
        "max_tuples":max_tuples.min(MAX_TUPLES),"property":format!("{property:?}")});
    let mut m = Measured::default();
    let positions = match domain::positions(&declarations) {
        Ok(p) => p,
        Err(e) => {
            return Ok(finish(
                packet,
                &m,
                Verdict::Unknown,
                json!({"reason":e.to_string()}),
            ));
        }
    };
    let (observe, secret, selectors) = match property {
        Property::Commute {
            first,
            second,
            observe,
            within,
        } => (
            fields(observe, &positions)?,
            Vec::new(),
            Some((
                Selector::compile(first, &declarations, &rules, &positions)?,
                Selector::compile(second, &declarations, &rules, &positions)?,
                *within,
            )),
        ),
        Property::Noninterference { secret, public } => {
            let s = fields(secret, &positions)?;
            let p = fields(public, &positions)?;
            if s.iter().any(|i| p.contains(i)) {
                return Err(invalid("a field is both secret and public"));
            }
            (p, s, None)
        }
    };
    let size = domain::domain_size(&positions);
    packet["domain_size"] = json!(size.map(|v| v.to_string()));
    if size.is_none_or(|n| n == 0 || n > u128::from(max_tuples.min(MAX_TUPLES))) {
        return Ok(finish(
            packet,
            &m,
            Verdict::Unknown,
            json!({"reason":"declared domain exceeds tuple limit or is empty"}),
        ));
    }
    let state_indices = positions
        .iter()
        .enumerate()
        .filter(|(_, p)| p.source == Source::State)
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    let input_indices = (state_indices.len()..positions.len()).collect::<Vec<_>>();
    let states = assignments(&positions, &state_indices);
    let inputs = assignments(&positions, &input_indices);
    // The complete product passed the cap, so every radix and their products
    // fit usize. Original state ordinals retain their digits after filtering.
    let state_sizes = state_indices
        .iter()
        .map(|i| {
            positions[*i]
                .domain
                .size()
                .and_then(|n| usize::try_from(n).ok())
                .ok_or_else(|| invalid("state domain cardinality"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let group_fields = (0..state_sizes.len())
        .filter(|i| !secret.contains(i))
        .collect::<Vec<_>>();
    let group_count = group_fields
        .iter()
        .map(|i| state_sizes[*i])
        .product::<usize>();

    m.inputs = inputs.len() as u64;
    let schema = schema::encode(&declarations)?;
    let contract = Contract::build(&declarations, &rules, schema_commitment(&schema)?)?;
    let framer = Framer::new(&positions, &contract);
    policy::with_authority(&contract, &schema, |a| {
        let laws = evaluate::state_laws(a.descriptor());
        let states = states
            .into_iter()
            .enumerate()
            .filter(|(_, s)| {
                evaluate::first_unsatisfied_state_law(
                    a.descriptor(),
                    &laws,
                    &framer,
                    &tuple(s, &inputs[0]),
                )
                .is_none()
            })
            .collect::<Vec<_>>();
        m.states = states.len() as u64;
        if states.is_empty() {
            return finish(
                packet,
                &m,
                Verdict::Unknown,
                json!({"reason":"no admissible states"}),
            );
        }
        if let Some((first, second, within)) = selectors {
            let select = |s: &Selector| {
                inputs
                    .iter()
                    .enumerate()
                    .filter_map(|(i, v)| match s.matches(v) {
                        Ok(true) => Some(Ok(i)),
                        Ok(false) => None,
                        Err(e) => Some(Err(e)),
                    })
                    .collect::<Result<Vec<_>, _>>()
            };
            let choices = select(&first).and_then(|l| select(&second).map(|r| (l, r)));
            let (left, right) = match choices {
                Ok(v) => v,
                Err(reason) => {
                    return finish(packet, &m, Verdict::Unknown, json!({"reason":reason}));
                }
            };
            if left.is_empty() || right.is_empty() {
                return finish(
                    packet,
                    &m,
                    Verdict::Unknown,
                    json!({"reason":"selector admits no complete input"}),
                );
            }
            for (_, state) in &states {
                for i in &left {
                    for j in &right {
                        let l = run(a, &framer, state, &inputs[*i], &inputs[*j], &mut m);
                        let r = run(a, &framer, state, &inputs[*j], &inputs[*i], &mut m);
                        m.comparisons += 1;
                        if let Some(diff) = difference(&l, &r, &observe, within, &positions) {
                            return finish(
                                packet,
                                &m,
                                Verdict::Refuted,
                                json!({"state":coordinates(&positions[..state.len()],state),"inputs":[coordinates(&positions[state.len()..],&inputs[*i]),coordinates(&positions[state.len()..],&inputs[*j])],"runs":[run_json(&l,&positions,&observe),run_json(&r,&positions,&observe)],"first_difference":diff}),
                            );
                        }
                    }
                }
            }
        } else {
            // One representative per public-state key per complete input. The
            // table is fresh for each input: O(states * inputs) invocations.
            for input in &inputs {
                let mut groups: Vec<Option<(Vec<i128>, Outcome)>> = vec![None; group_count];
                for (ordinal, state) in &states {
                    // Project the original mixed-radix ordinal onto exactly
                    // the nonsecret coordinates. No hashing or tree lookup.
                    let mut remaining = *ordinal;
                    let mut key = 0;
                    let mut scale = 1;
                    for (i, radix) in state_sizes.iter().enumerate().rev() {
                        let digit = remaining % radix;
                        remaining /= radix;
                        if group_fields.contains(&i) {
                            key += digit * scale;
                            scale *= radix;
                        }
                    }
                    let outcome = invoke(a, &framer, state, input, &mut m);
                    if let Some((other, previous)) = &groups[key] {
                        m.comparisons += 1;
                        if let Some(diff) = ni_difference(previous, &outcome, &observe, &positions)
                        {
                            return finish(
                                packet,
                                &m,
                                Verdict::Refuted,
                                json!({"state":coordinates(&positions[..state.len()],other),"other_state":coordinates(&positions[..state.len()],state),"inputs":[coordinates(&positions[state.len()..],input),coordinates(&positions[state.len()..],input)],"runs":[{"decisions":[outcome_json(previous)],"observed":observed(&positions,&observe,&after(other,previous))},{"decisions":[outcome_json(&outcome)],"observed":observed(&positions,&observe,&after(state,&outcome))}],"first_difference":diff}),
                            );
                        }
                    } else {
                        groups[key] = Some((state.clone(), outcome));
                    }
                }
            }
        }
        finish(
            packet,
            &m,
            Verdict::Checked,
            json!({"all_comparisons_equal":true}),
        )
    })
}
#[cfg(test)]
mod tests;
