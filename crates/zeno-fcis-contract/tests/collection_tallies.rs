//! Grouped cross-map induction and its negative controls.
use zeno_fcis_contract::collections::{Limits, Verdict, check};

const PROJECT: &str = include_str!("../../../examples/collections/library/project.zeno");
const RULES: &str = include_str!("../../../examples/collections/library/v2/policy.json");

#[test]
fn library_tally_allows_return_without_a_zero_count_guard() -> Result<(), Box<dyn std::error::Error>>
{
    assert!(!RULES.contains("loans == 0"));
    let result = check(PROJECT, RULES, Limits::default())?;
    assert_eq!(result.verdict(), Verdict::Checked, "{}", result.packet());
    assert_eq!(result.packet()["tallies"], serde_json::json!([510]));
    // Without the invariant, return from a zero-count view underflows.
    let unlinked = PROJECT.replace(
        "tally 510 loans_match_copies 110.140 = count 115 by 146 where 145;",
        "",
    );
    assert_eq!(
        check(&unlinked, RULES, Limits::default())?.verdict(),
        Verdict::Refuted
    );
    Ok(())
}

#[test]
fn dropping_either_side_of_the_cross_map_delta_is_refuted() -> Result<(), Box<dyn std::error::Error>>
{
    for field in ["161", "171"] {
        let mut rules: serde_json::Value = serde_json::from_str(RULES)?;
        let case = rules["cases"]
            .as_array_mut()
            .ok_or("cases")?
            .iter_mut()
            .find(|c| c["rule"] == "borrow")
            .ok_or("borrow")?;
        case["post"].as_object_mut().ok_or("post")?.remove(field);
        let result = check(PROJECT, &rules.to_string(), Limits::default())?;
        assert_eq!(result.verdict(), Verdict::Refuted, "{}", result.packet());
        if field == "161" {
            assert_eq!(result.packet()["result"]["obligation"], "cross-map delta");
        }
    }
    Ok(())
}

#[test]
fn cross_map_laws_outside_the_fragment_are_refused() {
    for source in [
        PROJECT.replace("by 146", "by 145"),
        PROJECT.replace("where 145", "where 146"),
        PROJECT.replace("count 115", "sum 115.146"),
        PROJECT.replace("0..=2", "-1..=2"),
        PROJECT.replace("loans_match_copies 110.140", "loans_match_copies 115.145"),
        format!(
            "{PROJECT}\nlaw 511 unsupported_join on commit, genesis = post.100.161 == post.100.171;"
        ),
        PROJECT.replace("tally 510", "tally 500"),
    ] {
        assert!(
            zeno_fcis_spec::parse_project(&source, Default::default()).is_err(),
            "accepted: {source}"
        );
    }
}

#[test]
fn tally_metadata_is_bound_into_format_seven() -> Result<(), Box<dyn std::error::Error>> {
    let encode = |source: &str| -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let spec = zeno_fcis_spec::elaborate_project(
            zeno_fcis_spec::parse_project(source, Default::default()).map_err(|e| e.to_string())?,
            Default::default(),
        )
        .map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        spec.encode_to(&mut bytes)?;
        Ok(bytes)
    };
    let bytes = encode(PROJECT)?;
    assert_eq!(&bytes[19..21], &7u16.to_be_bytes());
    assert_ne!(bytes, encode(&PROJECT.replace(" where 145", ""))?);
    assert_ne!(bytes, encode(&PROJECT.replace("count 115", "sum 115.145"))?);
    Ok(())
}
