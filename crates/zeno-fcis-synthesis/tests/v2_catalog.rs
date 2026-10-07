//! Public catalog custody and deterministic refusal order on the actual crate API.
use zeno_fcis_synthesis::finite::{
    self, canonical_v2::schema, v2_catalog as catalog, v2_composition as c,
};

#[test]
fn public_catalog_refuses_before_exposing_an_incomplete_descriptor() {
    // A complete original schema with one legal empty state record, independently encoded.
    let mut raw = b"ZFCISSCHEMA1\0".to_vec();
    raw.extend(1u16.to_be_bytes());
    raw.extend(b"P");
    raw.extend(0u16.to_be_bytes());
    raw.extend(0u32.to_be_bytes());
    raw.extend(1u32.to_be_bytes());
    raw.extend(0u32.to_be_bytes());
    raw.extend(1u16.to_be_bytes());
    raw.extend(b"S");
    raw.push(8);
    raw.extend(0u32.to_be_bytes());
    let defs = [schema::Definition {
        id: 0,
        name: b"S",
        kind: schema::Kind::Record(&[]),
    }];
    let description = schema::Description {
        profile: b"P",
        version: 0,
        root: 0,
        definitions: &defs,
    };
    let checked = schema::admit(
        &raw,
        &description,
        schema::Limits {
            bytes: u64::MAX,
            types: 1,
            fields: 0,
            variants: 0,
        },
    )
    .unwrap_or_else(|error| panic!("public_catalog_refuses_before_exposing_an_incomplete_descriptor fixture failed: {error:?}"));
    let d = c::Descriptor {
        state: c::Schema::Record(&[]),
        command: c::Schema::Record(&[]),
        context: c::Schema::Record(&[]),
        program: finite::V2ScalarProgram {
            inputs: &[],
            outputs: &[],
            nodes: &[],
            roots: &[],
        },
        bindings: &[],
        output_types: &[],
        decision_output: 0,
        branches: &[],
        reasons: &[],
        channels: &[],
        laws: &[],
        required: &[],
        limits: finite::v2_zero_limits(),
    };
    let binding = c::FrameBinding {
        root: 0,
        schema: [0; 32],
        max_bytes: 0,
    };
    let framing = c::Framing {
        state: binding,
        command: binding,
        context: binding,
    };
    let limits = |contract_bytes| catalog::Limits {
        schema: schema::Limits {
            bytes: u64::MAX,
            types: 1,
            fields: 0,
            variants: 0,
        },
        contract_bytes,
    };
    assert_eq!(
        catalog::bind_original(
            checked.original(),
            checked.description(),
            limits(0),
            b"x",
            &d,
            &framing,
            &[]
        )
        .err()
        .unwrap_or_else(|| panic!(
            "expected refusal in public_catalog_refuses_before_exposing_an_incomplete_descriptor"
        )),
        catalog::Failure::Size
    );
    assert_eq!(
        catalog::bind_original(
            checked.original(),
            checked.description(),
            limits(1),
            b"x",
            &d,
            &framing,
            &[]
        )
        .err()
        .unwrap_or_else(|| panic!(
            "expected refusal in public_catalog_refuses_before_exposing_an_incomplete_descriptor"
        )),
        catalog::Failure::Descriptor
    );
}
