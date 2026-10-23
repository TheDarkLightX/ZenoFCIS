//! Bounded ordered collection syntax and assurance checks.

use std::collections::BTreeSet;
use zeno_fcis_contract::collections::{
    Limits, OrderedQuery, Verdict, check, ordered_key, runtime::Layout,
};

const BOOK: &str = include_str!("../../../examples/collections/order-book/project.zeno");
const RULES: &str = include_str!("../../../examples/collections/order-book/v2/policy.json");

fn checked(
    project: &str,
    rules: &str,
) -> Result<zeno_fcis_contract::collections::CheckResult, Box<dyn std::error::Error>> {
    Ok(check(project, rules, Limits::default())?)
}

#[test]
fn order_book_checks_and_runtime_refuses_unsupported_framing()
-> Result<(), Box<dyn std::error::Error>> {
    let result = checked(BOOK, RULES)?;
    assert_eq!(result.verdict(), Verdict::Checked, "{}", result.packet());
    assert_eq!(result.packet()["key_types"][0]["patterns"], "3");
    assert!(
        Layout::read(BOOK, RULES)
            .err()
            .is_some_and(|e| e.reason().contains("ordered collection framing"))
    );
    let result = check(BOOK, RULES, Limits { max_tuples: 1 })?;
    assert_eq!(result.verdict(), Verdict::Unknown);
    Ok(())
}

#[test]
fn weak_ordering_catches_best_bid_failure_when_first_key_is_greater()
-> Result<(), Box<dyn std::error::Error>> {
    // Anchor occurs first, candidate second, read result third. Equality-only
    // canonical labels cannot represent anchor > candidate, best == anchor.
    let project = BOOK.replace("field 131 101 price 103;", "field 129 101 anchor 103;\nfield 131 101 price 103;\nentry 155 anchor of 110 at command.101.129;");
    let mut rules: serde_json::Value = serde_json::from_str(RULES)?;
    rules["genesis"]["155"] = 0.into();
    rules["genesis"]["156"] = 0.into();
    rules["cases"][2]["when"] =
        "context.102.133 == 0 || pre.100.160 == 0 || pre.100.155 == 0 || command.101.129 != best"
            .into();
    let result = checked(&project, &rules.to_string())?;
    assert_eq!(result.verdict(), Verdict::Refuted, "{}", result.packet());
    let view = &result.packet()["result"]["view"];
    let rank = |name: &str| view[name].as_str().and_then(|v| v.parse::<i128>().ok());
    assert!(rank("command.101.129") > rank("command.101.131"), "{view}");
    Ok(())
}

#[test]
fn successor_empty_interval_excludes_a_present_middle_key() -> Result<(), Box<dyn std::error::Error>>
{
    let project = BOOK.replace(" max into", " successor command.101.131 into")
        .replace("field 131 101 price 103;", "field 131 101 price 103;\nfield 134 101 middle 103;\nentry 175 middle of 110 at command.101.134;")
        .replace("context.102.133 == 1 && pre.100.160 == 1 && command.101.131 >= context.102.132", "context.102.133 == 0 || pre.100.175 == 0 || command.101.134 <= command.101.131 || command.101.134 >= context.102.132");
    let mut rules: serde_json::Value = serde_json::from_str(RULES)?;
    rules["genesis"]["175"] = 0.into();
    rules["genesis"]["176"] = 0.into();
    rules["cases"][2]["when"] = "false".into();
    let result = checked(&project, &rules.to_string())?;
    assert_eq!(result.verdict(), Verdict::Checked, "{}", result.packet());
    Ok(())
}

#[test]
fn all_queries_use_strict_neighbours_and_closed_ranges() {
    let keys: BTreeSet<i128> = [1, 3, 7].into_iter().collect();
    for (query, expected) in [
        (OrderedQuery::Min, Some(1)),
        (OrderedQuery::Max, Some(7)),
        (OrderedQuery::Successor(3), Some(7)),
        (OrderedQuery::Successor(7), None),
        (OrderedQuery::Predecessor(3), Some(1)),
        (OrderedQuery::Predecessor(1), None),
        (OrderedQuery::Range(3, 3), Some(3)),
        (OrderedQuery::Range(4, 6), None),
        (OrderedQuery::Range(7, 1), None),
    ] {
        assert_eq!(ordered_key(&keys, query), expected);
    }
}

#[test]
fn ordered_rules_refuse_arithmetic_constants_and_cross_type_keys()
-> Result<(), Box<dyn std::error::Error>> {
    for expression in [
        "price == 0",
        "price + 1 == best",
        "price == action",
        "to_i128(price) == 0",
    ] {
        let rules = RULES.replace("price != best", expression);
        assert!(
            check(BOOK, &rules, Limits::default()).is_err(),
            "{expression}"
        );
    }
    Ok(())
}

