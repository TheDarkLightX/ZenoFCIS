//! The finite machine a contract decides: every state and every input of its
//! declared domain, and the library Authority's decision on each pair. The
//! specification-gap, profile, lemma, scenario and probe judges read it.
//!
//! A tuple is a state followed by an input, the command then the context, in
//! program order. Ordinals run over the whole declared domain with the last
//! position fastest, as `contract review` enumerates a full domain, so the
//! ordinal of state `s` and input `x` is `s * inputs + x`. A domain larger
//! than the tuple limit is not enumerated: the judge is then inconclusive,
//! never a pass.

use std::collections::BTreeMap;

use super::super::declarations::{Declarations, Source};
use super::super::model::Contract;
use super::super::rules::{Class, Constant, Rules};
use super::super::{ContractError, Current};
use super::super::{policy, schema, schema_commitment};
use super::domain::{self, Position};
use super::evaluate::{self, Framer, Outcome, RefusalClass};
use super::parallel;
use zeno_fcis_contract::internals::genesis;
use zeno_fcis_synthesis::finite::v2_authority::Authority;

/// A contract read and compiled, ready to bind.
pub(super) struct Loaded {
    pub(super) rules: Rules,
    pub(super) declarations: Declarations,
    pub(super) schema: Vec<u8>,
    pub(super) commitment: [u8; 32],
    pub(super) positions: Vec<Position>,
    pub(super) state_width: usize,
    /// Source bytes of a checked current subject; proposals have no such binding.
    current_source: Option<(String, String)>,
}

impl Loaded {
    /// Reads `project.zeno` and `v2/policy.json` as generation does.
    ///
    /// # Errors
    /// The first declaration or rule with no contract form.
    pub(super) fn read(project: &str, rules: &str) -> Result<Self, ContractError> {
        let rules = Rules::read_for_project(project, rules)?;
        Self::with(project, rules)
    }

    /// Prepare only the exact replay-validated current source snapshot.
    pub(super) fn read_current(current: &Current<'_>) -> Result<Self, ContractError> {
        let mut loaded = Self::read(current.project, current.rules_source)?;
        loaded.current_source = Some((current.project.to_owned(), current.rules_source.to_owned()));
        Ok(loaded)
    }

    /// `project` with `rules` already read.
    ///
    /// # Errors
    /// The first declaration with no contract form.
    pub(super) fn with(project: &str, rules: Rules) -> Result<Self, ContractError> {
        rules.refuse_unsigned("scalar review consumer binding")?;
        let declarations = Declarations::read(project, &rules.leaf_bindings)?;
        let schema = schema::encode(&declarations)?;
        let commitment = schema_commitment(&schema)?;
        let positions = domain::positions(&declarations)?;
        let state_width = positions
            .iter()
            .filter(|position| position.source == Source::State)
            .count();
        Ok(Self {
            rules,
            declarations,
            schema,
            commitment,
            positions,
            state_width,
            current_source: None,
        })
    }

    /// Binds the contract of `rules` over these declarations to the library
    /// Authority, as an application binds it, and runs `work` with it.
    ///
    /// # Errors
    /// The generator's or the library's refusal, or `work`'s error.
    pub(super) fn bind<R>(
        &self,
        rules: &Rules,
        work: impl FnOnce(&Authority<'_>, &Framer) -> Result<R, ContractError>,
    ) -> Result<R, ContractError> {
        self.bind_proposal(rules, work)
    }

    /// Bind hypothetical rules independently of any adopted baseline.
    pub(super) fn bind_proposal<R>(
        &self,
        rules: &Rules,
        work: impl FnOnce(&Authority<'_>, &Framer) -> Result<R, ContractError>,
    ) -> Result<R, ContractError> {
        let contract = Contract::build(&self.declarations, rules, self.commitment)?;
        let framer = Framer::new(&self.positions, &contract);
        policy::with_authority(&contract, &self.schema, |authority| {
            work(authority, &framer)
        })?
    }

    /// Bind the replay-validated current contract for baseline observations.
    pub(super) fn bind_current<R>(
        &self,
        current: Option<&Current<'_>>,
        work: impl FnOnce(&Authority<'_>, &Framer) -> Result<R, ContractError>,
    ) -> Result<R, ContractError> {
        let Some(current) = current else {
            return self.bind_proposal(&self.rules, work);
        };
        let expected = Some((current.project, current.rules_source));
        let actual = self
            .current_source
            .as_ref()
            .map(|(project, rules)| (project.as_str(), rules.as_str()));
        if self.schema.as_slice() != current.schema || actual != expected {
            return Err(ContractError::new(
                "lineage binding",
                "judge sources or schema differ from the replayed current contract",
            ));
        }
        let framer = Framer::new(&self.positions, current.contract);
        policy::with_authority(current.contract, current.schema, |authority| {
            work(authority, &framer)
        })?
    }

    /// The IDs of the laws `project.zeno` declares, in declaration order;
    /// the framework laws generation adds are not among them.
    pub(super) fn declared_laws(&self) -> Vec<u32> {
        self.declarations.laws.iter().map(|law| law.id).collect()
    }
}

/// Why a judge did not run to the end: the domain or the work exceeds the
/// limit. An inconclusive judge claims nothing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Inconclusive {
    pub(crate) reason: &'static str,
    /// The size that exceeded the limit; `None` when it exceeds `u128`.
    pub(crate) size: Option<u128>,
    pub(crate) limit: u64,
}

impl Inconclusive {
    pub(crate) fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "status": "inconclusive",
            "reason": self.reason,
            "size": self.size.map(|size| size.to_string()),
            "limit": self.limit,
        })
    }
}

