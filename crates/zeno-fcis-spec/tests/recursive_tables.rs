//! Requirement-linked recursive table controls and expansion checks.

use zeno_fcis_spec::{
    CompareOp, ProjectLimits, RelExpr, SourceLimits, ValueExpr, elaborate_project, expand_tables,
    parse_project,
};

const HEADER: &str = "zeno 1; project 1 recursive; type 100 state S; type 101 command C; type 102 context X; type 105 int V in 0..=3; table 110 100 parent 105 size 4;";

fn laws(source: &str) -> Vec<RelExpr> {
    elaborate_project(
        parse_project(source, SourceLimits::default())
            .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}")),
        ProjectLimits::default(),
    )
    .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}"))
    .laws()
    .iter()
    .map(|l| l.formula().clone())
    .collect()
}

fn holds(f: &RelExpr, fields: &[i128]) -> bool {
    fn value(v: &ValueExpr, fields: &[i128]) -> i128 {
        match v {
            ValueExpr::Int(i) => *i,
            ValueExpr::Add(a, b) => value(a, fields) + value(b, fields),
            ValueExpr::Sub(a, b) => value(a, fields) - value(b, fields),
            ValueExpr::Mul(a, b) => value(a, fields) * value(b, fields),
            ValueExpr::Projection(p) => {
                let id = p
                    .segments()
                    .last()
                    .unwrap_or_else(|| panic!("projection must have a field"))
                    .get();
                fields[(id - if id >= 210 { 210 } else { 110 }) as usize]
            }
            _ => panic!("unexpected value {v:?}"),
        }
    }
    match f {
        RelExpr::Circuit(c) => c
            .evaluate(|f| Some(holds(f, fields)))
            .unwrap_or_else(|| panic!("circuit reference must evaluate")),
        RelExpr::Bool(v) => *v,
        RelExpr::Not(f) => !holds(f, fields),
        RelExpr::And(a, b) => holds(a, fields) & holds(b, fields),
        RelExpr::Or(a, b) => holds(a, fields) | holds(b, fields),
        RelExpr::Implies(a, b) => !holds(a, fields) | holds(b, fields),
        RelExpr::Compare(op, a, b) => {
            let (a, b) = (value(a, fields), value(b, fields));
            match op {
                CompareOp::Eq => a == b,
                CompareOp::NotEq => a != b,
                CompareOp::Less => a < b,
                CompareOp::LessEq => a <= b,
                CompareOp::Greater => a > b,
                CompareOp::GreaterEq => a >= b,
            }
        }
        _ => panic!("unexpected formula {f:?}"),
    }
}

#[test]
fn least_fixed_point_starts_false() {
    let s = "zeno 1; project 1 empty; type 100 state S; type 105 int V in 0..=1; table 110 100 indices 105 size 2; table 210 100 edge 105 size 4;
    derived table 700 reach[i][j] in table 110 rounds 4 {
      when true then (exists e in table 210 where e == i * 2 + j { pre.100.210[e] == 1 }) || exists k in table 110 { reach[i][k] && reach[k][j] };
      default false;
    };
    law 500 empty = !reach[0][0] && !reach[0][1] && !reach[1][0] && !reach[1][1];";
    assert!(
        holds(&laws(s)[0], &[0, 0, 0, 0]),
        "empty two-node relation has no reachable pair"
    );
}

#[test]
fn final_propagation_round_reaches_four_chain_end() {
    let s = format!(
        "{HEADER} derived table 700 reaches_end[i] in table 110 rounds 4 {{
      when true then i == 3 || exists j in table 110 {{ pre.100.110[i] == j && reaches_end[j] }};
      default false;
    }}; law 500 end_to_end = reaches_end[0];"
    );
    assert!(
        holds(&laws(&s)[0], &[1, 2, 3, 3]),
        "four-chain end-to-end reach needs the fourth propagation round"
    );
}

#[test]
fn same_stratum_negation_is_refused() {
    let s = format!(
        "{HEADER} derived table 700 x[i] in table 110 rounds 4 {{ when true then !x[i]; default false; }};"
    );
    assert!(
        parse_project(&s, SourceLimits::default()).is_err(),
        "same-stratum negation must be refused"
    );
}

#[test]
fn self_reading_guard_is_refused() {
    let s = format!(
        "{HEADER} derived table 700 x[i] in table 110 rounds 4 {{ when x[i] then true; default false; }};"
    );
    assert!(
        parse_project(&s, SourceLimits::default()).is_err(),
        "a self-reading guard must be refused"
    );
}

#[test]
fn unsafe_forms_and_later_strata_are_refused() {
    for rule in [
        "!x[i]",
        "__recursive_entry(0)",
        "x[i] == 1",
        "count j in table 110 { x[j] } >= 1",
        "later[i]",
        "x[i] -> true",
        "forall j in 0..4 { x[i] }",
    ] {
        let s = format!(
            "{HEADER} derived table 700 x[i] in table 110 rounds 4 {{ when true then {rule}; default false; }};"
        );
        assert!(
            parse_project(&s, SourceLimits::default()).is_err(),
            "unsafe rule accepted: {rule}"
        );
    }
}

