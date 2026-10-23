//! Keyed collections checked by data independence.

use zeno_fcis_contract::collections::{Limits, PREMISES, Verdict, check};

const TOKENS: &str = include_str!("../../../examples/collections/token-balances/project.zeno");
const TOKEN_RULES: &str =
    include_str!("../../../examples/collections/token-balances/v2/policy.json");

fn rules_with(edit: impl FnOnce(&mut serde_json::Value)) -> String {
    let mut rules: serde_json::Value =
        serde_json::from_str(TOKEN_RULES).unwrap_or_else(|error| panic!("{error:?}"));
    edit(&mut rules);
    rules.to_string()
}

fn case(rules: &mut serde_json::Value, rule: &str) -> usize {
    rules["cases"]
        .as_array()
        .unwrap_or_else(|| panic!("expected as_array"))
        .iter()
        .position(|c| c["rule"] == rule)
        .unwrap_or_else(|| panic!("missing case: {rule}"))
}

#[test]
fn collections_token_balances_check_over_every_admissible_view() {
    let result =
        check(TOKENS, TOKEN_RULES, Limits::default()).unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(result.verdict(), Verdict::Checked, "{}", result.packet());
    let packet = result.packet();
    assert_eq!(
        packet["premises"]
            .as_array()
            .unwrap_or_else(|| panic!("expected as_array"))
            .len(),
        PREMISES.len()
    );
    assert_eq!(packet["key_types"][0]["positions"], 2);
    assert_eq!(packet["key_types"][0]["patterns"], "2");
    assert_eq!(packet["maps"][0]["capacity"], "1048576");
    // Independent count: two key patterns. Distinct accounts: 5 x 5 views
    // with supply >= their balances, 115 states; one account: 5 views, 29.
    // Each of the 144 takes 2 actions x 4 amounts x 2 desks.
    assert_eq!(packet["measured"]["admissible_views"], 2304);
    assert_eq!(packet["measured"]["tuples"], 28_672);
}

#[test]
fn collections_self_transfer_that_mints_is_refuted_on_the_aliased_pattern() {
    // Credit the receiver on a transfer to oneself: with sender == receiver
    // the account gains the amount while the supply stays put.
    let rules = rules_with(|r| {
        let index = case(r, "a transfer to the same account changes nothing");
        r["cases"][index]["post"] =
            serde_json::json!({"170": "1", "171": "receiver_balance + amount"});
    });
    let result =
        check(TOKENS, &rules, Limits::default()).unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(result.verdict(), Verdict::Refuted, "{}", result.packet());
    let found = &result.packet()["result"];
    assert_eq!(found["obligation"], "summary delta");
    // The witness names one account twice.
    assert_eq!(
        found["view"]["command.101.131"],
        found["view"]["command.101.132"]
    );
}

#[test]
fn collections_aliased_entries_that_write_different_values_are_refuted() {
    // Without the self-transfer case, a transfer to oneself writes the
    // debit to the sender and the credit to the receiver: one account, two
    // values.
    let rules = rules_with(|r| {
        let index = case(r, "a transfer to the same account changes nothing");
        r["cases"]
            .as_array_mut()
            .unwrap_or_else(|| panic!("expected as_array_mut"))
            .remove(index);
    });
    let result =
        check(TOKENS, &rules, Limits::default()).unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(result.verdict(), Verdict::Refuted, "{}", result.packet());
    assert_eq!(result.packet()["result"]["obligation"], "alias merge");
}

#[test]
fn collections_a_supply_the_rules_forget_to_raise_is_refuted() {
    let rules = rules_with(|r| {
        let index = case(r, "mint to the receiver");
        r["cases"][index]["post"]["120"] = serde_json::json!("supply");
    });
    let result =
        check(TOKENS, &rules, Limits::default()).unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(result.verdict(), Verdict::Refuted, "{}", result.packet());
    assert_eq!(result.packet()["result"]["obligation"], "summary delta");
}

#[test]
fn collections_rules_use_keys_only_by_equality() {
    for misuse in [
        "command.101.131 < command.101.132",
        "command.101.131 == 0",
        "command.101.131 + 1 == command.101.132",
    ] {
        let rules = rules_with(|r| {
            r["variables"]["self_transfer"] = serde_json::json!(misuse);
        });
        let refused = check(TOKENS, &rules, Limits::default())
            .err()
            .unwrap_or_else(|| panic!("expected refusal"));
        assert!(
            refused.reason().contains("a key is compared only"),
            "{misuse}: {refused:?}"
        );
    }
}

