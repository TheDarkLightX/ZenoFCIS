//! Exhaustive representation refinement of two rules-compiled contracts.
//!
//! [`check`] binds both contracts to their ordinary library Authority. A
//! source-pinned abstraction maps each concrete raw state ordinal to an
//! abstract ordinal or `null`. Every abstract state must have a representative.
//! Both initial states must be admitted and related. For each concrete state
//! and shared command/context input, in state-major order, it checks exact
//! decision class, reason and ordered canonical deliveries, related successors,
//! unchanged rejections, and a designated law refusal on invalid states.
//!
//! Ordinals enumerate declared field domains, last field fastest, including
//! unreachable and law-invalid concrete states. They do not include malformed
//! byte encodings or values outside those domains. State roots are records;
//! input layouts and their scalar domains must match exactly. This is finite
//! model checking, not contextual refinement, publication compatibility,
//! resource equality, or permission to replace a running application.
//!
//! Inputs are borrowed immutable text. All mutable compilation and tally data
//! is exclusively owned by one call. No cache or preparation handle survives
//! the call; results own their data and expose no mutable references.
// SPDX-License-Identifier: MIT OR Apache-2.0
// LLM-generated implementation; checked by the repository's qualification gates.

mod machine;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::ContractError;
use crate::checked_composition::{AuthorityRefusal, Identity, LawRefusal};
use crate::internals::model::Contract;
use crate::internals::policy;
use crate::internals::review::evaluate::{Framer, digest, hex};
use machine::{Kind, Loaded, Space, Work};

/// Abstraction-file version. Unknown and duplicate fields are refused.
pub const SCHEMA: &str = "zeno-fcis/contract-refinement/1";
/// Structured result version.
pub const RESULT_SCHEMA: &str = "zeno-fcis/contract-refinement-result/1";
/// Maximum UTF-8 bytes of each of the five source inputs.
pub const MAX_SOURCE_BYTES: usize = 1 << 20;
/// Maximum states of either contract, including invalid concrete states.
pub const MAX_STATES: usize = 4096;
/// Maximum shared command/context input tuples.
pub const MAX_INPUTS: usize = 64;
/// Maximum concrete state/input comparisons.
pub const MAX_CHECKS: u64 = (MAX_STATES * MAX_INPUTS) as u64;

/// Source texts, never paths or caller-supplied transition tables.
#[derive(Clone, Copy)]
pub struct Sources<'a> {
    /// Abstract project.zeno.
    pub abstract_project: &'a str,
    /// Abstract v2/policy.json.
    pub abstract_policy: &'a str,
    /// Concrete project.zeno.
    pub concrete_project: &'a str,
    /// Concrete v2/policy.json.
    pub concrete_policy: &'a str,
    /// Source pins, abstraction ordinals and designated invalid-state law ID.
    pub abstraction: &'a str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Mapping {
    schema: String,
    abstract_project_sha256: String,
    abstract_policy_sha256: String,
    concrete_project_sha256: String,
    concrete_policy_sha256: String,
    abstraction: Vec<Option<usize>>,
    /// Must be a genuine Law(Violated) diagnostic, not a meter failure.
    invalid_state_law: u32,
}

/// A completed finite comparison, a counterexample, or unfinished checking.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    /// Every required obligation held on the entire declared domain.
    Refines,
    /// An initial/coverage obligation or the least state/input pair failed.
    Counterexample,
    /// Domain, comparison budget, or Authority work could not finish.
    Inconclusive,
}

impl Verdict {
    /// Stable JSON spelling.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Refines => "refines",
            Self::Counterexample => "counterexample",
            Self::Inconclusive => "inconclusive",
        }
    }
}

/// Owned evidence. Only the checker constructs it; it grants no authority.
/// Mutable JSON returned by [`Self::json`] is an independent rendering.
pub struct Report {
    verdict: Verdict,
    reason: Option<&'static str>,
    witness: Option<Value>,
    raw_states: Option<usize>,
    abstract_states: Option<usize>,
    inputs: Option<usize>,
    valid_states: usize,
    checked_rows: u64,
    max_checks: u64,
    work: Work,
    source_hashes: [String; 5],
}

impl Report {
    /// The verdict; an inconclusive result never implies refinement.
    #[must_use]
    pub const fn verdict(&self) -> Verdict {
        self.verdict
    }

    /// Concrete rows whose comparison completed, including a failing row.
    #[must_use]
    pub const fn checked_rows(&self) -> u64 {
        self.checked_rows
    }

