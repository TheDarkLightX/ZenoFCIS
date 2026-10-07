//! Private correspondence controls against the byte-preserved original evaluator.
use super::*;
use crate::model_diagnostics::*;
use crate::original_logic as old;

struct NoPred;
impl old::PredicateProvider for NoPred {
    fn evaluate(&self, _: &Identifier, _: &[i128]) -> Option<bool> {
        None
    }
}
fn id(value: u32) -> StableId {
    StableId::new(value).unwrap_or_else(|| unreachable!())
}
fn name(value: &str) -> Identifier {
    Identifier::try_new(value).unwrap_or_else(|| unreachable!())
}
fn path(value: u32) -> ProjectionPath {
    ProjectionPath::try_new(ProjectionRoot::Pre, vec![id(value)]).unwrap_or_else(|| unreachable!())
}
fn closed_step(values: &[(u32, i128)]) -> ModelStep {
    ModelStep::try_new(
        values
            .iter()
            .map(|(k, v)| ModelObservation::new(path(*k), *v))
            .collect(),
    )
    .unwrap_or_else(|| unreachable!())
}
fn original_step(values: &[(u32, i128)]) -> old::TraceStep {
    old::TraceStep::try_new(
        values
            .iter()
            .map(|(k, v)| old::Observation::new(path(*k), *v))
            .collect(),
    )
    .unwrap_or_else(|| unreachable!())
}
fn reason(value: old::IndeterminateReason) -> ModelReason {
    match value {
        old::IndeterminateReason::MissingProjection => ModelReason::MissingProjection,
        old::IndeterminateReason::MissingPredicate => ModelReason::MissingPredicate,
        old::IndeterminateReason::Overflow => ModelReason::Overflow,
        old::IndeterminateReason::DivisionByZero => ModelReason::DivisionByZero,
        old::IndeterminateReason::NonExactDivision => ModelReason::NonExactDivision,
        old::IndeterminateReason::InvalidRange => ModelReason::InvalidRange,
        old::IndeterminateReason::OperationLimit => ModelReason::OperationLimit,
        old::IndeterminateReason::QuantifierLimit => ModelReason::QuantifierLimit,
        old::IndeterminateReason::PredicateLimit => ModelReason::PredicateLimit,
        old::IndeterminateReason::EmptyTrace => ModelReason::EmptyTrace,
        old::IndeterminateReason::HorizonExceeded => ModelReason::HorizonExceeded,
        old::IndeterminateReason::ResourceLimit => ModelReason::ResourceLimit,
    }
}
fn relation(
    formula: &RelExpr,
    values: &[(u32, i128)],
    limits: (u64, u64, u64),
) -> DiagnosticOutcome {
    let new_limits =
        DiagnosticLimits::try_new(limits.0, limits.1, limits.2).unwrap_or_else(|| unreachable!());
    let old_limits =
        old::EvalLimits::try_new(limits.0, limits.1, limits.2).unwrap_or_else(|| unreachable!());
    let actual = check_relation(formula, ModelContext::new(&closed_step(values), new_limits));
    let reference = old::evaluate_relational(
        formula,
        old::EvaluationContext::new(&original_step(values), &NoPred, old_limits),
    );
    let expected = match reference {
        old::EvalOutcome::True => DiagnosticOutcome::True,
        old::EvalOutcome::False => DiagnosticOutcome::False,
        old::EvalOutcome::Indeterminate(r) => DiagnosticOutcome::Indeterminate(reason(r)),
    };
    assert_eq!(
        actual, expected,
        "{formula:?}, values={values:?}, limits={limits:?}"
    );
    actual
}
fn cmp(value: ValueExpr) -> RelExpr {
    RelExpr::Compare(CompareOp::Eq, value, ValueExpr::Int(0))
}
const LIMITS: (u64, u64, u64) = (1_000_000, 100_000, 100_000);

