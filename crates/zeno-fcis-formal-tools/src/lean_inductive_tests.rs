use super::*;
use spec_authoring::{ProjectLimits, SourceLimits, elaborate_project, parse_project};

#[test]
fn recursive_circuit_lean_rendering_keeps_shared_bindings() {
    use spec_authoring::{BoolCircuit, BoolNode};
    let circuit = BoolCircuit::try_new(
        vec![
            BoolNode::Literal(false),
            BoolNode::Not(0),
            BoolNode::Or(0, 1),
        ],
        2,
    )
    .unwrap_or_else(|| unreachable!());
    let formula = RelExpr::Circuit(Box::new(circuit));
    let rendered = render_relation(&formula, &mut SmtRenderBudget::new(ExportLimits::default()))
        .unwrap_or_else(|e| panic!("{e:?}"));
    assert!(rendered.term.contains("let circuit_0 : Prop := False"));
    assert!(rendered.term.contains("(circuit_0 ∨ circuit_1)"));
}

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

#[derive(Clone)]
enum AdmittedFact {
    Path(ProjectionPath),
    Domain(ProjectionPath, DeclaredDomain),
    Law(LawDecl, Option<u8>),
    DecisionKinds,
}

// This bounded decoder recognizes the exporter's top-level assumption grammar.
// It reads source bytes, not ExportedObligation.hypotheses. Unknown, duplicate,
// omitted or differently guarded terms cannot silently inherit metadata.
fn admitted_facts(
    obligation: &ExportedObligation,
    spec: &ProjectSpec,
    reference: &StepHypotheses,
) -> (StepHypotheses, bool) {
    let mut catalog = BTreeMap::new();
    let mut add = |term, fact| {
        assert!(catalog.insert(term, fact).is_none(), "ambiguous fact");
    };
    for path in reference.paths() {
        let term = projection(path).unwrap_or_else(|_| unreachable!());
        add(format!("inI128 {term}"), AdmittedFact::Path(path.clone()));
        if let Some(domain) = declared_domain(spec, path) {
            add(
                domain_formula(&term, &domain),
                AdmittedFact::Domain(path.clone(), domain),
            );
        }
    }
    add(
        "(kind = 1 ∨ kind = 2)".to_owned(),
        AdmittedFact::DecisionKinds,
    );
    for law in spec.laws() {
        let rendered = render_relation(
            law.formula(),
            &mut SmtRenderBudget::new(ExportLimits::default()),
        )
        .unwrap_or_else(|_| unreachable!());
        let holds = satisfied(&rendered);
        add(holds.clone(), AdmittedFact::Law(law.clone(), None));
        for kind in [1, 2] {
            add(
                format!("(kind = {kind} → {holds})"),
                AdmittedFact::Law(law.clone(), Some(kind)),
            );
        }
    }
    let source = std::str::from_utf8(obligation.source()).unwrap_or_else(|_| unreachable!());
    let (_, body) = source
        .split_once("def Admitted ")
        .unwrap_or_else(|| unreachable!());
    let (_, body) = body
        .split_once(": Prop :=\n  ")
        .unwrap_or_else(|| unreachable!());
    let (body, _) = body.split_once("\n\n").unwrap_or_else(|| unreachable!());
    let body = body
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or_else(|| panic!("expected a conjunction: {body}"));
    let mut depth = 0_u32;
    let mut start = 0;
    let mut terms = Vec::new();
    for (index, ch) in body.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.checked_sub(1).unwrap_or_else(|| unreachable!()),
            '∧' if depth == 0 => {
                terms.push(body[start..index].trim());
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    assert_eq!(depth, 0, "unbalanced assumptions");
    terms.push(body[start..].trim());
    let mut facts = StepHypotheses::default();
    let mut every_commit = Vec::new();
    let mut accepts = Vec::new();
    let mut failures = Vec::new();
    let mut split = false;
    for term in terms {
        match catalog
            .remove(term)
            .unwrap_or_else(|| panic!("unknown/duplicate emitted fact: {term}"))
        {
            AdmittedFact::Path(path) => {
                facts.paths.insert(path);
            }
            AdmittedFact::Domain(path, domain) => {
                facts.domains.insert(path, domain);
            }
            AdmittedFact::DecisionKinds => split = true,
            AdmittedFact::Law(law, None) => every_commit.push(law),
            AdmittedFact::Law(law, Some(1)) => accepts.push(law),
            AdmittedFact::Law(law, Some(2)) => failures.push(law),
            AdmittedFact::Law(_, Some(_)) => unreachable!(),
        }
    }
    facts.every_commit = every_commit.into_boxed_slice();
    facts.accepts = accepts.into_boxed_slice();
    facts.committed_failures = failures.into_boxed_slice();
    (facts, split)
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
        // Decode the actual Admitted body, independently of retained metadata.
        // Compare typed paths, domains and laws grouped by their emitted guard.
        let (facts, split) = admitted_facts(&lean, &spec, smt.hypotheses());
        assert_eq!(&facts, smt.hypotheses(), "{name}");
        assert!(split, "{name}: missing decision-kind alternatives");
        assert!(!lean.hypotheses().paths().is_empty(), "{name}");
        assert_eq!(lean.scope(), ObligationScope::InductiveStepOverLaws);
        assert!(source_is_bound(&lean));
        let text = String::from_utf8_lossy(lean.source());
        record_source(name, lean.source());
        assert!(text.contains("theorem history_holds (g : State) (hg : StateInv g)"));
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
    let int = || Box::new(ValueExpr::Int(1));
    for value in [
        ValueExpr::U128(u128::MAX),
        ValueExpr::ToI128(int()),
        ValueExpr::ToU128(int()),
        ValueExpr::MulDiv(spec_authoring::DivisionMode::Floor, int(), int(), int()),
        ValueExpr::SignedMulDivFloor(int(), int(), int()),
        ValueExpr::ScaledMulDiv(
            spec_authoring::DivisionMode::Floor,
            false,
            int(),
            int(),
            int(),
            1,
        ),
        ValueExpr::RepeatMulDiv(false, int(), int(), int(), 1),
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
        assert!(!is_constant(&value));
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
        timeout_ms: 300_000,
        max_output_bytes: 1024 * 1024,
        allowed_axioms: vec!["Quot.sound".to_owned(), "propext".to_owned()],
    }
}

fn run_fixture(name: &str, source: &str, expected: bool) {
    let obligation = exported_fixture(source);
    run_obligation(name, obligation, &pinned_config(), expected);
}

fn run_obligation(name: &str, obligation: ExportedObligation, config: &ToolConfig, expected: bool) {
    record_source(name, obligation.source());
    let started = Instant::now();
    let run = execute_tool(config, obligation).unwrap_or_else(|error| panic!("{name}: {error:?}"));
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
        "{name}: {:?} (adapter wall {:.3}s)\n{}",
        run.status(),
        started.elapsed().as_secs_f64(),
        String::from_utf8_lossy(run.stdout())
    );
    if run.status() != &ToolRunStatus::KernelChecked {
        assert!(matches!(
            run.status(),
            ToolRunStatus::Failed(ToolFailure::Crash(Some(1)))
        ));
        let stdout = String::from_utf8_lossy(run.stdout());
        let errors: Vec<_> = stdout
            .lines()
            .filter(|line| line.contains("error:"))
            .collect();
        assert!(!errors.is_empty(), "{name}: missing proof diagnostic");
        assert!(
            errors
                .iter()
                .all(|line| line.contains("omega could not prove the goal")),
            "{name}: unrelated declaration or compilation failure: {errors:?}"
        );
    }
    assert_eq!(
        run.status() == &ToolRunStatus::KernelChecked,
        expected,
        "{name}: {}",
        String::from_utf8_lossy(run.stdout())
    );
}

