//! Public registered output port, compared with the independent original codec.
use zeno_fcis_codec::{CanonicalEncode, Envelope, Hash32};
use zeno_fcis_synthesis::finite::canonical_v2::{envelope, output};
use zeno_fcis_synthesis::finite::v2_composition::{Atom, Field};
use zeno_fcis_value::{Field as ValueField, Value};

#[test]
fn public_original_output_preserves_typed_record_and_immutable_framing() {
    let fields = [
        Field {
            id: 0,
            value: Atom::I128(i128::MIN),
        },
        Field {
            id: u16::MAX,
            value: Atom::Sum {
                type_id: u32::MAX,
                variant: 0,
            },
        },
    ];
    let expected_value = Value::record_canonical(vec![
        ValueField::new(0, Value::signed(i128::MIN)),
        ValueField::new(u16::MAX, Value::sum(u32::MAX, 0, None)),
    ])
    .unwrap_or_else(|error| panic!("canonical original fields: {error:?}"));
    let body = output::encode_record(&fields, usize::MAX)
        .unwrap_or_else(|error| panic!("typed record: {error:?}"));
    assert_eq!(
        body,
        expected_value
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("original codec: {error:?}"))
    );
    assert_eq!(
        output::encode_atom(Atom::I128(i128::MIN), 17),
        Value::signed(i128::MIN)
            .canonical_bytes()
            .map_err(|_| output::Failure::Length)
    );
    let schema = core::array::from_fn(|i| (i * 7 + 1) as u8);
    let wire = output::encode_envelope(0, &schema, &body, body.len() + 48)
        .unwrap_or_else(|error| panic!("exact cap: {error:?}"));
    assert_eq!(
        wire,
        Envelope::new(0, Hash32::new(schema), expected_value)
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("original frame: {error:?}"))
    );
    let parsed = envelope::frame(&wire, 0, &schema, wire.len() as u64)
        .unwrap_or_else(|error| panic!("actual reader: {error:?}"));
    assert_eq!(parsed.bytes(), body);
    assert_eq!(
        output::encode_envelope(0, &schema, &body, wire.len() - 1),
        Err(output::Failure::Limit)
    );
    assert_eq!(fields[0].value, Atom::I128(i128::MIN));
}
