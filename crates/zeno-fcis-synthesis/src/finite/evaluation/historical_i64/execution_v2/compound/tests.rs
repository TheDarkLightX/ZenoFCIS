//! Independent original-value encodings and actual unfiltered parser refusals.
use super::super::{meter, zero_limits};
use super::*;
use crate::finite::historical_i64::canonical_v2::output;
use schema::{Definition, Description, Field, Kind, SumVariant, Variant};

fn amount(v: u128) -> Vec<u8> {
    let mut b = alloc::vec![3];
    b.extend_from_slice(&v.to_be_bytes());
    b
}
fn text(v: &[u8]) -> Vec<u8> {
    let mut b = alloc::vec![6];
    b.extend_from_slice(&(v.len() as u32).to_be_bytes());
    b.extend_from_slice(v);
    b
}
fn enumeration() -> Vec<u8> {
    alloc::vec![7, 0, 0, 0, 7, 0, 2]
}
fn event(move_amount: Option<u128>) -> Vec<u8> {
    let mut b = alloc::vec![
        10,
        0,
        0,
        0,
        9,
        0,
        if move_amount.is_some() { 2 } else { 1 },
        if move_amount.is_some() { 1 } else { 0 }
    ];
    if let Some(a) = move_amount {
        b.extend_from_slice(&amount(a));
    }
    b
}
fn sequence(tag: u8, children: &[Vec<u8>]) -> Vec<u8> {
    let mut b = alloc::vec![tag];
    b.extend_from_slice(&(children.len() as u32).to_be_bytes());
    for v in children {
        b.extend_from_slice(v);
    }
    b
}
fn scores(entries: &[(u128, u128)]) -> Vec<u8> {
    let mut b = alloc::vec![12];
    b.extend_from_slice(&(entries.len() as u32).to_be_bytes());
    for (k, v) in entries {
        for x in [amount(*k), amount(*v)] {
            b.extend_from_slice(&(x.len() as u32).to_be_bytes());
            b.extend_from_slice(&x);
        }
    }
    b
}
fn record(values: &[Vec<u8>]) -> Vec<u8> {
    let mut b = alloc::vec![9];
    b.extend_from_slice(&(values.len() as u32).to_be_bytes());
    for (i, v) in values.iter().enumerate() {
        b.extend_from_slice(&((i + 1) as u16).to_be_bytes());
        b.extend_from_slice(v);
    }
    b
}
fn limits(steps: u64) -> super::super::Limits {
    zero_limits().with_limit(Resource::Step, steps)
}
fn schema_limits() -> schema::Limits {
    schema::Limits {
        bytes: 65536,
        types: 64,
        fields: 1024,
        variants: 1024,
    }
}

