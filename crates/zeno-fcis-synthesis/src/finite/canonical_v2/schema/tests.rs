extern crate std;
use super::*;
use std::vec::Vec;

fn text(output: &mut Vec<u8>, bytes: &[u8]) {
    output.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
    output.extend_from_slice(bytes);
}
fn oracle(description: &Description<'_>) -> Vec<u8> {
    let mut output = Vec::from(&b"ZFCISSCHEMA1\0"[..]);
    text(&mut output, description.profile);
    output.extend_from_slice(&description.version.to_be_bytes());
    output.extend_from_slice(&description.root.to_be_bytes());
    output.extend_from_slice(&(description.definitions.len() as u32).to_be_bytes());
    for definition in description.definitions {
        output.extend_from_slice(&definition.id.to_be_bytes());
        text(&mut output, definition.name);
        match definition.kind {
            Kind::Unit => output.push(0),
            Kind::Bool => output.push(1),
            Kind::U128 { min, max } => {
                output.push(2);
                output.extend_from_slice(&min.to_be_bytes());
                output.extend_from_slice(&max.to_be_bytes());
            }
            Kind::I128 { min, max } => {
                output.push(3);
                output.extend_from_slice(&min.to_be_bytes());
                output.extend_from_slice(&max.to_be_bytes());
            }
            Kind::Bytes { min, max } | Kind::Text { min, max } => {
                output.push(if matches!(definition.kind, Kind::Bytes { .. }) {
                    4
                } else {
                    5
                });
                output.extend_from_slice(&min.to_be_bytes());
                output.extend_from_slice(&max.to_be_bytes());
            }
            Kind::Enum(variants) | Kind::Sum(variants) => {
                let sum = matches!(definition.kind, Kind::Sum(_));
                output.push(if sum { 9 } else { 6 });
                output.extend_from_slice(&(variants.len() as u32).to_be_bytes());
                for variant in variants {
                    output.extend_from_slice(&variant.id.to_be_bytes());
                    text(&mut output, variant.name);
                    if sum {
                        output.push(0);
                    }
                }
            }
            Kind::Record(fields) => {
                output.push(8);
                output.extend_from_slice(&(fields.len() as u32).to_be_bytes());
                for field in fields {
                    output.extend_from_slice(&field.id.to_be_bytes());
                    text(&mut output, field.name);
                    output.extend_from_slice(&field.type_id.to_be_bytes());
                }
            }
            Kind::Tuple(items) => {
                output.push(7);
                output.extend_from_slice(&(items.len() as u32).to_be_bytes());
                for id in items {
                    output.extend_from_slice(&id.to_be_bytes());
                }
            }
            Kind::SumPayload(variants) => {
                output.push(9);
                output.extend_from_slice(&(variants.len() as u32).to_be_bytes());
                for variant in variants {
                    output.extend_from_slice(&variant.id.to_be_bytes());
                    text(&mut output, variant.name);
                    match variant.payload {
                        None => output.push(0),
                        Some(id) => {
                            output.push(1);
                            output.extend_from_slice(&id.to_be_bytes());
                        }
                    }
                }
            }
            Kind::Vector { element, min, max } => {
                output.push(10);
                output.extend_from_slice(&element.to_be_bytes());
                output.extend_from_slice(&min.to_be_bytes());
                output.extend_from_slice(&max.to_be_bytes());
            }
            Kind::Map {
                key,
                value,
                min,
                max,
            } => {
                output.push(11);
                output.extend_from_slice(&key.to_be_bytes());
                output.extend_from_slice(&value.to_be_bytes());
                output.extend_from_slice(&min.to_be_bytes());
                output.extend_from_slice(&max.to_be_bytes());
            }
        }
    }
    output
}

