//! Exact unsigned schema, finite alphabet, static-bound and legacy-byte checks.
use std::{collections::BTreeMap, path::PathBuf, sync::OnceLock};
use t::{Value, ValueRef};
use zeno_fcis_codec::{CommitmentHasher, RustCryptoSha256};
use zeno_fcis_contract::internals::{tables, tables_review};
use zeno_fcis_synthesis::finite::v2_authority::tables as t;
fn fixtures() -> &'static PathBuf {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| {
        let root = std::env::temp_dir().join(format!("u128-table-tests-{}", std::process::id()));
        for (script, family) in [
            ("u128_table", "unsigned"),
            ("table", "table"),
            ("ordered", "ordered"),
        ] {
            let output = std::process::Command::new("python3")
                .arg(
                    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join(format!("../../tools/{script}_test_projects.py")),
                )
                .arg(root.join(family))
                .output()
                .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
            assert!(output.status.success(), "{output:?}");
        }
        root
    })
}
fn compile(family: &str, name: &str) -> tables::Compiled {
    let p = fixtures().join(family).join(name);
    tables::compile(
        &std::fs::read_to_string(p.join("project.zeno"))
            .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
        &std::fs::read_to_string(p.join("v2/policy.json"))
            .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
    )
    .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
}
fn record(xs: Vec<(u16, Value)>) -> Value {
    Value::record_canonical(xs.into_iter().map(|(id, v)| t::Field::new(id, v)).collect())
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
}
fn state(entries: &BTreeMap<u128, u128>) -> Value {
    record(vec![(
        110,
        Value::map_canonical(
            entries
                .iter()
                .map(|(k, v)| {
                    t::MapEntry::try_new(Value::unsigned(*k), Value::unsigned(*v))
                        .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
                })
                .collect(),
        )
        .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
    )])
}
fn command(op: i128, key: u128, value: u128) -> Value {
    record(vec![
        (120, Value::signed(op)),
        (121, Value::unsigned(key)),
        (122, Value::unsigned(value)),
    ])
}
fn run(a: &t::Authority, pre: &Value, cmd: &Value) -> t::Evaluation {
    let s = a
        .frame(0, pre)
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    let c = a
        .frame(1, cmd)
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    let x = a
        .frame(2, &record(vec![]))
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    a.evaluate(t::Raw {
        state: &s,
        command: &c,
        context: &x,
    })
}
fn check(holds: bool, witness: &str) {
    if std::env::var_os("U128_TABLE_DISABLE_COMPARISON").is_none() {
        assert!(holds, "U128_TABLE_MISMATCH {witness}");
    }
}
#[test]
fn u128_table_exhaustive_small_alphabet() {
    let values = [0, 1, 1u128 << 127, u128::MAX];
    let keys = [0, u128::MAX];
    let mut total = 0;
    for capacity in 0..=3 {
        let c = compile("unsigned", &format!("map-{capacity}"));
        let a = t::Authority::bind(&c.schema, &c.policy)
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
        for left in 0..=4 {
            for right in 0..=4 {
                let mut entries = BTreeMap::new();
                if left < 4 {
                    entries.insert(keys[0], values[left]);
                }
                if right < 4 {
                    entries.insert(keys[1], values[right]);
                }
                if entries.len() > capacity {
                    continue;
                }
                let pre = state(&entries);
                let frozen = pre.clone();
                for op in 0..8 {
                    for key in keys {
                        for value in values {
                            let mut expected = entries.clone();
                            let mut reason = None;
                            match op {
                                0 if !entries.contains_key(&key) && entries.len() == capacity => {
                                    reason = Some(307)
                                }
                                0 => {
                                    expected.insert(key, value);
                                }
                                1 if !entries.contains_key(&key) => reason = Some(303),
                                1 => {
                                    expected.insert(key, value);
                                }
                                2 => {
                                    expected.remove(&key);
                                }
                                _ => {}
                            }
                            let cmd = command(op, key, value);
                            let eval = run(&a, &pre, &cmd);
                            let decision = eval.result().unwrap_or_else(|e| {
                                panic!("C={capacity} op={op} {entries:?}: {e:?}")
                            });
                            assert_eq!(decision.reason(), reason);
                            assert_eq!(
                                decision.post(),
                                reason.is_none().then(|| state(&expected)).as_ref()
                            );
                            assert!(
                                a.check_laws(&pre, &cmd, &record(vec![]), decision.post(), reason)
                                    .is_ok()
                            );
                            assert_eq!(pre, frozen);
                            assert_eq!(eval.usage(), run(&a, &pre, &cmd).usage());
                            total += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(total, 3840);
}
#[test]
fn u128_table_keys_preserve_kind() {
    println!("U128_TABLE_WITNESS key MAX remains unsigned");
    let c = compile("unsigned", "map-2");
    let a = t::Authority::bind(&c.schema, &c.policy)
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    let pre = state(&BTreeMap::from([(u128::MAX, 1u128 << 127)]));
    let cmd = command(3, u128::MAX, 0);
    let e = run(&a, &pre, &cmd);
    check(
        matches!(e.result(),Ok(d) if d.reason().is_none() && d.post()==Some(&pre)),
        "exact high-key lookup",
    );
    // Signed bytes cannot enter an unsigned key schema, even on the shared interval.
    let mixed = record(vec![(
        110,
        Value::map_canonical(vec![
            t::MapEntry::try_new(Value::signed(0), Value::unsigned(0))
                .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
        ])
        .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
    )]);
    assert!(a.check_state(&mixed).is_err());
    let s = a
        .frame(0, &pre)
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    let cmd = a
        .frame(1, &command(2, u128::MAX, 0))
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    let x = a
        .frame(2, &record(vec![]))
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    if std::env::var_os("U128_TABLE_DISABLE_COMPARISON").is_none() {
        let published = a.publish(t::Raw {
            state: &s,
            command: &cmd,
            context: &x,
        });
        assert!(matches!(published, t::PublicationOutcome::Commit(_)));
    }
}
#[test]
fn u128_sum_requires_static_bound() {
    println!("U128_TABLE_WITNESS empty sum with unestablished body bound");
    let p = fixtures().join("unsigned/map-0");
    let source = std::fs::read_to_string(p.join("project.zeno"))
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    let mut rules: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(p.join("v2/policy.json"))
            .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
    )
    .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    // Each source is U128, but adding 1 to its declared MAX has no static fit bound.
    rules["laws"] = serde_json::json!([{"id":500,"scope":"state","assert":["eq",
        ["sum_u128",["input","pre",[110]],["add",["value"],["literal",201,"1"]]],
        ["literal",201,"0"]]}]);
    let result = tables::compile(&source, &rules.to_string());
    check(result.is_err(), "sum without static bound was admitted");
    // The wide type is law-only and cannot authorize a state assignment or guard.
    let mut d = compile("unsigned", "map-0").descriptor;
    d.cases[0].when = t::Expr::Eq(
        Box::new(t::Expr::SumU128(
            Box::new(t::Expr::Input(0, vec![110])),
            Box::new(t::Expr::EntryValue),
        )),
        Box::new(t::Expr::Literal(201, Value::unsigned(0))),
    );
    assert!(d.policy_bytes().is_err());
}
#[test]
fn legacy_i128_table_bytes_are_unchanged() {
    let baseline: serde_json::Value =
        serde_json::from_str(include_str!("test-data/u128-tables/legacy-i128.json"))
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    for row in baseline["rows"]
        .as_array()
        .unwrap_or_else(|| panic!("required value missing"))
    {
        let c = compile(
            row["family"]
                .as_str()
                .unwrap_or_else(|| panic!("required value missing")),
            row["name"]
                .as_str()
                .unwrap_or_else(|| panic!("required value missing")),
        );
        for (name, bytes) in [("schema", c.schema), ("policy", c.policy)] {
            let hash: String = RustCryptoSha256::hash(&bytes)
                .as_bytes()
                .iter()
                .map(|n| format!("{n:02x}"))
                .collect();
            assert_eq!(
                hash,
                row[name]
                    .as_str()
                    .unwrap_or_else(|| panic!("required value missing")),
                "{} {name}",
                row["name"]
            );
        }
    }
}
#[test]
fn u128_table_decimal_and_schema_bounds_are_exact() {
    let p = fixtures().join("unsigned/map-1");
    let source = std::fs::read_to_string(p.join("project.zeno"))
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    let rules: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(p.join("v2/policy.json"))
            .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
    )
    .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    for bad in [
        serde_json::json!(0),
        serde_json::json!(true),
        serde_json::json!("01"),
        serde_json::json!("-1"),
        serde_json::json!("340282366920938463463374607431768211456"),
    ] {
        let mut r = rules.clone();
        r["genesis"]["110"] = serde_json::json!([[bad, "1"]]);
        assert!(tables::compile(&source, &r.to_string()).is_err());
    }
    let c = compile("unsigned", "map-1");
    assert!(tables_review::values(&c.descriptor, 200, 1000).is_err());
    let source = source.replace(&format!("Key in 0..={}", u128::MAX), "Key in 0..=1");
    let c = tables::compile(&source, &rules.to_string())
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    let a = t::Authority::bind(&c.schema, &c.policy)
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    assert!(a.check_state(&state(&BTreeMap::from([(2, 0)]))).is_err());
}

fn unsigned_item(p: u128) -> Value {
    record(vec![
        (1, Value::unsigned(p)),
        (2, Value::unit()),
        (3, Value::boolean(false)),
    ])
}
fn ordered_state(items: &[(i128, u128)], ids: &[i128]) -> Value {
    let mut entries: Vec<_> = items
        .iter()
        .map(|(id, p)| {
            t::MapEntry::try_new(Value::signed(*id), unsigned_item(*p))
                .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
        })
        .collect();
    entries.sort_by(|a, b| a.encoded_key().cmp(b.encoded_key()));
    record(vec![
        (
            110,
            Value::map_canonical(entries)
                .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
        ),
        (
            111,
            Value::vector(ids.iter().map(|id| Value::signed(*id)).collect())
                .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
        ),
    ])
}
#[test]
fn u128_ordered_v1_high_priorities_and_complete_laws() {
    let c = compile("unsigned", "ordered-3");
    let a = t::Authority::bind(&c.schema, &c.policy)
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    for priorities in [
        [1, 1, 1],
        [1, 1u128 << 127, u128::MAX],
        [u128::MAX, 1, 1u128 << 127],
    ] {
        let mut pre = ordered_state(&[], &[]);
        let mut items: Vec<(i128, u128)> = vec![];
        let mut ids: Vec<i128> = vec![];
        for (id, p) in priorities.into_iter().enumerate() {
            let id = id as i128;
            let cmd = record(vec![
                (120, Value::signed(0)),
                (121, Value::signed(id)),
                (122, unsigned_item(p)),
                (123, Value::sum(207, 0, None)),
                (124, Value::sum(207, 0, None)),
            ]);
            // V1 with absent hints inserts before the head on equality; otherwise
            // the first descending gap. Each independent complete law checks it.
            let pos = ids
                .iter()
                .position(|key| {
                    items
                        .iter()
                        .find(|(k, _)| k == key)
                        .unwrap_or_else(|| panic!("required value missing"))
                        .1
                        <= p
                })
                .unwrap_or(ids.len());
            ids.insert(pos, id);
            items.push((id, p));
            let expected = ordered_state(&items, &ids);
            let e = run(&a, &pre, &cmd);
            let d = e
                .result()
                .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
            assert_eq!(d.post(), Some(&expected));
            assert!(
                a.check_ordered_result(&pre, &cmd, &record(vec![]), d.post(), None, None)
                    .is_ok()
            );
            pre = expected;
        }
        let framed = a
            .frame(0, &pre)
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
        let query = a.ordered_query(&framed, true);
        let ValueRef::Tuple(pair) = query
            .result()
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
            .unwrap_or_else(|| panic!("required value missing"))
            .view()
        else {
            panic!()
        };
        assert_eq!(
            pair[0],
            Value::signed(
                *ids.last()
                    .unwrap_or_else(|| panic!("required value missing"))
            )
        );
    }
}
#[test]
fn u128_table_noncanonical_and_limit_refusals_keep_work() {
    let c = compile("unsigned", "map-2");
    let a = t::Authority::bind(&c.schema, &c.policy)
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    let pre = state(&BTreeMap::from([(0, 1), (u128::MAX, u128::MAX)]));
    let raw = a
        .frame(0, &pre)
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    let cmd = a
        .frame(1, &command(3, u128::MAX, 0))
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    let ctx = a
        .frame(2, &record(vec![]))
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    // The map starts after the 48-byte envelope and the 7-byte record/field prefix.
    for bad in [0u32, 1, 3] {
        let mut wire = raw.clone();
        wire[56..60].copy_from_slice(&bad.to_be_bytes());
        let e = a.evaluate(t::Raw {
            state: &wire,
            command: &cmd,
            context: &ctx,
        });
        assert!(matches!(e.result(), Err(t::Failure::Wire)));
        assert!(e.usage().used(t::Resource::Step) > 0);
        assert_eq!(
            e.usage(),
            a.evaluate(t::Raw {
                state: &wire,
                command: &cmd,
                context: &ctx
            })
            .usage()
        );
    }
    for duplicate in [false, true] {
        let mut wire = raw.clone();
        if duplicate {
            wire[102..144].copy_from_slice(&raw[60..102]);
        } else {
            wire[60..102].copy_from_slice(&raw[102..144]);
            wire[102..144].copy_from_slice(&raw[60..102]);
        }
        assert!(matches!(
            a.evaluate(t::Raw {
                state: &wire,
                command: &cmd,
                context: &ctx
            })
            .result(),
            Err(t::Failure::Wire)
        ));
    }
    for item_bound in [49, 50] {
        let mut d = c.descriptor.clone();
        let t::Shape::Table { ref mut limits, .. } = d
            .types
            .iter_mut()
            .find(|t| t.id == 204)
            .unwrap_or_else(|| panic!("required value missing"))
            .shape
        else {
            panic!()
        };
        limits.max_item_bytes = item_bound;
        let bound = t::Authority::bind(
            &d.schema_bytes()
                .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
            &d.policy_bytes()
                .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
        )
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
        assert_eq!(bound.check_state(&pre).is_ok(), item_bound == 50);
    }
    let full = run(&a, &pre, &command(3, u128::MAX, 0));
    let mut d = c.descriptor.clone();
    d.limits = d
        .limits
        .with_limit(t::Resource::Step, full.usage().used(t::Resource::Step) - 1);
    let limited = t::Authority::bind(
        &d.schema_bytes()
            .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
        &d.policy_bytes()
            .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
    )
    .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    let one = run(&limited, &pre, &command(3, u128::MAX, 0));
    assert!(matches!(one.result(), Err(t::Failure::Budget(_))));
    assert_eq!(
        one.usage(),
        run(&limited, &pre, &command(3, u128::MAX, 0)).usage()
    );
    assert_eq!(
        pre,
        state(&BTreeMap::from([(0, 1), (u128::MAX, u128::MAX)]))
    );
}

#[test]
fn u128_table_arithmetic_and_destination_refuse_without_output() {
    let c = compile("unsigned", "map-1");
    for bounded in [false, true] {
        let mut d = c.descriptor.clone();
        if bounded {
            d.types
                .iter_mut()
                .find(|t| t.id == 201)
                .unwrap_or_else(|| panic!("required value missing"))
                .shape = t::Shape::U128 { min: 0, max: 1 };
        }
        // This probe isolates checked arithmetic and the destination schema;
        // the full finite suite above separately checks independent table laws.
        d.laws.clear();
        d.cases = vec![t::Case {
            when: t::Expr::Literal(203, Value::boolean(true)),
            plan: t::Plan::MatchEdit(
                t::Expr::Insert(
                    204,
                    Box::new(t::Expr::Input(0, vec![110])),
                    Box::new(t::Expr::Input(1, vec![121])),
                    Box::new(t::Expr::Add(
                        Box::new(t::Expr::Input(1, vec![122])),
                        Box::new(t::Expr::Literal(201, Value::unsigned(1))),
                    )),
                ),
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
        let pre = state(&BTreeMap::new());
        let saved = pre.clone();
        let cmd = command(0, u128::MAX, if bounded { 1 } else { u128::MAX });
        let e = run(&a, &pre, &cmd);
        assert!(
            matches!(e.result(), Err(t::Failure::Wire)) && bounded
                || matches!(e.result(), Err(t::Failure::Arithmetic)) && !bounded
        );
        assert!(e.subject().is_none());
        assert_eq!(pre, saved);
        assert_eq!(e.usage(), run(&a, &pre, &cmd).usage());
        let ok = run(&a, &pre, &command(0, u128::MAX, 0));
        assert_eq!(
            ok.result()
                .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
                .post(),
            Some(&state(&BTreeMap::from([(u128::MAX, 1)])))
        );
    }
}

#[test]
fn u128_set_keys_cover_the_complete_small_alphabet() {
    use std::collections::BTreeSet;
    fn set(keys: &BTreeSet<u128>) -> Value {
        record(vec![(
            110,
            Value::map_canonical(
                keys.iter()
                    .map(|k| {
                        t::MapEntry::try_new(Value::unsigned(*k), Value::unit())
                            .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
                    })
                    .collect(),
            )
            .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
        )])
    }
    let keys = [0, u128::MAX];
    let mut total = 0;
    for capacity in 0..=3 {
        let c = compile("unsigned", &format!("set-{capacity}"));
        let a = t::Authority::bind(&c.schema, &c.policy)
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
        for mask in 0..4 {
            let old: BTreeSet<_> = keys
                .into_iter()
                .enumerate()
                .filter_map(|(i, k)| (mask & (1 << i) != 0).then_some(k))
                .collect();
            if old.len() > capacity {
                continue;
            }
            for op in 0..8 {
                for key in keys {
                    let mut expected = old.clone();
                    let mut reason = None;
                    match op {
                        0 if !old.contains(&key) && old.len() == capacity => reason = Some(307),
                        0 => {
                            expected.insert(key);
                        }
                        1 => reason = Some(313),
                        2 => {
                            expected.remove(&key);
                        }
                        _ => {}
                    }
                    let pre = set(&old);
                    let e = run(&a, &pre, &command(op, key, 0));
                    let result = e
                        .result()
                        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
                    assert_eq!(result.reason(), reason);
                    assert_eq!(
                        result.post(),
                        reason.is_none().then(|| set(&expected)).as_ref()
                    );
                    total += 1;
                }
            }
        }
    }
    assert_eq!(total, 192);
}
