//! Execute the emitted B binding through owned native authority and replay.
mod v2_contract;
use t::Value;
use zeno_fcis_synthesis::finite::v2_authority::tables as t;
fn rec(xs: Vec<(u16, Value)>) -> Value {
    Value::record_canonical(xs.into_iter().map(|(i, v)| t::Field::new(i, v)).collect()).unwrap()
}
fn main() {
    let a = v2_contract::bind().unwrap();
    let d = a.descriptor();
    let spec = d.order.as_ref().unwrap();
    let cap = d
        .types
        .iter()
        .find_map(|t| {
            if let t::Shape::Table { limits, .. } = t.shape {
                Some(limits.max_entries)
            } else {
                None
            }
        })
        .unwrap();
    let item = rec(vec![
        (1, Value::signed(1)),
        (2, Value::unit()),
        (3, Value::boolean(true)),
    ]);
    let mut cmd = vec![
        (120, Value::signed(0)),
        (121, Value::signed(0)),
        (122, item.clone()),
    ];
    if spec.mode != 0 {
        cmd.extend([
            (123, Value::sum(207, 0, None)),
            (124, Value::sum(207, 0, None)),
        ]);
    }
    let context = a.frame(2, &rec(vec![])).unwrap();
    let c = a.frame(1, &rec(cmd)).unwrap();
    let s = a.frame(0, &d.genesis).unwrap();
    let raw = t::Raw {
        state: &s,
        command: &c,
        context: &context,
    };
    let e = a.evaluate(raw);
    let dec = e.result().unwrap();
    if cap == 0 {
        assert_eq!(dec.reason(), Some(307));
        assert!(dec.post().is_none());
    } else {
        let post = dec.post().unwrap();
        let wire = a.frame(0, post).unwrap();
        let q = a.ordered_query(&wire, true);
        assert_eq!(
            q.result().unwrap(),
            Some(&Value::tuple(vec![Value::signed(0), item]).unwrap())
        );
        let subject = e.subject().unwrap();
        assert!(a.replay(raw, subject).result().is_ok());
        let mut corrupted = subject.to_vec();
        corrupted[0] ^= 1;
        assert!(matches!(
            a.replay(raw, &corrupted).result(),
            Err(t::Failure::Replay)
        ));
        assert!(matches!(a.publish(raw), t::PublicationOutcome::Commit(_)));
    }
    println!(
        "ordered mode={} capacity={cap}: native generated binding/query/replay passed",
        spec.mode
    );
}