#[test]
fn lower_negation_prior_strata_and_expansion_are_exact() {
    let s = format!("{HEADER} derived table 700 x[i] in table 110 rounds 4 {{ when !(pre.100.110[i] == 0) then true; default x[i]; }};
    derived table 701 y[i] in table 110 rounds 4 {{ when !x[i] then true; default false; }};
    law 500 reads = forall i in table 110 {{ !y[i] || !x[i] }};");
    let expanded = expand_tables(&s, SourceLimits::default())
        .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}"));
    assert_eq!(laws(&s), laws(&expanded.text));
    assert!(holds(&laws(&s)[0], &[0, 1, 2, 3]));
}

#[test]
fn circuit_forward_references_are_refused() {
    let s = format!("{HEADER} law 500 invalid = circuit {{ and 0 0; }} output 0;");
    assert!(parse_project(&s, SourceLimits::default()).is_err());
}

#[test]
fn constant_recursive_rules_fold_once_and_rows_keep_priority() {
    let source = format!(
        "{HEADER} derived table 700 constant[i] in table 110 rounds 4 {{
        when i == 0 then true;
        default exists j in table 110 {{ i == j + 1 && constant[j] }};
    }}; law 500 end = constant[3];"
    );
    assert_eq!(laws(&source), vec![RelExpr::Bool(true)]);
    let expanded = expand_tables(&source, SourceLimits::default())
        .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}"));
    assert!(!expanded.text.contains("circuit"));
    assert_eq!(laws(&expanded.text), laws(&source));

    let source = format!(
        "{HEADER} derived table 700 priority[i] in table 110 rounds 4 {{
        when pre.100.110[0] == 0 then priority[i];
        when true then true;
        default false;
    }}; law 500 selected = priority[0];"
    );
    let law = &laws(&source)[0];
    assert!(!holds(law, &[0, 0, 0, 0]));
    assert!(holds(law, &[1, 0, 0, 0]));
}

#[test]
fn project_bound_expression_parser_consumes_exactly_one_formula() {
    use zeno_fcis_spec::parse_table_expression;
    let source = format!(
        "{HEADER} derived table 700 x[i] in table 110 rounds 4 {{ when true then pre.100.110[i] == 1 || x[i]; default false; }};"
    );
    let formula = parse_table_expression(&source, "x[0] || !x[1]", SourceLimits::default())
        .unwrap_or_else(|error| panic!("recursive fixture failed: {error:?}"));
    assert_eq!(
        formula,
        laws(&format!("{source} law 500 query = x[0] || !x[1];"))[0]
    );
    assert!(
        parse_table_expression(
            &source,
            "x[0]; law 501 injected = true;",
            SourceLimits::default()
        )
        .is_err()
    );
    assert!(parse_table_expression(&source, "later[0]", SourceLimits::default()).is_err());
}

#[test]
fn derived_ids_are_disjoint_from_all_declarations() {
    for extra in [
        "type 700 bool Duplicate;",
        "law 700 duplicate = true;",
        "derived table 700 y[i] in table 110 rounds 4 { when true then false; default false; };",
    ] {
        let source = format!(
            "{HEADER} derived table 700 x[i] in table 110 rounds 4 {{ when true then false; default false; }}; {extra}"
        );
        assert!(parse_project(&source, SourceLimits::default()).is_err());
    }
}

#[test]
fn self_counts_are_refused_before_constant_folding() -> Result<(), String> {
    for condition in [
        "circuit { formula { x[j] }; } output 0",
        "circuit { formula { !x[j] }; } output 0",
        "circuit { true; formula { x[j] }; } output 0",
    ] {
        let source = format!(
            "{HEADER} derived table 700 x[i] in table 110 rounds 4 {{
            when true then count j in table 110 {{ {condition} }} >= 0;
            default false;
        }}; law 500 query = x[0];"
        );
        let errors = parse_project(&source, SourceLimits::default())
            .err()
            .ok_or("a constant count must not hide a forbidden self reference")?;
        assert!(errors.to_string().contains("count"), "{errors}");
    }
    Ok(())
}

#[test]
fn guard_checks_precede_folding_and_leave_positive_values_eligible() -> Result<(), String> {
    let source = format!(
        "{HEADER} derived table 700 x[i] in table 110 rounds 4 {{
        when count j in table 110 {{ circuit {{ formula {{ x[j] }}; }} output 0 }} >= 0 then true;
        default false;
    }};"
    );
    let errors = parse_project(&source, SourceLimits::default())
        .err()
        .ok_or("self-reading guard")?;
    assert!(errors.to_string().contains("guard"), "{errors}");
    let source = format!(
        "{HEADER} derived table 700 x[i] in table 110 rounds 4 {{
        when true then exists j in table 110 where false {{ x[j] }};
        default x[i];
    }};"
    );
    assert!(
        parse_project(&source, SourceLimits::default()).is_ok(),
        "a positive body remains allowed"
    );
    Ok(())
}

#[test]
fn value_table_forms_cannot_hide_recursive_self_reads() {
    for formula in [
        "choose(x[i], 0, 1) == 1",
        "count j in table 110 { x[j] } <= pre.100.110[i]",
    ] {
        let source = format!(
            "{HEADER} derived table 700 x[i] in table 110 rounds 4 {{ when true then {formula}; default false; }};"
        );
        let refused = parse_project(&source, SourceLimits::default())
            .err()
            .unwrap_or_else(|| panic!("self-read must be refused: {formula}"));
        assert!(
            refused
                .diagnostics()
                .iter()
                .any(|d| d.path().as_str() == "derived.table"),
            "{formula}: {refused:?}"
        );
    }
}
