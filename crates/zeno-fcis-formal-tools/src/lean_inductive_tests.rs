use super::*;
use spec_authoring::{ProjectLimits, SourceLimits, elaborate_project, parse_project};

const PILOTS: &[(&str, &str)] = &[
    (
        "durable_counter",
        include_str!("../../zeno-fcis-cli/templates/durable-counter/project.zeno"),
    ),
    (
        "account_lockout",
        include_str!("../../zeno-fcis-cli/templates/account-lockout/project.zeno"),
    ),
];
const CONTROLS: &[(&str, &str, bool)] = &[
    (
        "decrement",
        include_str!("../tests/fixtures/lean-inductive/decrement.zeno"),
        false,
    ),
    (
        "kind_split",
        include_str!("../tests/fixtures/lean-inductive/kind_split.zeno"),
        false,
    ),
    (
        "domain",
        include_str!("../tests/fixtures/lean-inductive/domain.zeno"),
        true,
    ),
    (
        "wrong_domain",
        include_str!("../tests/fixtures/lean-inductive/wrong_domain.zeno"),
        false,
    ),
    (
        "overflow",
        include_str!("../tests/fixtures/lean-inductive/overflow.zeno"),
        false,
    ),
    (
        "law_scope",
        include_str!("../tests/fixtures/lean-inductive/law_scope.zeno"),
        false,
    ),
    (
        "projection",
        include_str!("../tests/fixtures/lean-inductive/projection.zeno"),
        false,
    ),
];

fn project(source: &str) -> ProjectSpec {
    elaborate_project(
        parse_project(source, SourceLimits::default())
            .unwrap_or_else(|error| panic!("parse: {error}")),
        ProjectLimits::default(),
    )
    .unwrap_or_else(|error| panic!("elaborate: {error}"))
}

fn exported_fixture(source: &str) -> ExportedObligation {
    let spec = project(source);
    export_inductive_lean(&spec.claims()[0], &spec)
        .unwrap_or_else(|error| panic!("export: {error:?}"))
}

fn record_source(name: &str, source: &[u8]) {
    if let Some(directory) = std::env::var_os("ZENO_FCIS_LEAN_EVIDENCE") {
        let directory = PathBuf::from(directory);
        fs::create_dir_all(&directory)
            .unwrap_or_else(|error| panic!("evidence directory: {error}"));
        fs::write(directory.join(format!("{name}.lean")), source)
            .unwrap_or_else(|error| panic!("source evidence: {error}"));
    }
}

#[test]
fn inductive_lean_and_smt_emit_identical_structured_facts() {
    for (name, source) in PILOTS
        .iter()
        .copied()
        .chain(CONTROLS.iter().map(|(name, source, _)| (*name, *source)))
    {
        let spec = project(source);
        let claim = &spec.claims()[0];
        let smt = export_inductive_smt(claim, &spec, ToolBackend::Z3)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let lean =
            export_inductive_lean(claim, &spec).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        // Structured facts are recorded inside each exporter's actual emission
        // loops: observed paths (i128), domain values/ranges, law ASTs and kinds.
        assert_eq!(lean.hypotheses(), smt.hypotheses(), "{name}");
        assert!(!lean.hypotheses().paths().is_empty(), "{name}");
        assert_eq!(lean.scope(), ObligationScope::InductiveStepOverLaws);
        assert!(source_is_bound(&lean));
        let text = String::from_utf8_lossy(lean.source());
        record_source(name, lean.source());
        assert!(text.contains("theorem history_holds (g : State) (hg : Inv g)"));
        assert!(!text.contains("theorem genesis_holds"));
    }
}

