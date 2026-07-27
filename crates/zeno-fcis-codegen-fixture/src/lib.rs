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

/// Compiled catalog-aware helpers emitted by `zeno-fcis-bootstrap`.
pub mod bootstrap_project {
    #![allow(dead_code, unused_imports, missing_docs, clippy::all, clippy::pedantic)]
    include!(concat!(env!("OUT_DIR"), "/bootstrap_project.rs"));
}

/// Compiled runtime skeleton emitted by `zeno-fcis-bootstrap`.
pub mod bootstrap_runtime {
    #![allow(dead_code, unused_imports, missing_docs, clippy::all, clippy::pedantic)]
    include!(concat!(env!("OUT_DIR"), "/bootstrap_runtime.rs"));
}

pub use zeno_fcis_codegen::{fixture_schema, fixture_spec};

#[cfg(test)]
mod tests {
    use super::generated::*;
    use super::generated::{VectorExpect, VectorKind};
    use zeno_fcis_codec::{CommitmentHasher, DecodeLimits, Hash32, decode_value};
    use zeno_fcis_crypto::RustCryptoSha256;
    use zeno_fcis_schema::{
        Schema, SchemaEnvelopeError, TypeId, ValidationLimits, ValueValidationError,
    };

    struct WrongHash;

    impl CommitmentHasher for WrongHash {
        const ALGORITHM_ID: &'static str = "test/wrong-hash";

        fn hash(_: &[u8]) -> Hash32 {
            Hash32::new([0xa5; 32])
        }
    }

    fn schema() -> Schema {
        zeno_fcis_codegen::fixture_schema().unwrap_or_else(|e| panic!("fixture schema failed: {e}"))
    }

    fn minimal_state() -> BalanceState {
        BalanceState {
            amount: Amount(0),
            signed: Signed(-1000),
            label: Label("a".into()),
            blob: Blob(vec![].into_boxed_slice()),
            flag: Flag(false),
            nil: Nil,
            tag: Tag::Idle,
            point: Point {
                field_0: Amount(0),
                field_1: Tag::Idle,
            },
            event: Event::Stop,
            labels: Labels(vec![].into_boxed_slice()),
            scores: Scores(vec![].into_boxed_slice()),
        }
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
        let state = minimal_state();
        let value = state
            .to_value()
            .unwrap_or_else(|e| panic!("to_value failed: {e:?}"));
        let round_tripped = BalanceState::try_from_value(value)
            .unwrap_or_else(|e| panic!("try_from_value failed: {e:?}"));
        assert_eq!(state, round_tripped);
    }

    #[test]
    fn generated_root_reconstructs_the_exact_source_schema() {
        let generated = BalanceState::zfcis_schema()
            .unwrap_or_else(|error| panic!("generated schema failed: {error}"));
        let source = schema();
        assert_eq!(generated, source);
        let generated_hash = generated
            .schema_hash::<RustCryptoSha256>()
            .unwrap_or_else(|error| panic!("generated schema hash failed: {error}"));
        assert_eq!(format!("{generated_hash}"), SCHEMA_HASH_HEX);
    }

    #[test]
    fn generated_root_envelope_binds_schema_type_and_metrics() {
        let state = minimal_state();
        let expected = state
            .to_value()
            .unwrap_or_else(|error| panic!("root conversion failed: {error:?}"));
        let envelope = state
            .to_root_envelope::<RustCryptoSha256>(ValidationLimits::default())
            .unwrap_or_else(|error| panic!("root envelope failed: {error:?}"));
        assert_eq!(envelope.root_type(), TypeId::new(ROOT_TYPE_ID));
        assert_eq!(format!("{}", envelope.schema_hash()), SCHEMA_HASH_HEX);
        assert_eq!(envelope.value().value(), &expected);
        assert_eq!(envelope.validation_report().nodes, 14);
        assert_eq!(envelope.validation_report().maximum_depth, 2);
    }

    #[test]
    fn generated_root_envelope_rejects_wrong_hash_provider() {
        assert_eq!(
            minimal_state().to_root_envelope::<WrongHash>(ValidationLimits::default()),
            Err(AdapterError::SchemaHashMismatch)
        );
    }

    #[test]
    fn generated_root_envelope_enforces_caller_validation_budget() {
        assert_eq!(
            minimal_state().to_root_envelope::<RustCryptoSha256>(ValidationLimits {
                max_depth: 0,
                max_nodes: 0,
            }),
            Err(AdapterError::SchemaEnvelope(
                SchemaEnvelopeError::SchemaValidation(ValueValidationError::BudgetExceeded)
            ))
        );
    }

    #[test]
    fn typed_value_failure_precedes_provider_mismatch() {
        let mut state = minimal_state();
        state.amount = Amount(1_000_001);
        assert_eq!(
            state.to_root_envelope::<WrongHash>(ValidationLimits::default()),
            Err(AdapterError::IntegerRange)
        );
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

    #[test]
    fn bootstrap_effect_helper_uses_catalogued_operation_and_payload() {
        let effect = crate::bootstrap_project::effect_20(
            7,
            zeno_fcis_codec::Hash32::new([1; 32]),
            zeno_fcis_codec::Hash32::ZERO,
            &Amount(9),
        )
        .unwrap_or_else(|error| panic!("bootstrap effect helper: {error:?}"));
        assert_eq!(effect.ordinal(), 7);
        assert_eq!(effect.operation(), 20);
        assert_eq!(effect.payload(), &zeno_fcis_value::Value::U128(9));
    }
}