#[test]
#[ignore = "requires the workflow-pinned Lean distribution"]
fn pinned_lean_inductive_pilots() {
    for (name, source) in PILOTS {
        run_fixture(name, source, true);
    }
}

#[test]
#[ignore = "requires the workflow-pinned Lean distribution"]
fn pinned_lean_constructive_symbolic_fragment() {
    // Unbounded i128 fields exercise symbolic proofs, not enumeration. Each law
    // gives a different arithmetic/logical route to its post-state invariant.
    let cases = [
        (
            "linear_order",
            "post.100.110 == pre.100.110 + context.102 && post.100.111 == pre.100.111 + context.102",
            "pre.100.110 <= pre.100.111",
        ),
        (
            "negative_scale",
            "post.100.110 == (0 - 3) * pre.100.111 && post.100.111 == pre.100.110 * (0 - 3)",
            "pre.100.110 <= pre.100.111",
        ),
        (
            "nested_logic",
            "!(post.100.110 <= 0 && post.100.111 <= 0) && !(post.100.110 == post.100.111)",
            "(pre.100.110 > 0 || pre.100.111 > 0) && pre.100.110 != pre.100.111",
        ),
        (
            "negated_implication",
            "!(post.100.110 >= 0 -> post.100.111 < 0)",
            "!(pre.100.110 < 0) && pre.100.111 >= 0",
        ),
        (
            "implication_goal",
            "post.100.110 < 0 || post.100.111 >= 0",
            "pre.100.110 >= 0 -> pre.100.111 >= 0",
        ),
        (
            "integer_rounding",
            "post.100.110 * 2 >= 1 && post.100.111 * 3 < 6",
            "pre.100.110 >= 1 && pre.100.111 <= 1",
        ),
        ("constant_true", "true", "true"),
        ("constant_false", "true", "false"),
    ];
    for (name, law, invariant) in cases {
        let source = CONTROLS[2]
            .1
            .replace("Bounded in 0..=3;", "Bounded;")
            .replace("post.100.110 == pre.100.110 + 1", law)
            .replace("pre.100.110 <= 3", invariant);
        let obligation = exported_fixture(&source);
        let mut config = pinned_config();
        if name.starts_with("constant_") {
            config.allowed_axioms = vec!["propext".to_owned()];
        }
        run_obligation(name, obligation, &config, true);
    }
}