#[test]
fn every_original_schema_byte_and_truncation_is_bound() {
    let variants = [
        Variant {
            id: 0,
            name: b"First",
        },
        Variant {
            id: u16::MAX,
            name: b"Last",
        },
    ];
    let fields = [
        Field {
            id: 0,
            name: b"zero",
            type_id: 1,
        },
        Field {
            id: u16::MAX,
            name: b"last",
            type_id: 2,
        },
    ];
    let definitions = [
        Definition {
            id: 0,
            name: b"State",
            kind: Kind::Record(&fields),
        },
        Definition {
            id: 1,
            name: b"Signed",
            kind: Kind::I128 {
                min: i128::MIN,
                max: i128::MAX,
            },
        },
        Definition {
            id: 2,
            name: b"Unsigned",
            kind: Kind::U128 {
                min: 0,
                max: u128::MAX,
            },
        },
        Definition {
            id: 3,
            name: b"Flag",
            kind: Kind::Bool,
        },
        Definition {
            id: 4,
            name: b"UnusedUnit",
            kind: Kind::Unit,
        },
        Definition {
            id: 5,
            name: b"Bytes",
            kind: Kind::Bytes {
                min: 0,
                max: u32::MAX,
            },
        },
        Definition {
            id: 6,
            name: b"Text",
            kind: Kind::Text { min: 1, max: 96 },
        },
        Definition {
            id: 7,
            name: b"Enumeration",
            kind: Kind::Enum(&variants),
        },
        Definition {
            id: u32::MAX,
            name: b"Command",
            kind: Kind::Sum(&variants),
        },
    ];
    let description = Description {
        profile: b"Original",
        version: 0,
        root: 0,
        definitions: &definitions,
    };
    let bytes = oracle(&description);
    assert!(encoding_matches(&bytes, &description, bytes.len() as u64));
    assert!(!encoding_matches(
        &bytes,
        &description,
        bytes.len() as u64 - 1
    ));
    for end in 0..bytes.len() {
        assert!(!encoding_matches(&bytes[..end], &description, u64::MAX));
    }
    for index in 0..bytes.len() {
        let mut changed = bytes.clone();
        for value in 0..=u8::MAX {
            changed[index] = value;
            assert_eq!(
                encoding_matches(&changed, &description, u64::MAX),
                value == bytes[index],
                "byte {index}, value {value}"
            );
        }
    }
    let mut trailing = bytes;
    trailing.push(0);
    assert!(!encoding_matches(&trailing, &description, u64::MAX));
}

#[test]
fn empty_records_variant_sets_and_unused_definitions_are_retained() {
    let definitions = [
        Definition {
            id: 0,
            name: b"EmptyState",
            kind: Kind::Record(&[]),
        },
        Definition {
            id: 1,
            name: b"EmptyEnum",
            kind: Kind::Enum(&[]),
        },
        Definition {
            id: 2,
            name: b"EmptySum",
            kind: Kind::Sum(&[]),
        },
    ];
    let description = Description {
        profile: b"Empty",
        version: u16::MAX,
        root: 0,
        definitions: &definitions,
    };
    let bytes = oracle(&description);
    assert!(encoding_matches(&bytes, &description, u64::MAX));
    let omitted = Description {
        definitions: &definitions[..1],
        ..description
    };
    assert!(!encoding_matches(&bytes, &omitted, u64::MAX));
    assert_eq!(blob(&bytes, usize::MAX, b"x"), None);
    assert_eq!(word(&bytes, usize::MAX, 16, 0), None);
    assert_eq!(signed(&bytes, usize::MAX, 0), None);
}

fn policy() -> Limits {
    Limits {
        bytes: u64::MAX,
        types: 64,
        fields: 1024,
        variants: 1024,
    }
}

