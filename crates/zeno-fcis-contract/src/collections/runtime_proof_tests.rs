//! Native samples of the standard-library contracts and production proof premises.
use super::*;
use crate::internals::review::domain::LeafDomain;
use std::cell::RefCell;
use std::error::Error;

type TestResult = Result<(), Box<dyn Error>>;
struct Records {
    rows: BTreeMap<(u32, Key), Vec<u8>>,
    reads: RefCell<Vec<(u32, Key)>>,
}
impl Entries for Records {
    fn record(&self, map: u32, key: &Key) -> Option<Vec<u8>> {
        self.reads.borrow_mut().push((map, *key));
        self.rows.get(&(map, *key)).cloned()
    }
}
fn example(name: &str) -> Result<Layout, Box<dyn Error>> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/collections")
        .join(name);
    Ok(Layout::read(
        &std::fs::read_to_string(root.join("project.zeno"))?,
        &std::fs::read_to_string(root.join("v2/policy.json"))?,
    )?)
}
fn position(layout: &Layout, field: u16) -> Result<usize, Box<dyn Error>> {
    layout
        .positions
        .iter()
        .position(|position| position.source == Source::State && position.field == Some(field))
        .ok_or_else(|| "missing test position".into())
}

#[test]
fn codec_matches_standard_signed_bytes_and_record_layout() {
    let values = [
        i128::MIN,
        i128::MIN + 1,
        -65537,
        -256,
        -1,
        0,
        1,
        255,
        256,
        65537,
        i128::MAX - 1,
        i128::MAX,
    ];
    for value in values {
        assert_eq!(encode_number(value), value.to_be_bytes());
        assert_eq!(decode_number(value.to_be_bytes()), value);
        let fields = [RecordValue::Number(value), RecordValue::Key([0x81; 32])];
        let mut expected = vec![0];
        expected.extend_from_slice(&value.to_be_bytes());
        expected.push(1);
        expected.extend_from_slice(&[0x81; 32]);
        assert_eq!(encode_record(&fields), expected);
        assert_eq!(decode_record(&expected), Some(fields.to_vec()));
        for cut in 1..17 {
            assert_eq!(decode_record(&expected[..cut]), None);
        }
        for cut in 18..50 {
            assert_eq!(decode_record(&expected[..cut]), None);
        }
        expected.push(7);
        assert_eq!(decode_record(&expected), None);
    }
    assert_eq!(decode_record(&[]), Some(Vec::new()));
    for value in 0u8..=255 {
        let bytes = [value; 16];
        assert_eq!(decode_number(bytes), i128::from_be_bytes(bytes));
        assert_eq!(encode_number(i128::from_be_bytes(bytes)), bytes);
    }
}

#[test]
fn standard_ordered_map_and_set_contract_samples() -> TestResult {
    let mut set = BTreeSet::new();
    let mut map = BTreeMap::new();
    let mut pairs = BTreeSet::new();
    for value in (0u8..=255).rev() {
        let mut key = [0u8; 32];
        key[0] = value;
        key[31] = 255 - value;
        assert!(set.insert(key));
        assert!(!set.insert(key));
        assert_eq!(map.insert((110u32, key), value), None);
        assert!(pairs.insert((110u32, key)));
        assert!(pairs.insert((115u32, key)));
    }
    let keys: Vec<_> = set.iter().copied().collect();
    for (index, key) in keys.iter().enumerate() {
        assert_eq!(usize::from(key[0]), index);
        assert!(set.contains(key));
        assert_eq!(map.get(&(110, *key)), Some(&key[0]));
        assert_eq!(map.remove(&(110, *key)), Some(key[0]));
        assert_eq!(map.remove(&(110, *key)), None);
        for other in &keys {
            assert_eq!(key.cmp(other), key.as_slice().cmp(other.as_slice()));
            assert_eq!(key == other, key.cmp(other) == std::cmp::Ordering::Equal);
            assert_eq!((110u32, *key).cmp(&(110u32, *other)), key.cmp(other));
        }
    }
    assert!(map.is_empty());
    assert_eq!(pairs.len(), 512);
    let mut nested = BTreeMap::from([(1u32, set)]);
    let mut taken = nested.remove(&1).ok_or("set missing")?;
    assert!(nested.is_empty());
    assert!(!taken.insert(keys[0]));
    nested.insert(1, taken);
    assert_eq!(nested[&1].len(), 256);
    Ok(())
}

