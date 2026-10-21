//! Independent integer reference, exhaustive table domains and tested balance journeys.
use std::{collections::BTreeMap, path::PathBuf, sync::OnceLock};
use t::{Value, ValueRef};
use zeno_fcis_contract::internals::{tables, tables_review};
use zeno_fcis_synthesis::finite::v2_authority::tables as t;
fn fixtures() -> &'static PathBuf {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| {
        let root =
            std::env::temp_dir().join(format!("sol-tables-contract-tests-{}", std::process::id()));
        let tool =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tools/table_test_projects.py");
        let output = std::process::Command::new("python3")
            .arg(tool)
            .arg(&root)
            .output()
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
        assert!(output.status.success(), "{output:?}");
        root
    })
}
fn compile(name: &str) -> tables::Compiled {
    let dir = fixtures().join(name);
    tables::compile(
        &std::fs::read_to_string(dir.join("project.zeno"))
            .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
        &std::fs::read_to_string(dir.join("v2/policy.json"))
            .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
    )
    .unwrap_or_else(|e| panic!("{name}: {e}"))
}
fn authority(c: &tables::Compiled) -> t::Authority {
    t::Authority::bind(&c.schema, &c.policy)
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
}
fn record(xs: Vec<(u16, Value)>) -> Value {
    Value::record_canonical(xs.into_iter().map(|(id, v)| t::Field::new(id, v)).collect())
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
}
fn number(v: &Value) -> i128 {
    match v.view() {
        ValueRef::I128(n) => n,
        _ => panic!("not integer"),
    }
}
fn field(v: &Value, id: u16) -> &Value {
    let ValueRef::Record(fs) = v.view() else {
        panic!("not record")
    };
    fs.iter()
        .find(|f| f.id() == id)
        .unwrap_or_else(|| panic!("required value missing"))
        .value()
}
fn map(v: &Value) -> BTreeMap<i128, i128> {
    let ValueRef::Map(xs) = field(v, 110).view() else {
        panic!()
    };
    xs.iter()
        .map(|e| {
            (
                number(e.key()),
                if matches!(e.value().view(), ValueRef::Unit) {
                    0
                } else {
                    number(e.value())
                },
            )
        })
        .collect()
}
fn state(xs: &BTreeMap<i128, i128>, set: bool) -> Value {
    record(vec![(
        110,
        Value::map_canonical(
            xs.iter()
                .map(|(k, v)| {
                    t::MapEntry::try_new(
                        Value::signed(*k),
                        if set {
                            Value::unit()
                        } else {
                            Value::signed(*v)
                        },
                    )
                    .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
                })
                .collect(),
        )
        .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
    )])
}
fn command(operation: i128, key: i128, value: i128) -> Value {
    record(vec![
        (120, Value::signed(operation)),
        (121, Value::signed(key)),
        (122, Value::signed(value)),
    ])
}
fn run(a: &t::Authority, pre: &Value, command: &Value, context: &Value) -> t::Evaluation {
    let s = a
        .frame(0, pre)
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    let c = a
        .frame(1, command)
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    let x = a
        .frame(2, context)
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    a.evaluate(t::Raw {
        state: &s,
        command: &c,
        context: &x,
    })
}
#[test]
fn collection_contracts_execute_and_match_independent_reference_c0_to_c3() {
    for kind in ["map", "set"] {
        for capacity in 0..=3 {
            let compiled = compile(&format!("{kind}-{capacity}"));
            let a = authority(&compiled);
            let states = tables_review::values(&compiled.descriptor, 100, 1000)
                .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
            assert_eq!(
                states.len(),
                if kind == "map" {
                    [1, 9, 33, 65][capacity]
                } else {
                    [1, 5, 11, 15][capacity]
                }
            );
            let set = kind == "set";
            let context = record(vec![]);
            for pre in states {
                for operation in 0..=7 {
                    for key in 0..=3 {
                        for value in 0..=1 {
                            let mut expected = map(&pre);
                            let old = expected.clone();
                            let reason = match operation {
                                0 => {
                                    if !expected.contains_key(&key) && expected.len() == capacity {
                                        Some(307)
                                    } else {
                                        expected.insert(key, if set { 0 } else { value });
                                        None
                                    }
                                }
                                1 if set => Some(313),
                                1 => {
                                    if let Some(v) = expected.get_mut(&key) {
                                        *v = value;
                                        None
                                    } else {
                                        Some(303)
                                    }
                                }
                                2 => {
                                    expected.remove(&key);
                                    None
                                }
                                _ => None,
                            };
                            let wire_before = a.frame(0, &pre).unwrap_or_else(|error| {
                                panic!("required result failed: {error:?}")
                            });
                            let result = run(&a, &pre, &command(operation, key, value), &context);
                            let d = result.result().unwrap_or_else(|e| {
                                panic!("{kind} C={capacity} op={operation} pre={old:?}: {e:?}")
                            });
                            assert_eq!(d.reason(), reason);
                            if reason.is_none() {
                                assert_eq!(d.post(), Some(&state(&expected, set)));
                            } else {
                                assert!(d.post().is_none());
                                assert!(d.patch().is_empty());
                            }
                            assert_eq!(
                                a.frame(0, &pre).unwrap_or_else(|error| panic!(
                                    "required result failed: {error:?}"
                                )),
                                wire_before
                            );
                        }
                    }
                }
            }
        }
    }
}
#[test]
fn finite_review_gap_and_state_lemma_c0_to_c3() {
    for kind in ["map", "set"] {
        for c in 0..=3 {
            let p = compile(&format!("{kind}-{c}"));
            let report = tables_review::review(&p, 20000, 100000000, true)
                .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
            assert_eq!(report["status"], "finite-checked", "{kind} C={c}: {report}");
            assert_eq!(report["gaps"]["determined"], report["tuples"]);
            assert_eq!(report["lemma"]["genesis"], "checked");
            println!("TABLE_FINITE {kind}-{c} {report}");
        }
    }
}
#[test]
fn map_replace_at_capacity() {
    let c = compile("map-1");
    let a = authority(&c);
    let pre = state(&BTreeMap::from([(0, 0)]), false);
    let r = run(&a, &pre, &command(0, 0, 1), &record(vec![]));
    assert_eq!(
        r.result()
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
            .post(),
        Some(&state(&BTreeMap::from([(0, 1)]), false))
    );
}
#[test]
fn remove_absent_is_ok() {
    let c = compile("map-0");
    let a = authority(&c);
    let r = run(
        &a,
        &c.descriptor.genesis,
        &command(2, 0, 0),
        &record(vec![]),
    );
    assert!(
        r.result()
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
            .reason()
            .is_none()
    );
    assert_eq!(
        r.result()
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
            .post(),
        Some(&c.descriptor.genesis)
    );
}
#[test]
fn decode_recounts_entries() {
    let c = compile("map-2");
    let a = authority(&c);
    let pre = state(&BTreeMap::from([(0, 1)]), false);
    let mut wire = a
        .frame(0, &pre)
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    wire[56..60].copy_from_slice(&0u32.to_be_bytes());
    let cmd = a
        .frame(1, &command(6, 0, 0))
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    let context = a
        .frame(2, &record(vec![]))
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    assert!(
        a.evaluate(t::Raw {
            state: &wire,
            command: &cmd,
            context: &context
        })
        .result()
        .is_err()
    );
}
#[test]
fn decode_rejects_unsorted_duplicates() {
    let c = compile("map-3");
    let a = authority(&c);
    let pre = state(&BTreeMap::from([(0, 0), (1, 1)]), false);
    let original = a
        .frame(0, &pre)
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    let cmd = a
        .frame(1, &command(6, 0, 0))
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    let context = a
        .frame(2, &record(vec![]))
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    for duplicate in [false, true] {
        let mut wire = original.clone();
        if duplicate {
            let key = wire[64..81].to_vec();
            wire[106..123].copy_from_slice(&key);
        } else {
            let first = wire[60..102].to_vec();
            let second = wire[102..144].to_vec();
            wire[60..102].copy_from_slice(&second);
            wire[102..144].copy_from_slice(&first);
        }
        assert!(
            a.evaluate(t::Raw {
                state: &wire,
                command: &cmd,
                context: &context
            })
            .result()
            .is_err()
        );
    }
}
#[test]
fn frame_union_domain() {
    let c = compile("map-3");
    let pre = c.descriptor.genesis.clone();
    let post = state(&BTreeMap::from([(3, 1)]), false);
    let d = c.descriptor;
    let frame = d
        .laws
        .iter()
        .find(|l| matches!(l, t::Law::Frame { .. }))
        .unwrap_or_else(|| panic!("required value missing"))
        .clone();
    let mut d = d.clone();
    d.laws = vec![frame];
    assert!(
        t::check_laws(
            &d,
            &pre,
            &command(3, 0, 0),
            &record(vec![]),
            Some(&post),
            None
        )
        .is_err()
    );
}
fn balance_command(op: i128, s: i128, d: i128, asset: i128, amount: i128, nonce: i128) -> Value {
    record(vec![
        (120, Value::signed(op)),
        (121, Value::signed(s)),
        (122, Value::signed(d)),
        (123, Value::signed(asset)),
        (124, Value::signed(amount)),
        (125, Value::signed(nonce)),
    ])
}
fn auth(b: bool) -> Value {
    record(vec![(130, Value::boolean(b))])
}
fn sums(state: &Value) -> [i128; 2] {
    let ValueRef::Map(entries) = field(state, 110).view() else {
        panic!()
    };
    let mut sum = [0i128; 2];
    for e in entries {
        let ValueRef::Tuple(k) = e.key().view() else {
            panic!()
        };
        sum[number(&k[1]) as usize] = sum[number(&k[1]) as usize]
            .checked_add(number(e.value()))
            .unwrap_or_else(|| panic!("required value missing"));
    }
    sum
}
#[test]
fn balances_multiple_users_assets_and_refusal_journey() {
    let c = compile("balances");
    let a = authority(&c);
    let mut state = c.descriptor.genesis.clone();
    assert!(
        a.genesis(
            &a.frame(0, &state)
                .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
        )
        .result()
        .is_ok()
    );
    let mut nonce = 0;
    for (op, s, d, asset, amount, totals) in [
        (0, 0, 0, 0, 5, [5, 0]),
        (0, 1, 1, 0, 2, [7, 0]),
        (2, 0, 1, 0, 3, [7, 0]),
        (2, 1, 1, 0, 5, [7, 0]),
        (1, 0, 0, 0, 2, [5, 0]),
        (0, 2, 2, 1, 7, [5, 7]),
    ] {
        let r = run(
            &a,
            &state,
            &balance_command(op, s, d, asset, amount, nonce),
            &auth(true),
        );
        state = r
            .result()
            .unwrap_or_else(|e| panic!("journey op={op}: {e:?}"))
            .post()
            .unwrap_or_else(|| panic!("required value missing"))
            .clone();
        nonce += 1;
        assert_eq!(number(field(&state, 111)), nonce);
        assert_eq!(sums(&state), totals);
    }
    for (op, s, d, asset, amount, reason) in [
        (0, 3, 3, 1, 0, 309),
        (0, 3, 3, 1, 1, 307),
        (1, 3, 3, 1, 1, 303),
        (1, 0, 0, 0, 1, 304),
        (2, 3, 3, 1, 0, 309),
        (2, 3, 3, 1, 1, 303),
    ] {
        let bytes = a
            .frame(0, &state)
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
        let r = run(
            &a,
            &state,
            &balance_command(op, s, d, asset, amount, nonce),
            &auth(true),
        );
        assert_eq!(
            r.result()
                .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
                .reason(),
            Some(reason)
        );
        assert!(
            r.result()
                .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
                .post()
                .is_none()
        );
        assert_eq!(
            a.frame(0, &state)
                .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
            bytes
        );
    }
}
#[test]
fn balance_self_transfer_conserves() {
    let c = compile("balances");
    let a = authority(&c);
    for maximum in [5, i128::from(u64::MAX)] {
        let first = run(
            &a,
            &c.descriptor.genesis,
            &balance_command(0, 0, 0, 0, maximum, 0),
            &auth(true),
        );
        let pre = first
            .result()
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
            .post()
            .unwrap_or_else(|| panic!("required value missing"));
        let r = run(
            &a,
            pre,
            &balance_command(2, 0, 0, 0, maximum, 1),
            &auth(true),
        );
        let post = r
            .result()
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
            .post()
            .unwrap_or_else(|| panic!("required value missing"));
        assert_eq!(field(pre, 110), field(post, 110));
        assert_eq!(sums(pre), sums(post));
        assert_eq!(number(field(post, 111)), 2);
    }
}
#[test]
fn zero_credit_does_not_create() {
    let c = compile("balances");
    let a = authority(&c);
    for op in 0..3 {
        let r = run(
            &a,
            &c.descriptor.genesis,
            &balance_command(op, 3, 2, 1, 0, 0),
            &auth(true),
        );
        assert_eq!(
            r.result()
                .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
                .reason(),
            Some(309)
        );
        assert!(
            r.result()
                .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
                .post()
                .is_none()
        );
    }
}
#[test]
fn balance_guard_order_and_arithmetic() {
    let c = compile("balances");
    let a = authority(&c);
    let max = i128::from(u64::MAX);
    let state = record(vec![
        (110, field(&c.descriptor.genesis, 110).clone()),
        (111, Value::signed(max)),
    ]);
    for (cmd_nonce, authorized, expected) in [(0, false, 300), (0, true, 301), (max, true, 302)] {
        let r = run(
            &a,
            &state,
            &balance_command(0, 0, 0, 0, 0, cmd_nonce),
            &auth(authorized),
        );
        assert_eq!(
            r.result()
                .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
                .reason(),
            Some(expected)
        );
    }
    let r = run(
        &a,
        &c.descriptor.genesis,
        &balance_command(0, 0, 0, 0, max, 0),
        &auth(true),
    );
    let pre = r
        .result()
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
        .post()
        .unwrap_or_else(|| panic!("required value missing"));
    let overflow = run(&a, pre, &balance_command(0, 0, 0, 0, 1, 1), &auth(true));
    assert_eq!(
        overflow
            .result()
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
            .reason(),
        Some(305)
    );
}
#[test]
fn budgets_replay_and_publication_are_source_bound() {
    let c = compile("map-2");
    let a = authority(&c);
    let s = a
        .frame(0, &c.descriptor.genesis)
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    let command = command(0, 1, 1);
    let cw = a
        .frame(1, &command)
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    let xw = a
        .frame(2, &record(vec![]))
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    let raw = t::Raw {
        state: &s,
        command: &cw,
        context: &xw,
    };
    let good = a.evaluate(raw);
    let again = a.evaluate(raw);
    assert_eq!(good.subject(), again.subject());
    assert_eq!(good.usage(), again.usage());
    assert!(
        a.replay(
            raw,
            good.subject()
                .unwrap_or_else(|| panic!("required value missing"))
        )
        .result()
        .is_ok()
    );
    assert!(a.replay(raw, b"stale").result().is_err());
    let t::PublicationOutcome::Commit(p) = a.publish(raw) else {
        panic!()
    };
    assert_eq!(
        p.subject(),
        good.subject()
            .unwrap_or_else(|| panic!("required value missing"))
    );
    assert_eq!(
        p.post(),
        a.frame(
            0,
            good.result()
                .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
                .post()
                .unwrap_or_else(|| panic!("required value missing"))
        )
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
    );
    for resource in [
        t::Resource::Read,
        t::Resource::Step,
        t::Resource::Byte,
        t::Resource::Write,
        t::Resource::Candidate,
    ] {
        let used = good.usage().used(resource);
        assert!(used > 0);
        let mut exact = c.descriptor.clone();
        exact.limits = exact.limits.with_limit(resource, used);
        let exact = t::Authority::bind(
            &exact
                .schema_bytes()
                .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
            &exact
                .policy_bytes()
                .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
        )
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
        assert!(exact.evaluate(raw).result().is_ok(), "exact {resource:?}");
        let mut d = c.descriptor.clone();
        d.limits = d.limits.with_limit(resource, used - 1);
        let budget = t::Authority::bind(
            &d.schema_bytes()
                .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
            &d.policy_bytes()
                .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
        )
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
        let refused = budget.evaluate(raw);
        assert!(
            matches!(refused.result(), Err(t::Failure::Budget(_))),
            "{resource:?}: {:?}",
            refused.result()
        );
        assert!(refused.usage().used(resource) < used);
        assert!(refused.subject().is_none());
    }
}
#[test]
fn unsupported_domains_are_inconclusive_and_scalar_consumers_refuse() {
    let c = compile("balances");
    assert_eq!(
        tables_review::review(&c, 100, 100, false)
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"))["status"],
        "inconclusive"
    );
    let dir = fixtures().join("map-1");
    let rules = std::fs::read_to_string(dir.join("v2/policy.json"))
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    assert!(zeno_fcis_contract::internals::rules::Rules::read(&rules).is_err());
}