    /// Deterministic report with source identities and measured Authority work.
    #[must_use]
    pub fn json(&self) -> Value {
        json!({
            "schema": RESULT_SCHEMA, "authority": "none", "claim": "finite-contract-refinement-only",
            "verdict": self.verdict.name(), "reason": self.reason, "witness": self.witness,
            "raw_states": self.raw_states, "abstract_states": self.abstract_states,
            "valid_states": self.valid_states, "inputs": self.inputs,
            "checked_rows": self.checked_rows, "max_checks": self.max_checks,
            "authority_evaluations": self.work.evaluations,
            "authority_usage": self.work.usage.map(|v| v.to_string()),
            "authority_usage_order": ["read", "write", "candidate", "effect", "byte", "witness-byte", "depth", "step"],
            "sources": {
                "abstract_project": self.source_hashes[0], "abstract_policy": self.source_hashes[1],
                "concrete_project": self.source_hashes[2], "concrete_policy": self.source_hashes[3],
                "abstraction": self.source_hashes[4],
            },
            "checker": Identity::current().json(),
        })
    }

    fn stop(mut self, reason: &'static str) -> Self {
        self.reason = Some(reason);
        self
    }

    fn fail(
        mut self,
        obligation: &'static str,
        state: Option<usize>,
        input: Option<usize>,
    ) -> Self {
        self.verdict = Verdict::Counterexample;
        self.witness = Some(json!({"obligation": obligation, "state": state, "input": input}));
        self
    }
}

/// Check the complete raw state/input product up to `max_checks` rows.
///
/// Work counts compared rows, not decoding/compilation instructions. Structural
/// bounds apply before domain allocation; both genuine genesis evaluations are
/// separate from this counter. Actual Authority usage includes refused work.
/// A row that hits the Authority meter is incomplete and is not credited.
/// There is no sampling or retry. Initial and coverage checks precede rows.
///
/// # Errors
/// Malformed or unpinned sources, unsupported contract syntax, incompatible
/// inputs, malformed abstraction, or ordinary contract binding refusal.
pub fn check(sources: Sources<'_>, max_checks: u64) -> Result<Report, ContractError> {
    check_inner(sources, max_checks, Comparison::Exact)
}

// The disabled arm exists only in unit-test builds. No public API can select it.
#[derive(Clone, Copy)]
enum Comparison {
    Exact,
    #[cfg(test)]
    Disabled,
    #[cfg(test)]
    ValidRelation,
    #[cfg(test)]
    Successor,
    #[cfg(test)]
    Reason,
    #[cfg(test)]
    NullLaw,
    #[cfg(test)]
    SourcePin,
}

impl Comparison {
    fn observations_equal(self, concrete: &machine::Step, abstract_step: &machine::Step) -> bool {
        #[cfg(test)]
        if matches!(self, Self::Disabled | Self::ValidRelation) {
            return true;
        }
        let reason_equal = concrete.reason == abstract_step.reason;
        #[cfg(test)]
        let reason_equal = reason_equal || matches!(self, Self::Reason);
        let _ = self;
        concrete.kind == abstract_step.kind
            && reason_equal
            && concrete.effects == abstract_step.effects
            && concrete.outbox == abstract_step.outbox
    }
    fn source_pin_equal(self, actual: &str, expected: &str) -> bool {
        #[cfg(test)]
        if matches!(self, Self::SourcePin) {
            return true;
        }
        let _ = self;
        actual == expected
    }
    fn successor_equal(self, actual: Option<usize>, expected: Option<usize>) -> bool {
        #[cfg(test)]
        if matches!(self, Self::ValidRelation) {
            return true;
        }
        #[cfg(test)]
        if matches!(self, Self::Successor) {
            return actual.is_some();
        }
        let _ = self;
        actual.is_some() && actual == expected
    }
    fn null_law_equal(self, kind: Kind, law: u32) -> bool {
        #[cfg(test)]
        if matches!(self, Self::NullLaw) {
            return true;
        }
        let _ = self;
        kind == Kind::Refused(AuthorityRefusal::Law {
            law: Some(law),
            refusal: LawRefusal::Violated,
        })
    }
}

#[cfg(test)]
pub(crate) enum TestComparison {
    ValidRelation,
    Successor,
    Reason,
    NullLaw,
    SourcePin,
}
#[cfg(test)]
pub(crate) fn check_with_disabled_for_test(
    sources: Sources<'_>,
    max_checks: u64,
    disabled: TestComparison,
) -> Result<Report, ContractError> {
    let comparison = match disabled {
        TestComparison::ValidRelation => Comparison::ValidRelation,
        TestComparison::Successor => Comparison::Successor,
        TestComparison::Reason => Comparison::Reason,
        TestComparison::NullLaw => Comparison::NullLaw,
        TestComparison::SourcePin => Comparison::SourcePin,
    };
    check_inner(sources, max_checks, comparison)
}

