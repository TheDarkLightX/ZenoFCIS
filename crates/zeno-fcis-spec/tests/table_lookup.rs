//! Computed positions retain exact scalar expansion and reject unsafe ranges.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use zeno_fcis_spec::{
    ProjectLimits, SourceLimits, elaborate_project, expand_tables, parse_project,
};

const BASE: &str = "zeno 1; project 1 fees;
type 100 state S; type 101 command C; type 102 context X;
type 105 int Fee in 0..=30; type 106 int Tier in 0..=3;
const table 110 fee_bps 105 size 4 [30,25,20,10];
field 120 101 tier 106; field 130 100 fee 105;
";

fn canonical(source: &str) -> Vec<u8> {
    elaborate_project(
        parse_project(source, SourceLimits::default()).unwrap(),
        ProjectLimits::default(),
    )
    .unwrap()
    .canonical_bytes()
    .unwrap()
}

fn roundtrip(source: &str) {
    let expanded = expand_tables(source, SourceLimits::default()).unwrap();
    assert_eq!(canonical(source), canonical(&expanded.text));
}

#[test]
fn computed_constant_read_matches_the_explicit_eager_select_chain() {
    let source = format!("{BASE} law 500 charged = post.100.130 == fee_bps[command.101.120 + 0];");
    let explicit = source.replace("fee_bps[command.101.120 + 0]", "choose(command.101.120 + 0 == 0,30,choose(command.101.120 + 0 == 1,25,choose(command.101.120 + 0 == 2,20,10)))");
    assert_eq!(canonical(&source), canonical(&explicit));
    roundtrip(&source);
    for index in [
        "0",
        "1 + 1",
        "3 - command.101.120",
        "div_floor(command.101.120, 2)",
        "choose(command.101.120 == 0, 3, 1)",
        "to_i128(to_u128(command.101.120))",
    ] {
        roundtrip(&format!(
            "{BASE} law 500 charged = post.100.130 == fee_bps[{index}];"
        ));
    }
}

#[test]
fn lookup_refuses_declared_out_of_range_tier() {
    let source = format!("{BASE} law 500 charged = post.100.130 == fee_bps[command.101.120];");
    assert!(parse_project(&source, SourceLimits::default()).is_ok());
    let bad = source.replace("Tier in 0..=3", "Tier in 0..=4");
    let refused = parse_project(&bad, SourceLimits::default());
    assert!(
        refused.is_err(),
        "tier 0..=4 must be refused for a size4 table"
    );
    assert!(
        refused
            .unwrap_err()
            .to_string()
            .contains("index range contained in 0..3")
    );
    for bad_index in [
        "command.101.120 + 1",
        "command.101.120 - 1",
        "unknown",
        "div_floor(command.101.120, 0)",
        "command.101.120 * 170141183460469231731687303715884105727",
    ] {
        let bad = source.replace("fee_bps[command.101.120]", &format!("fee_bps[{bad_index}]"));
        assert!(
            parse_project(&bad, SourceLimits::default()).is_err(),
            "{bad_index}"
        );
    }
}

#[test]
fn computed_stored_and_closed_derived_positions_roundtrip() {
    let base = format!("{BASE} table 210 100 amounts 105 size 4;
        derived table 700 flags[i] in table 110 rounds 4 {{ when true then fee_bps[i] < 30; default false; }};
        derived table 701 pairs[i][j] in table 110 rounds 16 {{ when true then i == j; default false; }};
        derived table 702 words[i] in table 110 bits 2 rounds 8 {{ when true then u128(3); default u128(0); }};");
    for formula in [
        "post.100.130 == pre.100.210[command.101.120]",
        "flags[3 - command.101.120]",
        "pairs[command.101.120][3 - command.101.120]",
        "words[command.101.120] == u128(3)",
    ] {
        roundtrip(&format!("{base} law 500 test = {formula};"));
    }
    roundtrip(&format!(
        "{base} law 500 unsigned = fee_bps[to_u128(command.101.120)] == 10;"
    ));
}

#[test]
fn computed_positions_do_not_hide_active_self_references() {
    let source = format!(
        "{BASE} derived table 700 x[i] in table 110 rounds 4 {{ when true then x[choose(x[i], 0, 1)]; default false; }};"
    );
    assert!(parse_project(&source, SourceLimits::default()).is_err());
    let source = format!(
        "{BASE} derived table 700 x[i] in table 110 rounds 4 {{ when true then fee_bps[choose(x[i],0,1)] > 0; default false; }};"
    );
    assert!(parse_project(&source, SourceLimits::default()).is_err());
}