#[test]
fn sum_overflow_is_undefined() {
    let c = compile("map-2");
    let mut d = c.descriptor;
    let amount = d
        .types
        .iter_mut()
        .find(|t| t.id == 201)
        .unwrap_or_else(|| panic!("required value missing"));
    amount.shape = t::Shape::I128 {
        min: 0,
        max: i128::MAX,
    };
    assert_eq!(
        d.policy_bytes().map_or_else(
            |error| error,
            |value| panic!("expected refusal, got {value:?}")
        ),
        t::Failure::Arithmetic
    );
}
#[test]
fn law_probe_checks_successor_state_invariants() {
    let c = compile("map-3");
    let mut d = c.descriptor;
    let table = t::Expr::Input(0, vec![110]);
    let length = t::Expr::Len(204, Box::new(table));
    let zero = t::Expr::Literal(201, Value::signed(0));
    d.laws = vec![t::Law::Predicate {
        id: 410,
        scope: t::Scope::State,
        expression: t::Expr::Eq(Box::new(length), Box::new(zero)),
    }];
    let post = state(&BTreeMap::from([(3, 1)]), false);
    assert_eq!(
        t::check_laws(
            &d,
            &d.genesis,
            &command(3, 0, 0),
            &record(vec![]),
            Some(&post),
            None
        )
        .map_or_else(
            |error| error,
            |value| panic!("expected refusal, got {value:?}")
        ),
        t::Failure::Law(410)
    );
}
#[test]
fn gap_candidates_include_unused_declared_reasons_and_exclude_991() {
    let mut c = compile("map-0");
    c.descriptor.cases = vec![t::Case {
        when: t::Expr::Literal(203, Value::boolean(true)),
        plan: t::Plan::Accept(vec![]),
    }];
    c.descriptor.reasons.push(399);
    c.descriptor.laws = vec![t::Law::Predicate {
        id: 401,
        scope: t::Scope::Reject,
        expression: t::Expr::Eq(
            Box::new(t::Expr::Reason),
            Box::new(t::Expr::Literal(206, Value::signed(399))),
        ),
    }];
    c.policy = c
        .descriptor
        .policy_bytes()
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    assert_eq!(
        tables_review::review(&c, 100, 100000, true)
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"))["status"],
        "law-gaps"
    );
    c.descriptor.laws = vec![t::Law::Predicate {
        id: 991,
        scope: t::Scope::Any,
        expression: t::Expr::Eq(
            Box::new(t::Expr::Reason),
            Box::new(t::Expr::Literal(206, Value::signed(0))),
        ),
    }];
    c.policy = c
        .descriptor
        .policy_bytes()
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    let r = tables_review::review(&c, 100, 100000, true)
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    assert_eq!(r["status"], "law-gaps");
    assert_eq!(r["gaps"]["case_conformance_991_used"], false);
}
#[test]
fn table_literal_rechecks_item_and_snapshot_limits() {
    let mut c = compile("map-1");
    if let t::Shape::Table { limits, .. } = &mut c
        .descriptor
        .types
        .iter_mut()
        .find(|t| t.id == 204)
        .unwrap_or_else(|| panic!("required value missing"))
        .shape
    {
        limits.max_item_bytes = 49;
    }
    let table = field(&state(&BTreeMap::from([(0, 1)]), false), 110).clone();
    c.descriptor.laws = vec![t::Law::Predicate {
        id: 401,
        scope: t::Scope::State,
        expression: t::Expr::Eq(
            Box::new(t::Expr::Len(204, Box::new(t::Expr::Literal(204, table)))),
            Box::new(t::Expr::Literal(201, Value::signed(1))),
        ),
    }];
    assert!(c.descriptor.policy_bytes().is_err());
}
#[test]
fn byte_item_full_snapshot_and_missing_precedence() {
    for (capacity, item, snapshot, op, expected) in [
        (0, 38, 29, 0, 306),
        (0, 100, 29, 0, 307),
        (1, 100, 29, 0, 308),
        (0, 38, 29, 1, 303),
    ] {
        let mut c = compile("map-1");
        let d = &mut c.descriptor;
        d.laws.clear();
        d.frame_bytes = [2048; 3];
        d.types
            .iter_mut()
            .find(|t| t.id == 201)
            .unwrap_or_else(|| panic!("required value missing"))
            .shape = t::Shape::Bytes { min: 0, max: 100 };
        if let t::Shape::Table { limits, .. } = &mut d
            .types
            .iter_mut()
            .find(|t| t.id == 204)
            .unwrap_or_else(|| panic!("required value missing"))
            .shape
        {
            *limits = t::CollectionLimits {
                max_entries: capacity,
                max_item_bytes: item,
                max_snapshot_bytes: snapshot,
            };
        }
        let table = t::Expr::Input(0, vec![110]);
        let key = t::Expr::Input(1, vec![121]);
        let value = t::Expr::Input(1, vec![122]);
        let edit = if op == 1 {
            t::Expr::Update(204, Box::new(table), Box::new(key), Box::new(value))
        } else {
            t::Expr::Insert(204, Box::new(table), Box::new(key), Box::new(value))
        };
        d.cases = vec![t::Case {
            when: t::Expr::Literal(203, Value::boolean(true)),
            plan: t::Plan::MatchEdit(
                edit,
                Box::new(t::Plan::Accept(vec![(110, t::Expr::Payload)])),
                Box::new(t::Plan::RejectEdit),
            ),
        }];
        let a = t::Authority::bind(
            &d.schema_bytes()
                .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
            &d.policy_bytes()
                .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
        )
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
        let cmd = record(vec![
            (120, Value::signed(op)),
            (121, Value::signed(0)),
            (
                122,
                Value::bytes(vec![1])
                    .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
            ),
        ]);
        let r = run(&a, &d.genesis, &cmd, &record(vec![]));
        assert_eq!(
            r.result()
                .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
                .reason(),
            Some(expected)
        );
        assert!(
            r.result()
                .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
                .patch()
                .is_empty()
        );
    }
}
#[test]
fn subject_binds_final_usage_and_exact_patch() {
    let c = compile("map-1");
    let a = authority(&c);
    let r = run(
        &a,
        &c.descriptor.genesis,
        &command(0, 1, 1),
        &record(vec![]),
    );
    let d = r
        .result()
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    assert_eq!(d.patch().len(), 1);
    assert_eq!(d.patch()[0].0, 110);
    assert_eq!(&d.patch()[0].1, field(&c.descriptor.genesis, 110));
    assert_eq!(
        &d.patch()[0].2,
        field(
            d.post().unwrap_or_else(|| panic!("required value missing")),
            110
        )
    );
    let subject = r
        .subject()
        .unwrap_or_else(|| panic!("required value missing"));
    let mut at = 0;
    let mut artifact = &[][..];
    for i in 0..8 {
        let tag = subject[at];
        let count = u128::from_be_bytes(
            subject[at + 1..at + 17]
                .try_into()
                .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
        ) as usize;
        at += 17;
        if tag == 1 {
            if i == 7 {
                artifact = &subject[at..at + count];
            }
            at += count;
        } else {
            assert_eq!(tag, 0);
        }
    }
    assert_eq!(at, subject.len());
    let artifact =
        zeno_fcis_codec::decode_value(artifact, zeno_fcis_codec::DecodeLimits::default())
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    let ValueRef::Tuple(fields) = artifact.view() else {
        panic!()
    };
    let ValueRef::Vector(usage) = fields[3].view() else {
        panic!()
    };
    assert_eq!(
        usage.iter().map(number).collect::<Vec<_>>(),
        [
            t::Resource::Read,
            t::Resource::Write,
            t::Resource::Candidate,
            t::Resource::Effect,
            t::Resource::Byte,
            t::Resource::WitnessByte,
            t::Resource::Depth,
            t::Resource::Step
        ]
        .map(|resource| i128::from(r.usage().used(resource)))
    );
}
#[test]
fn wrong_set_value_and_malformed_genesis_are_refused() {
    let c = compile("set-1");
    let a = authority(&c);
    let invalid = record(vec![(
        110,
        Value::map_canonical(vec![
            t::MapEntry::try_new(Value::signed(0), Value::signed(0))
                .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
        ])
        .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
    )]);
    assert!(
        run(&a, &invalid, &command(0, 0, 0), &record(vec![]))
            .result()
            .is_err()
    );
    assert!(
        a.genesis(
            &a.frame(0, &invalid)
                .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
        )
        .result()
        .is_err()
    );
    let not_genesis = state(&BTreeMap::from([(0, 0)]), true);
    assert!(
        a.genesis(
            &a.frame(0, &not_genesis)
                .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
        )
        .result()
        .is_err()
    );
}
#[test]
fn balances_zero_capacity_keeps_zero_before_full_and_missing() {
    let c = compile("balances-0");
    let a = authority(&c);
    for (operation, amount, expected) in [
        (0, 0, 309),
        (0, 1, 307),
        (1, 0, 309),
        (1, 1, 303),
        (2, 0, 309),
        (2, 1, 303),
    ] {
        let result = run(
            &a,
            &c.descriptor.genesis,
            &balance_command(operation, 0, 1, 0, amount, 0),
            &auth(true),
        );
        let decision = result
            .result()
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
        assert_eq!(decision.reason(), Some(expected));
        assert!(decision.post().is_none());
        assert!(decision.patch().is_empty());
    }
}