#[test]
fn inductive_lean_strictness_and_fragment_are_explicit() {
    let spec = project(PILOTS[0].1);
    let claim = &spec.claims()[0];
    let export = export_inductive_lean(claim, &spec).unwrap_or_else(|_| unreachable!());
    let text = String::from_utf8_lossy(export.source());
    assert!(text.contains("(kind = 1 →"));
    assert!(text.contains("(kind = 2 →"));
    assert!(text.contains("inI128 ((pre.p_100_110) + (1 : Int))"));
    assert!(!text.contains("reject_no_authority"));
    let changed = project(
        &PILOTS[0]
            .1
            .replace("pre.100.110 >= 0", "pre.100.110 * pre.100.111 >= 0"),
    );
    assert_eq!(
        export_inductive_lean(&changed.claims()[0], &changed),
        Err(ExportError::UnsupportedMode)
    );
    let overflow = RelExpr::Compare(
        CompareOp::Greater,
        ValueExpr::Add(
            Box::new(ValueExpr::Int(i128::MAX)),
            Box::new(ValueExpr::Int(1)),
        ),
        ValueExpr::Int(0),
    );
    for expr in [
        RelExpr::Not(Box::new(overflow.clone())),
        RelExpr::Or(Box::new(RelExpr::Bool(true)), Box::new(overflow.clone())),
        RelExpr::Implies(Box::new(RelExpr::Bool(false)), Box::new(overflow)),
    ] {
        let rendered = render_relation(&expr, &mut SmtRenderBudget::new(ExportLimits::default()))
            .unwrap_or_else(|_| unreachable!());
        assert!(rendered.defined.contains("inI128"));
    }
    let tiny = ExportLimits::try_new(1, 1, 1, 1, 1).unwrap_or_else(|| unreachable!());
    assert_eq!(
        export_inductive_lean_with_limits(claim, &spec, tiny),
        Err(ExportError::ResourceLimit)
    );
}

#[test]
fn inductive_lean_all_unsupported_expression_forms_refuse() {
    let identifier = Identifier::try_new("item").unwrap_or_else(|| unreachable!());
    let mut budget = SmtRenderBudget::new(ExportLimits::default());
    for value in [
        ValueExpr::Var(identifier.clone()),
        ValueExpr::Div(
            spec_authoring::DivisionMode::Floor,
            Box::new(ValueExpr::Int(1)),
            Box::new(ValueExpr::Int(2)),
        ),
        ValueExpr::Sum {
            variable: identifier.clone(),
            start: 0,
            end: 1,
            body: Box::new(ValueExpr::Int(1)),
        },
    ] {
        assert!(matches!(
            render_value(&value, &mut budget),
            Err(ExportError::UnsupportedMode)
        ));
    }
    for expr in [
        RelExpr::Predicate {
            name: identifier.clone(),
            arguments: vec![ValueExpr::Int(1)].into_boxed_slice(),
        },
        RelExpr::ForAll {
            variable: identifier.clone(),
            start: 0,
            end: 1,
            body: Box::new(RelExpr::Bool(true)),
        },
        RelExpr::Exists {
            variable: identifier,
            start: 0,
            end: 1,
            body: Box::new(RelExpr::Bool(true)),
        },
    ] {
        assert!(matches!(
            render_relation(&expr, &mut budget),
            Err(ExportError::UnsupportedMode)
        ));
    }
}

fn pinned_config() -> ToolConfig {
    let path = PathBuf::from(
        std::env::var_os("ZENO_FCIS_LEAN").unwrap_or_else(|| panic!("ZENO_FCIS_LEAN is required")),
    );
    let bytes = fs::read(&path).unwrap_or_else(|error| panic!("read Lean: {error}"));
    ToolConfig {
        backend: ToolBackend::Lean,
        path,
        version: LEAN_VERSION.to_owned(),
        sha256: hash_hex(RustCryptoSha256::hash(&bytes)),
        runtime: Some(ToolRuntimeConfig {
            root: PathBuf::from(
                std::env::var_os("ZENO_FCIS_LEAN_ROOT")
                    .unwrap_or_else(|| panic!("ZENO_FCIS_LEAN_ROOT is required")),
            ),
            tree_sha256: LEAN_LINUX_X86_64_TREE_SHA256.to_owned(),
        }),
        timeout_ms: 60_000,
        max_output_bytes: 1024 * 1024,
        allowed_axioms: vec!["Quot.sound".to_owned(), "propext".to_owned()],
    }
}