#[test]
fn closed_schema_admission_preserves_full_width_and_original_custody() {
    let fields = [Field {
        id: 0,
        name: b"wide",
        type_id: 1,
    }];
    let definitions = [
        Definition {
            id: 0,
            name: b"State",
            kind: Kind::Record(&fields),
        },
        Definition {
            id: 1,
            name: b"FullSigned",
            kind: Kind::I128 {
                min: i128::MIN,
                max: i128::MAX,
            },
        },
        Definition {
            id: 2,
            name: b"FullUnsigned",
            kind: Kind::U128 {
                min: 0,
                max: u128::MAX,
            },
        },
        Definition {
            id: 3,
            name: b"EmptyEnum",
            kind: Kind::Enum(&[]),
        },
        Definition {
            id: u32::MAX,
            name: b"EmptySum",
            kind: Kind::Sum(&[]),
        },
    ];
    let description = Description {
        profile: b"_Original9",
        version: 0,
        root: 0,
        definitions: &definitions,
    };
    let bytes = oracle(&description);
    let checked = admit(&bytes, &description, policy()).unwrap_or_else(|error| panic!("closed_schema_admission_preserves_full_width_and_original_custody fixture failed: {error:?}"));
    assert_eq!(checked.original(), bytes);
    assert_eq!(checked.original().as_ptr(), bytes.as_ptr());
    assert!(core::ptr::eq(checked.description(), &description));
    let exact = Limits {
        bytes: bytes.len() as u64,
        types: definitions.len() as u32,
        fields: 1,
        variants: 0,
    };
    assert!(admit(&bytes, &description, exact).is_ok());
    assert_eq!(
        admit(
            &bytes,
            &description,
            Limits {
                bytes: exact.bytes - 1,
                ..exact
            }
        )
        .err()
        .unwrap_or_else(|| panic!(
            "expected refusal in closed_schema_admission_preserves_full_width_and_original_custody"
        )),
        Failure::Size
    );
    assert_eq!(
        admit(
            &bytes,
            &description,
            Limits {
                types: exact.types - 1,
                ..exact
            }
        )
        .err()
        .unwrap_or_else(|| panic!(
            "expected refusal in closed_schema_admission_preserves_full_width_and_original_custody"
        )),
        Failure::Metadata
    );
    assert_eq!(
        admit(&bytes, &description, Limits { fields: 0, ..exact }).err().unwrap_or_else(|| panic!("expected refusal in closed_schema_admission_preserves_full_width_and_original_custody")),
        Failure::Metadata
    );
    let renamed = Description {
        profile: b"Renamed",
        ..description
    };
    assert_eq!(
        admit(&bytes, &renamed, policy()).err().unwrap_or_else(|| panic!("expected refusal in closed_schema_admission_preserves_full_width_and_original_custody")),
        Failure::Encoding
    );
    let omitted = Description {
        definitions: &definitions[..2],
        ..description
    };
    assert_eq!(
        admit(&bytes, &omitted, policy()).err().unwrap_or_else(|| panic!("expected refusal in closed_schema_admission_preserves_full_width_and_original_custody")),
        Failure::Encoding
    );
    let malformed = Description {
        profile: b"",
        ..description
    };
    assert_eq!(
        admit(&bytes, &malformed, Limits { bytes: 0, ..exact }).err().unwrap_or_else(|| panic!("expected refusal in closed_schema_admission_preserves_full_width_and_original_custody")),
        Failure::Size
    );
    assert_eq!(
        admit(&bytes, &malformed, policy()).err().unwrap_or_else(|| panic!("expected refusal in closed_schema_admission_preserves_full_width_and_original_custody")),
        Failure::Metadata
    );
}

