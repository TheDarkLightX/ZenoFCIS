//! The specification-gap judge: how completely the laws a contract declares
//! pin down its decisions.
//!
//! The case table decides; the laws `project.zeno` declares are the reviewed
//! requirements. Where the laws admit a decision the case table does not
//! make, a candidate optimized against the laws alone could be wrong. Over
//! the whole declared domain, and over the tuples whose pre-state satisfies
//! every state law, the judge reports:
//!
//! - admissible decisions: for each tuple, every decision a contract of this
//!   schema could make (a reject with each declared reason; an accept with
//!   each declared state; a committed failure with each reason and state),
//!   checked against the declared laws by the
//!   library's law evaluator. A tuple is determined when exactly one decision
//!   is admitted. Tuples where the contract's own decision is refused are
//!   counted apart.
//! - law activation: on how many tuples each declared law applies to the
//!   contract's decision; a law that never applies constrains nothing.
//! - the reject-everything core: a contract of the same declarations whose
//!   only firing case rejects, bound through the library, and the tuples
//!   where its Authority refuses it.
//! - mutants of the extended catalog, each bound through the library: a
//!   mutant that decides differently somewhere is caught when its Authority
//!   refuses it on a tuple where the contract decides, and admitted
//!   otherwise; an admitted mutant is a concrete decision table the laws
//!   allow and the contract excludes.
//! - proposed laws: each must hold wherever it applies to the contract's
//!   decisions; the judge then repeats the measures with them.
//!
//! The framework laws generation adds are not among the declared laws: law
//! 991 admits only the case table's own decision, so with it every tuple
//! would be determined. Every decision and every law verdict is the
//! library's; this module only proposes decisions and counts.

use std::collections::BTreeSet;

use serde_json::{Value, json};

use super::super::ContractError;
use super::super::declarations::Declarations;
use super::super::rules::{Class, LawKind, Rules};
use super::deliveries::{self, DeliveryUniverse};
use super::evaluate::{self, Framer, LawSet, Proposed};
use super::machine::{self, Inconclusive, Loaded, Machine, Observed, Space};
use super::mutants::{self, Mutant};
use super::parallel;
use zeno_fcis_synthesis::finite::v2_authority::Authority;

/// The default limit on law checks: tuples times candidate decisions times
/// declared laws.
pub(crate) const DEFAULT_MAX_WORK: u64 = 60_000_000;

/// Undetermined tuples and admitted mutants written out in full, at most.
const EXAMPLES: usize = 3;

/// The judge's settings.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Options<'a> {
    pub(crate) max_tuples: u64,
    pub(crate) max_work: u64,
    /// Proposed laws: `.zeno` law declarations, one per line.
    pub(crate) proposals: Option<&'a str>,
    /// Whether to bind the extended catalog's mutants and the
    /// reject-everything core. Without them those sections are `null`; every
    /// other measure is unchanged.
    pub(crate) mutants: bool,
}

/// The judge's result.
#[derive(Clone, Debug)]
pub(crate) enum Gaps {
    /// Every measure ran to the end over the whole domain.
    Measured(Value),
    /// A proposed law fails where it applies to the contract's decision.
    FalseAlarm(Value),
    /// The domain or the work exceeds its limit; nothing is claimed.
    Inconclusive(Inconclusive),
    /// The contract is outside the judge's scope; nothing is claimed.
    OutOfScope(String),
    /// The judge's own frames disagree with the library on the contract's
    /// decisions, so every measure is withheld.
    Withheld(String),
    /// The proposed laws have no contract form; nothing is measured with
    /// them.
    InvalidProposals { place: String, reason: String },
}

impl Gaps {
    pub(crate) fn status(&self) -> &'static str {
        match self {
            Self::Measured(_) => "measured",
            Self::FalseAlarm(_) => "proposal-false-alarm",
            Self::Inconclusive(_) => "inconclusive",
            Self::OutOfScope(_) => "out-of-scope",
            Self::Withheld(_) => "withheld",
            Self::InvalidProposals { .. } => "invalid-proposals",
        }
    }

    /// The review's status: `reviewed` once measured, else this status.
    pub(crate) fn review_status(&self) -> &'static str {
        match self {
            Self::Measured(_) => "reviewed",
            other => other.status(),
        }
    }

    pub(crate) fn json(&self) -> Value {
        match self {
            Self::Measured(value) | Self::FalseAlarm(value) => value.clone(),
            Self::Inconclusive(inconclusive) => inconclusive.json(),
            Self::OutOfScope(reason) => json!({"status": "out-of-scope", "reason": reason}),
            Self::Withheld(reason) => json!({"status": "withheld", "reason": reason}),
            Self::InvalidProposals { place, reason } => {
                json!({"status": "invalid-proposals", "place": place, "reason": reason})
            }
        }
    }

    /// The counts a summary carries.
    pub(crate) fn summary(&self) -> Value {
        let mut summary = json!({"status": self.status()});
        if let Self::Measured(value) = self {
            summary = measured_summary(value);
            if let Some(after) = value["proposals"].get("after").filter(|v| v.is_object()) {
                summary["with_proposals"] = measured_summary(after);
            }
        }
        summary
    }
}

