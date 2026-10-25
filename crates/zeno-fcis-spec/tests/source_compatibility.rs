//! Cross-cutting source boundaries, including intentional parser repairs.

use zeno_fcis_spec::{
    DiagnosticCode, LawApplicability, LawScope, ProjectLimits, SourceLimits, elaborate_project,
    expand_tables, parse_formula, parse_law_formula, parse_project, parse_table_expression,
};

const HEADER: &str = "zeno 1; project 1 checks;";

fn limits(bytes: usize, tokens: usize, diagnostics: usize) -> SourceLimits {
    SourceLimits::try_new(bytes, tokens, diagnostics)
        .unwrap_or_else(|| panic!("test limits must be valid"))
}

#[test]
fn source_exact_token_budget_ignores_final_trivia() {
    for trivia in ["", " ", "\n", "\t \r\n", "// trailing comment\n"] {
        let source = format!("{HEADER}{trivia}");
        assert!(
            parse_project(&source, limits(4096, 7, 8)).is_ok(),
            "trivia {trivia:?} must not spend a token"
        );
    }
    assert!(parse_project(&format!("{HEADER} extra"), limits(4096, 7, 8)).is_err());
}

#[test]
fn source_projection_boundary_retains_every_segment() {
    let at_limit = format!("{HEADER} law 2 path = pre{} == 0;", ".1".repeat(64));
    assert!(parse_project(&at_limit, SourceLimits::default()).is_ok());
    let over_limit = format!("{HEADER} law 2 path = pre{} == 0;", ".1".repeat(65));
    assert!(parse_project(&over_limit, SourceLimits::default()).is_err());

    // A dropped suffix must not become an apparently valid bound variable.
    let shadowed = format!(
        "{HEADER} law 2 path = forall pre in 0..1 {{ pre{} == 0 }};",
        ".1".repeat(65)
    );
    assert!(parse_project(&shadowed, SourceLimits::default()).is_err());
}

#[test]
fn source_invalid_bytes_keep_bounded_and_honest_diagnostics() {
    let source = "@".repeat(2000);
    let diagnostics = parse_project(&source, limits(4096, 128, 3))
        .err()
        .unwrap_or_else(|| panic!("invalid characters must refuse"));
    assert_eq!(diagnostics.len(), 3);
    assert!(diagnostics.is_truncated());
    assert!(
        diagnostics
            .diagnostics()
            .iter()
            .all(|diagnostic| diagnostic.code() == DiagnosticCode::UnexpectedCharacter)
    );
}

#[test]
fn source_malformed_tail_never_returns_a_recovered_project() {
    let mut valid = HEADER.to_owned();
    for id in 2..1002 {
        valid.push_str(&format!(" type {id} data item_{id};"));
    }
    assert!(parse_project(&valid, SourceLimits::default()).is_ok());
    for tail in [
        " @",
        " law 2000 broken = (1 + );",
        " type 2000 data broken",
        " law",
    ] {
        assert!(parse_project(&(valid.clone() + tail), SourceLimits::default()).is_err());
    }
}

#[test]
fn source_byte_and_token_limits_refuse_independently() {
    let byte_error = parse_project(HEADER, limits(HEADER.len() - 1, 128, 8))
        .err()
        .unwrap_or_else(|| panic!("byte budget must refuse"));
    assert!(
        byte_error
            .diagnostics()
            .iter()
            .any(|d| d.code() == DiagnosticCode::SourceTooLarge)
    );
    let token_error = parse_project(HEADER, limits(4096, 6, 8))
        .err()
        .unwrap_or_else(|| panic!("token budget must refuse"));
    assert!(
        token_error
            .diagnostics()
            .iter()
            .any(|d| d.code() == DiagnosticCode::TokenLimit)
    );
}

#[test]
fn source_recursive_expansion_refuses_declared_node_budget_overflow() {
    let source = format!(
        "{HEADER} type 100 state S; type 101 int I in 0..=63; \
         table 200 100 items 101 size 64; \
         derived table 700 reach[i][j] in table 200 rounds 4096 {{ \
         when true then i == j; default false; }};"
    );
    assert!(parse_project(&source, SourceLimits::default()).is_err());
    assert!(expand_tables(&source, SourceLimits::default()).is_err());
}

