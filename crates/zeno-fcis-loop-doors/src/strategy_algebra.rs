//! Closed, typed loop plans and bounded trace-prefix checks. LLM-generated.
//! These checks cover composition structure, never program semantics or intent.
use alloc::collections::BTreeSet;
use alloc::vec;
use alloc::vec::Vec;

/// A value available at a declared loop boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(missing_docs)]
pub enum Artifact {
    Original,
    Proposal,
    Qualified,
    Vocabulary,
    Examples,
    Matches,
    Cover,
    Tree,
    Committee,
    Agreement,
    Model,
    Observations,
    Diagnoses,
    Question,
    Readings,
    Answers,
    OwnerGate,
    Answer,
}
impl Artifact {
    // BEGIN public structural knowledge catalog.
    /// Closed artifact inventory, also used by structural knowledge planning.
    pub const ALL: [Self; 18] = [
        Self::Original,
        Self::Proposal,
        Self::Qualified,
        Self::Vocabulary,
        Self::Examples,
        Self::Matches,
        Self::Cover,
        Self::Tree,
        Self::Committee,
        Self::Agreement,
        Self::Model,
        Self::Observations,
        Self::Diagnoses,
        Self::Question,
        Self::Readings,
        Self::Answers,
        Self::OwnerGate,
        Self::Answer,
    ];
    /// Stable knowledge-schema name; existing plan JSON names are unchanged.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Original => "original",
            Self::Proposal => "proposal",
            Self::Qualified => "qualified",
            Self::Vocabulary => "vocabulary",
            Self::Examples => "examples",
            Self::Matches => "matches",
            Self::Cover => "cover",
            Self::Tree => "tree",
            Self::Committee => "committee",
            Self::Agreement => "agreement",
            Self::Model => "model",
            Self::Observations => "observations",
            Self::Diagnoses => "diagnoses",
            Self::Question => "question",
            Self::Readings => "readings",
            Self::Answers => "answers",
            Self::OwnerGate => "owner-gate",
            Self::Answer => "answer",
        }
    }
    // END public structural knowledge catalog.
    fn bit(self) -> u64 {
        1 << (self as u32)
    }
}
/// Fixed built-in operations; callers cannot add executable primitives.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(missing_docs)]
pub enum Step {
    CheckProgram,
    Complete,
    Repair,
    Canonical,
    Resubstitute,
    Sample,
    LearnTerms,
    Cover,
    LearnSplit,
    Assemble,
    CommitteeQuestion,
    SpotCheck,
    Observe,
    Explain,
    ChooseProbe,
    Measure,
    Isolate,
    Formalize,
    Distinct,
    SolveAll,
    ConfirmReading,
    Decide,
    /// Acquire and check the complete capture table; no qualification authority.
    Capture,
}
impl Step {
    // BEGIN public structural knowledge catalog.
    /// Closed step inventory; it supplies no execution callbacks.
    pub const ALL: [Self; 23] = [
        Self::CheckProgram,
        Self::Complete,
        Self::Repair,
        Self::Canonical,
        Self::Resubstitute,
        Self::Sample,
        Self::LearnTerms,
        Self::Cover,
        Self::LearnSplit,
        Self::Assemble,
        Self::CommitteeQuestion,
        Self::SpotCheck,
        Self::Observe,
        Self::Explain,
        Self::ChooseProbe,
        Self::Measure,
        Self::Isolate,
        Self::Formalize,
        Self::Distinct,
        Self::SolveAll,
        Self::ConfirmReading,
        Self::Decide,
        Self::Capture,
    ];
    /// Read-only view of the existing type signature, without semantic authority.
    pub fn types(self) -> (&'static [Artifact], &'static [Artifact]) {
        self.signature()
    }
    // END public structural knowledge catalog.
    /// Stable operation name.
    pub const fn name(self) -> &'static str {
        match self {
            Self::CheckProgram => "check-program",
            Self::Complete => "complete",
            Self::Repair => "repair",
            Self::Canonical => "canonical",
            Self::Resubstitute => "resubstitute",
            Self::Sample => "sample",
            Self::LearnTerms => "learn-terms",
            Self::Cover => "cover",
            Self::LearnSplit => "learn-split",
            Self::Assemble => "assemble",
            Self::CommitteeQuestion => "committee-question",
            Self::SpotCheck => "spot-check",
            Self::Observe => "observe",
            Self::Explain => "explain",
            Self::ChooseProbe => "choose-probe",
            Self::Measure => "measure",
            Self::Isolate => "isolate",
            Self::Formalize => "formalize-alternatives",
            Self::Distinct => "distinct-readings",
            Self::SolveAll => "solve-every-reading",
            Self::ConfirmReading => "confirm-reading",
            Self::Decide => "decide",
            Self::Capture => "capture",
        }
    }
    fn signature(self) -> (&'static [Artifact], &'static [Artifact]) {
        use Artifact::*;
        match self {
            Self::CheckProgram => (&[Original, Proposal], &[Qualified]),
            Self::Complete | Self::Repair | Self::Canonical | Self::Resubstitute => {
                (&[Original, Proposal], &[Proposal])
            }
            Self::Sample => (&[Original], &[Examples]),
            Self::LearnTerms => (&[Examples, Vocabulary], &[Matches]),
            Self::Cover => (&[Matches], &[Cover]),
            Self::LearnSplit => (&[Cover, Examples, Vocabulary], &[Tree]),
            Self::Assemble => (&[Tree], &[Proposal, Committee]),
            Self::CommitteeQuestion => (&[Committee, Original], &[Examples]),
            Self::SpotCheck => (&[Proposal, Original], &[Examples, Agreement]),
            Self::Observe => (&[Model], &[Observations]),
            Self::Explain => (&[Model, Observations], &[Diagnoses]),
            Self::ChooseProbe => (&[Model, Diagnoses], &[Question]),
            Self::Measure => (&[Question, Model], &[Observations]),
            Self::Isolate => (&[Diagnoses], &[Answer]),
            Self::Formalize => (&[Proposal], &[Readings]),
            Self::Distinct => (&[Readings, Model], &[Readings]),
            Self::SolveAll => (&[Readings, Model], &[Answers]),
            Self::ConfirmReading => (&[Answers, Readings, OwnerGate], &[Readings]),
            Self::Decide => (&[Answers], &[Answer]),
            Self::Capture => (&[Original, Proposal], &[Proposal]),
        }
    }
}
/// Owned plan: sequence, tagged alternative, or explicitly bounded feedback.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(missing_docs)]
pub enum Node {
    Step(Step),
    Seq(Vec<Node>),
    Branch {
        on: Artifact,
        cases: Vec<Node>,
    },
    Repeat {
        body: alloc::boxed::Box<Node>,
        until: Artifact,
        bound: u32,
    },
}
/// A declaration owning its input, goal and plan; getters are immutable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Plan {
    needs: Vec<Artifact>,
    goal: Vec<Artifact>,
    node: Node,
}
impl Plan {
    /// Declares a structural plan over fixed operations, without execution.
    pub fn new(needs: Vec<Artifact>, goal: Vec<Artifact>, node: Node) -> Self {
        Self { needs, goal, node }
    }
    /// Declared inputs.
    pub fn needs(&self) -> &[Artifact] {
        &self.needs
    }
    /// Declared goal.
    pub fn goal(&self) -> &[Artifact] {
        &self.goal
    }
    /// Owned plan's immutable view.
    pub fn node(&self) -> &Node {
        &self.node
    }
}
/// Type-flow problems; a blocked step contributes no produced artifacts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypeReport {
    /// Stable structural failure codes.
    pub problems: Vec<&'static str>,
}
/// Bound stop, with charged work retained even on refusal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Stop {
    /// Stable refusal or stop code.
    pub code: &'static str,
    /// Charged structural/proposer work.
    pub work: u64,
    /// True only for resource exhaustion.
    pub inconclusive: bool,
    /// Invoked macro-step prefix retained without partial result data.
    pub trace: Vec<Step>,
}
/// Exclusive invocation scratch; never decides program acceptance.
pub(crate) struct Work {
    used: u64,
    limit: u64,
    trace: Vec<Step>,
}
impl Work {
    pub(crate) fn new(limit: u64) -> Self {
        Self {
            used: 0,
            limit,
            trace: Vec::new(),
        }
    }
    pub(crate) fn trace(&self) -> &[Step] {
        &self.trace
    }
    pub(crate) fn record(&mut self, step: Step) -> Result<(), Stop> {
        if self.trace.len() >= 512 {
            return Err(self.stop("strategy-trace-size", true));
        }
        self.trace.push(step);
        self.charge(1)
    }
    pub(crate) fn used(&self) -> u64 {
        self.used
    }
    pub(crate) fn charge(&mut self, n: usize) -> Result<(), Stop> {
        self.used = self.used.saturating_add(n as u64);
        if self.used > self.limit {
            Err(self.stop("strategy-work-budget", true))
        } else {
            Ok(())
        }
    }
    pub(crate) fn stop(&self, code: &'static str, inconclusive: bool) -> Stop {
        Stop {
            code,
            work: self.used,
            inconclusive,
            trace: self.trace.clone(),
        }
    }
}
fn mask(items: &[Artifact]) -> u64 {
    items.iter().fold(0, |m, a| m | a.bit())
}
fn includes(have: u64, need: u64) -> bool {
    #[cfg(test)]
    if std::env::var_os("ZENO_FCIS_STRATEGY_STUB_TYPES").is_some() {
        return true;
    }
    have & need == need
}
fn same(a: Step, b: Step) -> bool {
    #[cfg(test)]
    if std::env::var_os("ZENO_FCIS_STRATEGY_STUB_TRACE").is_some() {
        return true;
    }
    a == b
}
/// Checks inputs on every plan path and goal reachability on at least one path.
/// A structural pass is neither program equivalence nor a completed run.
pub fn check(plan: &Plan, max_work: u64) -> Result<TypeReport, Stop> {
    fn walk(
        n: &Node,
        have: u64,
        depth: usize,
        w: &mut Work,
        p: &mut Vec<&'static str>,
    ) -> Result<BTreeSet<u64>, Stop> {
        w.charge(1)?;
        if depth > 16 {
            return Err(w.stop("strategy-plan-depth", true));
        }
        let out = match n {
            Node::Step(s) => {
                let (need, out) = s.signature();
                if !includes(have, mask(need)) {
                    p.push("step-input-missing");
                    BTreeSet::from([have])
                } else {
                    BTreeSet::from([have | mask(out)])
                }
            }
            Node::Seq(steps) => {
                let mut states = BTreeSet::from([have]);
                for s in steps {
                    let mut next = BTreeSet::new();
                    for st in states {
                        next.extend(walk(s, st, depth + 1, w, p)?);
                        if next.len() > 2048 {
                            return Err(w.stop("strategy-plan-states", true));
                        }
                    }
                    states = next;
                }
                states
            }
            Node::Branch { on, cases } => {
                if !includes(have, on.bit()) {
                    p.push("branch-input-missing");
                }
                if cases.is_empty() {
                    p.push("empty-branch");
                }
                let mut states = BTreeSet::new();
                for case in cases {
                    states.extend(walk(case, have, depth + 1, w, p)?);
                    if states.len() > 2048 {
                        return Err(w.stop("strategy-plan-states", true));
                    }
                }
                states
            }
            Node::Repeat { body, until, bound } => {
                if *bound == 0 || *bound > 64 {
                    return Err(w.stop("strategy-repeat-bound", false));
                }
                let once = walk(body, have, depth + 1, w, p)?;
                if !once.iter().any(|st| includes(*st, until.bit())) {
                    p.push("repeat-goal-missing");
                }
                once
            }
        };
        Ok(out)
    }
    let mut w = Work::new(max_work);
    let mut problems = Vec::new();
    let ends = walk(&plan.node, mask(&plan.needs), 0, &mut w, &mut problems)?;
    if !ends.iter().any(|st| includes(*st, mask(&plan.goal))) {
        problems.push("goal-unreachable");
    }
    Ok(TypeReport { problems })
}
/// Checks a run's step names as a prefix of a bounded declared plan path.
/// Verdicts/guards remain the operations' responsibility, not this checker.
pub fn conforms(plan: &Plan, trace: &[Step], max_work: u64) -> Result<bool, Stop> {
    fn matches(
        n: &Node,
        i: usize,
        t: &[Step],
        depth: usize,
        w: &mut Work,
    ) -> Result<BTreeSet<usize>, Stop> {
        w.charge(1)?;
        if depth > 16 {
            return Err(w.stop("strategy-plan-depth", true));
        }
        if i == t.len() {
            return Ok(BTreeSet::from([i]));
        }
        let out = match n {
            Node::Step(s) => {
                if same(*s, t[i]) {
                    BTreeSet::from([i + 1])
                } else {
                    BTreeSet::new()
                }
            }
            Node::Seq(steps) => {
                let mut pos = BTreeSet::from([i]);
                for s in steps {
                    let mut next = BTreeSet::new();
                    for p in pos {
                        next.extend(matches(s, p, t, depth + 1, w)?);
                    }
                    pos = next;
                }
                pos
            }
            Node::Branch { cases, .. } => {
                let mut pos = BTreeSet::new();
                for case in cases {
                    pos.extend(matches(case, i, t, depth + 1, w)?);
                }
                pos
            }
            Node::Repeat { body, bound, .. } => {
                if *bound == 0 || *bound > 64 {
                    return Err(w.stop("strategy-repeat-bound", false));
                }
                let mut seen = BTreeSet::from([i]);
                let mut pos = seen.clone();
                for _ in 0..*bound {
                    let mut next = BTreeSet::new();
                    for p in pos {
                        next.extend(matches(body, p, t, depth + 1, w)?);
                    }
                    next.retain(|p| !seen.contains(p));
                    if next.is_empty() {
                        break;
                    }
                    seen.extend(&next);
                    pos = next;
                }
                seen
            }
        };
        Ok(out)
    }
    if trace.len() > 512 {
        return Err(Stop {
            code: "strategy-trace-size",
            work: 0,
            inconclusive: true,
            trace: Vec::new(),
        });
    }
    matches(&plan.node, 0, trace, 0, &mut Work::new(max_work))
        .map(|ends| ends.contains(&trace.len()))
}
/// Pipeline and strategy selection share this same guarded program plan.
pub fn pipeline(stages: &[Step]) -> Plan {
    let mut nodes = vec![Node::Branch {
        on: Artifact::Proposal,
        cases: vec![Node::Step(Step::CheckProgram), Node::Seq(vec![])],
    }];
    for s in stages {
        nodes.push(Node::Branch {
            on: Artifact::Proposal,
            cases: vec![
                Node::Seq(vec![
                    Node::Step(*s),
                    Node::Branch {
                        on: Artifact::Proposal,
                        cases: vec![Node::Step(Step::CheckProgram), Node::Seq(vec![])],
                    },
                ]),
                Node::Seq(vec![]),
            ],
        });
    }
    Plan::new(
        vec![Artifact::Original, Artifact::Proposal],
        vec![Artifact::Qualified],
        Node::Seq(nodes),
    )
}
/// Named supported strategy plan, with finite per-invocation feedback bound.
pub fn named(name: &str, bound: u32) -> Option<Plan> {
    use Artifact::*;
    use alloc::boxed::Box;
    let step = Node::Step;
    Some(match name {
        "split-by-cases" => Plan::new(
            vec![Original, Vocabulary],
            vec![Qualified, Agreement],
            Node::Seq(vec![
                step(Step::Sample),
                Node::Repeat {
                    body: Box::new(Node::Seq(vec![
                        step(Step::LearnTerms),
                        step(Step::Cover),
                        step(Step::LearnSplit),
                        step(Step::Assemble),
                        Node::Branch {
                            on: Committee,
                            cases: vec![step(Step::CommitteeQuestion), step(Step::SpotCheck)],
                        },
                    ])),
                    until: Agreement,
                    bound,
                },
                step(Step::Capture),
                step(Step::CheckProgram),
            ]),
        ),
        "diagnose" => Plan::new(
            vec![Model],
            vec![Answer],
            Node::Seq(vec![
                step(Step::Observe),
                Node::Repeat {
                    body: Box::new(Node::Seq(vec![
                        step(Step::Explain),
                        step(Step::ChooseProbe),
                        step(Step::Measure),
                    ])),
                    until: Question,
                    bound,
                },
                step(Step::Explain),
                step(Step::Isolate),
            ]),
        ),
        "ask-when-it-matters" => Plan::new(
            vec![Proposal, Model, OwnerGate],
            vec![Answer],
            Node::Seq(vec![
                step(Step::Formalize),
                step(Step::Distinct),
                step(Step::SolveAll),
                Node::Repeat {
                    body: Box::new(Node::Seq(vec![
                        step(Step::ConfirmReading),
                        step(Step::SolveAll),
                    ])),
                    until: Answers,
                    bound,
                },
                step(Step::Decide),
            ]),
        ),
        _ => return None,
    })
}
// BEGIN public structural knowledge catalog.
/// Named strategy inventory in deterministic lexical order.
pub const NAMED_STRATEGIES: [&str; 3] = ["ask-when-it-matters", "diagnose", "split-by-cases"];
// END public structural knowledge catalog.
#[cfg(test)]
extern crate std;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strategy_algebra_missing_input_cannot_create_a_goal() {
        let bad = Plan::new(
            vec![],
            vec![Artifact::Qualified],
            Node::Step(Step::CheckProgram),
        );
        let checked = check(&bad, 100).unwrap_or_else(|s| panic!("{s:?}"));
        assert!(checked.problems.contains(&"step-input-missing"));
        assert!(checked.problems.contains(&"goal-unreachable"));
    }
    #[test]
    fn strategy_algebra_trace_cannot_reorder_or_add_a_step() {
        let plan = pipeline(&[Step::Complete]);
        assert!(conforms(&plan, &[Step::Complete, Step::CheckProgram], 100).unwrap_or(false));
        assert!(!conforms(&plan, &[Step::Observe], 100).unwrap_or(true));
        assert!(!conforms(&plan, &[Step::Complete, Step::Complete], 100).unwrap_or(true));
    }
    #[test]
    fn strategy_algebra_named_plans_and_early_prefixes_check() {
        for name in ["split-by-cases", "diagnose", "ask-when-it-matters"] {
            let p = named(name, 8).unwrap_or_else(|| panic!("plan"));
            assert!(
                check(&p, 1000)
                    .unwrap_or_else(|s| panic!("{s:?}"))
                    .problems
                    .is_empty()
            );
            assert!(conforms(&p, &[], 100).unwrap_or(false));
        }
    }
    #[test]
    fn strategy_algebra_bounds_and_owned_views_hold() {
        let p = pipeline(&[Step::Complete]);
        let mut copy = p.clone();
        if let Node::Seq(nodes) = &mut copy.node {
            nodes.clear();
        }
        assert_ne!(p, copy);
        assert!(check(&p, 0).is_err());
        assert!(conforms(&p, &[Step::Complete], 0).is_err());
    }
    #[test]
    fn strategy_algebra_no_candidate_can_continue_without_a_phantom_check() {
        let p = pipeline(&[Step::Canonical, Step::Resubstitute]);
        assert!(
            conforms(
                &p,
                &[
                    Step::CheckProgram,
                    Step::Canonical,
                    Step::Resubstitute,
                    Step::CheckProgram
                ],
                1000
            )
            .unwrap_or(false)
        );
    }

    #[test]
    fn strategy_algebra_branch_cannot_skip_a_required_step() {
        let plan = Plan::new(
            vec![Artifact::Model, Artifact::Original, Artifact::Proposal],
            vec![Artifact::Qualified],
            Node::Seq(vec![
                Node::Branch {
                    on: Artifact::Model,
                    cases: vec![Node::Step(Step::Observe)],
                },
                Node::Step(Step::CheckProgram),
            ]),
        );
        assert!(!conforms(&plan, &[Step::CheckProgram], 1000).unwrap_or(true));
        assert!(conforms(&plan, &[Step::Observe, Step::CheckProgram], 1000).unwrap_or(false));
    }
}
