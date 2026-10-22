//! Exact finite counts and bounded min-plus/max-plus accumulated rewards.
//! All decisions use the bound Authority. Scratch is owned by one invocation;
//! results own their JSON and expose only shared access. No runtime authority.

use crate::{
    ContractError,
    internals::{
        declarations::{Declarations, Source},
        model::Contract,
        policy,
        review::{
            domain::{self, Position},
            evaluate::{self, Framer, LawSet, Outcome, Proposed},
        },
        rules::{Class, Constant, LawKind, Rules},
        schema, schema_commitment,
    },
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use zeno_fcis_spec::{ProjectionRoot, Rational};
use zeno_fcis_synthesis::finite::{
    v2_authority::Authority,
    v2_laws::{self as laws, Failure, Verdict},
};

/// Hard tuple cap; no sampling or boundary fallback.
pub const MAX_TUPLES: u64 = 1 << 20;
/// Hard work cap, covering calls, folds, relaxations and witness copies.
pub const MAX_WORK: u64 = 1 << 24;
/// Hard inclusive command horizon.
pub const MAX_STEPS: u32 = 64;

/// Caller limits; values above the hard limits are clamped.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Maximum declared Cartesian tuples (including state positions).
    pub max_tuples: u64,
    /// Maximum charged operations; incomplete results claim nothing.
    pub max_work: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_tuples: MAX_TUPLES,
            max_work: MAX_WORK,
        }
    }
}

/// The finite question; strings are compiled by the existing law compiler.
#[derive(Clone, Debug)]
pub enum Question {
    /// Count complete declared pre-state/command/context tuples.
    Inputs {
        /// Transition relation in the project formula language.
        condition: String,
    },
    /// Count distinct reachable states through inclusive depth `steps`.
    States {
        /// State formula, evaluated on each distinct state as a genesis frame.
        condition: String,
        /// Inclusive command horizon from admitted genesis.
        steps: u32,
    },
    /// Sum one declared integer reward over runs of zero through `steps` commands.
    Extremes {
        /// Stable ID of a declared integer reward projection.
        reward: u32,
        /// Inclusive command horizon from admitted genesis.
        steps: u32,
    },
}

/// Owned packet with no mutable alias into invocation scratch.
#[derive(Clone, Debug)]
pub struct CheckResult {
    complete: bool,
    packet: Value,
}
impl CheckResult {
    /// Whether the requested exact answer completed.
    pub const fn complete(&self) -> bool {
        self.complete
    }
    /// Source-bound result, limits, witnesses and measured work.
    pub const fn packet(&self) -> &Value {
        &self.packet
    }
}

type Run = Vec<Vec<i128>>;
type Answer = Result<Value, Value>;
fn numbers(values: &[i128]) -> Value {
    json!(values.iter().map(ToString::to_string).collect::<Vec<_>>())
}
fn rational(v: Rational) -> Value {
    json!({"numerator":v.numerator().to_string(),"denominator":v.denominator().to_string()})
}
fn stop(reason: &str) -> Value {
    json!({"reason":reason})
}
fn invalid(reason: impl Into<String>) -> ContractError {
    ContractError::new("risk", reason)
}
fn truth(diagnostics: &[laws::Diagnostic], id: u32) -> Result<bool, Value> {
    match diagnostics
        .first()
        .filter(|d| d.id == id)
        .map(|d| d.verdict)
    {
        Some(Verdict::Satisfied) => Ok(true),
        Some(Verdict::Refused(Failure::Violated)) => Ok(false),
        _ => Err(stop("undefined property or law evaluator refusal")),
    }
}

struct Progress {
    work: u64,
    calls: u64,
    cap: u64,
}
impl Progress {
    fn charge(&mut self, n: u64) -> Result<(), Value> {
        if n > self.cap - self.work {
            return Err(stop("work budget exceeded"));
        }
        self.work += n;
        Ok(())
    }
}

