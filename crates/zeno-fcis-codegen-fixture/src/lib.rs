//! Compiled generated Rust fixture for `zeno-fcis-codegen`.
//!
//! The `generated` module is produced at build time by `build.rs` from the
//! canonical fixture schema and is included verbatim. It exposes typed domain
//! adapters, strict `to_value`/`try_from_value` conversions, typed patch-path
//! constructors, and the codec vector evidence table.

#![forbid(unsafe_code)]
#![cfg_attr(not(feature = "std"), no_std)]
#![allow(missing_docs, clippy::all, clippy::pedantic)]

pub mod generated {
    #![allow(dead_code, unused_imports, missing_docs, clippy::all, clippy::pedantic)]
    include!(concat!(env!("OUT_DIR"), "/codegen_fixture.rs"));
}

pub use zeno_fcis_codegen::{fixture_schema, fixture_spec};

#[cfg(test)]
mod tests {
    use super::generated::*;
    use super::generated::{VectorExpect, VectorKind};
    use zeno_fcis_codec::{DecodeLimits, decode_value};
    use zeno_fcis_schema::{Schema, TypeId, ValidationLimits};

    fn schema() -> Schema {
        zeno_fcis_codegen::fixture_schema().unwrap_or_else(|e| panic!("fixture schema failed: {e}"))
    }

    #[test]
    fn positive_vector_round_trips_through_codec() {
        for case in VECTORS {
            if !matches!(case.kind, VectorKind::Positive | VectorKind::Boundary) {
                continue;
            }
            let decoded = decode_value(case.bytes, DecodeLimits::default());
            assert!(decoded.is_ok(), "positive vector {} must decode", case.name);
        }
    }

    #[test]
    fn negative_vectors_reject_at_decode_or_validate() {
        let schema = schema();
        for case in VECTORS {
            let decode_result = decode_value(case.bytes, DecodeLimits::default());
            match case.expect {
                VectorExpect::Accept => {
                    let value = decode_result
                        .unwrap_or_else(|e| panic!("{} should decode: {e}", case.name));
                    if let Some(type_id) = case.validate_type {
                        let type_id = TypeId::new(type_id);
                        assert!(
                            schema
                                .validate_value(type_id, &value, ValidationLimits::default())
                                .is_ok(),
                            "{} should validate",
                            case.name
                        );
                    }
                }
                VectorExpect::DecodeReject(_) => {
                    assert!(
                        decode_result.is_err(),
                        "{} should be rejected at decode",
                        case.name
                    );
                }
                VectorExpect::ValidateReject(_) => {
                    let value = decode_result
                        .unwrap_or_else(|e| panic!("{} should decode: {e}", case.name));
                    if let Some(type_id) = case.validate_type {
                        let type_id = TypeId::new(type_id);
                        assert!(
                            schema
                                .validate_value(type_id, &value, ValidationLimits::default())
                                .is_err(),
                            "{} should be rejected at validation",
                            case.name
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn typed_adapter_round_trips_minimal() {
        let amount = Amount(0);
        let signed = Signed(-1000);
        let label = Label("a".into());
        let blob = Blob(vec![].into_boxed_slice());
        let flag = Flag(false);
        let nil = Nil;
        let tag = Tag::Idle;
        let point = Point {
            field_0: Amount(0),
            field_1: Tag::Idle,
        };
        let event = Event::Stop;
        let labels = Labels(vec![].into_boxed_slice());
        let scores = Scores(vec![].into_boxed_slice());
        let state = BalanceState {
            amount,
            signed,
            label,
            blob,
            flag,
            nil,
            tag,
            point,
            event,
            labels,
            scores,
        };
        let value = state
            .to_value()
            .unwrap_or_else(|e| panic!("to_value failed: {e:?}"));
        let round_tripped = BalanceState::try_from_value(value)
            .unwrap_or_else(|e| panic!("try_from_value failed: {e:?}"));
        assert_eq!(state, round_tripped);
    }

    #[test]
    fn patch_path_constructors_use_stable_field_ids() {
        let amount_path = BalanceState::amount_path();
        let segments = amount_path.segments();
        assert_eq!(segments.len(), 1);
        match &segments[0] {
            zeno_fcis_patch::PathSegment::Field(id) => {
                assert_eq!(*id, FIELD_BALANCESTATE_AMOUNT);
            }
            other => panic!("expected Field segment, got {other:?}"),
        }
    }

    #[test]
    fn unknown_variant_rejected_by_adapter() {
        let bad = zeno_fcis_value::Value::Enum {
            type_id: TYPE_TAG,
            variant: 999,
        };
        let result = Tag::try_from_value(bad);
        assert!(result.is_err());
    }

    #[test]
    fn integer_range_enforced_by_adapter() {
        let over_max = zeno_fcis_value::Value::U128(1_000_001);
        let result = Amount::try_from_value(over_max);
        assert!(result.is_err());
    }

    #[test]
    fn outgoing_schema_bounds_are_enforced_by_adapter() {
        assert_eq!(
            Amount(1_000_001).to_value(),
            Err(AdapterError::IntegerRange)
        );
        assert_eq!(Signed(-1_001).to_value(), Err(AdapterError::IntegerRange));
        assert_eq!(
            Blob(vec![0; 33].into_boxed_slice()).to_value(),
            Err(AdapterError::Length)
        );
        assert_eq!(Label("".into()).to_value(), Err(AdapterError::Length));
        assert_eq!(
            Label("é".into()).to_value(),
            Err(AdapterError::NonAsciiText)
        );
        let labels = (0..5)
            .map(|_| Label("a".into()))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        assert_eq!(Labels(labels).to_value(), Err(AdapterError::Length));
        let scores = (0..5)
            .map(|index| ScoresEntry {
                key: Amount(index),
                value: Amount(index),
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
        assert_eq!(Scores(scores).to_value(), Err(AdapterError::Length));
    }
}