/// Project measured counts without changing either checked measure.
fn measured_summary(value: &Value) -> Value {
    let mut summary = json!({"status": "measured"});
    for domain in ["full", "valid"] {
        summary[domain] = json!({
            "tuples": value["admissible"][domain]["tuples"],
            "determined": value["admissible"][domain]["determined"],
            "mutants_admitted": value["mutation"][domain]["admitted"],
            "mutants_caught": value["mutation"][domain]["caught"],
        });
    }
    summary["inactive_laws"] = value["activation"]["inactive"].clone();
    summary["reject_everything_admitted"] =
        value["reject_everything"]["admitted_where_the_contract_decides"].clone();
    summary
}

/// Runs the judge on a contract.
///
/// # Errors
/// A declaration, rule or proposed law with no contract form, or the
/// library's refusal of the contract.
pub(crate) fn judge(
    project: &str,
    rules: &str,
    options: Options<'_>,
) -> Result<Gaps, ContractError> {
    let loaded = Loaded::read(project, rules)?;
    if let Some(reason) = out_of_scope(&loaded) {
        return Ok(Gaps::OutOfScope(reason));
    }
    let space = match Space::new(&loaded.positions, loaded.state_width, options.max_tuples) {
        Ok(space) => space,
        Err(inconclusive) => return Ok(Gaps::Inconclusive(inconclusive)),
    };
    let work = work_of(&loaded, &space)?;
    if let Some(over) = over_limit(work, options.max_work, "work-over-limit") {
        return Ok(over);
    }
    let before = match measure(&loaded, space, options.max_tuples, options.mutants) {
        Ok(measured) => measured,
        Err(Stop::Contract(error)) => return Err(error),
        Err(Stop::Withheld(reason)) => return Ok(Gaps::Withheld(reason)),
        Err(Stop::OutOfScope(reason)) => return Ok(Gaps::OutOfScope(reason)),
    };
    let Some(text) = options.proposals else {
        return Ok(Gaps::Measured(before.json(work, options.max_work, None)));
    };
    let (extended, ids, texts) = match extend(project, &loaded.rules, text) {
        Ok(extended) => extended,
        Err(error) => {
            return Ok(Gaps::InvalidProposals {
                place: error.place().to_owned(),
                reason: error.reason().to_owned(),
            });
        }
    };
    let checked = check_proposals(&extended, &ids, texts, &before.machine)?;
    if let Some(alarm) = &checked.false_alarm {
        return Ok(Gaps::FalseAlarm(json!({
            "status": "proposal-false-alarm",
            "proposals": checked.laws_json(),
            "false_alarm": alarm,
        })));
    }
    let space = Space::new(
        &extended.positions,
        extended.state_width,
        options.max_tuples,
    )
    .map_err(|_| ContractError::new("proposed laws", "changed the domain"))?;
    // The repeat measure runs the proposals too: its work is its own.
    let work_after = work_of(&extended, &space)?;
    if let Some(over) = over_limit(
        work_after,
        options.max_work,
        "work-with-proposals-over-limit",
    ) {
        return Ok(over);
    }
    let after = match measure(&extended, space, options.max_tuples, options.mutants) {
        Ok(measured) => measured,
        Err(Stop::Contract(error)) => return Err(error),
        Err(Stop::Withheld(reason)) => return Ok(Gaps::Withheld(reason)),
        Err(Stop::OutOfScope(reason)) => return Ok(Gaps::OutOfScope(reason)),
    };
    let (admitted, killed) = match (&before.mutation, &after.mutation) {
        (Some(earlier), Some(later)) => (
            json!({
                "full": [earlier.counts[0].admitted, later.counts[0].admitted],
                "valid": [earlier.counts[1].admitted, later.counts[1].admitted],
            }),
            json!(
                earlier
                    .admitted_ids()
                    .difference(&later.admitted_ids())
                    .copied()
                    .collect::<Vec<&str>>()
            ),
        ),
        _ => (Value::Null, Value::Null),
    };
    let proposals = json!({
        "laws": checked.laws_json(),
        "inactive": checked.inactive(),
        "determined": {
            "full": [before.admissible[0].determined, after.admissible[0].determined],
            "valid": [before.admissible[1].determined, after.admissible[1].determined],
        },
        "mutants_admitted": admitted,
        "now_caught": killed,
        "after": after.json(work_after, options.max_work, None),
    });
    Ok(Gaps::Measured(before.json(
        work,
        options.max_work,
        Some(proposals),
    )))
}

/// Inconclusive when `work` exceeds the limit.
fn over_limit(work: u128, max_work: u64, reason: &'static str) -> Option<Gaps> {
    (work > u128::from(max_work)).then_some(Gaps::Inconclusive(Inconclusive {
        reason,
        size: Some(work),
        limit: max_work,
    }))
}

