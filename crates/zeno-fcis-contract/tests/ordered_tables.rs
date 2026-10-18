//! Independent finite ordered-view qualification; no law 991 or producer conformance.
use std::{path::PathBuf, sync::OnceLock};
use t::{Value, ValueRef};
use zeno_fcis_contract::internals::{tables, tables_review};
use zeno_fcis_synthesis::finite::v2_authority::tables as t;
fn fixtures() -> &'static PathBuf {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| {
        let root = std::env::temp_dir().join(format!("sol-ordered-tests-{}", std::process::id()));
        let r = std::process::Command::new("python3")
            .arg(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tools/ordered_test_projects.py"),
            )
            .arg(&root)
            .output()
            .unwrap();
        assert!(r.status.success(), "{r:?}");
        root
    })
}
fn compile(name: &str) -> tables::Compiled {
    let p = fixtures().join(name);
    tables::compile(
        &std::fs::read_to_string(p.join("project.zeno")).unwrap(),
        &std::fs::read_to_string(p.join("v2/policy.json")).unwrap(),
    )
    .unwrap_or_else(|e| panic!("{name}: {e}"))
}
fn record(xs: Vec<(u16, Value)>) -> Value {
    Value::record_canonical(xs.into_iter().map(|(id, v)| t::Field::new(id, v)).collect()).unwrap()
}
fn field(v: &Value, id: u16) -> &Value {
    let ValueRef::Record(fs) = v.view() else {
        panic!()
    };
    fs.iter().find(|f| f.id() == id).unwrap().value()
}
fn number(v: &Value) -> i128 {
    let ValueRef::I128(x) = v.view() else {
        panic!()
    };
    x
}
fn item(p: i128, b: bool) -> Value {
    record(vec![
        (1, Value::signed(p)),
        (2, Value::unit()),
        (3, Value::boolean(b)),
    ])
}
fn state(mode: u8, items: &[(i128, i128, bool)], order: &[i128]) -> Value {
    let mut es = items
        .iter()
        .map(|(id, p, b)| t::MapEntry::try_new(Value::signed(*id), item(*p, *b)).unwrap())
        .collect::<Vec<_>>();
    es.sort_by(|a, b| a.encoded_key().cmp(b.encoded_key()));
    let mut fs = vec![(110, Value::map_canonical(es).unwrap())];
    if mode != 0 {
        fs.push((
            111,
            Value::vector(order.iter().map(|x| Value::signed(*x)).collect()).unwrap(),
        ));
    }
    record(fs)
}
fn command(
    mode: u8,
    op: i128,
    id: i128,
    p: i128,
    b: bool,
    upper: Option<i128>,
    lower: Option<i128>,
) -> Value {
    let mut fs = vec![
        (120, Value::signed(op)),
        (121, Value::signed(id)),
        (122, item(p, b)),
    ];
    if mode != 0 {
        fs.extend([
            (
                123,
                Value::sum(
                    207,
                    if upper.is_some() { 1 } else { 0 },
                    upper.map(Value::signed),
                ),
            ),
            (
                124,
                Value::sum(
                    207,
                    if lower.is_some() { 1 } else { 0 },
                    lower.map(Value::signed),
                ),
            ),
        ]);
    }
    record(fs)
}
fn run(a: &t::Authority, pre: &Value, cmd: &Value) -> t::Evaluation {
    let s = a.frame(0, pre).unwrap();
    let c = a.frame(1, cmd).unwrap();
    let x = a.frame(2, &record(vec![])).unwrap();
    a.evaluate(t::Raw {
        state: &s,
        command: &c,
        context: &x,
    })
}
fn valid(v: &Value, mode: u8) -> bool {
    if mode == 0 {
        return true;
    }
    let ValueRef::Map(es) = field(v, 110).view() else {
        panic!()
    };
    let ValueRef::Vector(xs) = field(v, 111).view() else {
        panic!()
    };
    if es.len() != xs.len() {
        return false;
    }
    let mut seen = Vec::new();
    let mut previous = i128::MAX;
    for id in xs {
        if seen.contains(&number(id)) {
            return false;
        }
        seen.push(number(id));
        let Some(e) = es.iter().find(|e| e.key() == id) else {
            return false;
        };
        let p = number(field(e.value(), 1));
        if p > previous {
            return false;
        }
        previous = p;
    }
    true
}
#[test]
fn ordered_complete_domains_have_unique_lawful_outcomes() {
    let mut total = [0usize; 2];
    for mode in 0..=1 {
        for capacity in 0..=3 {
            let c = compile(&format!("ordered-{mode}-{capacity}"));
            let a = t::Authority::bind(&c.schema, &c.policy).unwrap();
            let states = tables_review::values(&c.descriptor, 100, 10000)
                .unwrap()
                .into_iter()
                .filter(|s| valid(s, mode))
                .collect::<Vec<_>>();
            assert_eq!(
                states.len(),
                if mode == 0 {
                    [1, 9, 25, 25]
                } else {
                    [1, 9, 33, 33]
                }[capacity]
            );
            let commands = tables_review::values(&c.descriptor, 101, 1000).unwrap();
            assert_eq!(commands.len(), if mode == 0 { 32 } else { 288 });
            for pre in &states {
                for cmd in &commands {
                    let evaluation = run(&a, pre, cmd);
                    let decision = evaluation.result().unwrap_or_else(|e| {
                        panic!("mode={mode} C={capacity} pre={pre:?} cmd={cmd:?}: {e:?}")
                    });
                    let report = a
                        .check_laws(
                            pre,
                            cmd,
                            &record(vec![]),
                            decision.post(),
                            decision.reason(),
                        )
                        .unwrap();
                    assert!(report.iter().all(|r| r.holds), "{report:?}");
                    // Independent complete-law predicate is checked against every business reason;
                    // reject-all and a wrong reason must have no alternative lawful decision.
                    for reason in [303, 306, 307, 308, 314, 315] {
                        if decision.reason() != Some(reason) {
                            assert!(
                                a.check_laws(pre, cmd, &record(vec![]), None, Some(reason))
                                    .is_err()
                            );
                        }
                    }
                    for candidate in &states {
                        let lawful = a
                            .check_laws(pre, cmd, &record(vec![]), Some(candidate), None)
                            .is_ok();
                        assert_eq!(
                            lawful,
                            decision.post() == Some(candidate),
                            "independent complete successor domain"
                        );
                    }
                    a.check_ordered_result(
                        pre,
                        cmd,
                        &record(vec![]),
                        decision.post(),
                        decision.reason(),
                        decision.removed(),
                    )
                    .unwrap();
                    if decision.removed().is_some() {
                        assert!(
                            a.check_ordered_result(
                                pre,
                                cmd,
                                &record(vec![]),
                                decision.post(),
                                None,
                                None
                            )
                            .is_err()
                        );
                    }
                    total[mode as usize] += 1;
                }
            }
        }
    }
    assert_eq!(total, [1920, 21888]);
    println!("ORDERED_FINITE generic={} committed={}", total[0], total[1]);
}

