//! Check the public protected-record interface against actual ZCVE encoding.
use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_synthesis::finite::{
    V2InputField, V2InputLeaf, V2InputVariant, V2RecordFailure, V2Resource, project_record_v2,
    v2_zero_limits,
};
use zeno_fcis_value::{Field, Value};

#[test]
fn actual_canonical_records_preserve_closed_mapping_and_opaque_usage() {
    let record = Value::record_canonical(vec![
        Field::new(0, Value::signed(i64::MIN as i128)),
        Field::new(7, Value::boolean(true)),
        Field::new(32, Value::enumeration(u32::MAX, u16::MAX)),
        Field::new(u16::MAX, Value::sum(0, 7, None)),
    ])
    .unwrap_or_else(|error| panic!("canonical record: {error}"));
    let bytes = record
        .canonical_bytes()
        .unwrap_or_else(|error| panic!("record bytes: {error}"));
    let fields = [
        V2InputField {
            id: 0,
            leaf: V2InputLeaf::I128 {
                min: i64::MIN,
                max: i64::MIN,
            },
        },
        V2InputField {
            id: 7,
            leaf: V2InputLeaf::Bool,
        },
        V2InputField {
            id: 32,
            leaf: V2InputLeaf::Enum {
                type_id: u32::MAX,
                min: 10,
                max: 11,
                variants: vec![
                    V2InputVariant { id: 7, code: 11 },
                    V2InputVariant {
                        id: u16::MAX,
                        code: 10,
                    },
                ],
            },
        },
        V2InputField {
            id: u16::MAX,
            leaf: V2InputLeaf::Sum {
                type_id: 0,
                min: -11,
                max: -10,
                variants: vec![
                    V2InputVariant { id: 7, code: -11 },
                    V2InputVariant { id: 0, code: -10 },
                ],
            },
        },
    ];
    let limits = v2_zero_limits()
        .with_limit(V2Resource::Byte, bytes.len() as u64)
        .with_limit(V2Resource::Read, 4);
    let outcome = project_record_v2(&bytes, &fields, limits);
    assert_eq!(outcome.usage().used(V2Resource::Byte), bytes.len() as u64);
    assert_eq!(outcome.usage().used(V2Resource::Read), 4);
    assert_eq!(outcome.usage().used(V2Resource::Step), 0);
    let expected_attempts = [(0, true), (7, true), (32, true), (u16::MAX, true)];
    assert_eq!(
        outcome
            .attempts()
            .iter()
            .map(|a| (a.field_id(), a.permitted()))
            .collect::<Vec<_>>(),
        expected_attempts
    );
    assert_eq!(outcome.into_parts().0, Ok(vec![i64::MIN, 1, 10, -11]));
    let refused = project_record_v2(&bytes, &fields, limits.with_limit(V2Resource::Read, 3));
    assert_eq!(refused.usage().used(V2Resource::Read), 3);
    assert_eq!(refused.usage().used(V2Resource::Byte), bytes.len() as u64);
    assert_eq!(
        refused
            .attempts()
            .last()
            .map(|a| (a.field_id(), a.permitted())),
        Some((u16::MAX, false))
    );
    assert!(
        matches!(refused.into_parts().0, Err(V2RecordFailure::Budget(error))
        if error.resource == V2Resource::Read && error.attempted == 4 && error.limit == 3)
    );
    let mut malformed = bytes.clone();
    malformed[0] = 255;
    let refused = project_record_v2(
        &malformed,
        &fields,
        limits.with_limit(V2Resource::Byte, bytes.len() as u64 - 1),
    );
    assert!(refused.attempts().is_empty());
    assert_eq!(refused.usage().used(V2Resource::Byte), 0);
    assert!(
        matches!(refused.into_parts().0, Err(V2RecordFailure::Budget(error))
        if error.resource == V2Resource::Byte)
    );
    for resource in [
        V2Resource::Write,
        V2Resource::Candidate,
        V2Resource::Effect,
        V2Resource::WitnessByte,
        V2Resource::Depth,
        V2Resource::Step,
    ] {
        assert_eq!(
            project_record_v2(&bytes, &fields, limits)
                .usage()
                .used(resource),
            0
        );
    }
}