#[test]
fn balances_laws_pin_decision_independently() {
    let c = compile("balances");
    assert!(c.descriptor.laws.iter().all(|law| law.id() != 991));
    let a = authority(&c);
    let pre = &c.descriptor.genesis;
    let cmd = balance_command(0, 0, 1, 0, 1, 0);
    let credit_post = |account| {
        record(vec![
            (
                110,
                Value::map_canonical(vec![
                    t::MapEntry::try_new(
                        Value::tuple(vec![Value::signed(account), Value::signed(0)])
                            .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
                        Value::signed(1),
                    )
                    .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
                ])
                .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
            ),
            (111, Value::signed(1)),
        ])
    };
    let wrong = credit_post(1);
    // The independent review's witness: conserved total, wrong recipient.
    assert!(
        a.check_laws(pre, &cmd, &auth(true), Some(&wrong), None)
            .is_err()
    );
    assert!(
        a.check_laws(pre, &cmd, &auth(false), Some(&wrong), None)
            .is_err()
    );
    let right = credit_post(0);
    assert!(
        a.check_laws(pre, &cmd, &auth(true), Some(&right), None)
            .is_ok()
    );
    assert!(
        a.check_laws(pre, &cmd, &auth(false), Some(&right), None)
            .is_err()
    );
    for reason in 300..=309 {
        assert!(
            a.check_laws(pre, &cmd, &auth(true), None, Some(reason))
                .is_err(),
            "reason {reason}"
        );
    }
    // Change only the producer, preserving every independent law. Reject-all,
    // unauthorized acceptance and wrong-recipient acceptance must all fail closed.
    for (post, authorized) in [(None, true), (Some(&right), false), (Some(&wrong), true)] {
        let mut d = c.descriptor.clone();
        let plan = match post {
            None => t::Plan::Reject(300),
            Some(post) => t::Plan::Accept(vec![
                (110, t::Expr::Literal(204, field(post, 110).clone())),
                (111, t::Expr::Literal(203, Value::signed(1))),
            ]),
        };
        d.cases = vec![t::Case {
            when: t::Expr::Literal(208, Value::boolean(true)),
            plan,
        }];
        let faulty = t::Authority::bind(
            &d.schema_bytes()
                .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
            &d.policy_bytes()
                .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
        )
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
        let result = run(&faulty, pre, &cmd, &auth(authorized));
        assert!(matches!(result.result(), Err(t::Failure::Law(_))));
        assert!(result.subject().is_none());
    }
}

