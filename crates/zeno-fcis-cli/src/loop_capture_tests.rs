use super::*;
use zeno_fcis_synthesis::finite::{Domain, Op};

fn ok<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
    r.unwrap_or_else(|e| panic!("fixture failed: {e:?}"))
}
fn fixture() -> proposal::ReferenceTable {
    ok(proposal::ReferenceTable::new(
        vec![Domain::Int { min: 0, max: 3 }],
        vec![Domain::Bool],
        vec![vec![1], vec![1], vec![0], vec![0]],
    ))
}
#[test]
fn capture_strict_json_rejects_duplicate_fields_floats_trailing_and_deep_input() {
    for bytes in [b"{\"a\":1,\"a\":2}".as_slice(), b"{\"a\":1.0}", b"{}{}"] {
        assert!(strict_json(bytes).is_err());
    }
    assert!(strict_json(format!("{}0{}", "[".repeat(200), "]".repeat(200)).as_bytes()).is_err());
    assert_eq!(
        ok(strict_json(b"[true,null,-1,18446744073709551615]")),
        json!([true, null, -1, u64::MAX])
    );
}
#[test]
fn capture_reference_protocol_refuses_booleans_wrong_arity_and_outside_outputs() {
    let p = ok(fixture().program());
    for reply in [
        json!({"rows":[[true]]}),
        json!({"rows":[[2]]}),
        json!({"rows":[[0,1]]}),
        json!({"rows":[]}),
        json!({"rows":[[0]],"extra":1}),
    ] {
        assert!(reference_rows(reply, &p, 1).is_err());
    }
}

#[test]
fn capture_reference_replay_control_fails_with_stubbed_comparison() {
    let order = vec![0, 1];
    let first = vec![vec![0], vec![1]];
    let stable = vec![vec![0], vec![1], vec![0], vec![1]];
    let nondeterministic = vec![vec![0], vec![1], vec![0], vec![0]];
    let controls = |stub: bool| {
        let same = |a: &[Vec<i64>], b: &[Vec<i64>]| stub || a == b;
        reconcile_reference(Some(&first), &order, &stable, same).is_ok()
            && reconcile_reference(Some(&first), &order, &nondeterministic, same).is_err()
            && reconcile_reference(Some(&[vec![1], vec![1]]), &order, &stable, same).is_err()
    };
    assert!(controls(false));
    assert!(!controls(true));
}
#[test]
fn capture_expressions_cannot_read_original_outputs_or_use_holes_keep_parameters() {
    let t = fixture();
    for expr in ["keep", "?e", "?c", "$C", "out0", "unknown"] {
        let bytes = ok(serde_json::to_vec(
            &json!({"schema":"zeno-fcis/capture-expressions/1","outputs":[expr]}),
        ));
        assert!(proposed_program(&bytes, true, &t).is_err(), "{expr}");
    }
    let bytes = ok(serde_json::to_vec(
        &json!({"schema":"zeno-fcis/capture-expressions/1","outputs":["x0 < 2"]}),
    ));
    let p = ok(proposed_program(&bytes, true, &t));
    assert!(ok(proposal::differences(&t, &p)).is_empty());
}

// Exercise the production mapping seam, including a mismatched receipt that
// must fail replay. Each stub MUST falsify one load-bearing assertion.
fn mapping_controls(always_equivalent: bool, always_matched: bool) -> bool {
    let t = fixture();
    let reference = ok(encode(&ok(t.program())));
    let right = ok(encode(&ok(Program::try_new(
        t.inputs().to_vec(),
        t.outputs().to_vec(),
        vec![Op::Input(0), Op::Int(2), Op::Lt(0, 1)],
        vec![2],
    ))));
    let wrong = ok(encode(&ok(Program::try_new(
        t.inputs().to_vec(),
        t.outputs().to_vec(),
        vec![Op::Bool(false)],
        vec![0],
    ))));
    let limits = transform::Limits {
        steps: transform::DEFAULT_STEP_LIMIT,
        input_tuples: 4096,
    };
    let mapped = |candidate: &[u8], forged: bool| {
        let checked = transform::check(
            &reference,
            if always_equivalent || forged {
                &reference
            } else {
                candidate
            },
            limits,
        );
        checker_reply(checked, |receipt| {
            if always_matched {
                transform::Replay::Matched
            } else {
                transform::replay(receipt, &reference, candidate, 4096)
            }
        })
    };
    let good = mapped(&right, false);
    let bad = mapped(&wrong, false);
    let forged = mapped(&wrong, true);
    good.0["verdict"] == "equivalent"
        && good.0["replay"] == "matched"
        && good.1.is_some()
        && bad.0["verdict"] == "counterexample"
        && bad.1.is_none()
        && forged.0["replay"] == "differs"
}
#[test]
fn capture_production_mapping_controls_fail_under_equivalence_and_replay_stubs() {
    assert!(mapping_controls(false, false));
    assert!(!mapping_controls(true, false));
    assert!(!mapping_controls(false, true));
    assert!(!mapping_controls(true, true));
}
#[test]
fn capture_public_detail_hides_reservations_and_reference_receipt_fields() {
    let private = json!({"exact_reserved_visits": 57216, "precheck_reserved_visits": 512,
        "check":{"verdict":"counterexample","ordinal":3,"receipt_sha256":"private"}});
    let public = public_detail(private.clone());
    for field in ["exact_reserved_visits", "precheck_reserved_visits"] {
        assert!(public.get(field).is_none());
        assert!(private.get(field).is_some());
    }
    assert!(public["check"].get("ordinal").is_none());
    assert!(public["check"].get("receipt_sha256").is_none());
}
#[test]
fn capture_reference_runtime_failures_are_refusals_and_search_deadlines_are_inconclusive() {
    for reason in [
        "capture-worker-timeout",
        "capture-worker-failed",
        "capture-worker-output-bound",
    ] {
        assert!(!budget_stop(reason));
        assert_eq!(refused(reason).0, crate::INVALID);
        assert_eq!(refused(reason).1["status"], "refused");
    }
    for reason in ["stage-deadline", "invocation-deadline", "stage-budget"] {
        assert!(budget_stop(reason));
        assert_eq!(inconclusive(reason).0, crate::BLOCKED);
        assert_eq!(inconclusive(reason).1["status"], "inconclusive");
    }
}
#[test]
fn capture_whole_domain_single_faults_and_changed_receipts_fail_existing_checker() {
    let t = fixture();
    let a = ok(encode(&ok(t.program())));
    for i in 0..4 {
        let mut rows = t.rows().to_vec();
        rows[i][0] = 1 - rows[i][0];
        let bad = ok(proposal::ReferenceTable::new(
            t.inputs().to_vec(),
            t.outputs().to_vec(),
            rows,
        ));
        let b = ok(encode(&ok(bad.program())));
        let transform::Rejection::Counterexample(c) = transform::check(
            &a,
            &b,
            transform::Limits {
                steps: transform::DEFAULT_STEP_LIMIT,
                input_tuples: 4096,
            },
        )
        .err()
        .unwrap_or_else(|| panic!("fault accepted")) else {
            panic!("no counterexample")
        };
        assert_eq!(c.ordinal, i as u64);
    }
    let equal = ok(transform::check(
        &a,
        &a,
        transform::Limits {
            steps: transform::DEFAULT_STEP_LIMIT,
            input_tuples: 4096,
        },
    ));
    let mut receipt = ok(strict_json(&equal.receipt()));
    receipt["inputs_checked"] = json!(0);
    assert_ne!(
        transform::replay(&ok(serde_json::to_vec(&receipt)), &a, &a, 4096),
        transform::Replay::Matched
    );
}