/// Checks a finite question on immutable source bytes. Syntax/binding errors
/// are errors. Execution refusals, unsupported projections and exhausted
/// limits produce UNKNOWN and never an exact answer from a partial traversal.
pub fn check(
    project: &str,
    rules_source: &str,
    question: &Question,
    limits: Limits,
) -> Result<CheckResult, ContractError> {
    let rules = Rules::read(rules_source)?;
    rules.refuse_unsigned("risk scalar binding")?;
    let declarations = Declarations::read(project, &rules.leaf_bindings)?;
    let positions = domain::positions(&declarations)?;
    let schema = schema::encode(&declarations)?;
    let contract = Contract::build(&declarations, &rules, schema_commitment(&schema)?)?;
    let framer = Framer::new(&positions, &contract);
    let tuple_cap = limits.max_tuples.min(MAX_TUPLES);
    let work_cap = limits.max_work.min(MAX_WORK);
    let size = domain::domain_size(&positions);
    let mut progress = Progress {
        work: 0,
        calls: 0,
        cap: work_cap,
    };
    let mut packet = json!({"schema":"zeno-fcis/contract-risk/1", "authority":"none",
        "sources":{"project_sha256":evaluate::hex(&evaluate::digest(project.as_bytes())),"rules_sha256":evaluate::hex(&evaluate::digest(rules_source.as_bytes()))},
        "checker":crate::checked_composition::identity::Identity::current().json(),
        "limits":{"max_tuples":tuple_cap,"max_work":work_cap,"max_steps":MAX_STEPS},
        "domain_size":size.map(|n|n.to_string()),
        "positions":positions.iter().map(Position::json).collect::<Vec<_>>(),
        "question":match question { Question::Inputs{condition}=>json!({"kind":"inputs","condition":condition}),
            Question::States{condition,steps}=>json!({"kind":"states","condition":condition,"steps":steps}),
            Question::Extremes{reward,steps}=>json!({"kind":"extremes","reward":reward,"steps":steps}) },
        "semantics":"counts are cardinalities, not probabilities; commands and contexts are nondeterministic; runs include zero commands; rejected attempts preserve state but have no declared reward",
        "work_unit":"one invocation, property fold, discovery or relaxation, plus each scalar in a retained or rendered run copy; compilation and library-internal metering excluded"});
    // Validate query syntax before the domain-budget check.
    let property = match question {
        Question::Inputs { condition } => {
            Some(property_contract(project, &rules, condition, false)?)
        }
        Question::States { condition, .. } => {
            Some(property_contract(project, &rules, condition, true)?)
        }
        Question::Extremes { .. } => None,
    };
    let reward = if let Question::Extremes { reward, .. } = question {
        let spec = zeno_fcis_spec::parse_project(project, Default::default())
            .and_then(|p| zeno_fcis_spec::elaborate_project(p, Default::default()))
            .map_err(|e| invalid(e.to_string()))?;
        let decl = spec
            .rewards()
            .iter()
            .find(|r| r.id().get() == *reward)
            .ok_or_else(|| invalid("reward ID is not declared"))?;
        let path = decl.outcome();
        if path.root() != ProjectionRoot::Post
            || path.segments().len() != 2
            || path.segments()[0].get() != 100
        {
            return Err(invalid("reward must project post.100.FIELD"));
        }
        let field = u16::try_from(path.segments()[1].get())
            .map_err(|_| invalid("reward field too wide"))?;
        let state_field = declarations
            .state_fields()?
            .iter()
            .find(|v| v.id == field)
            .ok_or_else(|| invalid("reward field absent"))?;
        if !matches!(
            declarations.kind(state_field.type_id)?,
            crate::internals::declarations::Kind::I128 { .. }
        ) {
            return Err(invalid("reward must be an integer"));
        }
        Some((field, decl.failure()))
    } else {
        None
    };
    let steps = match question {
        Question::Inputs { .. } => 0,
        Question::States { steps, .. } | Question::Extremes { steps, .. } => *steps,
    };
    let answer = if steps > MAX_STEPS {
        Err(stop("command horizon exceeds hard limit"))
    } else if size.is_none_or(|n| n == 0 || n > u128::from(tuple_cap)) {
        Err(stop("declared domain exceeds tuple budget or is empty"))
    } else {
        let lists = positions
            .iter()
            .map(|p| p.domain.values_within(tuple_cap))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| invalid("domain enumeration bound"))?;
        let mut run = |authority: &Authority<'_>, predicates: Option<(&LawSet<'_>, u32)>| {
            let mut engine = Engine {
                authority,
                framer: &framer,
                positions: &positions,
                lists: &lists,
                progress: &mut progress,
                predicates,
            };
            match question {
                Question::Inputs { .. } => engine.inputs(),
                _ => engine.runs(&rules, question, reward),
            }
        };
        if let Some((decls, extended_rules, id)) = &property {
            let extra_schema = schema::encode(decls)?;
            let extra_contract =
                Contract::build(decls, extended_rules, schema_commitment(&extra_schema)?)?;
            policy::with_authority(&extra_contract, &extra_schema, |property_authority| {
                let predicates = LawSet::new(property_authority.descriptor(), &[*id]);
                policy::with_authority(&contract, &schema, |authority| {
                    run(authority, Some((&predicates, *id)))
                })
            })??
        } else {
            policy::with_authority(&contract, &schema, |authority| run(authority, None))?
        }
    };
    let complete = answer.is_ok();
    packet["status"] = json!(if complete { "checked" } else { "UNKNOWN" });
    packet["claim"] = if complete {
        json!("exhaustively checked over the declared finite domains and inclusive command horizon")
    } else {
        Value::Null
    };
    packet["measured"] = json!({"work":progress.work,"authority_invocations":progress.calls});
    packet["result"] = match answer {
        Ok(v) | Err(v) => v,
    };
    Ok(CheckResult { complete, packet })
}

