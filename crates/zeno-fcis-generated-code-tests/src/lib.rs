//! Compiled generated Rust test code for `zeno-fcis-codegen`.
//!
//! The `generated` module is produced at build time by `build.rs` from the
//! shared test schema and is included verbatim. It exposes typed domain
//! adapters, strict `to_value`/`try_from_value` conversions, typed patch-path
//! constructors, and the codec vector evidence table.

#![forbid(unsafe_code)]
#![cfg_attr(not(feature = "std"), no_std)]
#![allow(missing_docs, clippy::all, clippy::pedantic)]

pub mod generated {
    #![allow(dead_code, unused_imports, missing_docs, clippy::all, clippy::pedantic)]
    include!(concat!(env!("OUT_DIR"), "/codegen_fixture.rs"));
    #[cfg(test)]
    mod binding_tests {
        use super::*;
        #[test]
        fn generated_root_envelope_rejects_wrong_schema_binding() {
            assert_eq!(
                BalanceState::validate_schema_hash(Hash32::new([0xa5; 32])),
                Err(AdapterError::SchemaHashMismatch)
            );
        }
    }
}

/// Compiled catalog-aware helpers emitted by `zeno-fcis-bootstrap`.
pub mod bootstrap_project {
    #![allow(dead_code, unused_imports, missing_docs, clippy::all, clippy::pedantic)]
    include!(concat!(env!("OUT_DIR"), "/bootstrap_project.rs"));
    #[cfg(test)]
    mod binding_tests {
        use super::*;
        #[test]
        fn generated_transition_rejects_wrong_root_type_after_schema_binding() {
            assert!(matches!(
                GeneratedProject::validate_root_binding(SCHEMA_HASH, TypeId::new(77)),
                Err(GeneratedProjectError::RootTypeMismatch {
                    expected: 12,
                    actual: 77
                })
            ));
            assert!(matches!(
                GeneratedProject::validate_root_binding(Hash32::ZERO, TypeId::new(77)),
                Err(GeneratedProjectError::SchemaHashMismatch { .. })
            ));
        }
    }
}

/// Compiled runtime skeleton emitted by `zeno-fcis-bootstrap`.
pub mod bootstrap_runtime {
    #![allow(dead_code, unused_imports, missing_docs, clippy::all, clippy::pedantic)]
    include!(concat!(env!("OUT_DIR"), "/bootstrap_runtime.rs"));
}

pub use zeno_fcis_codegen::{fixture_schema, fixture_spec};

#[cfg(test)]
#[path = "../test_catalog.rs"]
mod test_catalog;

#[cfg(test)]
mod tests {
    use super::generated::*;
    use super::generated::{VectorExpect, VectorKind};
    use zeno_fcis_codec::{DecodeLimits, Domain, Hash32, commitment, decode_value};
    use zeno_fcis_crypto::RustCryptoSha256;
    use zeno_fcis_schema::{
        Schema, SchemaEnvelopeError, TypeId, ValidationLimits, ValueValidationError,
    };

    use crate::bootstrap_project::{
        GeneratedCommandEnvelope, GeneratedContextEnvelope, GeneratedProject, GeneratedProjectError,
    };
    use crate::test_catalog::test_catalog;

    use zeno_fcis_codec::LibcruxSha256 as WrongHash;

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

    fn generated_project() -> GeneratedProject {
        GeneratedProject::try_new::<RustCryptoSha256>()
            .unwrap_or_else(|error| panic!("generated project rejected: {error}"))
    }

    fn admitted_command(project: &GeneratedProject) -> GeneratedCommandEnvelope {
        project
            .admit_command::<RustCryptoSha256>(&Event::Stop, ValidationLimits::default())
            .unwrap_or_else(|error| panic!("generated command rejected: {error}"))
    }

