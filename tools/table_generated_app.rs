//! Native generated-binding acceptance runner; no projection into scalar atoms.
mod v2_contract;
use std::collections::BTreeMap;
use t::{Value, ValueRef};
use zeno_fcis_synthesis::finite::v2_authority::tables as t;
fn rec(xs: Vec<(u16, Value)>) -> Value {
    Value::record_canonical(xs.into_iter().map(|(i, v)| t::Field::new(i, v)).collect()).unwrap()
}
fn n(v: &Value) -> i128 {
    let ValueRef::I128(n) = v.view() else {
        panic!()
    };
    n
}
fn main() {
    let a = v2_contract::bind().unwrap();
    let d = a.descriptor();
    let (set, capacity) = d
        .types
        .iter()
        .find_map(|t| {
            if let t::Shape::Table { set, limits, .. } = t.shape {
                Some((set, limits.max_entries as usize))
            } else {
                None
            }
        })
        .unwrap();
    let empty = rec(vec![]);
    let context = a.frame(2, &empty).unwrap();
    let mut observations = 0;
    // Independent powerset/product enumeration, not the compiler's domain routine.
    let mut states = vec![BTreeMap::new()];
    for key in 0..4 {
        let old = states.clone();
        for row in old {
            if row.len() == capacity {
                continue;
            }
            for val in 0..if set { 1 } else { 2 } {
                let mut next = row.clone();
                next.insert(key, val);
                states.push(next);
            }
        }
    }
    assert_eq!(
        states.len(),
        if set {
            [1, 5, 11, 15][capacity]
        } else {
            [1, 9, 33, 65][capacity]
        }
    );
    for pre in states {
        let value = rec(vec![(
            110,
            Value::map_canonical(
                pre.iter()
                    .map(|(k, v)| {
                        t::MapEntry::try_new(
                            Value::signed(*k),
                            if set {
                                Value::unit()
                            } else {
                                Value::signed(*v)
                            },
                        )
                        .unwrap()
                    })
                    .collect(),
            )
            .unwrap(),
        )]);
        let state = a.frame(0, &value).unwrap();
        for op in 0..8 {
            for key in 0..4 {
                for value in 0..2 {
                    let command = rec(vec![
                        (120, Value::signed(op)),
                        (121, Value::signed(key)),
                        (122, Value::signed(value)),
                    ]);
                    let command = a.frame(1, &command).unwrap();
                    let raw = t::Raw {
                        state: &state,
                        command: &command,
                        context: &context,
                    };
                    let result = a.evaluate(raw);
                    let decision = result.result().unwrap();
                    let mut expected = pre.clone();
                    let reason = match op {
                        0 if !pre.contains_key(&key) && pre.len() == capacity => Some(307),
                        0 => {
                            expected.insert(key, if set { 0 } else { value });
                            None
                        }
                        1 if set => Some(313),
                        1 if !pre.contains_key(&key) => Some(303),
                        1 => {
                            expected.insert(key, value);
                            None
                        }
                        2 => {
                            expected.remove(&key);
                            None
                        }
                        _ => None,
                    };
                    assert_eq!(decision.reason(), reason);
                    if let Some(post) = decision.post() {
                        let ValueRef::Record(fs) = post.view() else {
                            panic!()
                        };
                        let ValueRef::Map(es) = fs[0].value().view() else {
                            panic!()
                        };
                        let actual = es
                            .iter()
                            .map(|e| (n(e.key()), if set { 0 } else { n(e.value()) }))
                            .collect::<BTreeMap<_, _>>();
                        assert_eq!(actual, expected);
                    } else {
                        assert!(decision.patch().is_empty());
                    }
                    assert!(a.replay(raw, result.subject().unwrap()).result().is_ok());
                    observations += 1;
                }
            }
        }
        let command = a
            .frame(
                1,
                &rec(vec![
                    (120, Value::signed(6)),
                    (121, Value::signed(0)),
                    (122, Value::signed(0)),
                ]),
            )
            .unwrap();
        let mut bad = state.clone();
        bad.push(0);
        assert!(
            a.evaluate(t::Raw {
                state: &bad,
                command: &command,
                context: &context
            })
            .result()
            .is_err()
        );
    }
    println!("generated-app set={set} C={capacity} observations={observations} malformed=refused");
}