#[test]
fn standard_slice_option_result_and_diagnostic_samples() -> TestResult {
    let mut values = vec![1i128, 2, 3];
    values.push(4);
    values.extend_from_slice(&[5, 6]);
    assert_eq!(values.as_slice(), &[1, 2, 3, 4, 5, 6]);
    assert_eq!(values.get(9), None);
    values[2] = 7;
    assert_eq!(&values[1..4], &[2, 7, 4]);
    assert_eq!(values.len(), 6);
    let mut key = [0u8; 32];
    key.copy_from_slice(&[0x52; 32]);
    assert_eq!(key, [0x52; 32]);
    for (input, expected) in [(Some(9usize), 9), (None, 0)] {
        assert_eq!(input.unwrap_or(0), expected);
    }
    let number: Result<i128, &str> = Ok(7);
    assert_eq!(number?, 7);
    let failure: Result<i128, &str> = Err("missing");
    assert_eq!(failure.err(), Some("missing"));
    let error = ContractError::new("place", String::from("reason"));
    assert_eq!(error.place(), "place");
    assert_eq!(error.reason(), "reason");
    assert_eq!(error.to_string(), "place: reason");
    for (domain, least) in [
        (LeafDomain::Bool, Some(0)),
        (LeafDomain::Int { min: -3, max: 4 }, Some(-3)),
        (LeafDomain::Int { min: 2, max: 1 }, None),
        (
            LeafDomain::Sum {
                type_id: 1,
                variants: vec![9, 3],
            },
            Some(9),
        ),
        (
            LeafDomain::Sum {
                type_id: 1,
                variants: vec![],
            },
            None,
        ),
    ] {
        assert_eq!(domain.least(), least);
    }
    Ok(())
}

fn check_layout_premises(layout: &Layout) {
    let len = layout.positions.len();
    let mut input_indices = BTreeSet::new();
    let mut occupied = BTreeSet::new();
    for &index in &layout.scalars {
        assert!(index < len);
        assert_eq!(layout.positions[index].source, Source::State);
    }
    for &(index, _) in &layout.inputs {
        assert!(index < len);
        assert_ne!(layout.positions[index].source, Source::State);
        assert!(input_indices.insert(index));
    }
    for entry in &layout.entries {
        assert!(entry.presence < len);
        assert!(entry.values.len() <= usize::MAX / 33);
        let mut local = BTreeSet::from([entry.presence]);
        for &(index, _) in &entry.values {
            assert!(index < len);
            assert!(local.insert(index));
        }
        for index in local {
            assert_eq!(layout.positions[index].source, Source::State);
            assert!(!layout.scalars.contains(&index));
            assert!(occupied.insert(index));
        }
        for other in &layout.entries {
            if entry.map == other.map {
                assert_eq!(entry.values.len(), other.values.len());
                for ((left, left_kind), (right, right_kind)) in
                    entry.values.iter().zip(&other.values)
                {
                    assert_eq!(left_kind, right_kind);
                    assert_eq!(
                        layout.positions[*left].domain.least().unwrap_or(0),
                        layout.positions[*right].domain.least().unwrap_or(0)
                    );
                }
            }
        }
    }
}
#[test]
fn shipped_equality_layouts_instantiate_premises_and_ordered_layout_is_refused() -> TestResult {
    for name in ["escrow", "library", "token-balances"] {
        check_layout_premises(&example(name)?);
    }
    match example("order-book") {
        Err(error) => assert!(
            error
                .to_string()
                .contains("ordered collection framing is not supported")
        ),
        Ok(_) => panic!("ordered framing unexpectedly entered the equality-only proof scope"),
    }
    let deletion = Layout::read(
        include_str!("../../../../test-data/collections/deletion/project.zeno"),
        include_str!("../../../../test-data/collections/deletion/v2/policy.json"),
    )?;
    check_layout_premises(&deletion);
    Ok(())
}