#[test]
fn metadata_refuses_bad_names_ranges_duplicates_references_and_order() {
    let good_fields = [Field {
        id: 0,
        name: b"value",
        type_id: 1,
    }];
    let unknown_fields = [Field {
        type_id: 9,
        ..good_fields[0]
    }];
    let cyclic_fields = [Field {
        type_id: 0,
        ..good_fields[0]
    }];
    let duplicate_fields = [
        good_fields[0],
        Field {
            id: 1,
            ..good_fields[0]
        },
    ];
    let duplicate_field_ids = [
        good_fields[0],
        Field {
            id: 0,
            name: b"distinct",
            type_id: 1,
        },
    ];
    let reverse_fields = [
        Field {
            id: 2,
            name: b"later",
            type_id: 1,
        },
        good_fields[0],
    ];
    let good_variants = [
        Variant {
            id: 0,
            name: b"First",
        },
        Variant {
            id: 1,
            name: b"Second",
        },
    ];
    let duplicate_variants = [
        good_variants[0],
        Variant {
            id: 1,
            ..good_variants[0]
        },
    ];
    let duplicate_variant_ids = [
        good_variants[0],
        Variant {
            id: 0,
            name: b"Distinct",
        },
    ];
    let reverse_variants = [good_variants[1], good_variants[0]];
    let long_name = [b'A'; 97];
    let base = [
        Definition {
            id: 0,
            name: b"State",
            kind: Kind::Record(&good_fields),
        },
        Definition {
            id: 1,
            name: b"Integer",
            kind: Kind::I128 { min: 0, max: 2 },
        },
    ];
    let cases = [
        [
            Definition {
                kind: Kind::Record(&[]),
                ..base[0]
            },
            Definition { id: 0, ..base[1] },
        ],
        [
            Definition {
                kind: Kind::Record(&unknown_fields),
                ..base[0]
            },
            base[1],
        ],
        [
            Definition {
                kind: Kind::Record(&cyclic_fields),
                ..base[0]
            },
            base[1],
        ],
        [
            Definition {
                kind: Kind::Record(&duplicate_fields),
                ..base[0]
            },
            base[1],
        ],
        [
            Definition {
                kind: Kind::Record(&duplicate_field_ids),
                ..base[0]
            },
            base[1],
        ],
        [
            Definition {
                kind: Kind::Record(&reverse_fields),
                ..base[0]
            },
            base[1],
        ],
        [
            base[0],
            Definition {
                kind: Kind::I128 { min: 3, max: 2 },
                ..base[1]
            },
        ],
        [
            base[0],
            Definition {
                kind: Kind::U128 { min: 3, max: 2 },
                ..base[1]
            },
        ],
        [
            base[0],
            Definition {
                kind: Kind::Bytes { min: 3, max: 2 },
                ..base[1]
            },
        ],
        [
            base[0],
            Definition {
                kind: Kind::Text { min: 3, max: 2 },
                ..base[1]
            },
        ],
        [
            base[0],
            Definition {
                kind: Kind::Enum(&duplicate_variants),
                ..base[1]
            },
        ],
        [
            base[0],
            Definition {
                kind: Kind::Enum(&duplicate_variant_ids),
                ..base[1]
            },
        ],
        [
            base[0],
            Definition {
                kind: Kind::Sum(&reverse_variants),
                ..base[1]
            },
        ],
        [
            base[0],
            Definition {
                name: base[0].name,
                ..base[1]
            },
        ],
        [
            base[0],
            Definition {
                name: b"",
                ..base[1]
            },
        ],
        [
            base[0],
            Definition {
                name: b"9Bad",
                ..base[1]
            },
        ],
        [
            base[0],
            Definition {
                name: b"Has Space",
                ..base[1]
            },
        ],
        [
            base[0],
            Definition {
                name: b"\xff",
                ..base[1]
            },
        ],
        [
            base[0],
            Definition {
                name: &long_name,
                ..base[1]
            },
        ],
        [base[1], base[0]],
    ];
    for definitions in &cases {
        let description = Description {
            profile: b"Bad",
            version: 1,
            root: 0,
            definitions,
        };
        let bytes = oracle(&description);
        assert!(encoding_matches(&bytes, &description, u64::MAX));
        assert_eq!(
            admit(&bytes, &description, policy()).err().unwrap_or_else(|| panic!("expected refusal in metadata_refuses_bad_names_ranges_duplicates_references_and_order")),
            Failure::Metadata
        );
    }
    let no_root = Description {
        profile: b"BadRoot",
        version: 1,
        root: 2,
        definitions: &base,
    };
    assert_eq!(
        admit(&oracle(&no_root), &no_root, policy()).err().unwrap_or_else(|| panic!("expected refusal in metadata_refuses_bad_names_ranges_duplicates_references_and_order")),
        Failure::Metadata
    );
    let good = Description {
        profile: b"Good",
        version: 1,
        root: 0,
        definitions: &base,
    };
    assert!(admit(&oracle(&good), &good, policy()).is_ok());
}

#[test]
fn shared_dag_and_complete_depth_use_bounded_construction_passes() {
    // Sixty-four definitions with two references to every predecessor represent
    // more than 2^63 dependency paths. Admission processes the shared edges.
    let names: Vec<Vec<u8>> = (0..64)
        .map(|i| std::format!("Node{i}").into_bytes())
        .collect();
    let edges: Vec<[u32; 2]> = (0..64).map(|i| [i, i]).collect();
    let definitions: Vec<Definition<'_>> = (0..64)
        .map(|i| Definition {
            id: i as u32 + 1,
            name: &names[i],
            kind: if i == 0 {
                Kind::Unit
            } else {
                Kind::Tuple(&edges[i])
            },
        })
        .collect();
    let description = Description {
        profile: b"SharedGraph",
        version: 1,
        root: 64,
        definitions: &definitions,
    };
    let bytes = oracle(&description);
    assert!(admit(&bytes, &description, policy()).is_ok());
    let exact = Limits {
        bytes: bytes.len() as u64,
        types: 64,
        fields: 2,
        variants: 0,
    };
    assert!(admit(&bytes, &description, exact).is_ok());
    assert_eq!(
        admit(&bytes, &description, Limits { types: 63, ..exact }).err(),
        Some(Failure::Metadata)
    );
    assert_eq!(
        admit(&bytes, &description, Limits { fields: 1, ..exact }).err(),
        Some(Failure::Metadata)
    );
    assert_eq!(
        admit(
            &bytes,
            &description,
            Limits {
                bytes: exact.bytes - 1,
                ..exact
            }
        )
        .err(),
        Some(Failure::Size)
    );

    let chain: Vec<Definition<'_>> = (0..64)
        .map(|i| Definition {
            id: i as u32 + 1,
            name: &names[i],
            kind: if i == 63 {
                Kind::Unit
            } else {
                Kind::Vector {
                    element: i as u32 + 2,
                    min: 0,
                    max: 1,
                }
            },
        })
        .collect();
    let description = Description {
        profile: b"DeepChain",
        version: 1,
        root: 1,
        definitions: &chain,
    };
    assert!(admit(&oracle(&description), &description, policy()).is_ok());
}