#[test]
fn complete_original_record_every_child_boundary_and_actual_budget_is_checked() {
    let variants = [
        Variant {
            id: 1,
            name: b"Idle",
        },
        Variant {
            id: 2,
            name: b"Active",
        },
    ];
    let sum = [
        SumVariant {
            id: 1,
            name: b"Stop",
            payload: None,
        },
        SumVariant {
            id: 2,
            name: b"Move",
            payload: Some(1),
        },
    ];
    let point = [1, 7];
    let fields = [
        Field {
            id: 1,
            name: b"amount",
            type_id: 1,
        },
        Field {
            id: 2,
            name: b"signed",
            type_id: 2,
        },
        Field {
            id: 3,
            name: b"label",
            type_id: 3,
        },
        Field {
            id: 4,
            name: b"blob",
            type_id: 4,
        },
        Field {
            id: 5,
            name: b"flag",
            type_id: 5,
        },
        Field {
            id: 6,
            name: b"nil",
            type_id: 6,
        },
        Field {
            id: 7,
            name: b"tag",
            type_id: 7,
        },
        Field {
            id: 8,
            name: b"point",
            type_id: 8,
        },
        Field {
            id: 9,
            name: b"event",
            type_id: 9,
        },
        Field {
            id: 10,
            name: b"labels",
            type_id: 10,
        },
        Field {
            id: 11,
            name: b"scores",
            type_id: 11,
        },
    ];
    let definitions = [
        Definition {
            id: 1,
            name: b"Amount",
            kind: Kind::U128 {
                min: 0,
                max: 1_000_000,
            },
        },
        Definition {
            id: 2,
            name: b"Signed",
            kind: Kind::I128 {
                min: -1_000,
                max: 1_000,
            },
        },
        Definition {
            id: 3,
            name: b"Label",
            kind: Kind::Text { min: 1, max: 16 },
        },
        Definition {
            id: 4,
            name: b"Blob",
            kind: Kind::Bytes { min: 0, max: 32 },
        },
        Definition {
            id: 5,
            name: b"Flag",
            kind: Kind::Bool,
        },
        Definition {
            id: 6,
            name: b"Nil",
            kind: Kind::Unit,
        },
        Definition {
            id: 7,
            name: b"Tag",
            kind: Kind::Enum(&variants),
        },
        Definition {
            id: 8,
            name: b"Point",
            kind: Kind::Tuple(&point),
        },
        Definition {
            id: 9,
            name: b"Event",
            kind: Kind::SumPayload(&sum),
        },
        Definition {
            id: 10,
            name: b"Labels",
            kind: Kind::Vector {
                element: 3,
                min: 0,
                max: 4,
            },
        },
        Definition {
            id: 11,
            name: b"Scores",
            kind: Kind::Map {
                key: 1,
                value: 1,
                min: 0,
                max: 4,
            },
        },
        Definition {
            id: 12,
            name: b"BalanceState",
            kind: Kind::Record(&fields),
        },
    ];
    let description = Description {
        profile: b"CodegenFixture",
        version: 1,
        root: 12,
        definitions: &definitions,
    };
    // Immutable bytes from the actual original build/renderer bundle, object
    // complete-original-bundles/codegen/schema.zcve; SHA256 830a17ee25b7b06a3b3332d3e0e7205cea78b05702b4fae16c96a4cd377815c9.
    let original: &[u8] = &[
        90, 70, 67, 73, 83, 83, 67, 72, 69, 77, 65, 49, 0, 0, 14, 67, 111, 100, 101, 103, 101, 110,
        70, 105, 120, 116, 117, 114, 101, 0, 1, 0, 0, 0, 12, 0, 0, 0, 12, 0, 0, 0, 1, 0, 6, 65,
        109, 111, 117, 110, 116, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 15, 66, 64, 0, 0, 0, 2, 0, 6, 83, 105, 103, 110, 101, 100, 3, 255,
        255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 252, 24, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 3, 232, 0, 0, 0, 3, 0, 5, 76, 97, 98, 101, 108, 5, 0, 0, 0, 1, 0,
        0, 0, 16, 0, 0, 0, 4, 0, 4, 66, 108, 111, 98, 4, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0, 0, 5, 0, 4,
        70, 108, 97, 103, 1, 0, 0, 0, 6, 0, 3, 78, 105, 108, 0, 0, 0, 0, 7, 0, 3, 84, 97, 103, 6,
        0, 0, 0, 2, 0, 1, 0, 4, 73, 100, 108, 101, 0, 2, 0, 6, 65, 99, 116, 105, 118, 101, 0, 0, 0,
        8, 0, 5, 80, 111, 105, 110, 116, 7, 0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0, 7, 0, 0, 0, 9, 0, 5,
        69, 118, 101, 110, 116, 9, 0, 0, 0, 2, 0, 1, 0, 4, 83, 116, 111, 112, 0, 0, 2, 0, 4, 77,
        111, 118, 101, 1, 0, 0, 0, 1, 0, 0, 0, 10, 0, 6, 76, 97, 98, 101, 108, 115, 10, 0, 0, 0, 3,
        0, 0, 0, 0, 0, 0, 0, 4, 0, 0, 0, 11, 0, 6, 83, 99, 111, 114, 101, 115, 11, 0, 0, 0, 1, 0,
        0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 4, 0, 0, 0, 12, 0, 12, 66, 97, 108, 97, 110, 99, 101, 83,
        116, 97, 116, 101, 8, 0, 0, 0, 11, 0, 1, 0, 6, 97, 109, 111, 117, 110, 116, 0, 0, 0, 1, 0,
        2, 0, 6, 115, 105, 103, 110, 101, 100, 0, 0, 0, 2, 0, 3, 0, 5, 108, 97, 98, 101, 108, 0, 0,
        0, 3, 0, 4, 0, 4, 98, 108, 111, 98, 0, 0, 0, 4, 0, 5, 0, 4, 102, 108, 97, 103, 0, 0, 0, 5,
        0, 6, 0, 3, 110, 105, 108, 0, 0, 0, 6, 0, 7, 0, 3, 116, 97, 103, 0, 0, 0, 7, 0, 8, 0, 5,
        112, 111, 105, 110, 116, 0, 0, 0, 8, 0, 9, 0, 5, 101, 118, 101, 110, 116, 0, 0, 0, 9, 0,
        10, 0, 6, 108, 97, 98, 101, 108, 115, 0, 0, 0, 10, 0, 11, 0, 6, 115, 99, 111, 114, 101,
        115, 0, 0, 0, 11,
    ];

    assert_eq!(original.len(), 501);
    let checked = schema::admit(original, &description, schema_limits())
        .unwrap_or_else(|error| panic!("actual original schema: {error:?}"));
    let mut signed = alloc::vec![4];
    signed.extend_from_slice(&(-7i128).to_be_bytes());
    let values = [
        amount(5),
        signed,
        text(b"label"),
        alloc::vec![5, 0, 0, 0, 3, 0, 127, 255],
        alloc::vec![2],
        alloc::vec![0],
        enumeration(),
        sequence(8, &[amount(8), enumeration()]),
        event(Some(12)),
        sequence(11, &[text(b"one"), text(b"two")]),
        scores(&[(1, 4), (2, 6)]),
    ];
    let bytes = record(&values);
    let mut m = meter::new(limits(1000));
    let (decoded, end) = decode(&bytes, &checked, 0, bytes.len(), 12, &mut m)
        .unwrap_or_else(|error| panic!("complete original typed record: {error:?}"));
    assert_eq!(end, bytes.len());
    assert_eq!(m.used.used(Resource::Step), 48);
    assert_eq!(
        output::encode_atom(decoded, bytes.len())
            .unwrap_or_else(|error| panic!("strict83 unexpected typed test refusal: {error:?}")),
        bytes
    );
    let Atom::Canonical(span) = decoded else {
        panic!("original record shape");
    };
    assert_eq!(span.type_id(), 12);
    assert_eq!(span.schema(), original);
    assert_eq!(span.wire(), bytes);
    for cap in 0..48 {
        let mut m = meter::new(limits(cap));
        assert!(
            matches!(decode(&bytes,&checked,0,bytes.len(),12,&mut m),Err(Failure::Budget(e)) if e.resource==Resource::Step&&e.attempted==cap+1)
        );
        assert_eq!(m.used.used(Resource::Step), cap);
    }
    for end in 0..bytes.len() {
        let mut m = meter::new(limits(1000));
        assert_eq!(
            decode(&bytes[..end], &checked, 0, end, 12, &mut m).err(),
            Some(Failure::Wire)
        );
    }
    let bad = [
        (3, text(&[128])),
        (
            8,
            sequence(8, &[amount(8), alloc::vec![7, 0, 0, 0, 7, 0, 9]]),
        ),
        (9, event(Some(1_000_001))),
        (
            10,
            sequence(
                11,
                &[text(b"a"), text(b"b"), text(b"c"), text(b"d"), text(b"e")],
            ),
        ),
        (11, scores(&[(2, 6), (1, 4)])),
        (11, scores(&[(1, 4), (1, 5)])),
        (11, scores(&[(1, 1_000_001)])),
    ];
    for (field, value) in bad {
        let mut changed = values.clone();
        changed[field - 1] = value;
        let wire = record(&changed);
        let mut m = meter::new(limits(1000));
        assert_eq!(
            decode(&wire, &checked, 0, wire.len(), 12, &mut m).err(),
            Some(Failure::Wire)
        );
    }
    let mut over = values.clone();
    over[10] = scores(&[(1, 2)]);
    over[10][8] = 18;
    let wire = record(&over);
    let mut m = meter::new(limits(1000));
    assert_eq!(
        decode(&wire, &checked, 0, wire.len(), 12, &mut m).err(),
        Some(Failure::Wire)
    );
    for wire in [event(None), event(Some(0)), event(Some(1_000_000))] {
        let mut m = meter::new(limits(100));
        let (v, end) = decode(&wire, &checked, 0, wire.len(), 9, &mut m)
            .unwrap_or_else(|error| panic!("strict83 unexpected typed test refusal: {error:?}"));
        assert_eq!(end, wire.len());
        assert_eq!(
            output::encode_atom(v, wire.len()).unwrap_or_else(|error| panic!(
                "strict83 unexpected typed test refusal: {error:?}"
            )),
            wire
        );
    }
    for wire in [
        alloc::vec![10, 0, 0, 0, 9, 0, 1, 1],
        alloc::vec![10, 0, 0, 0, 9, 0, 2, 0],
        alloc::vec![10, 0, 0, 0, 9, 0, 3, 0],
    ] {
        let mut m = meter::new(limits(100));
        assert_eq!(
            decode(&wire, &checked, 0, wire.len(), 9, &mut m).err(),
            Some(Failure::Wire)
        );
    }

    let mut m = meter::new(limits(1000).with_limit(Resource::Read, 11));
    let mut reads = Vec::new();
    let parsed = decode_root(&bytes, &checked, 12, &mut m, &mut reads)
        .unwrap_or_else(|error| panic!("complete original record observations: {error:?}"));
    assert_eq!(
        output::encode_atom(parsed.atom, bytes.len())
            .unwrap_or_else(|error| panic!("strict83 unexpected typed test refusal: {error:?}")),
        bytes
    );
    let actual = parsed
        .fields
        .unwrap_or_else(|| panic!("strict83 expected original root-record fields"));
    assert_eq!(actual.len(), 11);
    assert!(parsed.sum.is_none());
    for (index, field) in actual.iter().enumerate() {
        assert_eq!(field.id, (index + 1) as u16);
        assert_eq!(
            output::encode_atom(field.value, values[index].len()).unwrap_or_else(|error| panic!(
                "strict83 unexpected typed test refusal: {error:?}"
            )),
            values[index]
        );
    }
    assert_eq!(m.used.used(Resource::Step), 48);
    assert_eq!(m.used.used(Resource::Read), 11);
    assert_eq!(
        reads,
        (1..=11)
            .map(|id| Access::Field(id, true))
            .collect::<Vec<_>>()
    );
    for cap in 0..11 {
        let mut m = meter::new(limits(1000).with_limit(Resource::Read, cap));
        let mut reads = Vec::new();
        assert!(
            matches!(decode_root(&bytes,&checked,12,&mut m,&mut reads),Err(Failure::Budget(e)) if e.resource==Resource::Read&&e.attempted==cap+1)
        );
        assert_eq!(m.used.used(Resource::Read), cap);
        assert_eq!(reads.len(), cap as usize + 1);
        assert_eq!(reads.last(), Some(&Access::Field((cap + 1) as u16, false)));
    }
    for cap in 0..48 {
        let mut m = meter::new(limits(cap).with_limit(Resource::Read, 11));
        let mut reads = Vec::new();
        assert!(
            matches!(decode_root(&bytes,&checked,12,&mut m,&mut reads),Err(Failure::Budget(e)) if e.resource==Resource::Step&&e.attempted==cap+1)
        );
        assert_eq!(m.used.used(Resource::Step), cap);
    }
    for end in 0..bytes.len() {
        let mut m = meter::new(limits(1000).with_limit(Resource::Read, 11));
        let mut reads = Vec::new();
        assert_eq!(
            decode_root(&bytes[..end], &checked, 12, &mut m, &mut reads).err(),
            Some(Failure::Wire)
        );
    }
    for wire in [event(None), event(Some(0)), event(Some(1_000_000))] {
        let mut m = meter::new(limits(100).with_limit(Resource::Read, 1));
        let mut reads = Vec::new();
        let value = decode_root(&wire, &checked, 9, &mut m, &mut reads)
            .unwrap_or_else(|error| panic!("strict83 unexpected typed test refusal: {error:?}"));
        assert_eq!(
            output::encode_atom(value.atom, wire.len()).unwrap_or_else(|error| panic!(
                "strict83 unexpected typed test refusal: {error:?}"
            )),
            wire
        );
        assert!(value.fields.is_none());
        assert_eq!(
            value
                .sum
                .unwrap_or_else(|| panic!("strict83 expected actual Sum variant"))
                .0,
            if wire.len() == 8 { 1 } else { 2 }
        );
        assert_eq!(reads, alloc::vec![Access::Root(true)]);
    }
    // A separate real Sum-to-Record schema exercises the zUSD-shaped access seam.
    // The original 12-type schema and its complete domains above remain unchanged.
    let payload_fields = [
        Field {
            id: 1,
            name: b"amount",
            type_id: 1,
        },
        Field {
            id: 2,
            name: b"flag",
            type_id: 5,
        },
    ];
    let sum_record = [
        SumVariant {
            id: 1,
            name: b"Stop",
            payload: None,
        },
        SumVariant {
            id: 2,
            name: b"Move",
            payload: Some(13),
        },
    ];
    let mut extended = definitions.to_vec();
    extended[8].kind = Kind::SumPayload(&sum_record);
    extended.push(Definition {
        id: 13,
        name: b"Payload",
        kind: Kind::Record(&payload_fields),
    });
    let mut extended_wire = original.to_vec();
    extended_wire[35..39].copy_from_slice(&13u32.to_be_bytes());
    let name = extended_wire
        .windows(4)
        .position(|w| w == b"Move")
        .unwrap_or_else(|| panic!("strict83 expected original Move declaration"));
    extended_wire[name + 8] = 13;
    extended_wire.extend_from_slice(&13u32.to_be_bytes());
    extended_wire.extend_from_slice(&7u16.to_be_bytes());
    extended_wire.extend_from_slice(b"Payload");
    extended_wire.push(8);
    extended_wire.extend_from_slice(&2u32.to_be_bytes());
    for f in payload_fields {
        extended_wire.extend_from_slice(&f.id.to_be_bytes());
        extended_wire.extend_from_slice(&(f.name.len() as u16).to_be_bytes());
        extended_wire.extend_from_slice(f.name);
        extended_wire.extend_from_slice(&f.type_id.to_be_bytes());
    }
    let extended_description = Description {
        definitions: &extended,
        ..description
    };
    let actual_schema = schema::admit(&extended_wire, &extended_description, schema_limits())
        .unwrap_or_else(|error| panic!("strict83 unexpected typed test refusal: {error:?}"));
    let mut command = alloc::vec![10, 0, 0, 0, 9, 0, 2, 1];
    command.extend_from_slice(&record(&[amount(9), alloc::vec![2]]));
    let mut m = meter::new(limits(100).with_limit(Resource::Read, 3));
    let mut reads = Vec::new();
    let value = decode_root(&command, &actual_schema, 9, &mut m, &mut reads)
        .unwrap_or_else(|error| panic!("strict83 unexpected typed test refusal: {error:?}"));
    assert_eq!(
        output::encode_atom(value.atom, command.len())
            .unwrap_or_else(|error| panic!("strict83 unexpected typed test refusal: {error:?}")),
        command
    );
    let (variant, fields) = value
        .sum
        .unwrap_or_else(|| panic!("strict83 expected actual Sum variant"));
    assert_eq!(variant, 2);
    let fields = fields.unwrap_or_else(|| panic!("strict83 expected actual active payload fields"));
    assert!(matches!(fields[0].value, Atom::U128(9)));
    assert!(matches!(fields[1].value, Atom::Bool(true)));
    assert_eq!(
        reads,
        alloc::vec![
            Access::Root(true),
            Access::PayloadField(1, true),
            Access::PayloadField(2, true)
        ]
    );
    assert_eq!(m.used.used(Resource::Step), 7);
    for cap in 0..3 {
        let mut m = meter::new(limits(100).with_limit(Resource::Read, cap));
        let mut reads = Vec::new();
        assert!(
            matches!(decode_root(&command,&actual_schema,9,&mut m,&mut reads),Err(Failure::Budget(e)) if e.resource==Resource::Read&&e.attempted==cap+1)
        );
        assert_eq!(
            reads.last(),
            Some(&match cap {
                0 => Access::Root(false),
                1 => Access::PayloadField(1, false),
                _ => Access::PayloadField(2, false),
            })
        );
    }
    for end in 0..command.len() {
        let mut m = meter::new(limits(100).with_limit(Resource::Read, 3));
        let mut reads = Vec::new();
        assert_eq!(
            decode_root(&command[..end], &actual_schema, 9, &mut m, &mut reads).err(),
            Some(Failure::Wire)
        );
    }
}

