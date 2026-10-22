//! Owned assessment data and pure usefulness decisions. No process or file authority.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) type Scope = BTreeMap<String, BTreeSet<String>>;
pub(crate) const DIMENSIONS: [&str; 6] = [
    "tool",
    "version",
    "family",
    "environment",
    "capability",
    "prerequisite",
];
pub(crate) const STAGES: [&str; 4] = ["complete", "repair", "canonical", "resubstitute"];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Action {
    pub skip: BTreeSet<String>,
    pub order: Vec<String>,
}
impl Action {
    pub(crate) fn check(&self) -> Result<(), &'static str> {
        if self.skip.len() > 4
            || self.order.len() > 4
            || self.order.iter().collect::<BTreeSet<_>>().len() != self.order.len()
            || self
                .skip
                .iter()
                .chain(self.order.iter())
                .any(|s| !STAGES.contains(&s.as_str()))
        {
            return Err("kb-action");
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub(crate) struct EvidenceFamily {
    pub subject: String,
    pub session: String,
    pub actor: String,
    pub tool: String,
    pub environment: String,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum State {
    Proposed,
    Promoted,
    Rejected,
    Pending,
    Refuted,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Candidate {
    pub id: String,
    pub statement_sha256: String,
    pub subject: String,
    #[serde(default)]
    pub claim: String,
    pub scope: Scope,
    pub effect: String,
    pub action: Action,
    pub proposing_tasks: BTreeSet<String>,
    pub families: BTreeSet<EvidenceFamily>,
    pub episodes: BTreeSet<String>,
    pub parents: BTreeSet<String>,
    pub dependencies: BTreeMap<String, String>,
    pub state: State,
    pub assessment: Option<String>,
    pub observed_work: u64,
    pub origin: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Policy {
    pub minimum_effect_bps: u16,
    pub rule: String,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            minimum_effect_bps: 0,
            rule: "held-out-pareto-v1".into(),
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Verdict {
    Accepted,
    Counterexample,
    Unrealizable,
    NotFound,
    Inconclusive,
    Refused,
}
impl Verdict {
    pub(crate) fn solved(self) -> bool {
        self == Self::Accepted
    }
    pub(crate) fn decisive(self) -> bool {
        matches!(
            self,
            Self::Accepted | Self::Counterexample | Self::Unrealizable
        )
    }
    fn rank(self) -> u8 {
        match self {
            Self::Accepted => 3,
            Self::Counterexample | Self::Unrealizable => 2,
            Self::NotFound => 1,
            Self::Inconclusive | Self::Refused => 0,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Run {
    pub task: String,
    pub seed: u64,
    pub budget: u64,
    pub inputs: String,
    pub verdict: Verdict,
    pub work: BTreeMap<String, u64>,
    pub result_cost: Option<[u64; 2]>,
    pub budget_stop: bool,
    pub stopping_bound: Option<String>,
    pub receipt_sha256: String,
    pub output_sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Case {
    pub task: String,
    pub scope: Scope,
    pub applicable: bool,
    pub off: Run,
    pub on: Run,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Decision {
    Promote,
    Reject,
    Inconclusive,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Annotation {
    pub kind: String,
    pub evidence: String,
    pub text: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Assessment {
    pub schema: String,
    pub suite_sha256: String,
    pub revision_sha256: String,
    pub candidates: BTreeSet<String>,
    pub proposing_tasks: BTreeSet<String>,
    pub proposing_subjects: BTreeSet<String>,
    pub total_budget: u64,
    pub justifications: BTreeMap<String, String>,
    pub engine_sha256: String,
    pub checker_sha256: String,
    pub tool_version: String,
    pub policy: Policy,
    pub cases: Vec<Case>,
    pub annotations: Vec<Annotation>,
    pub decision: Decision,
    pub reason: String,
    pub resource_stop: Option<String>,
    pub assessment_work: u64,
    pub reserved_work: u64,
    pub prior_assessment_work: u64,
    pub curation_work: u64,
}
pub(crate) fn digest(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(crate) fn valid_scope(scope: &Scope) -> bool {
    scope.len() <= 6
        && scope.iter().all(|(k, vs)| {
            DIMENSIONS.contains(&k.as_str())
                && !vs.is_empty()
                && vs.len() <= 16
                && (!matches!(k.as_str(), "tool" | "version" | "family" | "environment")
                    || vs.len() == 1)
                && vs
                    .iter()
                    .all(|v| !v.is_empty() && v.len() <= 96 && v != "unknown")
        })
}
pub(crate) fn applies(scope: &Scope, facts: &Scope, isa: &BTreeSet<(String, String)>) -> bool {
    scope.iter().all(|(key, values)| {
        values.iter().all(|wanted| {
            facts.get(key).is_some_and(|actual| {
                actual.iter().any(|got| {
                    got != "unknown"
                        && (wanted == "any"
                            || got == wanted
                            || (matches!(key.as_str(), "tool" | "family")
                                && member(got, wanted, isa)))
                })
            })
        })
    })
}
pub(crate) fn member(child: &str, parent: &str, isa: &BTreeSet<(String, String)>) -> bool {
    let mut seen = BTreeSet::from([child.to_owned()]);
    let mut pending = vec![child.to_owned()];
    while let Some(x) = pending.pop() {
        for (_, p) in isa.iter().filter(|(c, _)| c == &x) {
            if p == parent {
                return true;
            }
            if seen.insert(p.clone()) {
                pending.push(p.clone());
            }
        }
    }
    false
}
fn validate_run(run: &Run, task: &str) -> Result<(), &'static str> {
    let total_work = run
        .work
        .values()
        .try_fold(0u64, |a, b| a.checked_add(*b))
        .ok_or("kb-work-overflow")?;
    if run.task != task
        || !digest(task)
        || !digest(&run.inputs)
        || !digest(&run.output_sha256)
        || !digest(&run.receipt_sha256)
        || run.budget == 0
        || run.work.is_empty()
        || run.work.len() > 16
        || run.work.keys().any(|k| {
            !matches!(
                k.as_str(),
                "gate_pairs_reserved"
                    | "check_requests"
                    | "search_stage_requests"
                    | "control_frames"
                    | "capture_work_reserved"
                    | "capture_cells_reserved"
                    | "synthesis_work_used"
            )
        })
        || (run.verdict.solved() && run.result_cost.is_none())
        || (!run.verdict.solved() && run.result_cost.is_some())
        || total_work > run.budget && !run.budget_stop
    {
        return Err("kb-run-binding");
    }
    Ok(())
}
/// Componentwise comparison. No score, elapsed time, opinion or observational rate enters.
pub(crate) fn decide(
    cases: &[Case],
    proposing: &BTreeSet<String>,
    policy: &Policy,
    annotations: &[Annotation],
) -> Result<(Decision, &'static str), &'static str> {
    if policy.rule != "held-out-pareto-v1"
        || policy.minimum_effect_bps > 10_000
        || cases.len() > 64
        || annotations.len() > 64
    {
        return Err("kb-assessment-policy");
    }
    let mut ids = BTreeSet::new();
    let (mut applicable, mut improvement, mut stopped, mut regression) =
        (0usize, false, false, false);
    for c in cases {
        if !ids.insert(&c.task) || proposing.contains(&c.task) {
            return Err("kb-held-out-split");
        }
        validate_run(&c.off, &c.task)?;
        validate_run(&c.on, &c.task)?;
        if c.off.seed != c.on.seed
            || c.off.budget != c.on.budget
            || c.off.inputs != c.on.inputs
            || c.off.work.keys().ne(c.on.work.keys())
        {
            return Err("kb-pair-binding");
        }
        if !c.applicable {
            if c.off != c.on {
                return Err("kb-inapplicable-change");
            }
            continue;
        }
        applicable += 1;
        stopped |= c.on.budget_stop || c.off.budget_stop;
        regression |= c.on.verdict.rank() < c.off.verdict.rank()
            || (c.on.verdict.rank() == c.off.verdict.rank() && c.on.verdict != c.off.verdict)
            || c.on.work.iter().any(|(k, v)| v > &c.off.work[k])
            || c.on
                .result_cost
                .zip(c.off.result_cost)
                .is_some_and(|(on, off)| on.iter().zip(off).any(|(a, b)| *a > b));
        let enough = |on: u64, off: u64| {
            on < off
                && u128::from(off - on) * 10_000
                    >= u128::from(off) * u128::from(policy.minimum_effect_bps)
        };
        improvement |= c.on.verdict.rank() > c.off.verdict.rank()
            || c.on.work.iter().any(|(k, v)| enough(*v, c.off.work[k]))
            || c.on
                .result_cost
                .zip(c.off.result_cost)
                .is_some_and(|(on, off)| on.iter().zip(off).any(|(a, b)| enough(*a, b)));
    }
    for a in annotations {
        if !matches!(a.kind.as_str(), "veto" | "narrow" | "note")
            || a.evidence.is_empty()
            || a.text.len() > 4096
        {
            return Err("kb-review-annotation");
        }
    }
    if annotations
        .iter()
        .any(|a| matches!(a.kind.as_str(), "veto" | "narrow"))
    {
        return Ok((Decision::Reject, "review-veto-or-narrow"));
    }
    if regression {
        return Ok((Decision::Reject, "pareto-regression"));
    }
    if stopped {
        return Ok((Decision::Inconclusive, "budget-stop"));
    }
    if applicable == 0 {
        return Ok((Decision::Inconclusive, "no-applicable-task"));
    }
    if improvement {
        Ok((Decision::Promote, "strict-pareto-improvement"))
    } else {
        Ok((Decision::Inconclusive, "no-improvement"))
    }
}

/// Retraction is a pure least fixed point over justification edges.
pub(crate) fn retract(candidates: &[Candidate], roots: &BTreeSet<String>) -> Vec<Candidate> {
    let mut pending = roots.clone();
    loop {
        let before = pending.len();
        for c in candidates {
            if !c.parents.is_disjoint(&pending) {
                pending.insert(c.statement_sha256.clone());
            }
        }
        if pending.len() == before {
            break;
        }
    }
    candidates
        .iter()
        .map(|c| {
            let mut next = c.clone();
            if pending.contains(&c.statement_sha256) && c.state != State::Refuted {
                next.state = State::Pending;
            }
            next
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(work: u64) -> Run {
        Run {
            task: "a".repeat(64),
            seed: 1,
            budget: 100,
            inputs: "b".repeat(64),
            verdict: Verdict::Accepted,
            work: BTreeMap::from([("check_requests".into(), work)]),
            result_cost: Some([2, 30]),
            budget_stop: false,
            stopping_bound: None,
            receipt_sha256: "c".repeat(64),
            output_sha256: "d".repeat(64),
        }
    }
    fn case(off: u64, on: u64) -> Case {
        Case {
            task: "a".repeat(64),
            scope: Scope::new(),
            applicable: true,
            off: run(off),
            on: run(on),
        }
    }
    #[test]
    fn pareto_matches_finite_product_order() {
        let mut cases = Vec::new();
        for off_solved in [false, true] {
            for on_solved in [false, true] {
                for ow in [2u64, 3] {
                    for nw in [2u64, 3] {
                        for oc in [1u64, 2] {
                            for nc in [1u64, 2] {
                                let mut c = case(ow, nw);
                                c.off.verdict = if off_solved {
                                    Verdict::Accepted
                                } else {
                                    Verdict::NotFound
                                };
                                c.on.verdict = if on_solved {
                                    Verdict::Accepted
                                } else {
                                    Verdict::NotFound
                                };
                                c.off.result_cost = off_solved.then_some([oc, 30]);
                                c.on.result_cost = on_solved.then_some([nc, 30]);
                                let off = [
                                    i128::from(off_solved),
                                    -i128::from(ow),
                                    if off_solved && on_solved {
                                        -i128::from(oc)
                                    } else {
                                        0
                                    },
                                ];
                                let on = [
                                    i128::from(on_solved),
                                    -i128::from(nw),
                                    if off_solved && on_solved {
                                        -i128::from(nc)
                                    } else {
                                        0
                                    },
                                ];
                                let comparisons = on
                                    .iter()
                                    .zip(off)
                                    .map(|(n, o)| n.cmp(&o))
                                    .collect::<Vec<_>>();
                                cases.push((c, comparisons));
                            }
                        }
                    }
                }
            }
        }
        for (a, ac) in &cases {
            for (b, bc) in &cases {
                let mut b = b.clone();
                b.task = "b".repeat(64);
                b.off.task = b.task.clone();
                b.on.task = b.task.clone();
                let orders = ac.iter().chain(bc);
                let expected = if orders.clone().any(|x| *x == core::cmp::Ordering::Less) {
                    Decision::Reject
                } else if orders.clone().any(|x| *x == core::cmp::Ordering::Greater) {
                    Decision::Promote
                } else {
                    Decision::Inconclusive
                };
                assert_eq!(
                    decide(&[a.clone(), b], &BTreeSet::new(), &Policy::default(), &[])
                        .unwrap()
                        .0,
                    expected
                );
            }
        }
    }
    #[test]
    fn one_worse_task_cannot_be_hidden() {
        let a = case(10, 5);
        let mut b = case(3, 4);
        b.task = "b".repeat(64);
        b.off.task = b.task.clone();
        b.on.task = b.task.clone();
        assert_eq!(
            decide(&[a, b], &BTreeSet::new(), &Policy::default(), &[])
                .unwrap()
                .0,
            Decision::Reject
        );
    }
    #[test]
    fn wall_time_is_not_a_work_counter() {
        let mut c = case(10, 9);
        c.on.work.insert("wall_time".into(), 1);
        c.off.work.insert("wall_time".into(), 100);
        assert!(decide(&[c], &BTreeSet::new(), &Policy::default(), &[]).is_err());
    }
    #[test]
    fn points_are_not_policy() {
        assert!(
            serde_json::from_str::<Policy>(
                r#"{"minimum_effect_bps":0,"rule":"held-out-pareto-v1","points":100}"#
            )
            .is_err()
        );
    }
    #[test]
    fn pareto_rejects_one_worse_measure() {
        let mut c = case(10, 9);
        c.on.result_cost = Some([3, 30]);
        assert_eq!(
            decide(&[c], &BTreeSet::new(), &Policy::default(), &[])
                .unwrap()
                .0,
            Decision::Reject
        );
    }
    #[test]
    fn no_applicable_task_never_promotes() {
        assert_eq!(
            decide(&[], &BTreeSet::new(), &Policy::default(), &[])
                .unwrap()
                .0,
            Decision::Inconclusive
        );
    }
    #[test]
    fn work_counters_determine_usefulness() {
        assert_eq!(
            decide(&[case(10, 9)], &BTreeSet::new(), &Policy::default(), &[])
                .unwrap()
                .0,
            Decision::Promote
        );
        assert_eq!(
            decide(&[case(9, 10)], &BTreeSet::new(), &Policy::default(), &[])
                .unwrap()
                .0,
            Decision::Reject
        );
    }
    #[test]
    fn proposing_task_is_never_held_out() {
        assert_eq!(
            decide(
                &[case(10, 9)],
                &BTreeSet::from(["a".repeat(64)]),
                &Policy::default(),
                &[]
            ),
            Err("kb-held-out-split")
        );
    }
    #[test]
    fn budget_stop_never_passes() {
        let mut c = case(10, 9);
        c.on.budget_stop = true;
        assert_eq!(
            decide(&[c], &BTreeSet::new(), &Policy::default(), &[])
                .unwrap()
                .0,
            Decision::Inconclusive
        );
    }
    #[test]
    fn reviewer_annotation_cannot_promote() {
        assert_eq!(
            decide(
                &[case(10, 10)],
                &BTreeSet::new(),
                &Policy::default(),
                &[Annotation {
                    kind: "note".into(),
                    evidence: "episode".into(),
                    text: "approved".into()
                }]
            )
            .unwrap()
            .0,
            Decision::Inconclusive
        );
    }
    #[test]
    fn dependent_retraction_is_minimal() {
        let c = Candidate {
            id: "a".into(),
            statement_sha256: "a".repeat(64),
            subject: "pipeline".into(),
            claim: String::new(),
            scope: Scope::new(),
            effect: "helps".into(),
            action: Action {
                skip: BTreeSet::new(),
                order: vec![],
            },
            proposing_tasks: BTreeSet::new(),
            families: BTreeSet::new(),
            episodes: BTreeSet::new(),
            parents: BTreeSet::new(),
            dependencies: BTreeMap::new(),
            state: State::Promoted,
            assessment: None,
            observed_work: 0,
            origin: "test".into(),
        };
        let mut b = c.clone();
        b.statement_sha256 = "b".repeat(64);
        b.parents.insert(c.statement_sha256.clone());
        let mut d = c.clone();
        d.statement_sha256 = "d".repeat(64);
        let out = retract(&[c.clone(), b, d], &BTreeSet::from([c.statement_sha256]));
        assert_eq!(
            out.iter().map(|x| x.state).collect::<Vec<_>>(),
            vec![State::Pending, State::Pending, State::Promoted]
        );
    }
    #[test]
    fn exact_scope_and_unknowns() {
        let s = Scope::from([("family".into(), BTreeSet::from(["counter".into()]))]);
        assert!(!applies(&s, &Scope::new(), &BTreeSet::new()));
        assert!(!applies(
            &s,
            &Scope::from([("family".into(), BTreeSet::from(["unknown".into()]))]),
            &BTreeSet::new()
        ));
        assert!(applies(
            &s,
            &Scope::from([("family".into(), BTreeSet::from(["bounded-counter".into()]))]),
            &BTreeSet::from([("bounded-counter".into(), "counter".into())])
        ));
    }
}