fn property_contract(
    project: &str,
    rules: &Rules,
    formula: &str,
    state: bool,
) -> Result<(Declarations, Rules, u32), ContractError> {
    if formula.trim().is_empty()
        || [";", "\n", "\r", "//", "/*", "{", "}"]
            .iter()
            .any(|s| formula.contains(s))
    {
        return Err(invalid("property must be one formula"));
    }
    let id = project
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|v| v.parse::<u32>().ok())
        .chain([990, 991, 909, rules.failure_law, rules.reject_law])
        .max()
        .unwrap_or(991)
        .checked_add(1)
        .ok_or_else(|| invalid("no free property ID"))?;
    let scope = if state { "commit, genesis" } else { "any" };
    let source = format!("{project}\nlaw {id} risk_property_{id} on {scope} = {formula};\n");
    let mut extended = rules.clone();
    extended.law_kinds.insert(
        id,
        if state {
            LawKind::StateInvariant
        } else {
            LawKind::AuthoritySubjectRecipient
        },
    );
    Ok((
        Declarations::read(&source, &rules.leaf_bindings)?,
        extended,
        id,
    ))
}

struct Engine<'a, 'b> {
    authority: &'a Authority<'b>,
    framer: &'a Framer,
    positions: &'a [Position],
    lists: &'a [Vec<i128>],
    progress: &'a mut Progress,
    predicates: Option<(&'a LawSet<'b>, u32)>,
}
fn element(lists: &[Vec<i128>], mut index: usize) -> Vec<i128> {
    let mut tuple = vec![0; lists.len()];
    for (slot, list) in tuple.iter_mut().zip(lists).rev() {
        *slot = list[index % list.len()];
        index /= list.len();
    }
    tuple
}
fn cardinality(lists: &[Vec<i128>]) -> usize {
    lists.iter().map(Vec::len).product()
}

