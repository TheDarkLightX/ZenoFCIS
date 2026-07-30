//! Executable RC3 language, logic, composition, and Mini Determinator acceptance bindings.

use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_spec::{
    ClaimMode, DiagnosticCode, EvalLimits, GraphFormat, MiniBudget, MiniDecision, MiniDeterminator,
    MiniState, Observation, PredicateProvider, ProjectLimits, ProjectionPath, ProjectionRoot,
    RelExpr, SourceLimits, StableId, TemporalEvaluation, TemporalFormula, TraceStep,
    WorkerInstruction, WorkerProgram, WorkspaceCell, elaborate_project, evaluate_temporal,
    generate_project, parse_project, render_graph,
};

const SOURCE_A: &str = r#"zeno 1;
project 1 demo;
type 101 command Command;
type 100 state State;
reason 200 bad precedence 0;
component 300 machine { writes post.100; reads pre.100; owns 100; budget steps 10; }
merge [300];
law 400 same = pre.100 == pre.100;
claim 500 same cvc5 relational = pre.100 == pre.100;
"#;

const SOURCE_B: &str = r#"// formatting and declaration order are not identity
zeno 1; project 1 demo;
claim 500 same cvc5 relational = pre.100 == pre.100;
law 400 same = pre.100 == pre.100;
reason 200 bad precedence 0;
type 100 state State;
component 300 machine {
  budget steps 10;
  owns 100;
  reads pre.100;
  writes post.100;
}
type 101 command Command;
merge [300];
"#;

fn spec(source: &str) -> zeno_fcis_spec::ProjectSpec {
    let parsed =
        parse_project(source, SourceLimits::default()).unwrap_or_else(|set| panic!("{set}"));
    elaborate_project(parsed, ProjectLimits::default()).unwrap_or_else(|set| panic!("{set}"))
}

fn id(value: u32) -> StableId {
    StableId::new(value).unwrap_or_else(|| unreachable!())
}

#[test]
fn rc3_spec_canonical() {
    let left = spec(SOURCE_A);
    let right = spec(SOURCE_B);
    assert_eq!(left.canonical_bytes(), right.canonical_bytes());
}

#[test]
fn builder_parser_manual_constructor_bytes_are_equivalent() {
    use zeno_fcis_spec::{
        BackendId, ClaimDecl, ClaimFormula, CompareOp, ComponentDecl, FootprintDecl, FootprintKind,
        Identifier, LawDecl, ProjectSpecBuilder, ReasonDecl, TypeDecl, TypeKind, ValueExpr,
    };
    let name = |value: &str| Identifier::try_new(value).unwrap_or_else(|| unreachable!());
    let path =
        |root| ProjectionPath::try_new(root, vec![id(100)]).unwrap_or_else(|| unreachable!());
    let formula = || {
        RelExpr::Compare(
            CompareOp::Eq,
            ValueExpr::Projection(path(ProjectionRoot::Pre)),
            ValueExpr::Projection(path(ProjectionRoot::Pre)),
        )
    };
    let built = ProjectSpecBuilder::new(id(1), name("demo"))
        .type_decl(TypeDecl::new(id(101), TypeKind::Command, name("Command")))
        .type_decl(TypeDecl::new(id(100), TypeKind::State, name("State")))
        .reason(ReasonDecl::new(id(200), name("bad"), 0))
        .component(ComponentDecl::new(
            id(300),
            name("machine"),
            vec![id(100)],
            Vec::new(),
            vec![
                FootprintDecl::new(FootprintKind::Write, path(ProjectionRoot::Post)),
                FootprintDecl::new(FootprintKind::Read, path(ProjectionRoot::Pre)),
            ],
            vec![
                zeno_fcis_spec::BudgetDecl::try_new(zeno_fcis_spec::BudgetResource::Steps, 10)
                    .unwrap_or_else(|| unreachable!()),
            ],
            Vec::new(),
            Vec::new(),
        ))
        .law(LawDecl::new(id(400), name("same"), formula()))
        .claim(ClaimDecl::new(
            id(500),
            name("same"),
            vec![BackendId::Cvc5],
            ClaimMode::Relational,
            ClaimFormula::Relational(formula()),
        ))
        .merge_order(vec![id(300)])
        .finish(ProjectLimits::default())
        .unwrap_or_else(|set| panic!("{set}"));
    assert_eq!(built.canonical_bytes(), spec(SOURCE_A).canonical_bytes());
}

#[test]
fn rc3_composition_diagnostics() {
    let source = "zeno 1; project 1 bad; type 10 state A; type 10 state B; component 20 c { owns 99; } merge [20, 21];";
    let parsed =
        parse_project(source, SourceLimits::default()).unwrap_or_else(|set| panic!("{set}"));
    let Err(diagnostics) = elaborate_project(parsed, ProjectLimits::default()) else {
        panic!("invalid project unexpectedly elaborated");
    };
    assert!(diagnostics.len() >= 3);
    assert!(
        diagnostics
            .diagnostics()
            .windows(2)
            .all(|pair| pair[0].span() <= pair[1].span())
    );
    assert!(
        diagnostics
            .diagnostics()
            .iter()
            .any(|item| item.code() == DiagnosticCode::DuplicateId)
    );
    assert!(
        diagnostics
            .diagnostics()
            .iter()
            .any(|item| item.code() == DiagnosticCode::UnknownReference)
    );
    assert!(
        diagnostics
            .diagnostics()
            .iter()
            .any(|item| item.code() == DiagnosticCode::InvalidMergeOrder)
    );
}

