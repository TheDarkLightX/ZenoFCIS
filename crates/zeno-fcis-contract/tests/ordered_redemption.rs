//! Exact-integer differential vectors and refusal/ownership controls for traversal.
use serde_json::{Value as J, json};
use std::{path::PathBuf, sync::OnceLock};
use zeno_fcis_synthesis::finite::v2_authority::tables::{self as t, redemption as r};
fn num(j: &J) -> i128 {
    j.as_str().unwrap().parse().unwrap()
}
fn u32_(j: &J) -> u32 {
    u32::try_from(num(j)).unwrap()
}
fn u64_(j: &J) -> u64 {
    u64::try_from(num(j)).unwrap()
}
fn opt(j: &J) -> Option<u32> {
    if j.is_null() { None } else { Some(u32_(j)) }
}
fn pairs(j: &J) -> Vec<(u32, i128)> {
    j.as_array()
        .unwrap()
        .iter()
        .map(|x| (u32_(&x[0]), num(&x[1])))
        .collect()
}
fn state(j: &J) -> r::State {
    r::State {
        vaults: j["vaults"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| r::Vault {
                id: u32_(&v["id"]),
                owner: u32_(&v["owner"]),
                active: v["active"].as_bool().unwrap(),
                collateral: num(&v["collateral"]),
                debt: num(&v["debt"]),
                stake: num(&v["stake"]),
                snapshot_c: num(&v["snapshot_c"]),
                snapshot_d: num(&v["snapshot_d"]),
            })
            .collect(),
        order: j["order"].as_array().unwrap().iter().map(u32_).collect(),
        priorities: pairs(&j["priorities"]),
        reward_c: num(&j["reward_c"]),
        reward_d: num(&j["reward_d"]),
        stakes_snapshot: num(&j["stakes_snapshot"]),
        collateral_snapshot: num(&j["collateral_snapshot"]),
        active_collateral: num(&j["active_collateral"]),
        default_collateral: num(&j["default_collateral"]),
        active_debt: num(&j["active_debt"]),
        default_debt: num(&j["default_debt"]),
        holder: u32_(&j["holder"]),
        holder_balance: num(&j["holder_balance"]),
        reserve_balance: num(&j["reserve_balance"]),
        supply: num(&j["supply"]),
        holder_collateral: num(&j["holder_collateral"]),
        staking_collateral: num(&j["staking_collateral"]),
        surplus: pairs(&j["surplus"]),
        nonce: u64_(&j["nonce"]),
        price_e8: num(&j["price_e8"]),
        price_epoch: u64_(&j["price_epoch"]),
        oracle_profile: [7; 32],
        base_rate: num(&j["base_rate"]),
        last_fee_epoch: u64_(&j["last_fee_epoch"]),
    }
}
fn limits() -> t::Limits {
    let mut l = t::zero_limits();
    for lane in [
        t::Resource::Read,
        t::Resource::Write,
        t::Resource::Candidate,
        t::Resource::Effect,
        t::Resource::Byte,
        t::Resource::WitnessByte,
        t::Resource::Depth,
        t::Resource::Step,
    ] {
        l = l.with_limit(lane, u64::MAX);
    }
    l
}
fn policy(j: &J) -> r::Policy {
    r::Policy {
        capacity: u32_(&j["capacity"]),
        inventory_capacity: u32_(&j["inventory_capacity"]),
        retained_bytes: u64_(&j["retained_bytes"]),
        minimum_net_debt: num(&j["minimum_net_debt"]),
        reserve: num(&j["reserve"]),
        bootstrap_epoch: u64_(&j["bootstrap_epoch"]),
        epoch_seconds: u64_(&j["epoch_seconds"]),
        max_price_age: u64_(&j["max_price_age"]),
        oracle_profile: [7; 32],
        redemption_capacity: num(&j["redemption_capacity"]),
        limits: limits(),
    }
}
fn command(j: &J) -> r::Command {
    r::Command {
        actor: u32_(&j["actor"]),
        nonce: u64_(&j["nonce"]),
        maximum: num(&j["maximum"]),
        max_visits: u32_(&j["max_visits"]),
        max_fee: num(&j["max_fee"]),
        first: opt(&j["first"]),
        partial_nicr: num(&j["partial_nicr"]),
        upper: opt(&j["upper"]),
        lower: opt(&j["lower"]),
    }
}
fn context(j: &J) -> r::Context {
    r::Context {
        actor: u32_(&j["actor"]),
        epoch: u64_(&j["epoch"]),
    }
}
fn vectors() -> &'static Vec<J> {
    static X: OnceLock<Vec<J>> = OnceLock::new();
    X.get_or_init(|| {
        let path = std::env::temp_dir().join(format!(
            "sol-redemption-vectors-{}.json",
            std::process::id()
        ));
        let out = std::process::Command::new("python3")
            .arg(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tools/ordered_redemption_reference.py"),
            )
            .arg(&path)
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    })
}
fn trace_json(x: &r::Trace) -> J {
    json!({"visited":x.visited.iter().map(u32::to_string).collect::<Vec<_>>(),"closed":x.closed.iter().map(u32::to_string).collect::<Vec<_>>(),"settled":x.settled.iter().map(u32::to_string).collect::<Vec<_>>(),"first":x.first.map(|v|v.to_string()),"first_hint_used":x.first_hint_used,"stop":format!("{:?}",x.stop),"executed":x.executed.to_string(),"unfilled":x.unfilled.to_string(),"gross_collateral":x.gross_collateral.to_string(),"reserve_burn":x.reserve_burn.to_string(),"surplus":x.surplus.to_string(),"fee":x.fee.to_string()})
}
#[test]
fn redemption_matches_independent_v1_rule_integer_reference() {
    for row in vectors() {
        let name = row["name"].as_str().unwrap();
        let pre = state(&row["pre"]);
        let saved = pre.clone();
        let a = r::Traversal::bind(policy(&row["policy"])).unwrap();
        let cmd = command(&row["command"]);
        let ctx = context(&row["context"]);
        let evaluation = a.evaluate(&pre, &cmd, ctx);
        let result = evaluation
            .result()
            .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        if let Some(reason) = row["expected"]["reason"].as_str() {
            assert_eq!(format!("{:?}", result.unwrap_err()), reason, "{name}")
        } else {
            let plan = result.unwrap_or_else(|e| panic!("{name}: {e:?}"));
            assert_eq!(plan.state(), &state(&row["expected"]["state"]), "{name}");
            assert_eq!(trace_json(plan.trace()), row["expected"]["trace"], "{name}");
        }
        assert_eq!(pre, saved, "caller state changed: {name}");
        let retry = a.evaluate(&pre, &cmd, ctx);
        assert_eq!(evaluation.usage(), retry.usage(), "retry work: {name}");
    }
    println!("REDEMPTION_REFERENCE {} exact vectors", vectors().len());
}
#[test]
fn redemption_rechecks_eligibility_inside_floor_tie() {
    for name in [
        "tie-eligibility-departure",
        "first-hint-cannot-skip-lower-eligible",
    ] {
        let row = vectors().iter().find(|x| x["name"] == name).unwrap();
        let a = r::Traversal::bind(policy(&row["policy"])).unwrap();
        let pre = state(&row["pre"]);
        assert!(
            pre.priorities
                .iter()
                .all(|(_, p)| *p == pre.priorities[0].1)
        );
        let r = a.evaluate(&pre, &command(&row["command"]), context(&row["context"]));
        let plan = r.result().unwrap().unwrap();
        assert_eq!(plan.trace().visited, vec![2, 1, 0]);
        assert_eq!(plan.trace().closed, vec![2]);
        assert!(!plan.trace().first_hint_used);
        assert_ne!(row["expected"], row["v1_expected"]);
        assert_eq!(plan.state().vaults[1], pre.vaults[1]);
    }
}