#[test]
fn full_width_arithmetic_and_eager_error_priority_match_original() {
    let values = [
        i128::MIN,
        i128::MIN + 1,
        -7,
        -2,
        -1,
        0,
        1,
        2,
        7,
        i128::MAX - 1,
        i128::MAX,
    ];
    let mut cases = 0usize;
    for left in values {
        for right in values {
            let a = Box::new(ValueExpr::Int(left));
            let b = Box::new(ValueExpr::Int(right));
            for value in [
                ValueExpr::Add(a.clone(), b.clone()),
                ValueExpr::Sub(a.clone(), b.clone()),
                ValueExpr::Mul(a.clone(), b.clone()),
                ValueExpr::Div(DivisionMode::Exact, a.clone(), b.clone()),
                ValueExpr::Div(DivisionMode::Floor, a.clone(), b.clone()),
                ValueExpr::Div(DivisionMode::Ceil, a, b),
            ] {
                relation(&cmp(value), &[], LIMITS);
                cases += 1;
            }
            for op in [
                CompareOp::Eq,
                CompareOp::NotEq,
                CompareOp::Less,
                CompareOp::LessEq,
                CompareOp::Greater,
                CompareOp::GreaterEq,
            ] {
                relation(
                    &RelExpr::Compare(op, ValueExpr::Int(left), ValueExpr::Int(right)),
                    &[],
                    LIMITS,
                );
                cases += 1;
            }
        }
    }
    let errors = [
        cmp(ValueExpr::Add(
            Box::new(ValueExpr::Int(i128::MAX)),
            Box::new(ValueExpr::Int(1)),
        )),
        cmp(ValueExpr::Div(
            DivisionMode::Exact,
            Box::new(ValueExpr::Int(1)),
            Box::new(ValueExpr::Int(0)),
        )),
        cmp(ValueExpr::Div(
            DivisionMode::Exact,
            Box::new(ValueExpr::Int(1)),
            Box::new(ValueExpr::Int(2)),
        )),
        cmp(ValueExpr::Projection(path(99))),
    ];
    for error in &errors {
        for formula in [
            RelExpr::And(Box::new(RelExpr::Bool(false)), Box::new(error.clone())),
            RelExpr::Or(Box::new(RelExpr::Bool(true)), Box::new(error.clone())),
            RelExpr::Implies(Box::new(RelExpr::Bool(false)), Box::new(error.clone())),
        ] {
            assert!(matches!(
                relation(&formula, &[], LIMITS),
                DiagnosticOutcome::Indeterminate(_)
            ));
            cases += 1;
        }
        for second in &errors {
            relation(
                &RelExpr::And(Box::new(error.clone()), Box::new(second.clone())),
                &[],
                LIMITS,
            );
            cases += 1;
        }
    }
    assert_eq!(cases, 1480);
    println!("original arithmetic/eager correspondence cases={cases}");
}