fn check_inner(
    sources: Sources<'_>,
    max_checks: u64,
    comparison: Comparison,
) -> Result<Report, ContractError> {
    let texts = [
        sources.abstract_project,
        sources.abstract_policy,
        sources.concrete_project,
        sources.concrete_policy,
        sources.abstraction,
    ];
    if max_checks > MAX_CHECKS || texts.iter().any(|text| text.len() > MAX_SOURCE_BYTES) {
        return Err(ContractError::new(
            "refinement",
            "source bytes or max_checks exceeds the profile bound",
        ));
    }
    let mapping: Mapping = serde_json::from_str(sources.abstraction)
        .map_err(|e| ContractError::new("abstraction", e.to_string()))?;
    if mapping.schema != SCHEMA || mapping.abstraction.len() > MAX_STATES {
        return Err(ContractError::new(
            "abstraction",
            "unsupported schema or too many states",
        ));
    }
    let source_hashes = texts.map(|text| hex(&digest(text.as_bytes())));
    for (actual, expected) in source_hashes[..4].iter().zip([
        &mapping.abstract_project_sha256,
        &mapping.abstract_policy_sha256,
        &mapping.concrete_project_sha256,
        &mapping.concrete_policy_sha256,
    ]) {
        if !comparison.source_pin_equal(actual, expected) {
            return Err(ContractError::new(
                "abstraction",
                "source SHA-256 differs from its pin",
            ));
        }
    }
    let mut report = Report {
        verdict: Verdict::Inconclusive,
        reason: None,
        witness: None,
        raw_states: None,
        abstract_states: None,
        inputs: None,
        valid_states: mapping.abstraction.iter().flatten().count(),
        checked_rows: 0,
        max_checks,
        work: Work::default(),
        source_hashes,
    };
    let a = Loaded::read(sources.abstract_project, sources.abstract_policy)?;
    let c = Loaded::read(sources.concrete_project, sources.concrete_policy)?;
    if !a.same_inputs(&c) {
        return Err(ContractError::new(
            "refinement",
            "command/context layouts or domains differ",
        ));
    }
    let (Some(a_space), Some(c_space)) = (Space::new(&a), Space::new(&c)) else {
        return Ok(report.stop("domain-over-limit"));
    };
    report.raw_states = Some(c_space.states);
    report.abstract_states = Some(a_space.states);
    report.inputs = Some(c_space.inputs);
    if mapping.abstraction.len() != c_space.states
        || mapping
            .abstraction
            .iter()
            .flatten()
            .any(|s| *s >= a_space.states)
    {
        return Err(ContractError::new(
            "abstraction",
            "mapping length or abstract ordinal is outside the derived domains",
        ));
    }
    if report.valid_states < c_space.states
        && !c
            .declarations
            .laws
            .iter()
            .any(|law| law.id == mapping.invalid_state_law)
    {
        return Err(ContractError::new(
            "abstraction",
            "invalid_state_law is not declared by the concrete contract",
        ));
    }
    let a_contract = Contract::build(&a.declarations, &a.rules, a.commitment)?;
    let c_contract = Contract::build(&c.declarations, &c.rules, c.commitment)?;
    let a_initial = a_space
        .initial(&a_contract, &a.positions)
        .ok_or_else(|| ContractError::new("abstract genesis", "outside the state domain"))?;
    let c_initial = c_space
        .initial(&c_contract, &c.positions)
        .ok_or_else(|| ContractError::new("concrete genesis", "outside the state domain"))?;
    let a_framer = Framer::new(&a.positions, &a_contract);
    let c_framer = Framer::new(&c.positions, &c_contract);
    policy::with_authority(&a_contract, &a.schema, |a_authority| {
        policy::with_authority(&c_contract, &c.schema, |c_authority| {
            for (authority, framer, tuple) in [
                (a_authority, &a_framer, a_space.tuple(a_initial, 0)),
                (c_authority, &c_framer, c_space.tuple(c_initial, 0)),
            ] {
                if let Some(refusal) = machine::genesis(authority, framer, &tuple, &mut report.work)
                {
                    // A refused genesis cannot certify a refinement. Retain all measured work.
                    let probe = machine::Step {
                        kind: Kind::Refused(refusal),
                        successor: None,
                        reason: None,
                        effects: Vec::new(),
                        outbox: Vec::new(),
                        unchanged: true,
                    };
                    return if probe.meter() {
                        report.stop("authority-work-exhausted")
                    } else if probe.unknown() {
                        report.stop("unsupported-authority-outcome")
                    } else {
                        report.fail("initial-admission", Some(c_initial), None)
                    };
                }
            }
            if mapping.abstraction[c_initial] != Some(a_initial) {
                return report.fail("initial-state", Some(c_initial), None);
            }
            let mut covered = vec![false; a_space.states];
            for state in mapping.abstraction.iter().flatten() {
                covered[*state] = true;
            }
            if covered.iter().any(|v| !v) {
                return report.fail("abstract-state-coverage", None, None);
            }
            for state in 0..c_space.states {
                for input in 0..c_space.inputs {
                    if report.checked_rows == max_checks {
                        return report.stop("work-exhausted");
                    }
                    let concrete = machine::step(
                        c_authority,
                        &c_framer,
                        &c_space,
                        state,
                        input,
                        &mut report.work,
                    );
                    if concrete.meter() {
                        return report.stop("authority-work-exhausted");
                    }
                    if concrete.unknown() {
                        return report.stop("unsupported-authority-outcome");
                    }
                    let Some(abstract_state) = mapping.abstraction[state] else {
                        report.checked_rows += 1;
                        if !comparison.null_law_equal(concrete.kind, mapping.invalid_state_law)
                            || concrete.successor != Some(state)
                            || !concrete.unchanged
                        {
                            return report.fail("malformed-state", Some(state), Some(input));
                        }
                        continue;
                    };
                    let abstract_step = machine::step(
                        a_authority,
                        &a_framer,
                        &a_space,
                        abstract_state,
                        input,
                        &mut report.work,
                    );
                    if abstract_step.meter() {
                        return report.stop("authority-work-exhausted");
                    }
                    if abstract_step.unknown() {
                        return report.stop("unsupported-authority-outcome");
                    }
                    report.checked_rows += 1;
                    if !concrete.unchanged || !abstract_step.unchanged {
                        return report.fail("reject-is-no-op", Some(state), Some(input));
                    }
                    let successor = concrete
                        .successor
                        .and_then(|s| mapping.abstraction.get(s).copied().flatten());
                    if !comparison.successor_equal(successor, abstract_step.successor) {
                        return report.fail("successor-refinement", Some(state), Some(input));
                    }
                    if !comparison.observations_equal(&concrete, &abstract_step) {
                        return report.fail("observation-refinement", Some(state), Some(input));
                    }
                }
            }
            report.verdict = Verdict::Refines;
            report
        })
    })?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_native_fault_control_fails_when_observation_comparison_is_stubbed() {
        let abstract_project =
            include_str!("../../../examples/contract-refinement/reservation/abstract/project.zeno");
        let abstract_policy = include_str!(
            "../../../examples/contract-refinement/reservation/abstract/v2/policy.json"
        );
        let concrete_project =
            include_str!("../../../examples/contract-refinement/reservation/concrete/project.zeno");
        let concrete_policy = include_str!(
            "../../../examples/contract-refinement/reservation/concrete/v2/policy.json"
        );
        let abstraction =
            include_str!("../../../examples/contract-refinement/reservation/abstraction.json");
        let mut policy: Value =
            serde_json::from_str(concrete_policy).unwrap_or_else(|e| panic!("{e}"));
        policy["cases"][0]["outbox"][0]["payload"]["140"] = json!("1");
        let concrete_policy = policy.to_string();
        let mut mapping: Value =
            serde_json::from_str(abstraction).unwrap_or_else(|e| panic!("{e}"));
        mapping["concrete_policy_sha256"] = json!(hex(&digest(concrete_policy.as_bytes())));
        let abstraction = mapping.to_string();
        let sources = Sources {
            abstract_project,
            abstract_policy,
            concrete_project,
            concrete_policy: &concrete_policy,
            abstraction: &abstraction,
        };
        let control = |comparison| -> Result<(), &'static str> {
            let report = check_inner(sources, 384, comparison).unwrap_or_else(|e| panic!("{e}"));
            if report.verdict() == Verdict::Counterexample && report.checked_rows() == 1 {
                Ok(())
            } else {
                Err("observation fault escaped")
            }
        };
        assert_eq!(control(Comparison::Exact), Ok(()));
        assert_eq!(
            control(Comparison::Disabled),
            Err("observation fault escaped")
        );
    }
}