#[test]
fn ordered_complete_laws_admit_all_frames_at_138_139_boundary() {
    let compiled = compile("ordered-1-1");
    let pre = state(1, &[], &[]);
    let post = state(1, &[(0, 1, false)], &[0]);
    let cmd = command(1, 0, 0, 1, false, None, None);
    let ctx = record(vec![]);
    for bound in [67, 138, 139] {
        let mut d = compiled.descriptor.clone();
        d.laws.retain(|law| law.id() != 991);
        d.frame_bytes[0] = bound;
        let a = t::Authority::bind(&d.schema_bytes().unwrap(), &d.policy_bytes().unwrap()).unwrap();
        assert_eq!(a.frame(0, &pre).unwrap().len(), 67);
        assert_eq!(a.frame(0, &post).unwrap().len(), 139);
        assert!(a.check_state(&pre).is_ok());
        let evaluation = run(&a, &pre, &cmd);
        let laws = a.check_laws(&pre, &cmd, &ctx, Some(&post), None);
        let result = a.check_ordered_result(&pre, &cmd, &ctx, Some(&post), None, None);
        if bound < 139 {
            assert!(matches!(evaluation.result(), Err(t::Failure::Wire)));
            assert!(matches!(a.check_state(&post), Err(t::Failure::Wire)));
            assert!(matches!(laws, Err(t::Failure::Wire)));
            assert!(matches!(result, Err(t::Failure::Wire)));
        } else {
            assert!(evaluation.result().is_ok());
            assert!(a.check_state(&post).is_ok());
            let reports = result.unwrap();
            assert_eq!(reports.len(), 9);
            assert!(reports.iter().all(|r| r.holds));
            assert!(laws.unwrap().iter().all(|r| r.holds));
        }
        // Exercise independent expected-outcome eligibility through evaluate,
        // bypassing the public law probe and the producer's successor admission.
        let bool_ty = d
            .types
            .iter()
            .find(|t| matches!(t.shape, t::Shape::Bool))
            .unwrap()
            .id;
        d.cases = vec![t::Case {
            when: t::Expr::Literal(bool_ty, Value::boolean(true)),
            plan: t::Plan::Reject(303),
        }];
        let reject_all =
            t::Authority::bind(&d.schema_bytes().unwrap(), &d.policy_bytes().unwrap()).unwrap();
        let evaluation = run(&reject_all, &pre, &cmd);
        if bound < 139 {
            assert!(matches!(evaluation.result(), Err(t::Failure::Wire)));
        } else {
            assert!(matches!(evaluation.result(), Err(t::Failure::Law(_))));
        }
    }
    // Pre-state, command and context each have their own independent F bound.
    for source in 0..3 {
        let mut d = compiled.descriptor.clone();
        d.laws.retain(|law| law.id() != 991);
        let a = t::Authority::bind(&d.schema_bytes().unwrap(), &d.policy_bytes().unwrap()).unwrap();
        let value = [&pre, &cmd, &ctx][source];
        d.frame_bytes[source] = a.frame(source, value).unwrap().len() as u64 - 1;
        let a = t::Authority::bind(&d.schema_bytes().unwrap(), &d.policy_bytes().unwrap()).unwrap();
        assert!(matches!(
            a.check_laws(&pre, &cmd, &ctx, None, Some(303)),
            Err(t::Failure::Wire)
        ));
        assert!(matches!(
            a.check_ordered_result(&pre, &cmd, &ctx, None, Some(303), None),
            Err(t::Failure::Wire)
        ));
    }
}
#[test]
fn ordered_v1_hints_preserve_history_and_payload() {
    let c = compile("ordered-v1-three");
    let a = t::Authority::bind(&c.schema, &c.policy).unwrap();
    let pre = state(2, &[(0, 2, false), (1, 1, true), (2, 1, false)], &[0, 1, 2]);
    for (upper, lower, want) in [
        (None, None, vec![0, 1, 2]),
        (Some(2), None, vec![0, 2, 1]),
        (Some(0), Some(2), vec![0, 1, 2]),
        (Some(1), Some(1), vec![0, 1, 2]),
        (None, Some(2), vec![0, 2, 1]),
        (None, Some(1), vec![0, 1, 2]),
    ] {
        let r = run(&a, &pre, &command(2, 2, 1, 1, false, upper, lower));
        let post = r.result().unwrap().post().unwrap();
        assert_eq!(
            field(post, 111),
            &Value::vector(want.into_iter().map(Value::signed).collect()).unwrap(),
            "upper={upper:?} lower={lower:?}"
        );
        let ValueRef::Map(es) = field(post, 110).view() else {
            panic!()
        };
        assert_eq!(
            es.iter().find(|e| number(e.key()) == 1).unwrap().value(),
            &item(1, true)
        );
    }
}
#[test]
fn ordered_minimum_iteration_negative_priority_and_publication() {
    for mode in 0..3 {
        let c = compile(&format!("ordered-{mode}-3"));
        let mut d = c.descriptor.clone();
        if mode == 0 {
            let ty = d.types.iter_mut().find(|t| t.id == 201).unwrap();
            ty.shape = t::Shape::I128 { min: -2, max: 2 };
        }
        let a = t::Authority::bind(&d.schema_bytes().unwrap(), &d.policy_bytes().unwrap()).unwrap();
        let pre = if mode == 0 {
            state(mode, &[(0, 2, false), (1, -1, true)], &[])
        } else {
            state(mode, &[(0, 1, false), (1, 1, true)], &[0, 1])
        };
        let frame = a.frame(0, &pre).unwrap();
        let query = a.ordered_query(&frame, true);
        let min = query.result().unwrap().unwrap();
        let ValueRef::Tuple(xs) = min.view() else {
            panic!()
        };
        assert_eq!(number(&xs[0]), 1);
        let query = a.ordered_query(&frame, false);
        let ValueRef::Vector(xs) = query.result().unwrap().unwrap().view() else {
            panic!()
        };
        assert_eq!(xs.len(), 2);
        assert_eq!(&xs[0], min);
        let cmd = command(mode, 1, 0, 1, false, None, None);
        let s = a.frame(0, &pre).unwrap();
        let c = a.frame(1, &cmd).unwrap();
        let x = a.frame(2, &record(vec![])).unwrap();
        let raw = t::Raw {
            state: &s,
            command: &c,
            context: &x,
        };
        let evaluation = a.evaluate(raw);
        let subject = evaluation.subject().unwrap().to_vec();
        assert!(a.replay(raw, &subject).result().is_ok());
        let p = a.publish(raw);
        assert!(matches!(p, t::PublicationOutcome::Commit(_)));
    }
}
#[test]
fn ordered_malformed_sequence_and_byte_precedence() {
    let c = compile("ordered-1-2");
    let a = t::Authority::bind(&c.schema, &c.policy).unwrap();
    for seq in [&[0, 0][..], &[0][..], &[1, 0][..]] {
        let bad = state(1, &[(0, 2, false), (1, 1, true)], seq);
        assert!(matches!(
            run(&a, &bad, &command(1, 1, 0, 1, false, None, None)).result(),
            Err(t::Failure::Law(980))
        ));
    }
    let pre = state(1, &[(0, 1, false)], &[0]);
    // ItemTooLarge precedes full, snapshot and sequence; MissingKey precedes edit preflight.
    let mut d = c.descriptor.clone();
    if let t::Shape::Table { ref mut limits, .. } =
        d.types.iter_mut().find(|t| t.id == 204).unwrap().shape
    {
        limits.max_item_bytes = 62;
    }
    assert!(d.policy_bytes().is_ok());
    let a = t::Authority::bind(&d.schema_bytes().unwrap(), &d.policy_bytes().unwrap()).unwrap();
    let empty = state(1, &[], &[]);
    assert_eq!(
        run(&a, &empty, &command(1, 0, 0, 1, false, None, None))
            .result()
            .unwrap()
            .reason(),
        Some(306)
    );
    assert_eq!(
        run(&a, &empty, &command(1, 2, 0, 1, false, None, None))
            .result()
            .unwrap()
            .reason(),
        Some(303)
    );
    assert!(
        run(&a, &empty, &command(1, 1, 0, 1, false, None, None))
            .result()
            .unwrap()
            .post()
            .is_some()
    );
    let mut d = c.descriptor.clone();
    if let t::Shape::Table { ref mut limits, .. } =
        d.types.iter_mut().find(|t| t.id == 204).unwrap().shape
    {
        limits.max_snapshot_bytes = 92;
    }
    if let t::Shape::Vector { ref mut bytes, .. } =
        d.types.iter_mut().find(|t| t.id == 206).unwrap().shape
    {
        *bytes = 22;
    }
    let a = t::Authority::bind(&d.schema_bytes().unwrap(), &d.policy_bytes().unwrap()).unwrap();
    assert_eq!(
        run(&a, &pre, &command(1, 0, 1, 1, false, None, None))
            .result()
            .unwrap()
            .reason(),
        Some(308)
    );
    let mut d = c.descriptor.clone();
    if let t::Shape::Vector { ref mut bytes, .. } =
        d.types.iter_mut().find(|t| t.id == 206).unwrap().shape
    {
        *bytes = 22;
    }
    let a = t::Authority::bind(&d.schema_bytes().unwrap(), &d.policy_bytes().unwrap()).unwrap();
    assert_eq!(
        run(&a, &pre, &command(1, 0, 1, 1, false, None, None))
            .result()
            .unwrap()
            .reason(),
        Some(315)
    );
}
#[test]
fn ordered_v1_cut_matches_independent_linked_reference() {
    let path = std::env::temp_dir().join(format!("sol-cut-reference-{}.json", std::process::id()));
    let output = std::process::Command::new("python3")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tools/ordered_cut_reference.py"))
        .arg(&path)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let corpus: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let c = compile("ordered-v1-three");
    let a = t::Authority::bind(&c.schema, &c.policy).unwrap();
    let state_from = |s: &serde_json::Value| {
        let items = s["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                (
                    v[0].as_i64().unwrap() as i128,
                    v[1].as_i64().unwrap() as i128,
                    v[2].as_bool().unwrap(),
                )
            })
            .collect::<Vec<_>>();
        let order = s["order"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i64().unwrap() as i128)
            .collect::<Vec<_>>();
        state(2, &items, &order)
    };
    let states = corpus["states"]
        .as_array()
        .unwrap()
        .iter()
        .map(state_from)
        .collect::<Vec<_>>();
    let rows = corpus["rows"].as_array().unwrap();
    for row in rows {
        let pre = &states[row["state"].as_u64().unwrap() as usize];
        let opt = |key: &str| row[key].as_i64().map(i128::from);
        let cmd = command(
            2,
            row["op"].as_i64().unwrap() as i128,
            row["id"].as_i64().unwrap() as i128,
            row["priority"].as_i64().unwrap() as i128,
            row["payload"].as_bool().unwrap(),
            opt("upper"),
            opt("lower"),
        );
        let result = run(&a, pre, &cmd);
        let decision = result.result().unwrap_or_else(|e| panic!("{row}: {e:?}"));
        if let Some(reason) = row["expected"]["reason"].as_u64() {
            assert_eq!(decision.reason(), Some(reason as u32));
        } else {
            assert_eq!(
                decision.post(),
                Some(&state_from(&row["expected"])),
                "{row}"
            );
        }
    }
    println!(
        "V1_CUT_REFERENCE states={} observations={}",
        states.len(),
        rows.len()
    );
}
#[test]
fn ordered_v1_rejects_zero_key_profile_and_malformed_hint() {
    let c = compile("ordered-2-2");
    let rules = std::fs::read_to_string(fixtures().join("ordered-2-2/v2/policy.json")).unwrap();
    assert!(zeno_fcis_contract::internals::rules::Rules::read(&rules).is_err());
    let mut d = c.descriptor.clone();
    d.types.iter_mut().find(|t| t.id == 201).unwrap().shape = t::Shape::I128 { min: 0, max: 2 };
    assert!(d.policy_bytes().is_err());
    let mut d = c.descriptor.clone();
    let duplicate = state(2, &[(0, 1, false)], &[0, 0]);
    d.cases[0].when = t::Expr::Eq(
        Box::new(t::Expr::Literal(100, duplicate.clone())),
        Box::new(t::Expr::Literal(100, duplicate)),
    );
    assert!(d.policy_bytes().is_err());
    let mut d = c.descriptor.clone();
    if let t::Shape::Table { ref mut key, .. } =
        d.types.iter_mut().find(|t| t.id == 204).unwrap().shape
    {
        *key = 206;
    }
    assert!(d.policy_bytes().is_err());
    let a = t::Authority::bind(&c.schema, &c.policy).unwrap();
    let cmd = command(2, 0, 0, 1, false, None, None);
    let ValueRef::Record(fs) = cmd.view() else {
        panic!()
    };
    let bad = rec_from_fields(
        fs.iter()
            .map(|f| {
                (
                    f.id(),
                    if f.id() == 123 {
                        Value::sum(207, 2, None)
                    } else {
                        f.value().clone()
                    },
                )
            })
            .collect(),
    );
    assert!(matches!(
        run(&a, &c.descriptor.genesis, &bad).result(),
        Err(t::Failure::Wire)
    ));
    fn rec_from_fields(fs: Vec<(u16, Value)>) -> Value {
        record(fs)
    }
}