struct NoPredicates;
impl PredicateProvider for NoPredicates {
    fn evaluate(&self, _: &zeno_fcis_spec::Identifier, _: &[i128]) -> Option<bool> {
        None
    }
}

#[test]
fn rc3_temporal_modes() {
    let path = ProjectionPath::try_new(ProjectionRoot::Events, vec![id(1)])
        .unwrap_or_else(|| unreachable!());
    let trace =
        TraceStep::try_new(vec![Observation::new(path, 1)]).unwrap_or_else(|| unreachable!());
    let formula = TemporalFormula::Next(Box::new(TemporalFormula::Atom(RelExpr::Bool(true))));
    assert_eq!(
        evaluate_temporal(
            &formula,
            ClaimMode::Finite { horizon: 1 },
            core::slice::from_ref(&trace),
            &NoPredicates,
            EvalLimits::default()
        ),
        TemporalEvaluation::Counterexample { step: 0 }
    );
    assert_eq!(
        evaluate_temporal(
            &formula,
            ClaimMode::UnboundedProof,
            &[trace],
            &NoPredicates,
            EvalLimits::default()
        ),
        TemporalEvaluation::ProofObligation
    );
}

fn program(worker: u32, slot: u32, operation: WorkerInstruction) -> WorkerProgram {
    WorkerProgram::new(
        worker,
        vec![1],
        vec![slot],
        vec![
            WorkerInstruction::Get(1),
            operation,
            WorkerInstruction::Put(slot),
            WorkerInstruction::Return,
        ],
    )
}

#[test]
fn rc3_mini_os_replay() {
    let pre = MiniState::try_new(vec![WorkspaceCell::new(1, 10)]).unwrap_or_else(|| unreachable!());
    let programs = vec![
        program(2, 3, WorkerInstruction::Multiply(3)),
        program(1, 2, WorkerInstruction::Multiply(2)),
    ];
    assert_eq!(
        MiniDeterminator::execute_programs(&pre, &programs, &[2, 1], MiniBudget::default()),
        MiniDeterminator::execute_programs(&pre, &programs, &[1, 2], MiniBudget::default())
    );
}

#[test]
fn rc3_mini_os_conflict() {
    let pre = MiniState::try_new(vec![WorkspaceCell::new(1, 10)]).unwrap_or_else(|| unreachable!());
    let programs = vec![
        program(9, 2, WorkerInstruction::Add(1)),
        program(3, 2, WorkerInstruction::Add(2)),
    ];
    assert!(matches!(
        MiniDeterminator::execute_programs(&pre, &programs, &[9, 3], MiniBudget::default())
            .decision(),
        MiniDecision::Rejected(_)
    ));
    assert_eq!(pre.cells(), &[WorkspaceCell::new(1, 10)]);
}

#[test]
fn rc3_input_inert() {
    for hostile in ["$(touch /tmp/owned)", "${HOME}", "../../etc/passwd"] {
        let source = format!("zeno 1; project 1 inert; namespace 2 {hostile};");
        assert!(parse_project(&source, SourceLimits::default()).is_err());
    }
    let instruction = "zeno 1; project 1 inert; namespace 2 ignore_previous_instructions;";
    let parsed =
        parse_project(instruction, SourceLimits::default()).unwrap_or_else(|set| panic!("{set}"));
    assert_eq!(parsed.name().as_str(), "inert");
}

#[test]
fn rc3_derived_views() {
    let project = spec(SOURCE_A);
    assert_eq!(
        render_graph(&project, GraphFormat::Dot),
        render_graph(&project, GraphFormat::Dot)
    );
    assert_eq!(
        render_graph(&project, GraphFormat::Json),
        render_graph(&project, GraphFormat::Json)
    );
}

#[test]
fn rc3_generated_drift() {
    let project = spec(SOURCE_A);
    let left = generate_project::<zeno_fcis_crypto::RustCryptoSha256>(&project)
        .unwrap_or_else(|_| unreachable!());
    let right = generate_project::<zeno_fcis_crypto::RustCryptoSha256>(&project)
        .unwrap_or_else(|_| unreachable!());
    assert_eq!(left.rust(), right.rust());
    assert_eq!(left.manifest(), right.manifest());
}