/// Why a measure stopped.
enum Stop {
    OutOfScope(String),
    Contract(ContractError),
    Withheld(String),
}

impl From<ContractError> for Stop {
    fn from(error: ContractError) -> Self {
        Self::Contract(error)
    }
}

/// The same declaration admission and scalar profile used before gap enumeration.
/// Question preparation calls this over its owned source snapshot.
pub(crate) fn profile_scope(project: &str, rules: &str) -> Result<Option<String>, ContractError> {
    let loaded = Loaded::read(project, rules)?;
    Ok(out_of_scope(&loaded))
}

/// Why the scalar decision universe cannot cover this contract.
fn out_of_scope(loaded: &Loaded) -> Option<String> {
    if DeliveryUniverse::new(&loaded.declarations, &loaded.rules).is_some() {
        return None;
    }
    if !loaded.declarations.channels.is_empty() {
        return Some("declared channels require a complete delivery decision universe".to_owned());
    }
    let rules = &loaded.rules;
    if rules.cases.iter().any(|case| !case.outbox.is_empty()) || !rules.delivery_laws.is_empty() {
        return Some(
            "the contract delivers; the judge proposes decisions without deliveries".to_owned(),
        );
    }
    None
}

/// Tuples times candidate decisions times the contract's laws, framework
/// laws included: each candidate runs the declared laws first and then the
/// others, until one refuses.
///
/// # Errors
/// The generator's refusal of the contract.
fn work_of(loaded: &Loaded, space: &Space) -> Result<u128, ContractError> {
    let laws = super::super::model::Contract::build(
        &loaded.declarations,
        &loaded.rules,
        loaded.commitment,
    )?
    .laws
    .len() as u128;
    let rejects = loaded.reasons(Class::Reject).len() as u128;
    let failures = loaded.reasons(Class::CommittedFailure).len() as u128;
    let states = space.states as u128;
    let candidates = rejects + states * (1 + failures);
    let candidates = if DeliveryUniverse::new(&loaded.declarations, &loaded.rules).is_some() {
        2 * candidates + 1
    } else {
        candidates
    };
    Ok((space.tuples() as u128) * candidates * laws.max(1))
}

/// Every measure of one contract.
struct Measured {
    delivery_universe: Option<DeliveryUniverse>,
    machine: Machine,
    declared: Vec<u32>,
    /// Whether each state satisfies every state law.
    valid: Vec<bool>,
    state_laws: Vec<u32>,
    /// Over the whole domain, then over the tuples of valid pre-states.
    admissible: [Admissible; 2],
    activation: [Vec<usize>; 2],
    /// Reference decisions the judge's frames did not admit: zero unless the
    /// judge frames a decision unlike the library.
    frame_mismatches: usize,
    /// The admissible count of each tuple, `-` where the contract's own
    /// decision is refused, as a table digest.
    admissible_sha256: String,
    /// The reject-everything core and the mutants, when they were bound.
    reject_everything: Option<Value>,
    mutation: Option<Mutation>,
}

impl Measured {
    fn json(&self, work: u128, max_work: u64, proposals: Option<Value>) -> Value {
        let space = &self.machine.space;
        let valid_states = self.valid.iter().filter(|valid| **valid).count();
        let domain = |index: usize| -> Value {
            let admissible = &self.admissible[index];
            json!({
                "tuples": admissible.tuples,
                "reference_refused": admissible.refused,
                "determined": admissible.determined,
                "zero_outcome": admissible.zero,
                "undetermined": admissible.tuples - admissible.determined,
                "freedom": {
                    "class": admissible.class,
                    "reject_reason": admissible.reason,
                    "successor": admissible.successor,
                },
                "examples": admissible.examples.iter().map(|example| example.json(space, &self.machine, self.delivery_universe.as_ref())).collect::<Vec<_>>(),
            })
        };
        let activation = |index: usize| -> Value {
            self.declared
                .iter()
                .zip(&self.activation[index])
                .map(|(id, count)| (id.to_string(), json!(count)))
                .collect::<serde_json::Map<_, _>>()
                .into()
        };
        let inactive: Vec<u32> = self
            .declared
            .iter()
            .zip(&self.activation[0])
            .filter(|(_, count)| **count == 0)
            .map(|(id, _)| *id)
            .collect();
        let mut report = json!({
            "status": "measured",
            "domain": {
                "states": space.states,
                "inputs": space.inputs,
                "tuples": space.tuples(),
                "valid_states": valid_states,
                "state_laws": self.state_laws,
            },
            "completeness": {
                "status": if self.admissible[0].determined == space.tuples() && self.admissible[0].refused == 0 && self.machine.observed.iter().any(|o| matches!(o, Observed::Decided { class: Class::Accept, .. })) { "complete-finite" } else { "incomplete" },
                "law_991_used": false,
                "decision_universe": if self.delivery_universe.is_some() { "all declared scalar successors, reasons restricted to their catalog decision class, and schema-valid outboxes; exact determination by delivery independence; unrestricted delivery counts are lower bounds" } else { "all declared scalar successors with canonical difference patches, every declared reason restricted to its catalog decision class, Accept/Reject/CommittedFailure; effects and channels structurally excluded by the scalar profile" },
                "exactly_one": self.admissible[0].determined,
                "zero_outcome": self.admissible[0].zero,
                "execution_refusals": self.admissible[0].refused,
                "accepted_rows": self.machine.observed.iter().filter(|o| matches!(o, Observed::Decided { class: Class::Accept, .. })).count(),
                "residual_witnesses": self.admissible[0].examples.iter().map(|e| e.json(space, &self.machine, self.delivery_universe.as_ref())).collect::<Vec<_>>(),
                "meaning": "the declared laws only; compare against an independent reviewed relation before adoption",
            },
            "declared_laws": self.declared,
            "work": work.to_string(),
            "max_work": max_work,
            "frame_check": {
                "reference_decisions_rechecked": self.admissible[0].tuples - self.admissible[0].refused,
                "mismatches": self.frame_mismatches,
            },
            "admissible": {"full": domain(0), "valid": domain(1)},
            "activation": {"full": activation(0), "valid": activation(1), "inactive": inactive},
            "reject_everything": self.reject_everything,
            "tables": {
                "rule": "SHA-256 of one line per tuple in ordinal order, each ending in a newline: the number of admissible decisions, or `-` where the contract's own decision is refused",
                "admissible_sha256": self.admissible_sha256,
            },
            "mutation": self.mutation.as_ref().map(Mutation::json),
            "proposals": proposals,
        });
        if let Some(universe) = &self.delivery_universe {
            report["completeness"]["delivery_universe"] = universe.json();
        }
        report
    }
}