/// The declared domain as states and inputs.
#[derive(Debug)]
pub(super) struct Space {
    /// Every value of each position, in enumeration order.
    values: Vec<Vec<i128>>,
    state_width: usize,
    pub(super) states: usize,
    pub(super) inputs: usize,
}

impl Space {
    /// The whole declared domain, or why it is not enumerated.
    ///
    /// # Errors
    /// Inconclusive when the domain has more than `max_tuples` tuples.
    pub(super) fn new(
        positions: &[Position],
        state_width: usize,
        max_tuples: u64,
    ) -> Result<Self, Inconclusive> {
        let too_large = || Inconclusive {
            reason: "domain-over-limit",
            size: domain::domain_size(positions),
            limit: max_tuples,
        };
        if !super::probe_kernel::within_limit(domain::domain_size(positions), max_tuples) {
            return Err(too_large());
        }
        let values: Vec<Vec<i128>> = positions
            .iter()
            .map(|position| position.domain.values_within(max_tuples))
            .collect::<Option<_>>()
            .ok_or_else(too_large)?;
        let count = |lists: &[Vec<i128>]| lists.iter().map(Vec::len).product::<usize>();
        let states = count(&values[..state_width]);
        let inputs = count(&values[state_width..]);
        Ok(Self {
            values,
            state_width,
            states,
            inputs,
        })
    }

    pub(super) fn tuples(&self) -> usize {
        self.states * self.inputs
    }

    /// The values of the `index`th element of the product of `lists`, the
    /// last list fastest.
    fn element(lists: &[Vec<i128>], mut index: usize) -> Vec<i128> {
        let mut out = vec![0; lists.len()];
        for (slot, list) in out.iter_mut().zip(lists).rev() {
            *slot = list[index % list.len()];
            index /= list.len();
        }
        out
    }

    /// The index of `values` in the product of `lists`, if every value is in
    /// its list.
    fn index(lists: &[Vec<i128>], values: &[i128]) -> Option<usize> {
        if values.len() != lists.len() {
            return None;
        }
        lists
            .iter()
            .zip(values)
            .try_fold(0usize, |index, (list, value)| {
                let at = list.iter().position(|candidate| candidate == value)?;
                Some(index * list.len() + at)
            })
    }

    pub(super) fn state(&self, index: usize) -> Vec<i128> {
        Self::element(&self.values[..self.state_width], index)
    }

    pub(super) fn input(&self, index: usize) -> Vec<i128> {
        Self::element(&self.values[self.state_width..], index)
    }

    pub(super) fn state_of(&self, values: &[i128]) -> Option<usize> {
        Self::index(&self.values[..self.state_width], values)
    }

    /// The tuple of a state and an input.
    pub(super) fn tuple(&self, state: usize, input: usize) -> Vec<i128> {
        let mut tuple = self.state(state);
        tuple.extend(self.input(input));
        tuple
    }

    /// The tuple at `ordinal`.
    pub(super) fn at(&self, ordinal: usize) -> Vec<i128> {
        self.tuple(ordinal / self.inputs, ordinal % self.inputs)
    }