#[test]
fn ranges_shadowing_sums_limits_and_shape_match_original() {
    let variable = name("i");
    let predicate = RelExpr::Predicate {
        name: name("absent"),
        arguments: vec![ValueExpr::Div(
            DivisionMode::Exact,
            Box::new(ValueExpr::Int(1)),
            Box::new(ValueExpr::Int(0)),
        )]
        .into_boxed_slice(),
    };
    let mut formulas = vec![
        RelExpr::Bool(true),
        cmp(ValueExpr::Var(variable.clone())),
        predicate.clone(),
        RelExpr::And(Box::new(predicate.clone()), Box::new(predicate)),
    ];
    for (start, end) in [
        (i128::MIN, i128::MIN),
        (i128::MAX, i128::MAX),
        (-2, 3),
        (2, 1),
        (0, 1),
        (0, 4),
    ] {
        for body in [
            RelExpr::Bool(false),
            RelExpr::Bool(true),
            RelExpr::Compare(
                CompareOp::Eq,
                ValueExpr::Var(variable.clone()),
                ValueExpr::Int(0),
            ),
        ] {
            formulas.push(RelExpr::ForAll {
                variable: variable.clone(),
                start,
                end,
                body: Box::new(body.clone()),
            });
            formulas.push(RelExpr::Exists {
                variable: variable.clone(),
                start,
                end,
                body: Box::new(body),
            });
        }
        for body in [
            ValueExpr::Int(i128::MAX),
            ValueExpr::Var(variable.clone()),
            ValueExpr::Sum {
                variable: variable.clone(),
                start: 0,
                end: 2,
                body: Box::new(ValueExpr::Var(variable.clone())),
            },
        ] {
            formulas.push(cmp(ValueExpr::Sum {
                variable: variable.clone(),
                start,
                end,
                body: Box::new(body),
            }));
        }
    }
    formulas.push(RelExpr::ForAll {
        variable: variable.clone(),
        start: 0,
        end: 2,
        body: Box::new(RelExpr::And(
            Box::new(RelExpr::Exists {
                variable: variable.clone(),
                start: 3,
                end: 4,
                body: Box::new(RelExpr::Compare(
                    CompareOp::Eq,
                    ValueExpr::Var(variable.clone()),
                    ValueExpr::Int(3),
                )),
            }),
            Box::new(RelExpr::Compare(
                CompareOp::Less,
                ValueExpr::Var(variable),
                ValueExpr::Int(2),
            )),
        )),
    });
    let mut cases = 0usize;
    for formula in &formulas {
        for operations in [1, 2, 3, 10, 100] {
            for iterations in [1, 2, 3, 9] {
                for predicates in [1, 2] {
                    relation(formula, &[], (operations, iterations, predicates));
                    cases += 1;
                }
            }
        }
    }
    for depth in [255, 256, 257] {
        let mut formula = RelExpr::Bool(true);
        for _ in 1..depth {
            formula = RelExpr::Not(Box::new(formula));
        }
        relation(&formula, &[], LIMITS);
        cases += 1;
    }
    let step = closed_step(&[(2, 7), (1, -1)]);
    assert_eq!(
        step.observations()
            .iter()
            .map(|v| (v.path().clone(), v.value()))
            .collect::<Vec<_>>(),
        vec![(path(1), -1), (path(2), 7)]
    );
    assert!(
        ModelStep::try_new(vec![
            ModelObservation::new(path(1), 0),
            ModelObservation::new(path(1), 1)
        ])
        .is_none()
    );
    assert_eq!(
        relation(&RelExpr::Bool(true), &[], LIMITS).determinate(),
        Some(true)
    );
    assert_eq!(
        relation(&cmp(ValueExpr::Projection(path(9))), &[], LIMITS).determinate(),
        None
    );
    assert!(DiagnosticLimits::try_new(0, 1, 1).is_none());
    assert!(DiagnosticLimits::try_new(1, 0, 1).is_none());
    assert!(DiagnosticLimits::try_new(1, 1, 0).is_none());
    println!("original range/limit/shape correspondence cases={cases}");
}