fn run_fixture(name: &str, source: &str, expected: bool) {
    let obligation = exported_fixture(source);
    record_source(name, obligation.source());
    let run = execute_tool(&pinned_config(), obligation)
        .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    if let Some(directory) = std::env::var_os("ZENO_FCIS_LEAN_EVIDENCE") {
        let directory = PathBuf::from(directory);
        fs::create_dir_all(&directory)
            .unwrap_or_else(|error| panic!("evidence directory: {error}"));
        fs::write(
            directory.join(format!("{name}.lean")),
            run.obligation().source(),
        )
        .unwrap_or_else(|error| panic!("source evidence: {error}"));
        fs::write(directory.join(format!("{name}.stdout")), run.stdout())
            .unwrap_or_else(|error| panic!("output evidence: {error}"));
    }
    println!(
        "{name}: {:?}\n{}",
        run.status(),
        String::from_utf8_lossy(run.stdout())
    );
    assert_eq!(
        run.status() == &ToolRunStatus::KernelChecked,
        expected,
        "{name}: {}",
        String::from_utf8_lossy(run.stdout())
    );
    if !expected {
        assert!(matches!(
            run.status(),
            ToolRunStatus::Failed(ToolFailure::Crash(Some(1)))
        ));
        assert!(String::from_utf8_lossy(run.stdout()).contains("error:"));
    }
}

#[test]
#[ignore = "requires the workflow-pinned Lean distribution"]
fn pinned_lean_inductive_pilots() {
    for (name, source) in PILOTS {
        run_fixture(name, source, true);
    }
}

macro_rules! control {
    ($test:ident, $index:literal) => {
        #[test]
        #[ignore = "requires the workflow-pinned Lean distribution"]
        fn $test() {
            let (name, source, checked) = CONTROLS[$index];
            run_fixture(name, source, checked);
        }
    };
}
control!(pinned_lean_control_decrement, 0);
control!(pinned_lean_control_kind_split, 1);
control!(pinned_lean_control_domain, 2);
control!(pinned_lean_control_wrong_domain, 3);
control!(pinned_lean_control_overflow, 4);
control!(pinned_lean_control_law_scope, 5);
control!(pinned_lean_control_projection, 6);