#[test]
fn balances_finite_decisions_are_unique_without_991() {
    // Entire declared domain: two accounts, one asset, amount/nonce 0..1,
    // all three commands and both authorization contexts, C=0,1,2.
    for (capacity, tuples) in [(0, 192), (1, 960), (2, 1728)] {
        let c = compile(&format!("balances-finite-{capacity}"));
        assert!(c.descriptor.laws.iter().all(|law| law.id() != 991));
        let report = tables_review::review(&c, 20000, 100000000, true)
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
        assert_eq!(report["status"], "finite-checked", "{report}");
        assert_eq!(report["tuples"], tuples);
        assert_eq!(report["gaps"]["determined"], tuples);
        assert_eq!(report["gaps"]["case_conformance_991_used"], false);
        println!("BALANCES_FINITE {capacity} {report}");
    }
}

#[test]
fn record_literal_rechecks_nested_table_limits() {
    for (item, snapshot, admissible) in [(49, 79, false), (50, 78, false), (50, 79, true)] {
        let mut d = compile("map-1").descriptor;
        if let t::Shape::Table { limits, .. } = &mut d
            .types
            .iter_mut()
            .find(|t| t.id == 204)
            .unwrap_or_else(|| panic!("required value missing"))
            .shape
        {
            limits.max_item_bytes = item;
            limits.max_snapshot_bytes = snapshot;
        }
        let literal = state(&BTreeMap::from([(0, 1)]), false);
        let table = t::Expr::Project(Box::new(t::Expr::Literal(100, literal)), 110);
        d.cases = vec![t::Case {
            when: t::Expr::Eq(
                Box::new(t::Expr::Len(204, Box::new(table))),
                Box::new(t::Expr::Literal(201, Value::signed(1))),
            ),
            plan: t::Plan::Accept(vec![]),
        }];
        d.laws.clear();
        assert_eq!(d.policy_bytes().is_ok(), admissible);
        if admissible {
            let a = t::Authority::bind(
                &d.schema_bytes()
                    .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
                &d.policy_bytes()
                    .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
            )
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
            assert!(
                run(&a, &d.genesis, &command(3, 0, 0), &record(vec![]))
                    .result()
                    .is_ok()
            );
        }
    }
}