impl Engine<'_, '_> {
    fn decision(&mut self, tuple: &[i128]) -> Result<evaluate::Decision, Value> {
        self.progress.charge(1)?;
        self.progress.calls += 1;
        match evaluate::evaluate(self.authority, self.framer, tuple) {
            Outcome::Decision(d) => Ok(d),
            Outcome::Refused(r) => Err(
                json!({"reason":"Authority refused","refusal":r.to_string(),"input":numbers(tuple)}),
            ),
        }
    }
    fn inputs(&mut self) -> Answer {
        let mut count = Counts::default();
        let (predicates, id) = self.predicates.ok_or_else(|| stop("missing property"))?;
        for ordinal in 0..cardinality(self.lists) {
            let tuple = element(self.lists, ordinal);
            let d = self.decision(&tuple)?;
            let post = self.successor(&tuple, &d)?;
            self.progress.charge(1)?;
            let (_, ds) = predicates.on_transition_with_outbox(
                self.framer,
                &tuple,
                &Proposed {
                    class: d.class,
                    reason: d.reason,
                    post: &post,
                },
                &d.outbox,
            );
            count.add(
                truth(&ds, id)?,
                json!({"input":numbers(&tuple),"decision":d.class.name(),"post":numbers(&post)}),
            );
        }
        Ok(count.json())
    }
    fn successor(&self, tuple: &[i128], d: &evaluate::Decision) -> Result<Vec<i128>, Value> {
        self.positions
            .iter()
            .enumerate()
            .filter(|(_, p)| p.source == Source::State)
            .map(|(i, p)| {
                if d.class == Class::Reject {
                    return Ok(tuple[i]);
                }
                d.post
                    .iter()
                    .find(|(id, _)| Some(*id) == p.field)
                    .and_then(|(_, v)| v.number())
                    .filter(|v| self.lists[i].contains(v))
                    .ok_or_else(|| stop("successor missing or outside domain"))
            })
            .collect()
    }
    fn runs(
        &mut self,
        rules: &Rules,
        question: &Question,
        reward: Option<(u16, Option<i128>)>,
    ) -> Answer {
        let state_width = self
            .positions
            .iter()
            .take_while(|p| p.source == Source::State)
            .count();
        if self.positions[state_width..]
            .iter()
            .any(|p| p.source == Source::State)
        {
            return Err(stop("state positions must precede commands"));
        }
        let mut genesis = Vec::new();
        for (i, p) in self.positions[..state_width].iter().enumerate() {
            let value = match p.field.and_then(|f| rules.genesis.get(&f)) {
                Some(Constant::Int(v)) => *v,
                Some(Constant::Bool(v)) => i128::from(*v),
                _ => return Err(stop("missing scalar genesis")),
            };
            if !self.lists[i].contains(&value) {
                return Err(stop("genesis outside domain"));
            }
            genesis.push(value);
        }
        let mut tuple = genesis.clone();
        tuple.extend(element(&self.lists[state_width..], 0));
        self.progress.charge(1)?;
        let frames = self.framer.frame(&tuple);
        self.progress.calls += 1;
        let admission = self.authority.genesis(&frames[0]);
        if let Err(refusal) = admission.result() {
            return Err(
                json!({"reason":"genesis Authority refusal","refusal":format!("{refusal:?}")}),
            );
        }
        let steps = match question {
            Question::States { steps, .. } | Question::Extremes { steps, .. } => *steps,
            _ => 0,
        };
        let empty = Path {
            cost: Rational::ZERO,
            run: Vec::new(),
        };
        let mut frontier = BTreeMap::from([(genesis.clone(), [empty.clone(), empty.clone()])]);
        let mut seen = BTreeMap::from([(genesis, Vec::new())]);
        let mut best = empty.clone();
        let mut worst = empty;
        for _depth in 0..steps {
            let mut next: BTreeMap<Vec<i128>, [Path; 2]> = BTreeMap::new();
            for (state, paths) in &frontier {
                for input in 0..cardinality(&self.lists[state_width..]) {
                    let mut tuple = state.clone();
                    tuple.extend(element(&self.lists[state_width..], input));
                    let d = self.decision(&tuple)?;
                    let post = self.successor(&tuple, &d)?;
                    let cost = if let Some((field, failure)) = reward {
                        let value=match d.class {
                            Class::Accept=>d.post.iter().find(|(f,_)|*f==field).and_then(|(_,v)|v.number()),
                            Class::CommittedFailure=>failure, Class::Reject=>None,
                        }.ok_or_else(||json!({"reason":"undefined reward","input":numbers(&tuple),"decision":d.class.name()}))?;
                        Rational::try_new(value, 1)
                            .ok_or_else(|| stop("reward arithmetic bound"))?
                    } else {
                        Rational::ZERO
                    };
                    let mut candidates =
                        [paths[0].copy(self.progress)?, paths[1].copy(self.progress)?];
                    for path in &mut candidates {
                        self.progress.charge(1 + tuple.len() as u64)?;
                        path.cost = path
                            .cost
                            .checked_add(cost)
                            .ok_or_else(|| stop("accumulated reward arithmetic bound"))?;
                        path.run.push(tuple.clone());
                    }
                    if !seen.contains_key(&post) {
                        self.progress.charge(1)?;
                        if matches!(question, Question::States { .. }) {
                            seen.insert(post.clone(), candidates[0].copy(self.progress)?.run);
                        }
                    }
                    if candidates[0].cost < best.cost {
                        best = candidates[0].copy(self.progress)?;
                    }
                    if candidates[1].cost > worst.cost {
                        worst = candidates[1].copy(self.progress)?;
                    }
                    match next.get_mut(&post) {
                        None => {
                            next.insert(post, candidates);
                        }
                        Some(kept) => {
                            if candidates[0].cost < kept[0].cost {
                                kept[0] = candidates[0].copy(self.progress)?;
                            }
                            if candidates[1].cost > kept[1].cost {
                                kept[1] = candidates[1].copy(self.progress)?;
                            }
                        }
                    }
                }
            }
            frontier = next;
        }
        if matches!(question, Question::Extremes { .. }) {
            self.progress.charge(best.cells() + worst.cells())?;
            return Ok(json!({"best":best.json(),"worst":worst.json()}));
        }
        let (predicates, id) = self.predicates.ok_or_else(|| stop("missing property"))?;
        let mut counts = Counts::default();
        for (state, run) in seen {
            self.progress
                .charge(1 + run.iter().map(|v| v.len() as u64).sum::<u64>())?;
            let mut tuple = state.clone();
            tuple.extend(element(&self.lists[state_width..], 0));
            let fields = self.framer.fields(&self.framer.roots[0], &tuple);
            let (_, ds) = predicates.on_frame(&laws::Frame::Genesis {
                initial: self.framer.view(&self.framer.roots[0], &tuple, &fields),
            });
            counts.add(truth(&ds,id)?,json!({"state":numbers(&state),"run":run.iter().map(|v|numbers(v)).collect::<Vec<_>>()}));
        }
        Ok(counts.json())
    }
}
#[derive(Clone)]
struct Path {
    cost: Rational,
    run: Run,
}
impl Path {
    fn cells(&self) -> u64 {
        self.run.iter().map(|tuple| tuple.len() as u64).sum()
    }
    fn copy(&self, progress: &mut Progress) -> Result<Self, Value> {
        progress.charge(self.cells())?;
        Ok(self.clone())
    }
    fn json(&self) -> Value {
        json!({"value":rational(self.cost),"length":self.run.len(),"run":self.run.iter().map(|v|numbers(v)).collect::<Vec<_>>()})
    }
}
#[derive(Default)]
struct Counts {
    yes: u64,
    no: u64,
    positive: Option<Value>,
    negative: Option<Value>,
}
impl Counts {
    fn add(&mut self, holds: bool, witness: Value) {
        if holds {
            self.yes += 1;
            if self.positive.is_none() {
                self.positive = Some(witness);
            }
        } else {
            self.no += 1;
            if self.negative.is_none() {
                self.negative = Some(witness);
            }
        }
    }
    fn json(self) -> Value {
        json!({"count":self.yes.to_string(),"total":(self.yes+self.no).to_string(),"does_not_satisfy":self.no.to_string(),
        "share":Rational::try_new(i128::from(self.yes),u128::from(self.yes+self.no)).map(rational),"satisfying_witness":self.positive,"other_witness":self.negative})
    }
}