/// Determination over one domain.
#[derive(Clone, Debug, Default)]
struct Admissible {
    tuples: usize,
    refused: usize,
    determined: usize,
    zero: usize,
    /// Undetermined tuples where more than one class is admitted, more than
    /// one reject reason, or more than one committed successor.
    class: usize,
    reason: usize,
    successor: usize,
    examples: Vec<Example>,
}

/// One undetermined tuple.
#[derive(Clone, Debug)]
struct Example {
    ordinal: usize,
    reference: Observed,
    admissible: usize,
    /// Up to three admitted decisions other than the contract's.
    also: Vec<(Class, Option<u32>, Option<usize>)>,
}

impl Example {
    fn json(&self, space: &Space, machine: &Machine, universe: Option<&DeliveryUniverse>) -> Value {
        let mut value = json!({
            "input": crate::report_numbers::integers(&space.at(self.ordinal)),
            "contract": self.reference.json(space),
            "admissible": self.admissible,
            "also_admitted": self.also.iter().map(|(class, reason, next)| json!({
                "class": class.name(),
                "reason": reason,
                "post": next.map(|next| crate::report_numbers::integers(&space.state(next))),
            })).collect::<Vec<_>>(),
        });
        if let Some(universe) = universe {
            value["admissible_count_is_lower_bound"] = json!(true);
            if self.reference.commits() {
                let own = machine.outbox_at(self.ordinal);
                let alternate = if own.is_empty() {
                    universe.example()
                } else {
                    Vec::new()
                };
                value["delivery_choices"] =
                    json!([deliveries::shown(own), deliveries::shown(&alternate)]);
            }
        }
        value
    }
}

/// What the law check found on one tuple.
struct TupleGap {
    refused: bool,
    admissible: usize,
    classes: usize,
    reasons: usize,
    successors: usize,
    also: Vec<(Class, Option<u32>, Option<usize>)>,
    /// Laws that applied to the contract's decision.
    active: Vec<bool>,
    mismatch: bool,
}

