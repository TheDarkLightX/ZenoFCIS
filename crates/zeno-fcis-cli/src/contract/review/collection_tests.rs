//! Map routing controls; the legacy scalar view supplies a concrete false friend.
use super::{lemma, profile, scenario};
const PROJECT: &str =
    include_str!("../../../../../examples/collections/token-balances/project.zeno");
const RULES: &str =
    include_str!("../../../../../examples/collections/token-balances/v2/policy.json");
const SCENARIO: &str =
    include_str!("../../../../../examples/collections/token-balances/tests/decision-scenarios.txt");
#[test]
fn map_lemma_cannot_accept_a_view_only_role_invariant() -> Result<(), Box<dyn std::error::Error>> {
    let formula = vec!["post.100.161 == 0".to_owned()];
    let view =
        zeno_fcis_spec::expand_tables(PROJECT, Default::default()).map_err(|e| e.to_string())?;
    let lemma::Lemmas::Judged(old) = lemma::judge(&view.text, RULES, &formula, 1 << 22)? else {
        return Err("scalar view incomplete".into());
    };
    assert_eq!(old[0]["verdict"], "inductive");
    assert!(
        lemma::judge(PROJECT, RULES, &formula, 1 << 22).is_err(),
        "view-only lemma missed the key minted as receiver then read as sender"
    );
    let replay = scenario::replay(PROJECT, RULES, SCENARIO, "map scenario")?;
    assert_eq!(replay.failed, 0, "{}", replay.packet);
    Ok(())
}
#[test]
fn map_scenario_and_profile_keep_concrete_key_semantics() -> Result<(), Box<dyn std::error::Error>>
{
    let replay = scenario::replay(PROJECT, RULES, SCENARIO, "map scenario")?;
    assert_eq!(replay.failed, 0, "{}", replay.packet);
    assert_eq!(replay.packet["mode"], "concrete-maps");
    let profile::Profile::Measured(packet) = profile::profile(PROJECT, RULES, 1 << 22)? else {
        return Err("profile incomplete".into());
    };
    assert_eq!(packet["mode"], "concrete-maps");
    assert!(packet["states"]["reachable"].as_u64().ok_or("reachable")? > 5);
    Ok(())
}