#[test]
fn all_finite_temporal_operators_horizons_and_first_failure_match_original() {
    let atom = TemporalFormula::Atom(RelExpr::Compare(
        CompareOp::Greater,
        ValueExpr::Projection(path(1)),
        ValueExpr::Int(0),
    ));
    let trap = TemporalFormula::Atom(cmp(ValueExpr::Div(
        DivisionMode::Floor,
        Box::new(ValueExpr::Int(1)),
        Box::new(ValueExpr::Int(0)),
    )));
    let forms = vec![
        atom.clone(),
        TemporalFormula::Not(Box::new(atom.clone())),
        TemporalFormula::Next(Box::new(atom.clone())),
        TemporalFormula::Always(Box::new(atom.clone())),
        TemporalFormula::Eventually(Box::new(atom.clone())),
        TemporalFormula::Until(
            Box::new(atom.clone()),
            Box::new(TemporalFormula::Not(Box::new(atom.clone()))),
        ),
        TemporalFormula::And(Box::new(atom.clone()), Box::new(trap.clone())),
        TemporalFormula::Or(Box::new(atom.clone()), Box::new(trap)),
        TemporalFormula::Always(Box::new(TemporalFormula::Eventually(Box::new(atom)))),
    ];
    let modes = [
        ClaimMode::Relational,
        ClaimMode::Inductive,
        ClaimMode::UnboundedProof,
        ClaimMode::Finite { horizon: 0 },
        ClaimMode::Finite { horizon: 1 },
        ClaimMode::Finite { horizon: 2 },
        ClaimMode::Finite { horizon: 3 },
    ];
    let mut cases = 0usize;
    for length in 0..=3 {
        for code in 0..3usize.pow(length) {
            let mut code = code;
            let values: Vec<_> = (0..length)
                .map(|_| {
                    let v = (code % 3) as i128 - 1;
                    code /= 3;
                    v
                })
                .collect();
            let new: Vec<_> = values.iter().map(|v| closed_step(&[(1, *v)])).collect();
            let old: Vec<_> = values.iter().map(|v| original_step(&[(1, *v)])).collect();
            for mode in modes {
                for formula in &forms {
                    for bound in [1, 4, 40, 1000] {
                        let actual = check_trace(
                            formula,
                            mode,
                            &new,
                            DiagnosticLimits::try_new(bound, 20, 20)
                                .unwrap_or_else(|| unreachable!()),
                        );
                        let expected = match old::evaluate_temporal(
                            formula,
                            mode,
                            &old,
                            &NoPred,
                            old::EvalLimits::try_new(bound, 20, 20)
                                .unwrap_or_else(|| unreachable!()),
                        ) {
                            old::TemporalEvaluation::Satisfied => TemporalDiagnosis::Satisfied,
                            old::TemporalEvaluation::Counterexample { step } => {
                                TemporalDiagnosis::Counterexample { step }
                            }
                            old::TemporalEvaluation::Indeterminate(r) => {
                                TemporalDiagnosis::Indeterminate(reason(r))
                            }
                            old::TemporalEvaluation::ProofObligation => {
                                TemporalDiagnosis::ProofObligation
                            }
                        };
                        assert_eq!(
                            actual, expected,
                            "{formula:?}, values={values:?}, mode={mode:?},bound={bound}"
                        );
                        cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cases, 10080);
    println!("original temporal correspondence cases={cases}");
}

fn obligation(formula: RelExpr) -> ExportedObligation {
    let claim = ClaimDecl::new(
        id(991),
        name("comparison"),
        vec![BackendId::Z3],
        ClaimMode::Relational,
        ClaimFormula::Relational(formula),
    );
    let actual = export_smt(&claim, ToolBackend::Z3).unwrap_or_else(|e| panic!("{e:?}"));
    let original =
        original_export_smt_with_limits(&claim, ToolBackend::Z3, ExportLimits::default())
            .unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(actual, original);
    actual
}
fn model(values: &[(&str, i128)]) -> String {
    let entries = values
        .iter()
        .map(|(name, value)| format!("(define-fun {name} () Int {})\n", smt_int(*value)))
        .collect::<String>();
    format!("sat\n(model\n{entries})\n")
}
fn status_pair(obligation: &ExportedObligation, text: &str) -> ToolRunStatus {
    let actual = replay_model(obligation, text);
    let expected = reference::replay(obligation, text);
    assert_eq!(actual, expected, "{text}");
    // Exercise actual normal classification as well as its replay helper.
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        let output = ProcessOutput {
            status: ExitStatus::from_raw(0),
            stdout: text.as_bytes().to_vec(),
            stderr: Vec::new(),
        };
        let config = ToolConfig {
            backend: obligation.backend(),
            path: PathBuf::new(),
            version: Z3_VERSION.to_owned(),
            sha256: "0".repeat(64),
            runtime: None,
            timeout_ms: 10,
            max_output_bytes: 4096,
            allowed_axioms: Vec::new(),
        };
        assert_eq!(
            classify(&config, &output, obligation),
            reference::classification(&config, &output, obligation)
        );
    }
    actual
}
#[test]
fn actual_replay_and_classification_preserve_refuted_undefined_and_blocked() {
    let projected = ValueExpr::Projection(path(100));
    let increment = obligation(RelExpr::Compare(
        CompareOp::Greater,
        ValueExpr::Add(Box::new(projected.clone()), Box::new(ValueExpr::Int(1))),
        projected,
    ));
    for value in [i128::MIN, -1, 0, 1, i128::MAX] {
        status_pair(&increment, &model(&[("pre_100_t0", value)]));
    }
    status_pair(&increment, "sat\n(model)\n");
    let trap = cmp(ValueExpr::Div(
        DivisionMode::Exact,
        Box::new(ValueExpr::Int(1)),
        Box::new(ValueExpr::Int(0)),
    ));
    let with_pred = obligation(RelExpr::And(
        Box::new(trap.clone()),
        Box::new(RelExpr::Predicate {
            name: name("external"),
            arguments: Vec::new().into_boxed_slice(),
        }),
    ));
    assert_eq!(
        status_pair(&with_pred, "sat\n(model)\n"),
        ToolRunStatus::Blocked(ToolFailure::ModelReplayFailed)
    );
    assert_eq!(
        status_pair(&obligation(trap), "sat\n(model)\n"),
        ToolRunStatus::Undefined(UndefinedReason::DivisionByZero)
    );
    assert_eq!(
        status_pair(&obligation(RelExpr::Bool(false)), "sat\n(model)\n"),
        ToolRunStatus::Refuted
    );
    assert_eq!(
        status_pair(&obligation(RelExpr::Bool(true)), "sat\n(model)\n"),
        ToolRunStatus::Blocked(ToolFailure::ModelReplayFailed)
    );
    let finite = export_smt(
        &ClaimDecl::new(
            id(992),
            name("finite"),
            vec![BackendId::Z3],
            ClaimMode::Finite { horizon: 3 },
            ClaimFormula::Temporal(TemporalFormula::Next(Box::new(TemporalFormula::Atom(
                RelExpr::Bool(true),
            )))),
        ),
        ToolBackend::Z3,
    )
    .unwrap_or_else(|e| panic!("{e:?}"));
    for value in [-1, 0, 1, 2, 3, 4, i128::MAX] {
        status_pair(&finite, &model(&[("zeno_trace_len", value)]));
    }
    status_pair(&finite, "sat\n(model)\n");
}
#[test]
fn actual_inductive_replay_preserves_all_law_and_domain_assumptions() {
    let source = "zeno 1; project 1 counter; type 100 state State; type 101 command Command; type 102 context Context; type 105 int Count in 0..=9; field 110 100 count 105; field 120 101 amount 105; reason 200 bad precedence 0; component 300 machine { owns 100; reads pre.100; writes post.100; budget steps 10; } merge [300]; law 400 bound = command.101.120 <= 2; law 401 steps = post.100.110 == pre.100.110 + command.101.120; law 402 reset = post.100.110 == 0; claim 600 invariant all inductive assume [400] accept [401] failure [402] = pre.100.110 <= 3; claim 601 missing_failure all inductive assume [400] accept [401] = pre.100.110 <= 3;";
    let parsed = spec_authoring::parse_project(source, spec_authoring::SourceLimits::default())
        .unwrap_or_else(|e| panic!("{e}"));
    let spec = spec_authoring::elaborate_project(parsed, spec_authoring::ProjectLimits::default())
        .unwrap_or_else(|e| panic!("{e}"));
    let mut cases = 0usize;
    let mut observed = BTreeSet::new();
    for claim in spec.claims() {
        let bound =
            export_inductive_smt(claim, &spec, ToolBackend::Z3).unwrap_or_else(|e| panic!("{e:?}"));
        let original = original_export_inductive_smt_with_limits(
            claim,
            &spec,
            ToolBackend::Z3,
            ExportLimits::default(),
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(bound, original);
        for pre in [-1, 0, 3, 4, 9, 10, i128::MAX] {
            for amount in [-1, 0, 1, 2, 3] {
                for post in [-1, 0, 3, 4, 9, 10, i128::MAX] {
                    for kind in [0, 1, 2, 3] {
                        let text = model(&[
                            ("pre_100_110_t0", pre),
                            ("command_101_120_t0", amount),
                            ("post_100_110_t0", post),
                            (DECISION_KIND, kind),
                        ]);
                        let status = status_pair(&bound, &text);
                        observed.insert(run_status_name(&status));
                        cases += 1;
                    }
                }
            }
        }
        status_pair(&bound, "sat\n(model)\n");
    }
    assert!(observed.contains("refuted"));
    assert!(observed.contains("blocked"));
    assert_eq!(cases, 1960);
    println!("original actual induction replay/classification cases={cases}");
}

mod reference {
    use super::*;
    use crate::original_logic::{
        EvalLimits, EvalOutcome, EvaluationContext, IndeterminateReason, Observation,
        PredicateProvider, TemporalEvaluation, TraceStep, evaluate_relational, evaluate_temporal,
    };
    struct MissingPredicates;
    impl PredicateProvider for MissingPredicates {
        fn evaluate(&self, _: &Identifier, _: &[i128]) -> Option<bool> {
            None
        }
    }
    struct UndefinedReason;
    impl UndefinedReason {
        const fn from_indeterminate(reason: IndeterminateReason) -> Option<crate::UndefinedReason> {
            crate::UndefinedReason::from_original_indeterminate(reason)
        }
    }
    fn replay_model(obligation: &ExportedObligation, text: &str) -> ToolRunStatus {
        let assignments = parse_model_values(text);
        if matches!(obligation.claim.mode(), ClaimMode::Inductive) {
            return replay_inductive(obligation, &assignments);
        }
        let trace_len = match obligation.claim.mode() {
            ClaimMode::Relational => 1,
            ClaimMode::Finite { horizon } if horizon > 0 => {
                let Some(length) = assignments
                    .get("zeno_trace_len")
                    .copied()
                    .and_then(|value| u32::try_from(value).ok())
                    .filter(|length| *length > 0 && *length <= horizon)
                else {
                    return ToolRunStatus::Blocked(ToolFailure::ModelReplayFailed);
                };
                length
            }
            _ => return ToolRunStatus::Blocked(ToolFailure::ModelReplayFailed),
        };
        let mut paths = BTreeSet::new();
        let mut predicates = BTreeMap::new();
        collect_claim(&obligation.claim, &mut paths, &mut predicates);
        if !predicates.is_empty() {
            return ToolRunStatus::Blocked(ToolFailure::ModelReplayFailed);
        }
        let mut trace = Vec::new();
        for step in 0..trace_len {
            let mut observations = Vec::new();
            for path in &paths {
                let name = smt_path(path, step);
                let Some(value) = assignments.get(&name).copied() else {
                    return ToolRunStatus::Blocked(ToolFailure::ModelReplayFailed);
                };
                observations.push(Observation::new(path.clone(), value));
            }
            let Some(event) = TraceStep::try_new(observations) else {
                return ToolRunStatus::Blocked(ToolFailure::ModelReplayFailed);
            };
            trace.push(event);
        }
        match obligation.claim.formula() {
            ClaimFormula::Relational(formula) => match evaluate_relational(
                formula,
                EvaluationContext::new(&trace[0], &MissingPredicates, EvalLimits::default()),
            ) {
                EvalOutcome::False => ToolRunStatus::Refuted,
                EvalOutcome::Indeterminate(reason) => undefined_or_unreplayed(reason),
                EvalOutcome::True => ToolRunStatus::Blocked(ToolFailure::ModelReplayFailed),
            },
            ClaimFormula::Temporal(formula) => match evaluate_temporal(
                formula,
                obligation.claim.mode(),
                &trace,
                &MissingPredicates,
                EvalLimits::default(),
            ) {
                TemporalEvaluation::Counterexample { .. } => ToolRunStatus::Refuted,
                TemporalEvaluation::Indeterminate(reason) => undefined_or_unreplayed(reason),
                TemporalEvaluation::Satisfied | TemporalEvaluation::ProofObligation => {
                    ToolRunStatus::Blocked(ToolFailure::ModelReplayFailed)
                }
            },
        }
    }

    fn replay_inductive(
        obligation: &ExportedObligation,
        assignments: &BTreeMap<String, i128>,
    ) -> ToolRunStatus {
        let unreplayed = ToolRunStatus::Blocked(ToolFailure::ModelReplayFailed);
        let ClaimFormula::Relational(before) = obligation.claim.formula() else {
            return unreplayed;
        };
        let Some(after) = invariant_at(before, ProjectionRoot::Post) else {
            return unreplayed;
        };
        let hypotheses = &obligation.hypotheses;
        let case_laws = if hypotheses.splits_decisions() {
            match assignments.get(DECISION_KIND).copied() {
                Some(ACCEPT_KIND) => &hypotheses.accepts,
                Some(COMMITTED_FAILURE_KIND) => &hypotheses.committed_failures,
                _ => return unreplayed,
            }
        } else {
            &hypotheses.accepts
        };
        let mut paths = BTreeSet::new();
        let mut predicates = BTreeMap::new();
        for law in hypotheses
            .every_commit
            .iter()
            .chain(hypotheses.accepts.iter())
            .chain(hypotheses.committed_failures.iter())
        {
            collect_rel(law.formula(), &mut paths, &mut predicates);
        }
        collect_rel(before, &mut paths, &mut predicates);
        collect_rel(&after, &mut paths, &mut predicates);
        if !predicates.is_empty() {
            return unreplayed;
        }
        let mut observations = Vec::with_capacity(paths.len());
        for path in &paths {
            let Some(value) = assignments.get(&smt_path(path, 0)).copied() else {
                return unreplayed;
            };
            // The authority refuses an undeclared variant or an integer outside its
            // declared range, so such a model is no transition.
            if hypotheses
                .domains
                .get(path)
                .is_some_and(|domain| !domain.contains(value))
            {
                return unreplayed;
            }
            observations.push(Observation::new(path.clone(), value));
        }
        let Some(transition) = TraceStep::try_new(observations) else {
            return unreplayed;
        };
        let evaluate = |formula: &RelExpr| {
            evaluate_relational(
                formula,
                EvaluationContext::new(&transition, &MissingPredicates, EvalLimits::default()),
            )
        };
        let assumptions_hold = hypotheses
            .every_commit
            .iter()
            .chain(case_laws.iter())
            .all(|law| evaluate(law.formula()) == EvalOutcome::True);
        if !assumptions_hold || evaluate(before) != EvalOutcome::True {
            return unreplayed;
        }
        match evaluate(&after) {
            EvalOutcome::False => ToolRunStatus::Refuted,
            EvalOutcome::Indeterminate(reason) => undefined_or_unreplayed(reason),
            EvalOutcome::True => unreplayed,
        }
    }

    const fn undefined_or_unreplayed(reason: IndeterminateReason) -> ToolRunStatus {
        match UndefinedReason::from_indeterminate(reason) {
            Some(reason) => ToolRunStatus::Undefined(reason),
            None => ToolRunStatus::Blocked(ToolFailure::ModelReplayFailed),
        }
    }

    fn classify(
        config: &ToolConfig,
        output: &ProcessOutput,
        obligation: &ExportedObligation,
    ) -> ToolRunStatus {
        if !output.status.success() {
            return ToolRunStatus::Failed(ToolFailure::Crash(output.status.code()));
        }
        let text = String::from_utf8_lossy(&output.stdout);
        match config.backend {
            ToolBackend::Cvc5 => {
                let first = text
                    .lines()
                    .find(|line| !line.trim().is_empty())
                    .unwrap_or("")
                    .trim();
                match first {
                    "unsat" if text.contains("(step") => ToolRunStatus::ProposedUnsat,
                    "sat" => replay_model(obligation, &text),
                    "unknown" => ToolRunStatus::Blocked(ToolFailure::Unknown),
                    _ => ToolRunStatus::Blocked(ToolFailure::UnsupportedEvidence),
                }
            }
            ToolBackend::Z3 => {
                let first = text
                    .lines()
                    .find(|line| !line.trim().is_empty())
                    .unwrap_or("")
                    .trim();
                match first {
                    "sat" => replay_model(obligation, &text),
                    "unsat" => ToolRunStatus::Blocked(ToolFailure::UnsupportedEvidence),
                    "unknown" => ToolRunStatus::Blocked(ToolFailure::Unknown),
                    _ => ToolRunStatus::Blocked(ToolFailure::UnsupportedEvidence),
                }
            }
            ToolBackend::Lean
                if config
                    .runtime
                    .as_ref()
                    .is_none_or(|runtime| runtime.tree_sha256 != LEAN_LINUX_X86_64_TREE_SHA256) =>
            {
                ToolRunStatus::Blocked(ToolFailure::UnsupportedEvidence)
            }
            ToolBackend::Lean => match parse_lean_axioms(&text) {
                Some(axioms) if axioms == config.allowed_axioms => ToolRunStatus::KernelChecked,
                _ => ToolRunStatus::Blocked(ToolFailure::LeanAxiomReport),
            },
        }
    }

    pub(super) fn replay(o: &ExportedObligation, t: &str) -> ToolRunStatus {
        replay_model(o, t)
    }
    pub(super) fn classification(
        c: &ToolConfig,
        p: &ProcessOutput,
        o: &ExportedObligation,
    ) -> ToolRunStatus {
        classify(c, p, o)
    }
}
