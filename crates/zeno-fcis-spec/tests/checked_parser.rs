//! Acceptance tests at the text-to-AST boundary. These tests do not promote
//! syntax-derivation proofs into an unproved semantic-lowering theorem.

use zeno_fcis_spec::{
    CompareOp, DiagnosticCode, ProjectLimits, RelExpr, SourceLimits, ValueExpr, elaborate_project,
    parse_project,
};

fn law(expression: &str) -> String {
    format!("zeno 1; project 1 p; law 1 l = {expression};")
}

#[test]
fn projection_suffix_cannot_disappear_into_a_bound_variable() {
    for count in [65, 66, 128] {
        let suffix = std::iter::repeat_n("1", count)
            .collect::<Vec<_>>()
            .join(".");
        let source = law(&format!("forall pre in 0..1 {{ pre.{suffix} == 0 }}"));
        assert!(
            parse_project(&source, SourceLimits::default()).is_err(),
            "{count} segments"
        );
    }
    let sixty_four = std::iter::repeat_n("1", 64).collect::<Vec<_>>().join(".");
    assert!(
        parse_project(
            &law(&format!("pre.{sixty_four} == 0")),
            SourceLimits::default()
        )
        .is_ok()
    );
    let variable = parse_project(
        &law("forall pre in 0..1 { pre == 0 }"),
        SourceLimits::default(),
    )
    .unwrap_or_else(|e| panic!("valid variable: {e}"));
    assert!(elaborate_project(variable, ProjectLimits::default()).is_ok());
}

#[test]
fn trailing_trivia_does_not_spend_a_token_at_the_exact_budget() {
    let limits = SourceLimits::try_new(1024, 7, 256).unwrap_or_else(|| unreachable!());
    for tail in [
        "",
        " ",
        "\n",
        "\t\r\n",
        "// comment",
        " // Unicode comment λ\n ",
    ] {
        let source = format!("zeno 1; project 1 p;{tail}");
        assert!(parse_project(&source, limits).is_ok(), "trailing {tail:?}");
    }
    let error = parse_project("zeno 1; project 1 p; namespace 2 n;", limits)
        .err()
        .unwrap_or_else(|| panic!("additional real tokens were accepted"));
    assert!(
        error
            .diagnostics()
            .iter()
            .any(|d| d.code() == DiagnosticCode::TokenLimit)
    );
}

#[test]
fn arithmetic_precedence_and_left_association_reach_the_typed_ast() {
    let source = law("1 + 2 * 3 - 4 - 5 == -2");
    let parsed = parse_project(&source, SourceLimits::default()).unwrap_or_else(|e| panic!("{e}"));
    let project =
        elaborate_project(parsed, ProjectLimits::default()).unwrap_or_else(|e| panic!("{e}"));
    let b = Box::new;
    assert_eq!(
        project.laws()[0].formula(),
        &RelExpr::Compare(
            CompareOp::Eq,
            ValueExpr::Sub(
                b(ValueExpr::Sub(
                    b(ValueExpr::Add(
                        b(ValueExpr::Int(1)),
                        b(ValueExpr::Mul(b(ValueExpr::Int(2)), b(ValueExpr::Int(3))))
                    )),
                    b(ValueExpr::Int(4))
                )),
                b(ValueExpr::Int(5))
            ),
            ValueExpr::Sub(b(ValueExpr::Int(0)), b(ValueExpr::Int(2))),
        )
    );
}