fn measure(
    loaded: &Loaded,
    space: Space,
    max_tuples: u64,
    mutants: bool,
) -> Result<Measured, Stop> {
    let delivery_universe = DeliveryUniverse::new(&loaded.declarations, &loaded.rules);
    let declared = loaded.declared_laws();
    let rejects = loaded.reasons(Class::Reject);
    let failures = loaded.reasons(Class::CommittedFailure);
    let Some((machine, valid, state_laws, gaps)) =
        loaded.bind(&loaded.rules, |authority, framer| {
            let descriptor = authority.descriptor();
            let machine =
                Machine::observe(authority, framer, space, &loaded.positions, &loaded.rules)?;
            let space = &machine.space;
            let state_laws = evaluate::state_laws(descriptor);
            let input = space.input(0);
            let valid: Vec<bool> = (0..space.states)
                .map(|state| {
                    let mut tuple = space.state(state);
                    tuple.extend_from_slice(&input);
                    evaluate::first_unsatisfied_state_law(descriptor, &state_laws, framer, &tuple)
                        .is_none()
                })
                .collect();
            // The compiler's closed no-failure profile is independent of case order.
            // Its mandatory false law excludes CF even for legacy incomplete laws.
            let mut selected = declared.clone();
            if !loaded
                .rules
                .law_kinds
                .values()
                .any(|kind| *kind == LawKind::CommittedFailureEffects)
            {
                selected.push(loaded.rules.failure_law);
            }
            let laws = LawSet::new(descriptor, &selected);
            if delivery_universe.is_some() && !laws.delivery_independent() {
                return Ok(None);
            }

            let states: Vec<Vec<i128>> =
                (0..space.states).map(|state| space.state(state)).collect();
            let gaps = parallel(space.tuples(), |ordinal| {
                tuple_gap(
                    &laws,
                    framer,
                    &machine,
                    (&states, &rejects, &failures),
                    ordinal,
                    delivery_universe.as_ref(),
                )
            });
            Ok(Some((machine, valid, state_laws, gaps)))
        })?
    else {
        return Err(Stop::OutOfScope(
            "delivery-observing laws require a complete delivery decision universe".into(),
        ));
    };
    let space = &machine.space;
    let mut admissible = [Admissible::default(), Admissible::default()];
    let mut activation = [vec![0; declared.len()], vec![0; declared.len()]];
    let mut frame_mismatches = 0;
    for (ordinal, gap) in gaps.iter().enumerate() {
        frame_mismatches += usize::from(gap.mismatch);
        let domains: &[usize] = if valid[ordinal / space.inputs] {
            &[0, 1]
        } else {
            &[0]
        };
        for &index in domains {
            let counts = &mut admissible[index];
            counts.tuples += 1;
            if gap.refused {
                counts.refused += 1;
            }
            for (count, active) in activation[index].iter_mut().zip(&gap.active) {
                *count += usize::from(*active);
            }
            counts.zero += usize::from(gap.admissible == 0);
            if gap.admissible == 1 {
                counts.determined += 1;
                continue;
            }
            counts.class += usize::from(gap.classes > 1);
            counts.reason += usize::from(gap.reasons > 1);
            counts.successor += usize::from(gap.successors > 1);
            if counts.examples.len() < EXAMPLES {
                counts.examples.push(Example {
                    ordinal,
                    reference: machine.observed[ordinal],
                    admissible: gap.admissible,
                    also: gap.also.clone(),
                });
            }
        }
    }
    if frame_mismatches > 0 {
        return Err(Stop::Withheld(format!(
            "the declared laws refuse the contract's own decision, as the judge frames it, on \
             {frame_mismatches} tuples where the library Authority decides; the judge's frames \
             differ from the library's and its measures are withheld"
        )));
    }
    let admissible_sha256 = machine::table_digest(gaps.iter().map(|gap| {
        if gap.refused {
            "-".to_owned()
        } else {
            gap.admissible.to_string()
        }
    }));
    let (reject_everything, mutation) = if mutants {
        (
            Some(reject_everything(loaded, &machine, max_tuples)?),
            Some(mutation(loaded, &machine, &valid)?),
        )
    } else {
        (None, None)
    };
    Ok(Measured {
        delivery_universe,
        machine,
        declared,
        valid,
        state_laws,
        admissible,
        activation,
        frame_mismatches,
        admissible_sha256,
        reject_everything,
        mutation,
    })
}