#[test]
fn set_item_preflight_precedes_membership_and_preserves_usage() {
    let mut d = compile("set-1").descriptor;
    d.laws.clear();
    d.frame_bytes = [2048; 3];
    d.types
        .iter_mut()
        .find(|t| t.id == 200)
        .unwrap_or_else(|| panic!("required value missing"))
        .shape = t::Shape::Bytes { min: 0, max: 1 };
    if let t::Shape::Table { limits, .. } = &mut d
        .types
        .iter_mut()
        .find(|t| t.id == 204)
        .unwrap_or_else(|| panic!("required value missing"))
        .shape
    {
        *limits = t::CollectionLimits {
            max_entries: 1,
            max_item_bytes: 22,
            max_snapshot_bytes: 51,
        };
    }
    d.cases = vec![t::Case {
        when: t::Expr::Literal(203, Value::boolean(true)),
        plan: t::Plan::MatchEdit(
            t::Expr::Insert(
                204,
                Box::new(t::Expr::Input(0, vec![110])),
                Box::new(t::Expr::Input(1, vec![121])),
                Box::new(t::Expr::Literal(205, Value::unit())),
            ),
            Box::new(t::Plan::Accept(vec![(110, t::Expr::Payload)])),
            Box::new(t::Plan::RejectEdit),
        ),
    }];
    let pre = record(vec![(
        110,
        Value::map_canonical(vec![
            t::MapEntry::try_new(
                Value::bytes(vec![])
                    .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
                Value::unit(),
            )
            .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
        ])
        .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
    )]);
    let cmd = record(vec![
        (120, Value::signed(0)),
        (
            121,
            Value::bytes(vec![0])
                .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
        ),
        (122, Value::signed(0)),
    ]);
    let evaluate = |d: &t::Descriptor| {
        let a = t::Authority::bind(
            &d.schema_bytes()
                .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
            &d.policy_bytes()
                .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
        )
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
        run(&a, &pre, &cmd, &record(vec![]))
    };
    let good = evaluate(&d);
    assert_eq!(
        good.result()
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
            .reason(),
        Some(306)
    );
    assert_eq!(good.usage().used(t::Resource::Read), 9);
    for (resource, expected) in [
        (t::Resource::Read, 9),
        (t::Resource::Write, 0),
        (t::Resource::Candidate, 1),
        (t::Resource::Effect, 0),
        (t::Resource::Byte, 3538),
        (t::Resource::WitnessByte, 0),
        (t::Resource::Depth, 0),
        (t::Resource::Step, 120),
    ] {
        assert_eq!(good.usage().used(resource), expected, "{resource:?}");
    }
    d.limits = d.limits.with_limit(t::Resource::Read, 9);
    let limited = evaluate(&d);
    assert_eq!(
        limited
            .result()
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
            .reason(),
        Some(306)
    );
    assert_eq!(limited.usage(), good.usage());
    assert_eq!(evaluate(&d).usage(), limited.usage());
}