#[test]
fn every_order_read_checks_presence_extremality_and_empty_intervals()
-> Result<(), Box<dyn std::error::Error>> {
    let prefix = "context.102.133 == 1";
    let scenarios = [
        (
            "min",
            format!("pre.100.175 == 0 || ({prefix} && command.101.134 >= context.102.132)"),
        ),
        (
            "max",
            format!("pre.100.175 == 0 || ({prefix} && command.101.134 <= context.102.132)"),
        ),
        (
            "successor command.101.131",
            format!(
                "pre.100.175 == 0 || command.101.134 <= command.101.131 || ({prefix} && command.101.134 >= context.102.132)"
            ),
        ),
        (
            "predecessor command.101.131",
            format!(
                "pre.100.175 == 0 || command.101.134 >= command.101.131 || ({prefix} && command.101.134 <= context.102.132)"
            ),
        ),
        (
            "range command.101.131 to command.101.134",
            format!(
                "(pre.100.175 == 0 || command.101.134 < command.101.131 || {prefix}) && (context.102.133 == 0 || (command.101.131 <= context.102.132 && context.102.132 <= command.101.134))"
            ),
        ),
    ];
    for (query, law) in scenarios {
        let project = BOOK.replace(" max into", &format!(" {query} into"))
            .replace("field 131 101 price 103;", "field 131 101 price 103;\nfield 134 101 middle 103;\nentry 175 middle of 110 at command.101.134;")
            .replace("context.102.133 == 1 && pre.100.160 == 1 && command.101.131 >= context.102.132",&format!("({law}) && (context.102.133 == 0 || pre.100.170 == 1)"));
        let mut rules: serde_json::Value = serde_json::from_str(RULES)?;
        rules["genesis"]["175"] = 0.into();
        rules["genesis"]["176"] = 0.into();
        rules["cases"] = serde_json::json!([{"rule":"observe", "when":"true", "class":"Accept", "reason":null,"post":{},"outbox":[]}]);
        let result = checked(
            &project.replace("reason 200 no_fill precedence 0;", ""),
            &rules.to_string(),
        )?;
        assert_eq!(
            result.verdict(),
            Verdict::Checked,
            "{query}: {}",
            result.packet()
        );
    }
    Ok(())
}

#[test]
fn simultaneous_reads_share_presence_and_gap_facts() -> Result<(), Box<dyn std::error::Error>> {
    let project = BOOK.replace("field 133 102 best_found 106;", "field 133 102 best_found 106;\nfield 134 102 least_price 103;\nfield 135 102 least_found 106;\norder 181 least of 110 min into context.102.134 found context.102.135;")
        .replace("context.102.133 == 1 && pre.100.160 == 1 && command.101.131 >= context.102.132", "context.102.133 == context.102.135 && (context.102.133 == 0 || context.102.134 <= context.102.132)");
    let mut rules: serde_json::Value = serde_json::from_str(RULES)?;
    rules["cases"] = serde_json::json!([{"rule":"observe", "when":"true", "class":"Accept", "reason":null,"post":{},"outbox":[]}]);
    let result = checked(
        &project.replace("reason 200 no_fill precedence 0;", ""),
        &rules.to_string(),
    )?;
    assert_eq!(result.verdict(), Verdict::Checked, "{}", result.packet());
    Ok(())
}

#[test]
fn ordered_laws_are_rechecked_after_alias_merge() -> Result<(), Box<dyn std::error::Error>> {
    let project = BOOK[..BOOK.find("law 501").ok_or("law501")?].to_owned()
        + "law 501 merged_fill on commit = command.101.130 != 150 || post.100.171 == 0;\nlaw 502 marker on commit, genesis = true;\n";
    let mut rules: serde_json::Value = serde_json::from_str(RULES)?;
    rules["law_kinds"]["501"] = "AuthoritySubjectRecipient".into();
    rules["law_kinds"]["502"] = "StateInvariant".into();
    rules["cases"].as_array_mut().ok_or("cases")?.insert(
        0,
        serde_json::json!({
            "rule":"place only in empty book", "when":"action == 150 && context.102.133 == 1",
            "class":"Reject", "reason":200, "post":{}, "outbox":[]
        }),
    );
    let result = checked(&project, &rules.to_string())?;
    assert_eq!(result.verdict(), Verdict::Refuted, "{}", result.packet());
    assert_eq!(result.packet()["result"]["obligation"], "merged post laws");
    Ok(())
}
