//! Independent oracle and actual public-API consumer for the ZAL handoff.
//! Finite conformance does not discharge C0/S1/S2/S3 implementation refinement.
#![cfg(feature = "schema-profile-2")]

use core::cmp::Ordering;
use zeno_fcis_codec::profile2::Envelope;
use zeno_fcis_codec::{Hash32, RustCryptoSha256 as Hash};
use zeno_fcis_schema::profile2::{
    Schema, SchemaAdmittedEnvelope, SchemaEnvelopeError, SchemaLimits, SchemaProfile, TypeDef,
    TypeId, TypeKind, ValidationLimits, ValueValidationError,
};
use zeno_fcis_value::profile2::{I256, U256, Value};

fn bits(hex: &str) -> U256 {
    assert_eq!(hex.len(), 64);
    U256::from_words_le([
        u128::from_str_radix(&hex[32..], 16).unwrap(),
        u128::from_str_radix(&hex[..32], 16).unwrap(),
    ])
}

fn rows(text: &str) -> impl Iterator<Item = Vec<&str>> {
    text.lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .map(|line| line.split_whitespace().collect())
}

fn ordinal(order: Ordering) -> i8 {
    match order {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }
}

fn schema(kind: TypeKind, id: u32) -> Schema {
    let id = TypeId::new(id);
    Schema::try_new(
        SchemaProfile::VALUE2,
        "OntologyAmount",
        1,
        id,
        vec![TypeDef::try_new(id, "Amount", kind, SchemaLimits::default()).unwrap()],
        SchemaLimits::default(),
    )
    .unwrap()
}

#[test]
fn independent_vectors() {
    let mut count = 0;
    for row in rows(include_str!(
        "../../../test-data/ontology-arithmetic/order.tsv"
    )) {
        assert_eq!(row.len(), 4);
        let a = bits(row[0]);
        let b = bits(row[1]);
        // Expected words are parsed separately; compare against the actual view.
        assert_eq!(
            a.words_le()[0],
            u128::from_str_radix(&row[0][32..], 16).unwrap()
        );
        assert_eq!(
            a.words_le()[1],
            u128::from_str_radix(&row[0][..32], 16).unwrap()
        );
        let sa = I256::from_bits(a);
        assert_eq!(sa.bits(), a);
        assert_eq!(sa.is_negative(), row[0].as_bytes()[0] >= b'8');
        assert_eq!(ordinal(a.cmp(b)), row[2].parse::<i8>().unwrap());
        assert_eq!(
            ordinal(sa.cmp(I256::from_bits(b))),
            row[3].parse::<i8>().unwrap()
        );
        count += 1;
    }
    assert_eq!(count, 1600);
}

#[test]
fn schema_ranges_and_nominal_refusals() {
    let mut count = 0;
    for row in rows(include_str!(
        "../../../test-data/ontology-arithmetic/ranges.tsv"
    )) {
        assert_eq!(row.len(), 5);
        let (min, max, value) = (bits(row[1]), bits(row[2]), bits(row[3]));
        let (kind, correct, wrong) = match row[0] {
            "U256" => (
                TypeKind::U256 { min, max },
                Value::unsigned256(value),
                Value::signed256(I256::from_bits(value)),
            ),
            "I256" => (
                TypeKind::I256 {
                    min: I256::from_bits(min),
                    max: I256::from_bits(max),
                },
                Value::signed256(I256::from_bits(value)),
                Value::unsigned256(value),
            ),
            _ => panic!("unknown oracle kind"),
        };
        let selected = schema(kind, 1);
        let result = selected.validate_root(&correct, ValidationLimits::default());
        if row[4] == "1" {
            assert!(result.is_ok(), "{row:?}");
            let owned = SchemaAdmittedEnvelope::try_new::<Hash>(
                &selected,
                correct.clone(),
                ValidationLimits::default(),
            )
            .unwrap();
            assert_eq!(owned.value().value(), &correct);
            assert_eq!(owned.validation_report(), result.unwrap());
        } else {
            assert_eq!(result, Err(ValueValidationError::IntegerRange));
        }
        for rejected in [wrong, Value::unsigned(0), Value::signed(0)] {
            assert_eq!(
                selected.validate_root(&rejected, ValidationLimits::default()),
                Err(ValueValidationError::TypeMismatch)
            );
            // Even the expected schema hash cannot authorize a wrong nominal leaf.
            let raw = Envelope::new(1, selected.schema_hash::<Hash>().unwrap(), rejected);
            assert_eq!(
                SchemaAdmittedEnvelope::try_from_envelope::<Hash>(
                    &selected,
                    raw,
                    ValidationLimits::default()
                ),
                Err(SchemaEnvelopeError::SchemaValidation(
                    ValueValidationError::TypeMismatch
                ))
            );
        }
        count += 1;
    }
    assert_eq!(count, 320);
}

#[test]
fn requirements_select_actual_schema_types() {
    let mut count = 0;
    for row in rows(include_str!(
        "../../../test-data/ontology-arithmetic/requirements.tsv"
    )) {
        assert_eq!(row.len(), 4);
        let (expected_id, expected_kind) = match row[0] {
            "swapBalance" => (1, "U256"),
            "signedPnl" => (2, "I256"),
            _ => panic!("unexpected requirement"),
        };
        assert_eq!(row[1].parse::<u32>().unwrap(), expected_id);
        assert_eq!(
            row[2], expected_kind,
            "signed and unsigned requirements interchanged"
        );
        assert!(row[3].starts_with("r-") && row[3].len() == 66);
        let (kind, admitted) = match row[2] {
            "U256" => (
                TypeKind::U256 {
                    min: U256::ZERO,
                    max: U256::MAX,
                },
                Value::unsigned256(U256::MAX),
            ),
            "I256" => (
                TypeKind::I256 {
                    min: I256::MIN,
                    max: I256::MAX,
                },
                Value::signed256(I256::MIN),
            ),
            _ => panic!("unsupported requirement type"),
        };
        let selected = schema(kind, expected_id);
        assert!(
            selected
                .validate_root(&admitted, ValidationLimits::default())
                .is_ok()
        );
        // Type identity precedes hash and zero validation budget.
        let raw = Envelope::new(expected_id + 1, Hash32::ZERO, admitted);
        assert!(matches!(
            SchemaAdmittedEnvelope::try_from_envelope::<Hash>(
                &selected,
                raw,
                ValidationLimits {
                    max_depth: 0,
                    max_nodes: 0
                }
            ),
            Err(SchemaEnvelopeError::TypeIdentity { .. })
        ));
        count += 1;
    }
    assert_eq!(count, 2);
}