    fn admitted_context(project: &GeneratedProject) -> GeneratedContextEnvelope {
        project
            .admit_context::<RustCryptoSha256>(&minimal_state(), ValidationLimits::default())
            .unwrap_or_else(|error| panic!("generated context rejected: {error}"))
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
    fn typed_value_failure_precedes_schema_admission() {
        let mut state = minimal_state();
        state.amount = Amount(1_000_001);
        assert_eq!(
            state.to_root_envelope::<RustCryptoSha256>(ValidationLimits {
                max_depth: 0,
                max_nodes: 0
            }),
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
        let bad = zeno_fcis_value::Value::enumeration(TYPE_TAG, 999);
        let result = Tag::try_from_value(bad);
        assert!(result.is_err());
    }

    #[test]
    fn integer_range_enforced_by_adapter() {
        let over_max = zeno_fcis_value::Value::unsigned(1_000_001);
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
    fn generated_project_reconstructs_exact_catalog_profile_and_schema() {
        let project = generated_project();
        let expected = test_catalog(schema());
        assert_eq!(project.catalog(), &expected);
        assert_eq!(
            project
                .catalog()
                .commitment::<RustCryptoSha256>()
                .unwrap_or_else(|error| panic!("catalog commitment failed: {error}")),
            crate::bootstrap_project::CATALOG_HASH
        );
        assert_eq!(
            project.catalog().profile_hash(),
            crate::bootstrap_project::PROFILE_HASH
        );
        assert_eq!(
            project.catalog().schema_hash(),
            crate::bootstrap_project::SCHEMA_HASH
        );
    }

    #[test]
    fn generated_project_rejects_wrong_commitment_provider() {
        assert_eq!(
            GeneratedProject::try_new::<WrongHash>(),
            Err(GeneratedProjectError::HashAlgorithmMismatch)
        );
    }

    #[test]
    fn generated_inputs_bind_exact_types_schema_domains_and_commitments() {
        let project = generated_project();
        let command = admitted_command(&project);
        let context = admitted_context(&project);
        assert_eq!(command.admitted().type_id(), TypeId::new(9));
        assert_eq!(context.admitted().type_id(), TypeId::new(12));
        assert_eq!(
            command.admitted().schema_hash(),
            project.catalog().schema_hash()
        );
        assert_eq!(
            context.admitted().schema_hash(),
            project.catalog().schema_hash()
        );
        assert_eq!(command.validation_report().nodes, 1);
        assert_eq!(context.validation_report().nodes, 14);

        let command_bytes = command
            .admitted()
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("command encoding failed: {error}"));
        let command_domain = Domain::new(
            crate::bootstrap_project::COMMAND_DOMAIN,
            crate::bootstrap_project::INPUT_COMMITMENT_FORMAT_VERSION,
        )
        .unwrap_or_else(|error| panic!("command domain failed: {error}"));
        assert_eq!(
            command.commitment(),
            commitment::<RustCryptoSha256>(command_domain, &command_bytes)
                .unwrap_or_else(|error| panic!("command commitment failed: {error}"))
        );

        let context_bytes = context
            .admitted()
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("context encoding failed: {error}"));
        let context_domain = Domain::new(
            crate::bootstrap_project::CONTEXT_DOMAIN,
            crate::bootstrap_project::INPUT_COMMITMENT_FORMAT_VERSION,
        )
        .unwrap_or_else(|error| panic!("context domain failed: {error}"));
        assert_eq!(
            context.commitment(),
            commitment::<RustCryptoSha256>(context_domain, &context_bytes)
                .unwrap_or_else(|error| panic!("context commitment failed: {error}"))
        );
        let command_under_context_domain =
            commitment::<RustCryptoSha256>(context_domain, &command_bytes)
                .unwrap_or_else(|error| panic!("role-separated commitment failed: {error}"));
        assert_ne!(command.commitment(), command_under_context_domain);
        assert_ne!(command.commitment(), Hash32::ZERO);
        assert_ne!(context.commitment(), Hash32::ZERO);
    }

    #[test]
    fn generated_input_commitments_are_deterministic_and_value_sensitive() {
        let project = generated_project();
        let first = admitted_command(&project);
        let second = admitted_command(&project);
        let changed = project
            .admit_command::<RustCryptoSha256>(&Event::Move(Amount(1)), ValidationLimits::default())
            .unwrap_or_else(|error| panic!("changed command rejected: {error}"));
        assert_eq!(first, second);
        assert_ne!(first.commitment(), changed.commitment());

        let false_context = admitted_context(&project);
        let true_context = project
            .admit_context::<RustCryptoSha256>(
                &BalanceState {
                    flag: Flag(true),
                    ..minimal_state()
                },
                ValidationLimits::default(),
            )
            .unwrap_or_else(|error| panic!("changed context rejected: {error}"));
        assert_ne!(false_context.commitment(), true_context.commitment());
    }

    #[test]
    fn generated_input_admission_enforces_provider_and_validation_bounds() {
        let project = generated_project();
        let invalid = Event::Move(Amount(1_000_001));
        assert_eq!(
            project.admit_command::<WrongHash>(&invalid, ValidationLimits::default()),
            Err(GeneratedProjectError::HashAlgorithmMismatch)
        );
        assert_eq!(
            project.admit_command::<RustCryptoSha256>(&invalid, ValidationLimits::default()),
            Err(GeneratedProjectError::Adapter(AdapterError::IntegerRange))
        );
        assert_eq!(
            project.admit_context::<RustCryptoSha256>(
                &minimal_state(),
                ValidationLimits {
                    max_depth: 0,
                    max_nodes: 0,
                },
            ),
            Err(GeneratedProjectError::Envelope(
                SchemaEnvelopeError::SchemaValidation(ValueValidationError::BudgetExceeded)
            ))
        );
    }

    #[test]
    fn generated_root_admission_checks_provider_before_typed_conversion() {
        let project = generated_project();
        let mut invalid = minimal_state();
        invalid.amount = Amount(1_000_001);
        assert_eq!(
            project.admit_root::<WrongHash>(&invalid, ValidationLimits::default()),
            Err(GeneratedProjectError::HashAlgorithmMismatch)
        );
        assert_eq!(
            project.admit_root::<RustCryptoSha256>(&invalid, ValidationLimits::default()),
            Err(GeneratedProjectError::Adapter(AdapterError::IntegerRange))
        );
    }
}