/// Every candidate decision of one tuple through the declared laws.
fn tuple_gap(
    laws: &LawSet<'_>,
    framer: &Framer,
    machine: &Machine,
    domains: (&[Vec<i128>], &[u32], &[u32]),
    ordinal: usize,
    universe: Option<&DeliveryUniverse>,
) -> TupleGap {
    let (states, rejects, failures) = domains;
    let space = &machine.space;
    let tuple = space.at(ordinal);
    let state = ordinal / space.inputs;
    let reference = machine.observed[ordinal];
    let decided = match reference {
        Observed::Decided {
            class,
            reason,
            next,
        } => Some((class, reason, next)),
        _ => None,
    };
    let check =
        |class: Class, reason: Option<u32>, post: &[i128], outbox: &[evaluate::Delivery]| {
            laws.on_transition_with_outbox(
                framer,
                &tuple,
                &Proposed {
                    class,
                    reason,
                    post,
                },
                outbox,
            )
        };
    // Enumeration is independent of executable totality, including on refusals.
    let (admitted, diagnostics) = decided.map_or((true, Vec::new()), |(class, reason, next)| {
        check(class, reason, &states[next], machine.outbox_at(ordinal))
    });
    let active = laws
        .ids()
        .iter()
        .map(|id| {
            diagnostics.iter().any(|d| {
                d.id == *id && d.verdict != zeno_fcis_synthesis::finite::v2_laws::Verdict::Skipped
            })
        })
        .collect();
    let mut candidates: Vec<(Class, Option<u32>, Option<usize>)> = rejects
        .iter()
        .map(|reason| (Class::Reject, Some(*reason), None))
        .collect();
    for successor in 0..space.states {
        candidates.push((Class::Accept, None, Some(successor)));
    }
    for reason in failures {
        for successor in 0..space.states {
            candidates.push((Class::CommittedFailure, Some(*reason), Some(successor)));
        }
    }
    let own = decided
        .map(|(class, reason, next)| (class, reason, (class != Class::Reject).then_some(next)));
    let mut witness_mismatch = false;
    let mut admissible = 0;
    let mut classes = BTreeSet::new();
    let mut reject_reasons = BTreeSet::new();
    let mut successors = BTreeSet::new();
    let mut also = Vec::new();
    for candidate in candidates {
        let (class, reason, successor) = candidate;
        let post = successor.map_or(&states[state][..], |successor| &states[successor][..]);
        if !check(class, reason, post, &[]).0 {
            continue;
        }
        if class != Class::Reject
            && let Some(u) = universe
        {
            let witness = u.example();
            witness_mismatch |= witness.is_empty()
                || !u.allows(&witness)
                || !check(class, reason, post, &witness).0;
        }
        admissible += if universe.is_some() && class != Class::Reject {
            2
        } else {
            1
        };
        classes.insert(class.code());
        match successor {
            None => {
                reject_reasons.insert(reason);
            }
            Some(successor) => {
                successors.insert((class.code(), successor));
            }
        }
        if Some(candidate) != own && also.len() < EXAMPLES {
            also.push(candidate);
        }
    }
    TupleGap {
        refused: decided.is_none(),
        admissible,
        classes: classes.len(),
        reasons: reject_reasons.len(),
        successors: successors.len(),
        also,
        active,
        mismatch: witness_mismatch
            || !admitted
            || universe.is_some_and(|u| !u.allows(machine.outbox_at(ordinal))),
    }
}

/// The reject-everything core bound through the library, and the tuples
/// where its Authority refuses it.
fn reject_everything(
    loaded: &Loaded,
    machine: &Machine,
    max_tuples: u64,
) -> Result<Value, ContractError> {
    let Some((core, reason)) = machine::reject_everything(&loaded.rules) else {
        return Ok(json!({"status": "not-applicable", "reason": "no case rejects"}));
    };
    let observed = loaded.bind(&core, |authority, framer| {
        let space = Space::new(&loaded.positions, loaded.state_width, max_tuples)
            .map_err(|_| ContractError::new("reject-everything core", "domain changed"))?;
        Ok(Machine::observe(authority, framer, space, &loaded.positions, &core)?.observed)
    });
    let observed = match observed {
        Ok(observed) => observed,
        Err(error) => {
            return Ok(json!({
                "status": "not-bound",
                "reject_reason": reason,
                "place": error.place(),
                "reason": error.reason(),
            }));
        }
    };
    let mut refused = 0;
    let mut where_decided = 0;
    let mut first = None;
    for (ordinal, (core, contract)) in observed.iter().zip(&machine.observed).enumerate() {
        if let Observed::Refused { .. } = core {
            refused += 1;
            if let Observed::Decided { .. } = contract {
                where_decided += 1;
                first.get_or_insert(ordinal);
            }
        }
    }
    Ok(json!({
        "status": "bound",
        "reject_reason": reason,
        "refused_tuples": refused,
        "refused_where_the_contract_decides": where_decided,
        "admitted_where_the_contract_decides": where_decided == 0,
        "first_refusal": first.map(|ordinal| json!({
            "input": crate::report_numbers::integers(&machine.space.at(ordinal)),
            "refusal": observed[ordinal].json(&machine.space),
        })),
    }))
}

/// Counts of one mutant classification over one domain.
#[derive(Clone, Debug, Default)]
struct MutationCounts {
    caught: usize,
    admitted: usize,
    equivalent: usize,
}

/// How each mutant fared against the declared laws.
struct Mutation {
    /// Over the whole domain, then over the tuples of valid pre-states.
    counts: [MutationCounts; 2],
    refused_by_generator: usize,
    refused_by_library: usize,
    list: Vec<Value>,
    /// Admitted over the whole domain, by ID.
    admitted: Vec<String>,
}

impl Mutation {
    fn admitted_ids(&self) -> BTreeSet<&str> {
        self.admitted.iter().map(String::as_str).collect()
    }

    fn json(&self) -> Value {
        let domain = |index: usize| {
            let counts = &self.counts[index];
            let scored = counts.caught + counts.admitted;
            json!({
                "caught": counts.caught,
                "admitted": counts.admitted,
                "equivalent": counts.equivalent,
                "caught_of_scored": [counts.caught, scored],
            })
        };
        json!({
            "catalog": mutants::CATALOG_V2,
            "count": self.list.len(),
            "refused_by_generator": self.refused_by_generator,
            "refused_by_library": self.refused_by_library,
            "full": domain(0),
            "valid": domain(1),
            "list": self.list,
        })
    }
}