#[test]
#[ignore = "requires the workflow-pinned Lean distribution"]
fn pinned_lean_classical_proof_is_refused_by_name() {
    let mut obligation = exported_fixture(PILOTS[0].1);
    let original = String::from_utf8(obligation.source.clone()).unwrap_or_else(|_| unreachable!());
    let mut source = original.replace(
        "\ntheorem history_holds (",
        "\ntheorem constructive_history_holds (",
    );
    source = source.replace("#print axioms history_holds", "");
    source.push_str(
        r#"
theorem history_holds (g : State) (hg : StateInv g) :
  ∀ (h : List State), Chain Step g h → ∀ s ∈ h, StateInv s := by
  rcases Classical.em (StateInv g) with yes | no
  · exact constructive_history_holds g yes
  · exact False.elim (no hg)

#print axioms history_holds
"#,
    );
    obligation.source = source.into_bytes();
    obligation.source_hash =
        commitment::<RustCryptoSha256>(zeno_fcis_codec::domains::FORMAL_SOURCE, &obligation.source)
            .unwrap_or_else(|_| unreachable!());
    assert!(source_is_bound(&obligation));
    record_source("classical_control", obligation.source());
    // The manifest explicitly allows choice: the unconditional ceiling must
    // still refuse the successfully compiled theorem and name its dependency.
    let mut config = pinned_config();
    config
        .allowed_axioms
        .insert(0, "Classical.choice".to_owned());
    let run = execute_tool(&config, obligation)
        .unwrap_or_else(|error| panic!("classical control: {error:?}"));
    let stdout = String::from_utf8_lossy(run.stdout());
    println!("classical_control: {:?}\n{stdout}", run.status());
    if let Some(directory) = std::env::var_os("ZENO_FCIS_LEAN_EVIDENCE") {
        fs::write(
            PathBuf::from(directory).join("classical_control.stdout"),
            run.stdout(),
        )
        .unwrap_or_else(|error| panic!("classical control evidence: {error}"));
    }
    assert!(!stdout.contains("error:"), "{stdout}");
    assert!(stdout.contains("Classical.choice"), "{stdout}");
    assert_eq!(
        run.status(),
        &ToolRunStatus::Blocked(ToolFailure::DisallowedLeanAxiom(
            "Classical.choice".to_owned()
        ))
    );
}

static POSITIVE_BASELINE: std::sync::Once = std::sync::Once::new();

macro_rules! control {
    ($test:ident, $index:literal) => {
        #[test]
        #[ignore = "requires the workflow-pinned Lean distribution"]
        fn $test() {
            // Every isolated negative run first establishes a passing source,
            // runtime and axiom-admission baseline using the positive control.
            let (name, source, checked) = CONTROLS[$index];
            if !checked {
                POSITIVE_BASELINE.call_once(|| run_fixture("positive_baseline", PILOTS[0].1, true));
            }
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
            "example : {}({proposition}) := by\n  try unfold inI128\n  all_goals decide\n",
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
    // Even an explicitly widened manifest cannot admit a forbidden dependency.
    // Check names rather than a blacklist: an unknown future axiom is refused.
    for name in ["Classical.choice", "Lean.ofReduceBool", "unreviewed_axiom"] {
        config.allowed_axioms = vec![name.to_owned()];
        assert_eq!(
            classify(&config, &output(THEOREM, name), &obligation),
            ToolRunStatus::Blocked(ToolFailure::DisallowedLeanAxiom(name.to_owned()))
        );
    }
    // The fixed ceiling does not replace the manifest's exact admission rule.
    for axioms in [vec![], vec!["propext"], vec!["Quot.sound", "propext"]] {
        config.allowed_axioms = axioms.iter().map(|name| (*name).to_owned()).collect();
        assert_eq!(
            classify(&config, &output(THEOREM, &axioms.join(", ")), &obligation),
            ToolRunStatus::KernelChecked
        );
    }
    config.allowed_axioms = vec![];
    assert_eq!(
        classify(&config, &valid, &obligation),
        ToolRunStatus::Blocked(ToolFailure::LeanAxiomReport)
    );
    config.allowed_axioms = vec!["Quot.sound".to_owned(), "propext".to_owned()];
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
