//! Concrete maps survive changes in command roles; finite reachability is scoped.
use serde_json::{Value, json};
use zeno_fcis_contract::collections::{Limits, Verdict, judges};
const TOKENS: &str = include_str!("../../../examples/collections/token-balances/project.zeno");
const RULES: &str = include_str!("../../../examples/collections/token-balances/v2/policy.json");
const LIBRARY: &str = include_str!("../../../examples/collections/library/project.zeno");
const LOANS: &str = include_str!("../../../examples/collections/library/v2/policy.json");
fn key(n: u8) -> String {
    format!("{n:064x}")
}
fn token_scenario() -> Value {
    json!({"schema":"zeno-fcis/collection-scenarios/1","scenarios":[{"name":"roles-change","steps":[
      {"command":{"130":150,"131":key(0),"132":key(1),"133":2},"context":{"0":180},"expect":{"class":"Accept","scalars":{"120":2},"entries":[{"map":110,"key":key(1),"record":{"140":2}}]}},
      {"command":{"130":151,"131":key(1),"132":key(0),"133":1},"context":{"0":181},"expect":{"class":"Accept","scalars":{"120":2},"entries":[{"map":110,"key":key(1),"record":{"140":1}},{"map":110,"key":key(0),"record":{"140":1}}]}},
      {"command":{"130":151,"131":key(0),"132":key(1),"133":1},"context":{"0":181},"expect":{"class":"Accept","entries":[{"map":110,"key":key(0),"record":{"140":0}},{"map":110,"key":key(1),"record":{"140":2}}]}}
    ]}]})
}
#[test]
fn concrete_scenario_reads_the_same_key_in_different_roles()
-> Result<(), Box<dyn std::error::Error>> {
    let result = judges::scenarios(TOKENS, RULES, &token_scenario().to_string())?;
    assert_eq!(result.verdict(), Verdict::Checked, "{}", result.packet());
    assert_eq!(
        result.packet()["scenarios"][0]["steps"]
            .as_array()
            .ok_or("steps")?
            .len(),
        3
    );
    let mut wrong = token_scenario();
    wrong["scenarios"][0]["steps"][1]["expect"]["entries"][0]["record"]["140"] = 2.into();
    let result = judges::scenarios(TOKENS, RULES, &wrong.to_string())?;
    assert_eq!(result.verdict(), Verdict::Refuted);
    assert_eq!(result.packet()["scenarios"][0]["failure_step"], 2);
    Ok(())
}
#[test]
fn map_lemmas_use_induction_and_refuse_role_only_view_predicates()
-> Result<(), Box<dyn std::error::Error>> {
    let result = judges::lemma(
        LIBRARY,
        LOANS,
        "forall m in entries 110 { post.100.m.140 <= 2 }",
        false,
        Limits::default(),
    )?;
    assert_eq!(result.verdict(), Verdict::Checked, "{}", result.packet());
    assert_eq!(result.packet()["verdict"], "inductive");
    assert_eq!(result.packet()["mode"], "map-induction");
    let result = judges::lemma(
        TOKENS,
        RULES,
        "forall e in entries 110 { post.100.e.140 <= 1 }",
        false,
        Limits::default(),
    )?;
    assert_eq!(result.verdict(), Verdict::Refuted, "{}", result.packet());
    assert_eq!(result.packet()["verdict"], "not-inductive");
    assert!(
        judges::lemma(TOKENS, RULES, "post.100.161 == 0", false, Limits::default()).is_err(),
        "a role-only invariant is not a universal map lemma"
    );
    Ok(())
}
#[test]
fn map_profiles_count_concrete_reachable_maps_and_bound_work()
-> Result<(), Box<dyn std::error::Error>> {
    let one = judges::profile(TOKENS, RULES, 1, Limits::default())?;
    let two = judges::profile(TOKENS, RULES, 2, Limits::default())?;
    assert_eq!(one.verdict(), Verdict::Checked, "{}", one.packet());
    assert_eq!(two.verdict(), Verdict::Checked, "{}", two.packet());
    assert_eq!(one.packet()["states"]["reachable"], 5);
    assert!(
        two.packet()["states"]["reachable"].as_u64() > one.packet()["states"]["reachable"].as_u64()
    );
    assert!(
        two.packet()["claim"]
            .as_str()
            .ok_or("claim")?
            .contains("no all-size reachability")
    );
    let limited = judges::profile(TOKENS, RULES, 2, Limits { max_tuples: 64 })?;
    assert_eq!(limited.verdict(), Verdict::Unknown);
    assert!(limited.packet()["claim"].is_null());
    assert!(limited.packet().get("states").is_none());
    assert!(judges::profile(TOKENS, RULES, 5, Limits::default()).is_err());
    Ok(())
}
#[test]
fn scenario_input_and_expectations_are_closed_and_typed() -> Result<(), Box<dyn std::error::Error>>
{
    for mutate in [0, 1, 2] {
        let mut s = token_scenario();
        match mutate {
            0 => s["scenarios"][0]["steps"][0]["command"]["999"] = 0.into(),
            1 => s["scenarios"][0]["steps"][0]["command"]["131"] = "bad".into(),
            _ => s["scenarios"][0]["steps"][0]["expect"]["entries"][0]["record"]["999"] = 0.into(),
        };
        assert!(judges::scenarios(TOKENS, RULES, &s.to_string()).is_err());
    }
    Ok(())
}
#[test]
fn library_small_maps_reach_borrowed_and_returned_states() -> Result<(), Box<dyn std::error::Error>>
{
    let result = judges::profile(LIBRARY, LOANS, 2, Limits::default())?;
    assert_eq!(result.verdict(), Verdict::Checked, "{}", result.packet());
    assert!(
        result.packet()["states"]["reachable"]
            .as_u64()
            .ok_or("reachable")?
            > 10
    );
    Ok(())
}
#[test]
fn map_transition_relation_is_checked_on_merged_map_steps() -> Result<(), Box<dyn std::error::Error>>
{
    let result = judges::lemma(
        TOKENS,
        RULES,
        "post.100.120 >= pre.100.120",
        true,
        Limits::default(),
    )?;
    assert_eq!(result.verdict(), Verdict::Checked, "{}", result.packet());
    assert_eq!(result.packet()["verdict"], "checked");
    Ok(())
}