#[test]
#[ignore = "requires the workflow-pinned Lean distribution"]
fn pinned_lean_operator_truth_and_strict_definedness() {
    let int = |value| Box::new(ValueExpr::Int(value));
    let compare = |op, a, b| RelExpr::Compare(op, ValueExpr::Int(a), ValueExpr::Int(b));
    let overflow = RelExpr::Compare(
        CompareOp::Greater,
        ValueExpr::Add(int(i128::MAX), int(1)),
        ValueExpr::Int(0),
    );
    let cases = vec![
        (RelExpr::Bool(true), true),
        (RelExpr::Bool(false), false),
        (compare(CompareOp::Eq, 4, 4), true),
        (compare(CompareOp::NotEq, 4, 5), true),
        (compare(CompareOp::Less, -1, 0), true),
        (compare(CompareOp::LessEq, -1, -1), true),
        (compare(CompareOp::Greater, 1, 0), true),
        (compare(CompareOp::GreaterEq, 1, 1), true),
        (
            RelExpr::And(
                Box::new(RelExpr::Bool(true)),
                Box::new(RelExpr::Bool(false)),
            ),
            false,
        ),
        (
            RelExpr::Or(
                Box::new(RelExpr::Bool(false)),
                Box::new(RelExpr::Bool(true)),
            ),
            true,
        ),
        (
            RelExpr::Implies(
                Box::new(RelExpr::Bool(false)),
                Box::new(RelExpr::Bool(false)),
            ),
            true,
        ),
        (RelExpr::Not(Box::new(RelExpr::Bool(false))), true),
        (
            RelExpr::Compare(
                CompareOp::Eq,
                ValueExpr::Add(int(-3), int(7)),
                ValueExpr::Int(4),
            ),
            true,
        ),
        (
            RelExpr::Compare(
                CompareOp::Eq,
                ValueExpr::Sub(int(-3), int(7)),
                ValueExpr::Int(-10),
            ),
            true,
        ),
        (
            RelExpr::Compare(
                CompareOp::Eq,
                ValueExpr::Mul(int(-3), int(7)),
                ValueExpr::Int(-21),
            ),
            true,
        ),
        (
            RelExpr::Compare(
                CompareOp::Eq,
                ValueExpr::Sub(int(0), int(i128::MIN)),
                ValueExpr::Int(0),
            ),
            false,
        ),
        (
            RelExpr::Compare(
                CompareOp::Eq,
                ValueExpr::Sub(int(i128::MIN), int(1)),
                ValueExpr::Int(0),
            ),
            false,
        ),
        (
            RelExpr::Compare(
                CompareOp::Eq,
                ValueExpr::Mul(int(i128::MAX), int(2)),
                ValueExpr::Int(0),
            ),
            false,
        ),
        (
            RelExpr::Or(Box::new(RelExpr::Bool(true)), Box::new(overflow.clone())),
            false,
        ),
        (
            RelExpr::Implies(Box::new(RelExpr::Bool(false)), Box::new(overflow.clone())),
            false,
        ),
        (RelExpr::Not(Box::new(overflow)), false),
    ];
    let spec = project(CONTROLS[2].1);
    let original = &spec.claims()[0];
    let mut examples = String::new();
    // One kernel invocation checks all operator truth values independently of
    // the preservation proof, including undefined expressions beneath Booleans.
    for (index, (formula, expected)) in cases.into_iter().enumerate() {
        let claim = ClaimDecl::inductive(
            original.id(),
            original.name().clone(),
            original.backends().to_vec(),
            original.assumptions().clone(),
            formula,
        );
        let obligation = export_inductive_lean(&claim, &spec)
            .unwrap_or_else(|error| panic!("operator {index}: {error:?}"));
        let rendered = match claim.formula() {
            ClaimFormula::Relational(expr) => {
                render_relation(expr, &mut SmtRenderBudget::new(ExportLimits::default()))
                    .unwrap_or_else(|_| unreachable!())
            }
            ClaimFormula::Temporal(_) => unreachable!(),
        };
        let proposition = satisfied(&rendered);
        examples.push_str(&format!(
            "example : {}({proposition}) := by decide\n",
            if expected { "" } else { "¬ " }
        ));
        assert!(source_is_bound(&obligation));
    }
    let mut obligation = exported_fixture(PILOTS[0].1);
    obligation.source.extend_from_slice(examples.as_bytes());
    obligation.source_hash =
        commitment::<RustCryptoSha256>(zeno_fcis_codec::domains::FORMAL_SOURCE, &obligation.source)
            .unwrap_or_else(|_| unreachable!());
    record_source("operators", obligation.source());
    let run = execute_tool(&pinned_config(), obligation)
        .unwrap_or_else(|error| panic!("operator corpus: {error:?}"));
    assert_eq!(
        run.status(),
        &ToolRunStatus::KernelChecked,
        "{}",
        String::from_utf8_lossy(run.stdout())
    );
}