#[test]
fn numeric_payload_constraints_are_checked_after_syntax_recognition() {
    for source in [
        "zeno 65537; project 1 p;".to_owned(),
        "zeno 1; project 0 p;".to_owned(),
        "zeno 1; project 4294967296 p;".to_owned(),
        law("170141183460469231731687303715884105728 == 0"),
        law("-170141183460469231731687303715884105729 == 0"),
        law("0 == -(170141183460469231731687303715884105728)"),
        law("host_rule(170141183460469231731687303715884105728)"),
        law("0 == div_exact(170141183460469231731687303715884105728,1)"),
        law("0 == 170141183460469231731687303715884105728 + 0"),
        law("repeat_mul_div_floor(1,2,3,17) == 0"),
        law("mul_div_floor(1,2,3,e39) == 0"),
        "zeno 1; project 1 p; type 2 int n unit 2 scale e0;".to_owned(),
        "zeno 1; project 1 p; type 2 bool n in 0..=1;".to_owned(),
        "zeno 1; project 1 p; type 2 int n in 2..=1;".to_owned(),
        "zeno 1; project 1 p; component 2 c { budget steps 0; }".to_owned(),
    ] {
        assert!(
            parse_project(&source, SourceLimits::default()).is_err(),
            "{source}"
        );
    }
    assert!(
        parse_project(
            &law("-170141183460469231731687303715884105728 == 0"),
            SourceLimits::default()
        )
        .is_ok()
    );
    let too_long_scale = format!("e{}", "0".repeat(128));
    assert!(
        parse_project(
            &law(&format!("mul_div_floor(1,2,3,{too_long_scale}) == 0")),
            SourceLimits::default(),
        )
        .is_err()
    );
}

#[test]
fn recovered_placeholders_never_become_successful_projects() {
    for source in [
        "zeno 1; project 1 p; law 1 l = 1 == ;",
        "zeno 1; project 1 p; law 1 l = true; trailing",
        "zeno 1; project 1 p; component 2 c { owns 3;",
        "zeno 1; project 1 p; type 2 imaginary n;",
        "zeno 1; project 1 p; claim 1 c all inductive = true;",
        "zeno 1; project 1 p; law 1 l = true; $",
    ] {
        assert!(
            parse_project(source, SourceLimits::default()).is_err(),
            "{source}"
        );
    }
}

#[test]
fn operator_chain_admission_is_preserved_before_building_deep_ast() {
    for operands in [255, 256] {
        let expression = std::iter::repeat_n("true", operands)
            .collect::<Vec<_>>()
            .join(" && ");
        assert!(parse_project(&law(&expression), SourceLimits::default()).is_ok());
    }
    let too_long = std::iter::repeat_n("true", 257)
        .collect::<Vec<_>>()
        .join(" && ");
    assert!(parse_project(&law(&too_long), SourceLimits::default()).is_err());
}

#[test]
fn nested_constructs_preserve_the_bound_without_native_stack_growth() {
    let limit = zeno_fcis_spec::MAX_FORMULA_DEPTH;
    let temporal =
        |formula: &str| format!("zeno 1; project 1 p; claim 1 c z3 finite 1 = {formula};");
    for depth in [limit, limit + 1] {
        let cases = [
            ("not", law(&format!("{}true", "not ".repeat(depth)))),
            (
                "relational group",
                law(&format!("{}true{}", "(".repeat(depth), ")".repeat(depth))),
            ),
            (
                "implication",
                law(&format!("{}true", "true -> ".repeat(depth))),
            ),
            (
                "scalar group",
                law(&format!("0 == {}1{}", "(".repeat(depth), ")".repeat(depth))),
            ),
            (
                "scalar minus",
                law(&format!("0 == {}1", "- ".repeat(depth))),
            ),
            // The final minus is part of the representable minimum literal,
            // rather than an extra unary-expression nesting level.
            (
                "minimum magnitude",
                law(&format!(
                    "0 == {}170141183460469231731687303715884105728",
                    "- ".repeat(depth + 1)
                )),
            ),
            (
                "division",
                law(&format!(
                    "0 == {}1{}",
                    "div_exact(".repeat(depth),
                    ",1)".repeat(depth)
                )),
            ),
            (
                "fused",
                law(&format!(
                    "0 == {}1{}",
                    "mul_div_floor(".repeat(depth),
                    ",1,1)".repeat(depth)
                )),
            ),
            (
                "sum",
                law(&format!(
                    "0 == {}1{}",
                    "sum x in 0..1 { ".repeat(depth),
                    " }".repeat(depth)
                )),
            ),
            (
                "quantifier",
                law(&format!(
                    "{}true{}",
                    "forall x in 0..1 { ".repeat(depth),
                    " }".repeat(depth)
                )),
            ),
            (
                "temporal prefix",
                temporal(&format!("{}true", "next ".repeat(depth))),
            ),
            (
                "temporal group",
                temporal(&format!("{}true{}", "(".repeat(depth), ")".repeat(depth))),
            ),
            // Entering atom(Rel) itself consumes one existing nesting level.
            (
                "temporal relational atom",
                temporal(&format!("atom({}true)", "not ".repeat(depth - 1))),
            ),
        ];
        for (shape, source) in cases {
            let result = parse_project(&source, SourceLimits::default());
            if depth == limit {
                assert!(result.is_ok(), "{shape} at the nesting limit");
            } else {
                let error = result
                    .err()
                    .unwrap_or_else(|| panic!("{shape} above the nesting limit"));
                assert!(
                    error
                        .diagnostics()
                        .iter()
                        .any(|d| d.code() == DiagnosticCode::ResourceLimit),
                    "{shape} must report the resource limit"
                );
            }
        }
    }
}