#[test]
fn redemption_debt_only_rewards_explicitly_depart_from_pinned_v1() {
    for name in [
        "debt-only-reward-departure",
        "debt-only-reward-v1-constant-control",
    ] {
        let row = vectors().iter().find(|x| x["name"] == name).unwrap();
        assert_eq!(row["expected"], row["v1_corrected_expected"]);
        assert_eq!(row["v1_expected"], json!({"reason":"NoPositiveExecution"}));
        let pre = state(&row["pre"]);
        assert_eq!(pre.reward_c, 0);
        assert!(pre.reward_d > 0);
        let cmd = command(&row["command"]);
        assert_eq!(cmd.partial_nicr, 174129353233830845771);
        let a = r::Traversal::bind(policy(&row["policy"])).unwrap();
        let evaluation = a.evaluate(&pre, &cmd, context(&row["context"]));
        let plan = evaluation.result().unwrap().unwrap();
        assert_eq!(plan.trace().settled, vec![1]);
        assert_eq!(plan.trace().executed, 100 * 10i128.pow(18));
        assert_eq!(plan.state().vaults[1].debt, 2010 * 10i128.pow(18));
        assert_eq!(plan.state(), &state(&row["v1_corrected_expected"]["state"]));
        assert_ne!(3500 * 10i128.pow(20) / 2000, cmd.partial_nicr);
        assert_eq!(pre, state(&row["pre"]));
    }
}