    /// How many values each state position takes.
    pub(super) fn state_sizes(&self) -> Vec<usize> {
        self.values[..self.state_width]
            .iter()
            .map(Vec::len)
            .collect()
    }

    /// How many values each position takes, in program order.
    pub(super) fn sizes(&self) -> Vec<usize> {
        self.values.iter().map(Vec::len).collect()
    }
}

/// What the Authority made of one tuple, as the judges compare it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Observed {
    /// A decision; `next` is the successor state's index, the pre-state's
    /// for a reject.
    Decided {
        class: Class,
        reason: Option<u32>,
        next: usize,
    },
    /// A refusal, which commits nothing.
    Refused {
        class: RefusalClass,
        law: Option<u32>,
    },
}

impl Observed {
    /// Whether the decision commits a successor: an accept or a committed
    /// failure. A reject and a refusal keep the state.
    pub(super) fn commits(self) -> bool {
        matches!(self, Self::Decided { class, .. } if class != Class::Reject)
    }

    /// The state after this outcome from `state`.
    pub(super) fn after(self, state: usize) -> usize {
        match self {
            Self::Decided { next, .. } => next,
            Self::Refused { .. } => state,
        }
    }

    /// What a person sees of the outcome without the successor: the class
    /// and reason of a decision, or a refusal with its class and law.
    pub(super) fn signature(self) -> (u8, u8, Option<u32>) {
        match self {
            Self::Decided { class, reason, .. } => {
                (0, u8::try_from(class.code()).unwrap_or(u8::MAX), reason)
            }
            Self::Refused { class, law } => {
                let code = RefusalClass::ALL
                    .iter()
                    .position(|candidate| *candidate == class)
                    .unwrap_or(usize::MAX);
                (1, u8::try_from(code).unwrap_or(u8::MAX), law)
            }
        }
    }

    pub(super) fn json(self, space: &Space) -> serde_json::Value {
        use serde_json::json;
        match self {
            Self::Decided {
                class,
                reason,
                next,
            } => json!({
                "class": class.name(),
                "reason": reason,
                "post": crate::report_numbers::integers(&space.state(next)),
            }),
            Self::Refused { class, law } => json!({
                "class": "Refused",
                "refusal": class.name(),
                "law": law,
            }),
        }
    }
}

/// Every outcome over the whole domain.
#[derive(Debug)]
pub(super) struct Machine {
    pub(super) space: Space,
    /// The outcome of each ordinal.
    pub(super) observed: Vec<Observed>,
    /// SHA-256 of each decision's outbox, as the review digests it; zero for
    /// a refusal.
    pub(super) outboxes: Vec<[u8; 32]>,
    deliveries: BTreeMap<usize, Vec<evaluate::Delivery>>,
    /// The index of the genesis state.
    pub(super) genesis: usize,
    /// The Authority's admission of the declared starting state.
    pub(super) starting_state: Result<(), genesis::Refusal>,
}

impl Machine {
    /// The library Authority's outcome on every tuple of `space`.
    ///
    /// # Errors
    /// A genesis value or a successor outside the declared domain, which the
    /// library refuses to commit, so neither occurs for a bound contract.
    pub(super) fn observe(
        authority: &Authority<'_>,
        framer: &Framer,
        space: Space,
        positions: &[Position],
        rules: &Rules,
    ) -> Result<Self, ContractError> {
        let genesis = genesis(&space, positions, rules)?;
        let starting_state = genesis::admit(
            authority,
            &framer.frame(&space.at(genesis * space.inputs))[0],
        );
        let results = parallel(space.tuples(), |ordinal| {
            let tuple = space.at(ordinal);
            let outcome = evaluate::evaluate(authority, framer, &tuple);
            let deliveries = match &outcome {
                Outcome::Decision(d) => d.outbox.clone(),
                Outcome::Refused(_) => Vec::new(),
            };
            observe(&space, positions, &outcome, ordinal).map(|(a, b)| (a, b, deliveries))
        });
        let mut observed = Vec::with_capacity(results.len());
        let mut outboxes = Vec::with_capacity(results.len());
        let mut deliveries = BTreeMap::new();
        for (ordinal, result) in results.into_iter().enumerate() {
            let (outcome, outbox, full) = result?;
            if !full.is_empty() {
                deliveries.insert(ordinal, full);
            }
            observed.push(outcome);
            outboxes.push(outbox);
        }
        Ok(Self {
            space,
            observed,
            outboxes,
            deliveries,
            genesis,
            starting_state,
        })
    }