#[test]
fn source_named_entry_height_uses_actual_project_and_ambient_counts() {
    let mut source = format!(
        "{HEADER} type 100 state S; type 101 command C; type 102 context X; \
         type 103 key K; type 104 data V; type 105 bool B; field 140 104 value 105; \
         map 110 100 items 103 104 capacity 2048;"
    );
    for index in 0..300 {
        source.push_str(&format!(
            "field {} 101 key_{index} 103; entry {} entry_{index} of 110 at command.101.{};",
            10000 + index,
            20000 + 2 * index,
            10000 + index,
        ));
    }
    // The surrounding project is valid; refusal must come from expansion
    // height, not an unrelated project or declaration error.
    let parsed = parse_project(&source, SourceLimits::default())
        .unwrap_or_else(|errors| panic!("unexpanded collection must parse: {errors:?}"));
    assert!(elaborate_project(parsed, ProjectLimits::default()).is_ok());
    let formula = "forall e in entries 110 { post.100.e == 1 }";
    let with_law = format!("{source} law 900 all = {formula};");
    let outcomes = [
        parse_project(&with_law, SourceLimits::default()).map(|_| ()),
        expand_tables(&with_law, SourceLimits::default()).map(|_| ()),
        parse_formula(&source, formula, SourceLimits::default()).map(|_| ()),
        parse_law_formula(
            &source,
            formula,
            LawApplicability::new(LawScope::Always, true),
            SourceLimits::default(),
        )
        .map(|_| ()),
        parse_table_expression(&source, formula, SourceLimits::default()).map(|_| ()),
    ];
    for outcome in outcomes {
        let diagnostics = outcome
            .err()
            .unwrap_or_else(|| panic!("named-entry expansion must refuse before construction"));
        assert!(
            diagnostics
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code() == DiagnosticCode::ResourceLimit)
        );
    }
}

#[test]
fn source_bit_expansion_height_includes_packing_and_extraction_layers() {
    let project = "zeno 1; project 1 checks; type 2 state State; \
                   type 3 u128 Word in 0..=255; field 4 2 value 3;";
    for operation in [
        "complement(8, pre.2.4)",
        "join(8, pre.2.4, pre.2.4)",
        "meet(8, pre.2.4, pre.2.4)",
    ] {
        // The source operator chain fits, but the elaborated bit expression
        // adds comparison, extraction, Select and balanced packing layers.
        let expression = format!("{operation}{} == u128(0)", " + u128(0)".repeat(255));
        let with_law = format!("{project} law 900 deep = {expression};");
        for outcome in [
            parse_project(&with_law, SourceLimits::default()).map(|_| ()),
            parse_table_expression(project, &expression, SourceLimits::default()).map(|_| ()),
        ] {
            let diagnostics = outcome
                .err()
                .unwrap_or_else(|| panic!("bit-expanded expression must refuse: {operation}"));
            assert!(
                diagnostics
                    .diagnostics()
                    .iter()
                    .any(|diagnostic| diagnostic.code() == DiagnosticCode::ResourceLimit)
            );
        }
    }
}

#[test]
fn source_circuit_wrapper_is_included_in_composed_height() {
    let arithmetic = vec!["1"; 255].join(" + ");
    let expression =
        format!("choose(circuit {{ formula {{ {arithmetic} == 1 }}; }} output 0, 1, 0) == 1");
    let with_law = format!("{HEADER} law 2 deep = {expression};");
    for outcome in [
        parse_project(&with_law, SourceLimits::default()).map(|_| ()),
        parse_table_expression(HEADER, &expression, SourceLimits::default()).map(|_| ()),
    ] {
        let diagnostics = outcome
            .err()
            .unwrap_or_else(|| panic!("circuit wrapper must count towards composed height"));
        assert!(
            diagnostics
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code() == DiagnosticCode::ResourceLimit)
        );
    }
}

#[test]
fn source_derived_reads_compose_definition_and_consumer_heights() {
    let arithmetic = format!("pre.100.120{}", " + 0".repeat(180));
    let project = format!(
        "{HEADER} type 100 state S; type 101 int I in 0..=1; field 120 100 value 101; \
         table 200 100 items 101 size 1; \
         derived table 700 words[i] in table 200 bits 1 rounds 1 {{ \
         when {arithmetic} == 0 then u128(1); default u128(0); }};"
    );
    assert!(parse_project(&project, SourceLimits::default()).is_ok());
    let expression = format!("words[0]{} == u128(0)", " + u128(0)".repeat(100));
    let with_law = format!("{project} law 900 deep = {expression};");
    for outcome in [
        parse_project(&with_law, SourceLimits::default()).map(|_| ()),
        parse_table_expression(&project, &expression, SourceLimits::default()).map(|_| ()),
    ] {
        let diagnostics = outcome
            .err()
            .unwrap_or_else(|| panic!("derived-definition height must compose with its consumer"));
        assert!(
            diagnostics
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code() == DiagnosticCode::ResourceLimit)
        );
    }
}