#[test]
fn redemption_pending_detection_exhausts_both_snapshot_lags() {
    for collateral_lag in [false, true] {
        for debt_lag in [false, true] {
            let name = format!(
                "pending-gate-{}{}",
                u8::from(collateral_lag),
                u8::from(debt_lag)
            );
            let row = vectors().iter().find(|x| x["name"] == name).unwrap();
            assert_eq!(row["expected"], row["v1_corrected_expected"]);
            assert_eq!(
                row["expected"] != row["v1_expected"],
                debt_lag && !collateral_lag
            );
            let pre = state(&row["pre"]);
            let a = r::Traversal::bind(policy(&row["policy"])).unwrap();
            let evaluation = a.evaluate(&pre, &command(&row["command"]), context(&row["context"]));
            let plan = evaluation.result().unwrap().unwrap();
            assert_eq!(
                plan.trace().settled,
                if collateral_lag || debt_lag {
                    vec![1]
                } else {
                    vec![]
                }
            );
            assert_eq!(plan.state(), &state(&row["expected"]["state"]));
        }
    }
}
#[test]
fn redemption_refuses_stale_nicr_and_unsorted_redistribution() {
    let row = &vectors()[0];
    let mut pre = state(&row["pre"]);
    let a = r::Traversal::bind(policy(&row["policy"])).unwrap();
    let cmd = command(&row["command"]);
    let ctx = context(&row["context"]);
    pre.priorities[0].1 += 1;
    assert!(matches!(
        a.evaluate(&pre, &cmd, ctx).result(),
        Err(t::Failure::Law(983))
    ));
    // Two equal starting NICRs, different stake/collateral proportions. Debt-only
    // redistribution reverses their effective order; updating cached keys is not enough.
    let mut s = state(&row["pre"]);
    s.vaults.truncate(2);
    s.vaults[0].collateral = 2000 * 10i128.pow(18);
    s.vaults[0].debt = 1000 * 10i128.pow(18);
    s.vaults[0].stake = 1000 * 10i128.pow(18);
    s.vaults[1].collateral = s.vaults[0].collateral;
    s.vaults[1].debt = s.vaults[0].debt;
    s.vaults[1].stake = 10i128.pow(18);
    s.order = vec![0, 1];
    s.priorities = vec![(0, 2 * 10i128.pow(20)), (1, 2 * 10i128.pow(20))];
    s.active_collateral = 4000 * 10i128.pow(18);
    s.active_debt = 2000 * 10i128.pow(18);
    s.supply = s.active_debt;
    s.holder_balance = 1000 * 10i128.pow(18);
    s.reserve_balance = 100 * 10i128.pow(18);
    s.default_debt = 1001 * 10i128.pow(18);
    s.supply = s.active_debt + s.default_debt;
    let frozen = s.clone();
    let mut candidate = s.clone();
    candidate.reward_d = 10i128.pow(18);
    candidate.priorities = vec![(0, 10i128.pow(20)), (1, 2000 * 10i128.pow(20) / 1001)];
    let outcome = a.check_redistribution(&s, &candidate);
    assert!(
        matches!(outcome.result(), Err(t::Failure::Law(985))),
        "{outcome:?}"
    );
    assert_eq!(s, frozen);
    s.order = vec![1, 0];
    candidate.order = s.order.clone();
    assert!(a.check_redistribution(&s, &candidate).result().is_ok());
    let mut minted = candidate.clone();
    minted.supply += 1;
    minted.default_debt += 1;
    assert!(a.check_redistribution(&s, &minted).result().is_err());
}
#[test]
fn redemption_budget_denial_retains_work_and_no_candidate() {
    let row = &vectors()[0];
    let pre = state(&row["pre"]);
    let cmd = command(&row["command"]);
    let ctx = context(&row["context"]);
    let p = policy(&row["policy"]);
    let full = r::Traversal::bind(p.clone())
        .unwrap()
        .evaluate(&pre, &cmd, ctx);
    let used = full.usage().used(t::Resource::Step);
    assert!(used > 1000);
    let mut p = p;
    p.limits = p.limits.with_limit(t::Resource::Step, used - 1);
    let a = r::Traversal::bind(p).unwrap();
    let one = a.evaluate(&pre, &cmd, ctx);
    assert!(matches!(one.result(), Err(t::Failure::Budget(_))));
    let two = a.evaluate(&pre, &cmd, ctx);
    assert_eq!(one.usage(), two.usage());
    assert!(one.usage().used(t::Resource::Step) > 0);
    assert!(one.usage().used(t::Resource::Candidate) <= 1);
}
#[test]
fn redemption_complete_laws_reject_wrong_reason_trace_and_recipient() {
    for row in vectors() {
        let p = policy(&row["policy"]);
        let a = r::Traversal::bind(p).unwrap();
        let pre = state(&row["pre"]);
        let c = command(&row["command"]);
        let x = context(&row["context"]);
        let evaluation = a.evaluate(&pre, &c, x);
        let result = evaluation.result().unwrap();
        let reasons = [
            r::Reason::Unauthorized,
            r::Reason::Replay,
            r::Reason::NonceExhausted,
            r::Reason::Oracle,
            r::Reason::Capacity,
            r::Reason::FeeDomain,
            r::Reason::Bootstrap,
            r::Reason::Tcr,
            r::Reason::Amount,
            r::Reason::Balance,
            r::Reason::LastVault,
            r::Reason::NoPositiveExecution,
            r::Reason::FeeConsumesAll,
            r::Reason::FeeExceeded,
        ];
        for reason in reasons {
            assert_eq!(
                a.check_laws(&pre, &c, x, None, None, Some(reason))
                    .result()
                    .is_ok(),
                result.as_ref().is_err_and(|r| *r == reason),
                "{} {reason:?}",
                row["name"]
            );
        }
        if let Ok(plan) = result {
            assert!(
                a.check_laws(&pre, &c, x, Some(plan.state()), Some(plan.trace()), None)
                    .result()
                    .is_ok()
            );
            let mut wrong = plan.state().clone();
            wrong.holder_collateral += 1;
            wrong.staking_collateral -= 1;
            assert!(
                a.check_laws(&pre, &c, x, Some(&wrong), Some(plan.trace()), None)
                    .result()
                    .is_err()
            );
            let mut trace = plan.trace().clone();
            trace.visited.reverse();
            if trace != *plan.trace() {
                assert!(
                    a.check_laws(&pre, &c, x, Some(plan.state()), Some(&trace), None)
                        .result()
                        .is_err()
                );
            }
            let mut trace = plan.trace().clone();
            trace.visited.push(u32::MAX);
            assert!(
                a.check_laws(&pre, &c, x, Some(plan.state()), Some(&trace), None)
                    .result()
                    .is_err()
            );
        }
    }
}
#[test]
fn redemption_owned_policy_and_oversized_input_admission() {
    let row = &vectors()[0];
    let mut p = policy(&row["policy"]);
    let a = r::Traversal::bind(p.clone()).unwrap();
    p.reserve = 1;
    let mut pre = state(&row["pre"]);
    let snapshot = pre.clone();
    let c = command(&row["command"]);
    let x = context(&row["context"]);
    let e = a.evaluate(&pre, &c, x);
    let plan = e.result().unwrap().unwrap();
    assert_eq!(plan.trace().reserve_burn, 50 * 10i128.pow(18));
    assert_eq!(pre, snapshot);
    let mut q = policy(&row["policy"]);
    q.limits = t::zero_limits();
    let a = r::Traversal::bind(q).unwrap();
    pre.vaults.push(pre.vaults[0].clone());
    pre.vaults.push(pre.vaults[0].clone());
    let e = a.evaluate(&pre, &c, x);
    assert!(matches!(e.result(), Err(t::Failure::Wire)));
    assert_eq!(e.usage().used(t::Resource::Read), 0);
    assert_eq!(e.usage().used(t::Resource::Byte), 0);
}

