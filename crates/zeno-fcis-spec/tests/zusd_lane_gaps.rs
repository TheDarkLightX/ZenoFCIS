//! Preserved original inert zUSD authoring controls. Execution regressions
//! and their exact full-width formulas live in the private original oracle.
use zeno_fcis_spec::{
    DiagnosticCode, ProjectLimits, ProjectSpec, SourceLimits, StableId, elaborate_project,
    parse_project,
};
const LANE: &str = include_str!("data/zusd_lane_v1.zeno");
const STATE_FIELDS: usize = 32;

fn elaborate(source: &str) -> ProjectSpec {
    let parsed = parse_project(source, SourceLimits::default())
        .unwrap_or_else(|diagnostics| panic!("{diagnostics}"));
    elaborate_project(parsed, ProjectLimits::default())
        .unwrap_or_else(|diagnostics| panic!("{diagnostics}"))
}

fn id(value: u32) -> StableId {
    StableId::new(value).unwrap_or_else(|| panic!("stable id {value}"))
}

fn with_law(formula: &str) -> String {
    let declarations: Vec<&str> = LANE
        .lines()
        .filter(|line| !line.starts_with("law "))
        .collect();
    format!("{}\nlaw 900 probe = {formula};\n", declarations.join("\n"))
}

fn parse_codes(source: &str) -> Vec<DiagnosticCode> {
    match parse_project(source, SourceLimits::default()) {
        Ok(_) => Vec::new(),
        Err(diagnostics) => diagnostics
            .diagnostics()
            .iter()
            .map(|item| item.code())
            .collect(),
    }
}

#[test]
fn zusd_lane_declares_every_state_field_and_reason_in_registry_order() {
    let spec = elaborate(LANE);
    let state_fields = spec
        .fields()
        .iter()
        .filter(|field| field.owner() == id(100))
        .count();
    assert_eq!(state_fields, STATE_FIELDS);
    assert_eq!(spec.reasons().len(), 46);
    for (rank, reason) in spec.reasons().iter().enumerate() {
        assert_eq!(
            u32::try_from(rank).ok(),
            Some(reason.precedence()),
            "{}",
            reason.name().as_str()
        );
    }
    assert_eq!(spec.laws().len(), 9);
}

#[test]
fn v2_elaboration_rejects_the_original_unresolved_path_witness() {
    let source = with_law("post.100.999 == 0 && command.101.777 == 3 && post.555.1.2.3 == 1");
    let parsed = parse_project(&source, SourceLimits::default())
        .unwrap_or_else(|diagnostics| panic!("{diagnostics}"));
    let diagnostics = elaborate_project(parsed, ProjectLimits::default())
        .err()
        .unwrap_or_else(|| panic!("V2 must refuse every original unresolved path"));
    assert_eq!(diagnostics.len(), 3);
    assert!(
        diagnostics
            .diagnostics()
            .iter()
            .all(|diagnostic| { diagnostic.code() == DiagnosticCode::UnknownReference })
    );
}

#[test]
fn zeno_v1_integer_literals_stop_at_the_u64_range() {
    assert!(parse_codes(&with_law("post.100.110 <= 18446744073709551615")).is_empty());
    assert!(
        parse_codes(&with_law("post.100.110 <= 18446744073709551616"))
            .contains(&DiagnosticCode::InvalidNumber)
    );
    // 10^30 is written as a product instead.
    assert!(
        parse_codes(&with_law(
            "post.100.110 <= 1000000000000000 * 1000000000000000"
        ))
        .is_empty()
    );
}

#[test]
fn zeno_v1_comparisons_cannot_start_with_a_parenthesized_scalar() {
    assert!(!parse_codes(&with_law("(post.100.110 + 1) * 2 == 4")).is_empty());
    assert!(parse_codes(&with_law("2 * (post.100.110 + 1) == 4")).is_empty());
}