#[test]
fn contextual_keywords_and_identifier_hyphens_preserve_lexical_meaning() {
    let source = "zeno 1; project 1 project; namespace 2 namespace; law 1 l = forall a-b in 0..1 { a-b == 0 };";
    let parsed = parse_project(source, SourceLimits::default()).unwrap_or_else(|e| panic!("{e}"));
    let project =
        elaborate_project(parsed, ProjectLimits::default()).unwrap_or_else(|e| panic!("{e}"));
    let RelExpr::ForAll { variable, body, .. } = project.laws()[0].formula() else {
        panic!("quantifier missing");
    };
    assert_eq!(variable.as_str(), "a-b");
    assert!(
        matches!(body.as_ref(), RelExpr::Compare(CompareOp::Eq, ValueExpr::Var(name), ValueExpr::Int(0)) if name.as_str() == "a-b")
    );
}

#[test]
fn composed_expression_height_is_bounded_before_ast_construction() {
    // Each individual group and chain is small; their composition exceeds the
    // actual AST-height budget. No excessive recursive AST should be built.
    let mut expression = String::from("true");
    for _ in 0..16 {
        expression = format!("({expression}{})", " && true".repeat(32));
    }
    let source = format!("zeno 1; project 1 p; law 1 l = {expression};");
    let errors = parse_project(&source, SourceLimits::default())
        .err()
        .unwrap_or_else(|| panic!("composed height must be refused"));
    assert!(
        errors
            .diagnostics()
            .iter()
            .any(|d| d.code() == DiagnosticCode::ResourceLimit)
    );
    // Refused trailing syntax must not enter an AST-producing recovery path.
    assert!(parse_project(&(source + " trailing"), SourceLimits::default()).is_err());
}

#[test]
fn units_and_scales_remain_part_of_the_semantic_result() {
    let source = "zeno 1; project 1 p; type 2 int amount unit m/s*m/1 scale e02;";
    let parsed = parse_project(source, SourceLimits::default()).unwrap_or_else(|e| panic!("{e}"));
    let project =
        elaborate_project(parsed, ProjectLimits::default()).unwrap_or_else(|e| panic!("{e}"));
    let quantity = project.types()[0]
        .quantity()
        .unwrap_or_else(|| panic!("quantity lost"));
    assert_eq!(quantity.scale(), 2);
    assert_eq!(
        quantity
            .factors()
            .iter()
            .map(|(name, power)| (name.as_str(), *power))
            .collect::<Vec<_>>(),
        vec![("m", 2), ("s", -1)]
    );
}