#[test]
fn redistribution_repairs_pinned_swaps_and_preserves_committed_ties() {
    let path = std::env::temp_dir().join(format!(
        "sol-redistribution-vectors-{}.json",
        std::process::id()
    ));
    let out = std::process::Command::new("python3")
        .arg(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../tools/ordered_redemption_reference.py"),
        )
        .arg(&path)
        .arg("--redistribution")
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let rows: Vec<J> = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    for row in &rows {
        let pre = state(&row["pre"]);
        let saved = pre.clone();
        let expected = state(&row["expected"]);
        let p = policy(&row["policy"]);
        let a = r::Traversal::bind(p.clone()).unwrap();
        let repair = a.redistribute(&pre, expected.reward_c, expected.reward_d);
        let post = repair
            .result()
            .unwrap_or_else(|e| panic!("{}: {e:?}", row["name"]));
        assert_eq!(post, &expected, "{}", row["name"]);
        assert!(a.check_redistribution(&pre, post).result().is_ok());
        assert_eq!(pre, saved);
        assert_eq!(
            repair.usage(),
            a.redistribute(&pre, expected.reward_c, expected.reward_d)
                .usage()
        );
        let mut p = p;
        p.limits = p.limits.with_limit(
            t::Resource::Step,
            repair.usage().used(t::Resource::Step) - 1,
        );
        let limited = r::Traversal::bind(p).unwrap();
        let denied = limited.redistribute(&pre, expected.reward_c, expected.reward_d);
        assert!(matches!(denied.result(), Err(t::Failure::Budget(_))));
        assert_eq!(
            denied.usage(),
            limited
                .redistribute(&pre, expected.reward_c, expected.reward_d)
                .usage()
        );
        let mut wrong = post.clone();
        wrong.order.reverse();
        if wrong != *post {
            assert!(a.check_redistribution(&pre, &wrong).result().is_err());
        }
        let mut minted = post.clone();
        minted.supply += 1;
        minted.default_debt += 1;
        assert!(a.check_redistribution(&pre, &minted).result().is_err());
        assert!(
            a.redistribute(&pre, -1, expected.reward_d)
                .result()
                .is_err()
        );
    }
    println!("REDISTRIBUTION_REFERENCE {} exact vectors", rows.len());
}