/// Binds every mutant of the extended catalog and compares its outcomes
/// with the contract's on every tuple. Refusals compare equal to each
/// other: neither commits nor decides.
fn mutation(loaded: &Loaded, machine: &Machine, valid: &[bool]) -> Result<Mutation, ContractError> {
    let catalog = mutants::extended_catalog(&loaded.rules, &loaded.declarations);
    let space = &machine.space;
    let mut result = Mutation {
        counts: [MutationCounts::default(), MutationCounts::default()],
        refused_by_generator: 0,
        refused_by_library: 0,
        list: Vec::new(),
        admitted: Vec::new(),
    };
    for mutant in &catalog {
        let entry = |classification: &str| {
            json!({
                "id": mutant.id,
                "operator": mutant.operator,
                "site": mutant.site,
                "change": mutant.change,
                "classification": classification,
            })
        };
        let bound = bind_mutant(loaded, mutant, space);
        let (observed, outboxes) = match bound {
            Err(Refused::Generator(error)) => {
                result.refused_by_generator += 1;
                let mut value = entry("refused-by-generator");
                value["refusal"] = json!({"place": error.place(), "reason": error.reason()});
                result.list.push(value);
                continue;
            }
            Err(Refused::Library(error)) => {
                result.refused_by_library += 1;
                let mut value = entry("refused-by-library");
                value["refusal"] = json!({"place": error.place(), "reason": error.reason()});
                result.list.push(value);
                continue;
            }
            Ok(bound) => bound,
        };
        let mut differs = [None, None];
        let mut caught = [None, None];
        for ordinal in 0..space.tuples() {
            let contract = machine.observed[ordinal];
            let mine = observed[ordinal];
            let same = match (contract, mine) {
                (Observed::Refused { .. }, Observed::Refused { .. }) => true,
                _ => contract == mine && machine.outboxes[ordinal] == outboxes[ordinal],
            };
            if same {
                continue;
            }
            let refused_here = matches!(mine, Observed::Refused { .. })
                && matches!(contract, Observed::Decided { .. });
            let domains: &[usize] = if valid[ordinal / space.inputs] {
                &[0, 1]
            } else {
                &[0]
            };
            for &index in domains {
                differs[index].get_or_insert(ordinal);
                if refused_here {
                    caught[index].get_or_insert(ordinal);
                }
            }
            if caught[1].is_some() {
                break;
            }
        }
        let classify = |index: usize| match (differs[index], caught[index]) {
            (None, _) => "equivalent",
            (Some(_), Some(_)) => "caught",
            (Some(_), None) => "admitted",
        };
        for index in [0, 1] {
            let counts = &mut result.counts[index];
            match classify(index) {
                "equivalent" => counts.equivalent += 1,
                "caught" => counts.caught += 1,
                _ => counts.admitted += 1,
            }
        }
        let mut value = entry(classify(0));
        value["valid_domain"] = json!(classify(1));
        let witness = |ordinal: usize| {
            json!({
                "input": crate::report_numbers::integers(&space.at(ordinal)),
                "contract": machine.observed[ordinal].json(space),
                "mutant": observed[ordinal].json(space),
            })
        };
        if let Some(ordinal) = caught[0] {
            value["caught_at"] = witness(ordinal);
        } else if let Some(ordinal) = differs[0] {
            value["admitted_witness"] = witness(ordinal);
            result.admitted.push(mutant.id.clone());
        }
        result.list.push(value);
    }
    Ok(result)
}

/// Why a mutant was not bound.
enum Refused {
    Generator(ContractError),
    Library(ContractError),
}

/// A mutant's outcome and outbox digest on every tuple.
fn bind_mutant(
    loaded: &Loaded,
    mutant: &Mutant,
    space: &Space,
) -> Result<(Vec<Observed>, Vec<[u8; 32]>), Refused> {
    let contract = super::super::model::Contract::build(
        &loaded.declarations,
        &mutant.rules,
        loaded.commitment,
    )
    .map_err(Refused::Generator)?;
    let framer = Framer::new(&loaded.positions, &contract);
    super::super::policy::with_authority(&contract, &loaded.schema, |authority| {
        observe_all(authority, &framer, space, loaded, &mutant.rules)
    })
    .map_err(Refused::Library)?
    .map_err(Refused::Library)
}

fn observe_all(
    authority: &Authority<'_>,
    framer: &Framer,
    space: &Space,
    loaded: &Loaded,
    rules: &Rules,
) -> Result<(Vec<Observed>, Vec<[u8; 32]>), ContractError> {
    let copy = Space::new(&loaded.positions, loaded.state_width, space.tuples() as u64)
        .map_err(|_| ContractError::new("mutant", "domain changed"))?;
    let machine = Machine::observe(authority, framer, copy, &loaded.positions, rules)?;
    Ok((machine.observed, machine.outboxes))
}

/// Proposed laws, checked against the contract's decisions.
struct Checked {
    ids: Vec<u32>,
    texts: Vec<String>,
    /// Tuples where each proposal applies to the contract's decision.
    applies: Vec<usize>,
    false_alarm: Option<Value>,
}