#[test]
fn token_frames_named_reads_and_writes_exactly_one_merged_alias() -> TestResult {
    let layout = example("token-balances")?;
    let a = [0x21; 32];
    let b = [0x43; 32];
    let unnamed = [0x65; 32];
    let records = Records {
        rows: BTreeMap::from([
            ((110, a), encode_record(&[RecordValue::Number(2)])),
            ((110, b), encode_record(&[RecordValue::Number(1)])),
            ((110, unnamed), encode_record(&[RecordValue::Number(3)])),
        ]),
        reads: RefCell::new(Vec::new()),
    };
    let context = Input {
        scalars: BTreeMap::from([(0, 180)]),
        ..Input::default()
    };
    let mut command = Input {
        scalars: BTreeMap::from([(130, 150), (133, 1)]),
        keys: BTreeMap::from([(131, a), (132, b)]),
    };
    let state = BTreeMap::from([(120, 3)]);
    let framed = layout.frame(&state, &command, &context, &records)?;
    assert_eq!(*records.reads.borrow(), vec![(110, a), (110, b)]);
    assert_eq!(framed.named[&110], BTreeSet::from([a, b]));
    assert_eq!(framed.tuple[position(&layout, 161)?], 2);
    assert_eq!(framed.tuple[position(&layout, 171)?], 1);
    let unchanged = layout.writes(&framed, &BTreeMap::new())?;
    assert!(unchanged.entries.is_empty());
    assert_eq!(unchanged.scalars, state);
    let moved = layout.writes(&framed, &BTreeMap::from([(161, 1), (171, 2)]))?;
    assert_eq!(
        moved.entries,
        vec![
            (110, a, Some(encode_record(&[RecordValue::Number(1)]))),
            (110, b, Some(encode_record(&[RecordValue::Number(2)])))
        ]
    );
    command.keys.insert(132, a);
    records.reads.borrow_mut().clear();
    let alias = layout.frame(&state, &command, &context, &records)?;
    assert_eq!(*records.reads.borrow(), vec![(110, a), (110, a)]);
    assert_eq!(alias.named[&110], BTreeSet::from([a]));
    let merged = layout.writes(&alias, &BTreeMap::from([(161, 1), (171, 1)]))?;
    assert_eq!(
        merged.entries,
        vec![(110, a, Some(encode_record(&[RecordValue::Number(1)])))]
    );
    assert_eq!(
        layout.writes(&alias, &BTreeMap::from([(161, 1)]))?.entries,
        merged.entries
    );
    assert!(
        layout
            .writes(&alias, &BTreeMap::from([(161, 1), (171, 0)]))
            .is_err()
    );
    let removed = layout.writes(&alias, &BTreeMap::from([(160, 0)]))?;
    assert_eq!(removed.entries, vec![(110, a, None)]);
    let successor = layout.successor(&alias, &removed)?;
    for field in [160, 161, 170, 171] {
        assert_eq!(successor[position(&layout, field)?], 0);
    }
    assert!(records.rows.contains_key(&(110, unnamed)));
    Ok(())
}

#[test]
fn library_stored_keys_share_classes_and_absence_has_no_payload() -> TestResult {
    let layout = example("library")?;
    let member = [0x21; 32];
    let copy = [0x43; 32];
    let mut records = Records {
        rows: BTreeMap::from([
            ((110, member), encode_record(&[RecordValue::Number(1)])),
            (
                (115, copy),
                encode_record(&[RecordValue::Number(1), RecordValue::Key(member)]),
            ),
        ]),
        reads: RefCell::new(Vec::new()),
    };
    let context = Input {
        scalars: BTreeMap::from([(0, 180)]),
        ..Input::default()
    };
    let command = Input {
        scalars: BTreeMap::from([(130, 152)]),
        keys: BTreeMap::from([(131, member), (132, copy)]),
    };
    let framed = layout.frame(&BTreeMap::new(), &command, &context, &records)?;
    assert_eq!(*records.reads.borrow(), vec![(110, member), (115, copy)]);
    let holder = position(&layout, 172)?;
    let class = usize::try_from(framed.tuple[holder])?;
    assert_eq!(framed.classes[&103][class], member);
    let removed = layout.writes(&framed, &BTreeMap::from([(170, 0)]))?;
    assert_eq!(removed.entries, vec![(115, copy, None)]);
    let successor = layout.successor(&framed, &removed)?;
    assert_eq!(successor[position(&layout, 170)?], 0);
    for field in [171, 172] {
        let index = position(&layout, field)?;
        assert_eq!(
            successor[index],
            layout.positions[index].domain.least().unwrap_or(0)
        );
    }
    records.rows.remove(&(115, copy));
    records.reads.borrow_mut().clear();
    let absent = layout.frame(&BTreeMap::new(), &command, &context, &records)?;
    for field in [170, 171, 172] {
        assert_eq!(
            absent.tuple[position(&layout, field)?],
            successor[position(&layout, field)?]
        );
    }
    Ok(())
}