#[cfg(unix)]
#[test]
fn inductive_lean_tampered_evidence_is_blocked() {
    use std::os::unix::process::ExitStatusExt as _;
    let mut config = ToolConfig {
        backend: ToolBackend::Lean,
        path: "unused".into(),
        version: LEAN_VERSION.to_owned(),
        sha256: "0".repeat(64),
        runtime: Some(ToolRuntimeConfig {
            root: "unused".into(),
            tree_sha256: LEAN_LINUX_X86_64_TREE_SHA256.to_owned(),
        }),
        timeout_ms: 1000,
        max_output_bytes: 4096,
        allowed_axioms: vec!["Quot.sound".to_owned(), "propext".to_owned()],
    };
    let obligation = exported_fixture(PILOTS[0].1);
    let output = |name: &str, axioms: &str| ProcessOutput {
        status: ExitStatus::from_raw(0),
        stdout: format!("'{name}' depends on axioms: [{axioms}]\n").into_bytes(),
        stderr: Vec::new(),
    };
    let valid = output(THEOREM, "propext, Quot.sound");
    assert_eq!(
        classify(&config, &valid, &obligation),
        ToolRunStatus::KernelChecked
    );
    assert_ne!(
        classify(
            &config,
            &output("other", "propext, Quot.sound"),
            &obligation
        ),
        ToolRunStatus::KernelChecked
    );
    assert_ne!(
        classify(
            &config,
            &output(THEOREM, "propext, Quot.sound, Classical.choice"),
            &obligation
        ),
        ToolRunStatus::KernelChecked
    );
    let mut changed = obligation.clone();
    changed.source.push(b' ');
    assert!(!source_is_bound(&changed));
    assert_ne!(
        classify(&config, &valid, &changed),
        ToolRunStatus::KernelChecked
    );
    assert_eq!(
        execute_tool(&config, changed),
        Err(ToolFailure::UnsupportedEvidence)
    );
    config
        .runtime
        .as_mut()
        .unwrap_or_else(|| unreachable!())
        .tree_sha256 = "0".repeat(64);
    assert_ne!(
        classify(&config, &valid, &obligation),
        ToolRunStatus::KernelChecked
    );
}

#[cfg(unix)]
#[test]
fn inductive_lean_old_record_cannot_replace_current_evidence() {
    use std::os::unix::process::ExitStatusExt as _;
    let old = exported_fixture(PILOTS[0].1);
    let newer = exported_fixture(&PILOTS[0].1.replace("pre.100.110 >= 0", "pre.100.110 >= 1"));
    let transcript = ProcessTranscript {
        phase: ProcessPhase::Kernel,
        input: old.source().to_vec(),
        output: ProcessOutput {
            status: ExitStatus::from_raw(0),
            stdout: b"'history_holds' depends on axioms: [propext, Quot.sound]\n".to_vec(),
            stderr: Vec::new(),
        },
    };
    let identity = ToolIdentity {
        backend: ToolBackend::Lean,
        path: "unused".into(),
        version: LEAN_VERSION.to_owned(),
        binary_hash: RustCryptoSha256::hash(b"tool"),
        runtime_hash: Some(RustCryptoSha256::hash(b"runtime")),
    };
    let old_record = formal_run_record(
        &identity,
        &old,
        &ToolRunStatus::KernelChecked,
        std::slice::from_ref(&transcript),
    )
    .unwrap_or_else(|_| unreachable!());
    let new_record = formal_run_record(
        &identity,
        &newer,
        &ToolRunStatus::KernelChecked,
        std::slice::from_ref(&transcript),
    )
    .unwrap_or_else(|_| unreachable!());
    assert_ne!(old_record, new_record);
    let record_hash =
        commitment::<RustCryptoSha256>(zeno_fcis_codec::domains::FORMAL_RUN, &new_record)
            .unwrap_or_else(|_| unreachable!());
    let run = ToolRun {
        identity,
        obligation: newer,
        status: ToolRunStatus::KernelChecked,
        transcripts: vec![transcript],
        toolchain_inventory: None,
        record_hash,
    };
    let root = std::env::temp_dir().join(format!(
        "zeno-lean-record-{}-{}",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let directory = retain_run(&root, &run).unwrap_or_else(|error| panic!("retain: {error:?}"));
    fs::write(directory.join("formal-run-record.bin"), old_record)
        .unwrap_or_else(|_| unreachable!());
    assert_eq!(retain_run(&root, &run), Err(ToolFailure::RetentionConflict));
    fs::remove_dir_all(root).unwrap_or_else(|_| unreachable!());
}