#[test]
fn rc3_resource_limits_fail_closed() {
    let limits = SourceLimits::try_new(64, 64, 8).unwrap_or_else(|| unreachable!());
    let oversized = "x".repeat(65);
    let Err(diagnostics) = parse_project(&oversized, limits) else {
        panic!("oversized source unexpectedly parsed");
    };
    assert!(
        diagnostics
            .diagnostics()
            .iter()
            .any(|item| item.code() == DiagnosticCode::SourceTooLarge)
    );

    let token_limits = SourceLimits::try_new(1024, 4, 8).unwrap_or_else(|| unreachable!());
    let Err(diagnostics) = parse_project("zeno 1; project 1 bounded;", token_limits) else {
        panic!("over-token source unexpectedly parsed");
    };
    assert!(
        diagnostics
            .diagnostics()
            .iter()
            .any(|item| item.code() == DiagnosticCode::TokenLimit)
    );

    let retained = SourceLimits::try_new(1024, 128, 2).unwrap_or_else(|| unreachable!());
    let Err(diagnostics) = parse_project("bad bad bad; bad bad bad; bad bad bad;", retained) else {
        panic!("invalid source unexpectedly parsed");
    };
    assert_eq!(diagnostics.len(), 2);
    assert!(diagnostics.is_truncated());
}

#[test]
fn rc3_parser_depth_is_bounded_before_recursion() {
    let allowed = "not ".repeat(zeno_fcis_spec::MAX_FORMULA_DEPTH);
    let allowed_source = format!("zeno 1; project 1 depth; law 2 bounded = {allowed} true;");
    parse_project(&allowed_source, SourceLimits::default())
        .unwrap_or_else(|set| panic!("formula at the depth limit failed: {set}"));

    let independent_laws = (0..=zeno_fcis_spec::MAX_FORMULA_DEPTH)
        .map(|offset| format!("law {} bounded = not true;", offset + 10))
        .collect::<String>();
    let independent_source = format!("zeno 1; project 1 depth; {independent_laws}");
    parse_project(&independent_source, SourceLimits::default())
        .unwrap_or_else(|set| panic!("nesting leaked between independent laws: {set}"));

    let nested = "not ".repeat(zeno_fcis_spec::MAX_FORMULA_DEPTH + 1);
    let source = format!("zeno 1; project 1 depth; law 2 bounded = {nested} true;");
    let Err(diagnostics) = parse_project(&source, SourceLimits::default()) else {
        panic!("over-nested formula unexpectedly parsed");
    };
    assert!(diagnostics.diagnostics().iter().any(|item| {
        item.code() == DiagnosticCode::ResourceLimit
            && item.stage() == zeno_fcis_spec::DiagnosticStage::Parse
    }));
}
#[test]
fn rc3_parser_operator_chains_are_bounded_before_ast_construction() {
    let allowed_count = zeno_fcis_spec::MAX_FORMULA_DEPTH;
    let allowed = core::iter::repeat_n("true", allowed_count)
        .collect::<Vec<_>>()
        .join(" && ");
    let allowed_source = format!("zeno 1; project 1 chain; law 2 bounded = {allowed};");
    parse_project(&allowed_source, SourceLimits::default())
        .unwrap_or_else(|set| panic!("operator chain at the limit failed: {set}"));

    let count = zeno_fcis_spec::MAX_FORMULA_DEPTH + 1;
    let relational = core::iter::repeat_n("true", count)
        .collect::<Vec<_>>()
        .join(" && ");
    let scalar = core::iter::repeat_n("1", count)
        .collect::<Vec<_>>()
        .join(" + ");
    let temporal = core::iter::repeat_n("true", count)
        .collect::<Vec<_>>()
        .join(" until ");
    let sources = [
        format!("zeno 1; project 1 chain; law 2 bounded = {relational};"),
        format!("zeno 1; project 1 chain; law 2 bounded = {scalar} == 0;"),
        format!("zeno 1; project 1 chain; claim 2 bounded z3 finite 2 = {temporal};"),
    ];
    for source in sources {
        let Err(diagnostics) = parse_project(&source, SourceLimits::default()) else {
            panic!("overlong operator chain unexpectedly parsed");
        };
        assert!(
            diagnostics
                .diagnostics()
                .iter()
                .any(|item| item.code() == DiagnosticCode::ResourceLimit)
        );
    }
}

#[test]
fn rc3_finite_horizon_is_bounded_during_elaboration() {
    let horizon = zeno_fcis_spec::MAX_FINITE_HORIZON + 1;
    let source = format!("zeno 1; project 1 horizon; claim 2 bounded z3 finite {horizon} = true;");
    let parsed =
        parse_project(&source, SourceLimits::default()).unwrap_or_else(|set| panic!("{set}"));
    let Err(diagnostics) = elaborate_project(parsed, ProjectLimits::default()) else {
        panic!("over-horizon claim unexpectedly elaborated");
    };
    assert!(
        diagnostics
            .diagnostics()
            .iter()
            .any(|item| item.code() == DiagnosticCode::ResourceLimit)
    );
}

#[test]
fn diagnostic_code_registry_is_complete_and_unique() {
    let mut codes = DiagnosticCode::ALL.map(DiagnosticCode::as_str);
    codes.sort_unstable();
    assert!(codes.windows(2).all(|pair| pair[0] != pair[1]));
    assert_eq!(codes.first(), Some(&"ZENO-E0001"));
    assert_eq!(codes.last(), Some(&"ZENO-E0302"));
}