    pub(super) fn outbox_at(&self, ordinal: usize) -> &[evaluate::Delivery] {
        self.deliveries
            .get(&ordinal)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// The outcome of `state` on `input`.
    pub(super) fn of(&self, state: usize, input: usize) -> Observed {
        self.observed[state * self.space.inputs + input]
    }

    /// Complete immutable-table projection, states then inputs ascending.
    /// Each returned vector is exclusively owned by this invocation's caller.
    pub(super) fn kernel_tables(&self) -> (Vec<usize>, Vec<bool>) {
        let mut after = Vec::with_capacity(self.observed.len());
        let mut commits = Vec::with_capacity(self.observed.len());
        for state in 0..self.space.states {
            for input in 0..self.space.inputs {
                let outcome = self.of(state, input);
                after.push(outcome.after(state));
                commits.push(outcome.commits());
            }
        }
        (after, commits)
    }

    /// Every state reachable from genesis through committed decisions, and
    /// for each the state and input that first reached it in breadth-first
    /// order, inputs ascending; genesis has none.
    pub(super) fn reachable(&self) -> (Vec<bool>, Vec<Option<(usize, usize)>>) {
        let mut seen = vec![false; self.space.states];
        let mut parent = vec![None; self.space.states];
        if self.starting_state.is_err() {
            return (seen, parent);
        }
        let mut queue = std::collections::VecDeque::from([self.genesis]);
        seen[self.genesis] = true;
        while let Some(state) = queue.pop_front() {
            for input in 0..self.space.inputs {
                let next = self.of(state, input).after(state);
                if !seen[next] {
                    seen[next] = true;
                    parent[next] = Some((state, input));
                    queue.push_back(next);
                }
            }
        }
        (seen, parent)
    }

    /// The inputs, in order, that lead from genesis to `target` along the
    /// breadth-first parents: a shortest trace.
    pub(super) fn trace(parent: &[Option<(usize, usize)>], mut target: usize) -> Vec<usize> {
        let mut inputs = Vec::new();
        while let Some((state, input)) = parent[target] {
            inputs.push(input);
            target = state;
        }
        inputs.reverse();
        inputs
    }
}

/// The genesis state's index, from the rules' genesis values.
fn genesis(space: &Space, positions: &[Position], rules: &Rules) -> Result<usize, ContractError> {
    let values = positions[..space.state_width]
        .iter()
        .map(|position| {
            let field = position.field.ok_or_else(|| {
                ContractError::new("project.zeno", "a scalar state root has no genesis field")
            })?;
            match rules.genesis.get(&field) {
                Some(Constant::U128(_)) => Err(ContractError::new(
                    "legacy machine",
                    "U128SubjectUnsupported",
                )),
                Some(Constant::Bool(value)) => Ok(i128::from(*value)),
                Some(Constant::Int(value)) => Ok(*value),
                None => Err(ContractError::new(
                    "v2/policy.json genesis",
                    format!("has no field {field}"),
                )),
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    space.state_of(&values).ok_or_else(|| {
        ContractError::new(
            "v2/policy.json genesis",
            "lies outside the declared state domain",
        )
    })
}

/// One outcome as the judges compare it, with its outbox digest.
fn observe(
    space: &Space,
    positions: &[Position],
    outcome: &Outcome,
    ordinal: usize,
) -> Result<(Observed, [u8; 32]), ContractError> {
    let state = ordinal / space.inputs;
    match outcome {
        Outcome::Refused(refusal) => Ok((
            Observed::Refused {
                class: refusal.class,
                law: refusal.law,
            },
            [0; 32],
        )),
        Outcome::Decision(decision) => {
            let next = if decision.class == Class::Reject {
                state
            } else {
                let by_field: BTreeMap<u16, Option<i128>> = decision
                    .post
                    .iter()
                    .map(|(field, value)| (*field, value.number()))
                    .collect();
                let values: Option<Vec<i128>> = positions[..space.state_width]
                    .iter()
                    .map(|position| {
                        let number = (*by_field.get(&position.field.unwrap_or(0))?)?;
                        Some(number)
                    })
                    .collect();
                values
                    .and_then(|values| space.state_of(&values))
                    .ok_or_else(|| {
                        ContractError::new(
                            "library authority",
                            format!("committed a successor outside the declared state domain at input {ordinal}"),
                        )
                    })?
            };
            Ok((
                Observed::Decided {
                    class: decision.class,
                    reason: decision.reason,
                    next,
                },
                decision.outbox_digest,
            ))
        }
    }
}

/// SHA-256, in hexadecimal, of `lines`, each followed by a newline: the
/// digest of a per-tuple or per-state table, which a separately written
/// reference can recompute line for line.
pub(super) fn table_digest(lines: impl Iterator<Item = String>) -> String {
    let mut text = String::new();
    for line in lines {
        text.push_str(&line);
        text.push('\n');
    }
    evaluate::hex(&evaluate::digest(text.as_bytes()))
}

/// A tuple's outcome as one table line: `A`, `R` or `F` for the class, the
/// reason or `-`, then the state after it; `X` for a refusal.
pub(super) fn decision_line(space: &Space, outcome: Observed, state: usize) -> String {
    match outcome {
        Observed::Refused { .. } => "X".to_owned(),
        Observed::Decided { class, reason, .. } => {
            let class = match class {
                Class::Accept => "A",
                Class::Reject => "R",
                Class::CommittedFailure => "F",
            };
            let reason = reason.map_or_else(|| "-".to_owned(), |reason| reason.to_string());
            let after: Vec<String> = space
                .state(outcome.after(state))
                .iter()
                .map(ToString::to_string)
                .collect();
            format!("{class} {reason} {}", after.join(" "))
        }
    }
}

/// The smallest number of bits that tell `count` values apart: 0 for at
/// most one.
pub(super) fn bits(count: usize) -> u32 {
    if count <= 1 {
        0
    } else {
        usize::BITS - (count - 1).leading_zeros()
    }
}

/// A contract's machine, with which states satisfy every state law.
pub(super) struct Observation {
    pub(super) machine: Machine,
    /// Whether each state satisfies every state law, as `contract review`
    /// decides it.
    pub(super) valid: Vec<bool>,
    /// The contract's state laws, in its law order.
    pub(super) state_laws: Vec<u32>,
}

/// Binds the contract and observes its whole domain.
///
/// # Errors
/// The generator's or the library's refusal of the contract; inconclusive
/// when the domain exceeds `max_tuples`.
#[cfg(test)]
pub(super) fn observe_contract(
    loaded: &Loaded,
    max_tuples: u64,
) -> Result<Result<Observation, Inconclusive>, ContractError> {
    observe_current(loaded, None, max_tuples)
}

/// Observe the verified current Authority, or the ordinary rules-only baseline.
pub(super) fn observe_current(
    loaded: &Loaded,
    current: Option<&Current<'_>>,
    max_tuples: u64,
) -> Result<Result<Observation, Inconclusive>, ContractError> {
    let space = match Space::new(&loaded.positions, loaded.state_width, max_tuples) {
        Ok(space) => space,
        Err(inconclusive) => return Ok(Err(inconclusive)),
    };
    loaded
        .bind_current(current, |authority, framer| {
            let descriptor = authority.descriptor();
            let machine =
                Machine::observe(authority, framer, space, &loaded.positions, &loaded.rules)?;
            let state_laws = evaluate::state_laws(descriptor);
            let valid = valid_states(&machine.space, |tuple| {
                evaluate::first_unsatisfied_state_law(descriptor, &state_laws, framer, tuple)
                    .is_none()
            });
            Ok(Observation {
                machine,
                valid,
                state_laws,
            })
        })
        .map(Ok)
}

/// `test` on each state, framed with the first input; a state test reads
/// only the state.
pub(super) fn valid_states(space: &Space, test: impl Fn(&[i128]) -> bool + Sync) -> Vec<bool> {
    let input = space.input(0);
    parallel(space.states, |state| {
        let mut tuple = space.state(state);
        tuple.extend_from_slice(&input);
        test(&tuple)
    })
}

/// The contract with one more law, `law ID NAME on SCOPE = FORMULA;`,
/// appended to `project.zeno` and given `kind`, and the law's ID: an ID
/// above every number `project.zeno` holds and every framework law's.
///
/// # Errors
/// A formula with trailing text, undeclared paths or no contract form, named as
/// `what`.
pub(super) fn with_law(
    project: &str,
    rules: &Rules,
    what: &str,
    scope: &str,
    formula: &str,
    kind: super::super::rules::LawKind,
) -> Result<(Loaded, u32), ContractError> {
    let invalid = |reason: String| ContractError::new(what, reason);
    if formula.trim().is_empty() {
        return Err(invalid("is empty".to_owned()));
    }
    let highest = project
        .split(|character: char| !character.is_ascii_digit())
        .filter_map(|digits| digits.parse::<u32>().ok())
        .chain([990, 991, 909, rules.failure_law, rules.reject_law])
        .max()
        .unwrap_or(0);
    let id = highest
        .checked_add(1)
        .ok_or_else(|| invalid("has no free law ID".to_owned()))?;
    let (scope, genesis) = match scope {
        "commit, genesis" => (zeno_fcis_spec::LawScope::Committing, true),
        "commit" => (zeno_fcis_spec::LawScope::Committing, false),
        "any" => (zeno_fcis_spec::LawScope::Always, false),
        _ => return Err(invalid("unsupported formula scope".to_owned())),
    };
    let applicability = zeno_fcis_spec::LawApplicability::new(scope, genesis);
    let parsed = zeno_fcis_spec::parse_law_formula(
        project,
        formula,
        applicability,
        zeno_fcis_spec::SourceLimits::default(),
    )
    .map_err(|diagnostics| invalid(diagnostics.to_string()))?;
    let formula = super::super::expr::law(&parsed).map_err(invalid)?;
    let mut extended = rules.clone();
    extended.law_kinds.insert(id, kind);
    let mut loaded = Loaded::with(project, extended)
        .map_err(|error| invalid(format!("{}: {}", error.place(), error.reason())))?;
    loaded
        .declarations
        .laws
        .push(super::super::declarations::Law {
            id,
            name: format!("judged_formula_{id}"),
            scope,
            genesis,
            formula,
        });
    Ok((loaded, id))
}

#[cfg(test)]
mod lineage_binding_tests {
    use super::{Loaded, Space, evaluate, policy};
    use crate::contract::{
        Adoption, AdoptionSources, ContractSources, Usage, generate_contract, program_bytes,
        with_adoption, with_current,
    };
    use crate::transform::{self, Limits};
    use zeno_fcis_synthesis::finite::v2_composition::Raw;
    use zeno_fcis_synthesis::finite::{Op, Program};
    use zeno_fcis_synthesis::finite_runtime::import_program;

    const PROJECT: &str = include_str!("../../../templates/durable-counter/project.zeno");
    const RULES: &str = include_str!("../../../templates/durable-counter/v2/policy.json");

    #[test]
    fn current_binding_preserves_every_original_envelope_and_uses_adopted_graph() {
        let base = ContractSources {
            project: PROJECT,
            rules: RULES,
            schema_origin: None,
            adoptions: &[],
            replayed: &[],
            evolutions: &[],
        };
        let original = generate_contract(base).unwrap_or_else(|e| panic!("{e}"));
        let old_bytes = original.program().unwrap_or_else(|| panic!("program"));
        let old = import_program(old_bytes).unwrap_or_else(|e| panic!("{e}"));
        let mut nodes = old.nodes().to_vec();
        nodes.push(Op::Int(0));
        let changed = Program::try_new(
            old.inputs().to_vec(),
            old.outputs().to_vec(),
            nodes,
            old.roots().to_vec(),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let candidate = program_bytes(&changed).unwrap_or_else(|e| panic!("{e}"));
        let receipt = transform::check(
            old_bytes,
            &candidate,
            Limits {
                steps: transform::DEFAULT_STEP_LIMIT,
                input_tuples: 1024,
            },
        )
        .unwrap_or_else(|e| panic!("{e:?}"))
        .receipt();
        let adoption = Adoption {
            candidate_sha256: transform::sha256_hex(&candidate),
            receipt_sha256: transform::sha256_hex(&receipt),
            usage: Usage::NewVersion,
            superseded_policy_sha256: transform::sha256_hex(original.policy()),
        };
        let rules = with_adoption(RULES, &adoption).unwrap_or_else(|e| panic!("{e}"));
        let attachments = [AdoptionSources {
            candidate: &candidate,
            receipt: &receipt,
        }];
        let sources = ContractSources {
            rules: &rules,
            adoptions: &attachments,
            ..base
        };
        let generated = generate_contract(sources).unwrap_or_else(|e| panic!("{e}"));
        with_current(sources, &generated, |current| {
            let old_loaded = Loaded::read(PROJECT, RULES)?;
            let current_loaded = Loaded::read_current(&current)?;
            let space = Space::new(&old_loaded.positions, old_loaded.state_width, 1024)
                .unwrap_or_else(|e| panic!("space: {e:?}"));
            assert_eq!(space.tuples(), 64);
            // Independently rebuild and bind a descriptor from the retained
            // candidate, then require its complete encoded policy to be the
            // generator's current policy before comparing raw reports.
            let mut reference = super::Contract::build(
                &current_loaded.declarations,
                &current_loaded.rules,
                current_loaded.commitment,
            )?;
            let decoded = import_program(&candidate).unwrap_or_else(|e| panic!("{e}"));
            reference.adopt(&decoded, "independent current")?;
            assert_eq!(
                policy::encode(&reference, current.schema)?,
                generated.policy()
            );
            old_loaded.bind(&old_loaded.rules, |old_authority, old_framer| {
                policy::with_authority(&reference, current.schema, |reference_authority| {
                    current_loaded.bind_current(Some(&current), |new_authority, new_framer| {
                        assert_eq!(
                            new_authority.descriptor().program.nodes.len(),
                            old.nodes().len() + 1
                        );
                        assert_ne!(new_authority.identity(), old_authority.identity());
                        assert_eq!(new_authority.identity(), reference_authority.identity());
                        for ordinal in 0..space.tuples() {
                            let tuple = space.at(ordinal);
                            let frames = new_framer.frame(&tuple);
                            assert_eq!(old_framer.frame(&tuple), frames);
                            let before = evaluate::evaluate(old_authority, old_framer, &tuple);
                            let after = evaluate::evaluate(new_authority, new_framer, &tuple);
                            assert_eq!(before, after, "tuple {tuple:?}");
                            let raw = Raw {
                                state: &frames[0],
                                command: &frames[1],
                                context: &frames[2],
                            };
                            let actual = new_authority.evaluate(raw);
                            let expected = reference_authority.evaluate(raw);
                            assert_eq!(
                                evaluate::from_evaluation(&actual),
                                evaluate::from_evaluation(&expected)
                            );
                            assert_eq!(actual.kind(), expected.kind());
                            assert_eq!(actual.subject(), expected.subject());
                            assert_eq!(actual.raw().state, expected.raw().state);
                            assert_eq!(actual.raw().command, expected.raw().command);
                            assert_eq!(actual.raw().context, expected.raw().context);
                            assert_eq!(actual.ingress_usage(), expected.ingress_usage());
                            assert_eq!(actual.decision_usage(), expected.decision_usage());
                            assert_eq!(actual.usage(), expected.usage());
                            assert_eq!(actual.reads(), expected.reads());
                            assert_eq!(actual.decision_attempts(), expected.decision_attempts());
                            assert_eq!(actual.diagnostics(), expected.diagnostics());
                            assert_eq!(actual.law_reads(), expected.law_reads());
                        }
                        Ok(())
                    })
                })?
            })?;
            assert!(
                old_loaded
                    .bind_current(Some(&current), |_, _| Ok(()))
                    .is_err()
            );
            Ok(())
        })
        .unwrap_or_else(|e| panic!("{e}"));
    }
}

/// The reject-everything core of `rules`: every reason keeps a case of its
/// class that never fires, so the declarations stay the contract's, and a
/// final catch-all rejects with the least reason a reject case uses. `None`
/// when no case rejects.
pub(super) fn reject_everything(rules: &Rules) -> Option<(Rules, u32)> {
    use super::super::expr::Ast;
    use super::super::rules::Case;
    let reason = rules
        .cases
        .iter()
        .filter(|case| case.class == Class::Reject)
        .filter_map(|case| case.reason)
        .min()?;
    let mut cases: Vec<Case> = Vec::new();
    let mut kept = std::collections::BTreeSet::new();
    for case in &rules.cases {
        if let Some(used) = case.reason
            && kept.insert(used)
        {
            cases.push(Case {
                rule: None,
                when: Ast::Bool(false),
                ..case.clone()
            });
        }
    }
    cases.push(Case {
        rule: None,
        when: Ast::Bool(true),
        class: Class::Reject,
        reason: Some(reason),
        post: BTreeMap::new(),
        outbox: Vec::new(),
    });
    Some((
        Rules {
            cases,
            ..rules.clone()
        },
        reason,
    ))
}