#[test]
fn full_unsigned_wire_is_retained_without_scalar_projection() {
    let definitions = [Definition {
        id: 1,
        name: b"Amount",
        kind: Kind::U128 {
            min: 0,
            max: u128::MAX,
        },
    }];
    let description = Description {
        profile: b"Wide",
        version: 1,
        root: 1,
        definitions: &definitions,
    };
    let mut original = b"ZFCISSCHEMA1\0".to_vec();
    original.extend_from_slice(&4u16.to_be_bytes());
    original.extend_from_slice(b"Wide");
    original.extend_from_slice(&1u16.to_be_bytes());
    original.extend_from_slice(&1u32.to_be_bytes());
    original.extend_from_slice(&1u32.to_be_bytes());
    original.extend_from_slice(&1u32.to_be_bytes());
    original.extend_from_slice(&6u16.to_be_bytes());
    original.extend_from_slice(b"Amount");
    original.push(2);
    original.extend_from_slice(&0u128.to_be_bytes());
    original.extend_from_slice(&u128::MAX.to_be_bytes());
    let checked = schema::admit(&original, &description, schema_limits())
        .unwrap_or_else(|error| panic!("strict83 unexpected typed test refusal: {error:?}"));
    for value in [
        0,
        9,
        i64::MAX as u128,
        i64::MAX as u128 + 1,
        i128::MAX as u128 + 1,
        u128::MAX,
    ] {
        let bytes = amount(value);
        let mut m = meter::new(limits(1));
        let (actual, end) = decode(&bytes, &checked, 0, bytes.len(), 1, &mut m)
            .unwrap_or_else(|error| panic!("strict83 unexpected typed test refusal: {error:?}"));
        assert!(matches!(actual,Atom::U128(v) if v==value));
        assert_eq!(end, 17);
        assert_eq!(m.used.used(Resource::Step), 1);
        assert_eq!(
            output::encode_atom(actual, 17).unwrap_or_else(|error| panic!(
                "strict83 unexpected typed test refusal: {error:?}"
            )),
            bytes
        );
    }
    let bytes = amount(u128::MAX);
    let mut m = meter::new(limits(100));
    assert_eq!(
        decode(&bytes, &checked, 0, 16, 1, &mut m).err(),
        Some(Failure::Wire)
    );
}