#[test]
fn collections_genesis_starts_with_empty_maps() {
    let rules = rules_with(|r| r["genesis"]["160"] = serde_json::json!(1));
    assert!(
        check(TOKENS, &rules, Limits::default())
            .err()
            .unwrap_or_else(|| panic!("expected refusal"))
            .reason()
            .contains("absent")
    );
    let rules = rules_with(|r| r["genesis"]["120"] = serde_json::json!(2));
    assert!(
        check(TOKENS, &rules, Limits::default())
            .err()
            .unwrap_or_else(|| panic!("expected refusal"))
            .reason()
            .contains("summary")
    );
}

#[test]
fn collections_a_view_domain_beyond_the_limit_is_unknown() {
    let result = check(TOKENS, TOKEN_RULES, Limits { max_tuples: 100 })
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(result.verdict(), Verdict::Unknown);
    assert!(result.packet()["claim"].is_null());
}

#[test]
fn collections_genesis_obeys_all_runtime_startup_laws() {
    let project = format!("{TOKENS}\nlaw 980 startup_guard on accept, genesis = false;\n");
    let rules = rules_with(|r| r["law_kinds"]["980"] = serde_json::json!("FeeAndRounding"));
    let result = check(&project, &rules, Limits::default()).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(result.verdict(), Verdict::Refuted, "{}", result.packet());
    assert_eq!(
        result.packet()["result"]["obligation"],
        "runtime genesis admission"
    );
    assert_eq!(result.packet()["result"]["law"], 980);
    assert_eq!(result.packet()["measured"]["tuples"], 0);
}

#[test]
fn opaque_collection_laws_are_rechecked_after_alias_merge() {
    let project = format!(
        "{TOKENS}\nlaw 504 merged_sender on commit = command.101.130 != 150 || post.100.161 == pre.100.161;\n"
    );
    let rules = rules_with(|r| r["law_kinds"]["504"] = "DebitCreditEffectEquality".into());
    let result =
        check(&project, &rules, Limits::default()).unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(result.verdict(), Verdict::Refuted, "{}", result.packet());
    assert_eq!(result.packet()["result"]["obligation"], "merged post laws");
}

#[test]
fn opaque_collection_invariants_are_uniform_and_input_independent() {
    for formula in ["post.100.161 == 0", "command.101.130 == 150"] {
        let project =
            format!("{TOKENS}\nlaw 504 unsafe_invariant on commit, genesis = {formula};\n");
        assert!(
            zeno_fcis_spec::parse_project(&project, Default::default()).is_err(),
            "{formula}"
        );
    }
}

#[test]
fn stored_post_keys_require_a_concrete_pre_invocation_binding()
-> Result<(), Box<dyn std::error::Error>> {
    let project = "zeno 1; project 1 copy_keys; namespace 2 copies;
type 100 state Store; type 101 command Command; type 102 context Desk;
type 103 key HolderId; type 104 key CopyId; type 105 data Copy;
field 140 105 holder 103; map 110 100 copies 104 105 capacity 4;
field 130 101 copy 104; entry 160 copy of 110 at command.101.130;
variant 180 102 Librarian none;
component 400 core { owns 100; reads pre.100; writes post.100; contexts context.102; budget steps 128; }
merge [400]; law 500 marker on commit, genesis = true;";
    let mut rules = serde_json::json!({
        "schema":"zeno-fcis/template-declarative-policy/2", "template":"copy-keys",
        "roots":{"state":100,"command":101,"context":102},"leaf_bindings":{},"variables":{},
        "cases":[{"rule":"insert","when":"true","class":"Accept","reason":null,
            "post":{"160":"1","161":"pre.100.161"},"outbox":[]}],
        "genesis":{"160":0,"161":0},"law_kinds":{"500":"StateInvariant"}
    });
    let result = check(project, &rules.to_string(), Limits::default())?;
    assert_eq!(result.verdict(), Verdict::Refuted, "{}", result.packet());
    assert_eq!(
        result.packet()["result"]["obligation"],
        "stored key provenance"
    );
    // Supplying a real HolderId makes the same insertion representable.
    let project = project.replace(
        "field 130 101 copy 104;",
        "field 130 101 copy 104; field 131 101 holder 103;",
    );
    rules["cases"][0]["post"]["161"] = "command.101.131".into();
    let result = check(&project, &rules.to_string(), Limits::default())?;
    assert_eq!(result.verdict(), Verdict::Checked, "{}", result.packet());
    Ok(())
}