impl Checked {
    fn laws_json(&self) -> Vec<Value> {
        self.ids
            .iter()
            .zip(&self.texts)
            .zip(&self.applies)
            .map(|((id, text), applies)| {
                json!({"id": id, "declaration": text, "applies_on": applies})
            })
            .collect()
    }

    fn inactive(&self) -> Vec<u32> {
        self.ids
            .iter()
            .zip(&self.applies)
            .filter(|(_, applies)| **applies == 0)
            .map(|(id, _)| *id)
            .collect()
    }
}

/// The kind every proposed law takes. A kind does not change where a law
/// applies, except `InitialCondition`, which a proposal cannot take, and
/// this kind accepts every scope.
const PROPOSAL_KIND: LawKind = LawKind::AuthoritySubjectRecipient;

/// The contract with the proposed laws appended to `project.zeno`, the
/// IDs of the proposals and their declarations.
///
/// # Errors
/// A line that is not one law declaration, or a law with no contract form.
fn extend(
    project: &str,
    rules: &Rules,
    text: &str,
) -> Result<(Loaded, Vec<u32>, Vec<String>), ContractError> {
    let lines: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("//"))
        .map(str::to_owned)
        .collect();
    if lines.is_empty() {
        return Err(ContractError::new("proposed laws", "declare no law"));
    }
    for (number, line) in lines.iter().enumerate() {
        let single = line.starts_with("law ")
            && line.ends_with(';')
            && line.matches(';').count() == 1
            && !line.contains("//")
            && !line.contains("/*");
        if !single {
            return Err(ContractError::new(
                format!("proposed laws line {}", number + 1),
                "must be one `law` declaration ending in `;`",
            ));
        }
    }
    let laws_of = |source: &str| -> Result<Vec<u32>, ContractError> {
        Ok(Declarations::read(source, &rules.leaf_bindings)?
            .laws
            .iter()
            .map(|law| law.id)
            .collect())
    };
    let original = laws_of(project)?;
    let mut source = project.to_owned();
    if !source.ends_with('\n') {
        source.push('\n');
    }
    for line in &lines {
        source.push_str(line);
        source.push('\n');
    }
    let added = laws_of(&source)?;
    let ids: Vec<u32> = added
        .iter()
        .copied()
        .filter(|id| !original.contains(id))
        .collect();
    if ids.len() != lines.len() || added.len() != original.len() + lines.len() {
        return Err(ContractError::new(
            "proposed laws",
            "must each declare one new law",
        ));
    }
    let mut extended = rules.clone();
    for id in &ids {
        extended.law_kinds.insert(*id, PROPOSAL_KIND);
    }
    Ok((Loaded::with(&source, extended)?, ids, lines))
}

/// Each proposal, alone, on the contract's decision of every tuple where it
/// decides: where the proposal applies, and the first tuple where it fails.
fn check_proposals(
    extended: &Loaded,
    ids: &[u32],
    texts: Vec<String>,
    machine: &Machine,
) -> Result<Checked, ContractError> {
    use zeno_fcis_synthesis::finite::v2_laws::Verdict;
    extended.bind(&extended.rules, |authority, framer| {
        let sets: Vec<LawSet<'_>> = ids
            .iter()
            .map(|id| LawSet::new(authority.descriptor(), &[*id]))
            .collect();
        let space = &machine.space;
        let states: Vec<Vec<i128>> = (0..space.states).map(|state| space.state(state)).collect();
        let verdicts = parallel(space.tuples(), |ordinal| {
            let Observed::Decided {
                class,
                reason,
                next,
            } = machine.observed[ordinal]
            else {
                return Vec::new();
            };
            let tuple = space.at(ordinal);
            let proposed = Proposed {
                class,
                reason,
                post: &states[next],
            };
            sets.iter()
                .map(|set| {
                    let (_, diagnostics) = set.on_transition_with_outbox(
                        framer,
                        &tuple,
                        &proposed,
                        machine.outbox_at(ordinal),
                    );
                    diagnostics.first().map(|diagnostic| diagnostic.verdict)
                })
                .collect::<Vec<_>>()
        });
        let mut applies = vec![0; ids.len()];
        let mut false_alarm = None;
        for (ordinal, verdicts) in verdicts.iter().enumerate() {
            for (index, verdict) in verdicts.iter().enumerate() {
                match verdict {
                    Some(Verdict::Satisfied) => applies[index] += 1,
                    Some(Verdict::Skipped) => {}
                    _ => {
                        false_alarm.get_or_insert_with(|| {
                            json!({
                                "law": ids[index],
                                "input": crate::report_numbers::integers(&space.at(ordinal)),
                                "contract": machine.observed[ordinal].json(space),
                            })
                        });
                    }
                }
            }
        }
        Ok(Checked {
            ids: ids.to_vec(),
            texts,
            applies,
            false_alarm,
        })
    })
}