#[test]
fn every_compound_reference_kind_refuses_unknown_and_cyclic_types() {
    let missing_tuple = [99];
    let cyclic_tuple = [2];
    let missing_sum = [SumVariant {
        id: 1,
        name: b"Missing",
        payload: Some(99),
    }];
    let cyclic_sum = [SumVariant {
        id: 1,
        name: b"Cycle",
        payload: Some(2),
    }];
    let missing_record = [Field {
        id: 1,
        name: b"missing",
        type_id: 99,
    }];
    let cyclic_record = [Field {
        id: 1,
        name: b"cycle",
        type_id: 2,
    }];
    let cases = [
        Kind::Tuple(&missing_tuple),
        Kind::Tuple(&cyclic_tuple),
        Kind::SumPayload(&missing_sum),
        Kind::SumPayload(&cyclic_sum),
        Kind::Vector {
            element: 99,
            min: 0,
            max: 4,
        },
        Kind::Vector {
            element: 2,
            min: 0,
            max: 4,
        },
        Kind::Map {
            key: 99,
            value: 1,
            min: 0,
            max: 4,
        },
        Kind::Map {
            key: 1,
            value: 99,
            min: 0,
            max: 4,
        },
        Kind::Map {
            key: 2,
            value: 1,
            min: 0,
            max: 4,
        },
        Kind::Map {
            key: 1,
            value: 2,
            min: 0,
            max: 4,
        },
        Kind::Record(&missing_record),
        Kind::Record(&cyclic_record),
    ];
    for kind in cases {
        let definitions = [
            Definition {
                id: 1,
                name: b"Root",
                kind: Kind::Unit,
            },
            Definition {
                id: 2,
                name: b"Unused",
                kind,
            },
        ];
        let description = Description {
            profile: b"Closed",
            version: 1,
            root: 1,
            definitions: &definitions,
        };
        let bytes = oracle(&description);
        assert!(encoding_matches(&bytes, &description, u64::MAX));
        assert_eq!(
            admit(&bytes, &description, policy()).err(),
            Some(Failure::Metadata)
        );
    }
    let forward = [2];
    let back = [1];
    let definitions = [
        Definition {
            id: 1,
            name: b"First",
            kind: Kind::Tuple(&forward),
        },
        Definition {
            id: 2,
            name: b"Second",
            kind: Kind::Tuple(&back),
        },
    ];
    let description = Description {
        profile: b"Cycle",
        version: 1,
        root: 1,
        definitions: &definitions,
    };
    assert_eq!(
        admit(&oracle(&description), &description, policy()).err(),
        Some(Failure::Metadata)
    );
}

#[test]
fn original_twelve_type_schema_is_admitted_without_erasing_compound_definitions() {
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
    assert_eq!(oracle(&description), original);
    let checked = admit(original, &description, policy())
        .unwrap_or_else(|e| panic!("original schema refused: {e:?}"));
    assert_eq!(checked.original(), original);
    assert_eq!(checked.description().definitions.len(), 12);
    assert_eq!(checked.description().root, 12);
    for end in 0..original.len() {
        assert_eq!(
            admit(&original[..end], &description, policy()).err(),
            Some(Failure::Encoding)
        );
    }
    for index in 0..original.len() {
        let mut modified = original.to_vec();
        modified[index] ^= 1;
        assert_eq!(
            admit(&modified, &description, policy()).err(),
            Some(Failure::Encoding)
        );
    }
    let erased_sum = [
        SumVariant {
            id: 1,
            name: b"Stop",
            payload: None,
        },
        SumVariant {
            id: 2,
            name: b"Move",
            payload: None,
        },
    ];
    let mut changed = definitions;
    changed[8].kind = Kind::SumPayload(&erased_sum);
    let changed_description = Description {
        definitions: &changed,
        ..description
    };
    assert_eq!(
        admit(original, &changed_description, policy()).err(),
        Some(Failure::Encoding)
    );
    for index in 7..12 {
        let mut changed = definitions;
        changed[index].kind = Kind::Unit;
        let changed_description = Description {
            definitions: &changed,
            ..description
        };
        assert_eq!(
            admit(original, &changed_description, policy()).err(),
            Some(Failure::Encoding)
        );
    }
}
