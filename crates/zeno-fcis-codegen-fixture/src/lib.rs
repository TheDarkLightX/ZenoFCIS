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
#[path = "../catalog_fixture.rs"]
mod catalog_fixture;

#[cfg(test)]
mod tests {
    use super::generated::*;
    use super::generated::{VectorExpect, VectorKind};
    use zeno_fcis_catalog::NonZeroHash;
    use zeno_fcis_codec::{
        CanonicalEncode, CommitmentHasher, DecodeLimits, Domain, Hash32, commitment, decode_value,
    };
    use zeno_fcis_compose::PathAtom;
    use zeno_fcis_core::{BudgetUsed, Decision};
    use zeno_fcis_crypto::RustCryptoSha256;
    use zeno_fcis_patch::{PatchOp, PathSegment};
    use zeno_fcis_schema::{
        Schema, SchemaAdmittedEnvelope, SchemaEnvelopeError, SchemaLimits, TypeDef, TypeId,
        TypeKind, ValidationLimits, ValueValidationError,
    };
    use zeno_fcis_transition::{
        ExpectedInvocationBindings, TransitionLimits, validate_transition_decision,
    };

    use crate::bootstrap_project::{
        GeneratedCommandEnvelope, GeneratedContextEnvelope, GeneratedProject,
        GeneratedProjectError, ReasonClass, ReasonId, RejectReasonId,
    };
    use crate::catalog_fixture::fixture_catalog;

    struct WrongHash;

    impl CommitmentHasher for WrongHash {
        const ALGORITHM_ID: &'static str = "test/wrong-hash";

        fn hash(_: &[u8]) -> Hash32 {
            Hash32::new([0xa5; 32])
        }
    }

    struct ExpectedSchemaHash;

    impl CommitmentHasher for ExpectedSchemaHash {
        const ALGORITHM_ID: &'static str = "test/forced-generated-schema-hash";

        fn hash(_: &[u8]) -> Hash32 {
            crate::bootstrap_project::SCHEMA_HASH
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

    fn state_domain() -> Domain<'static> {
        Domain::new("codegen-fixture/state", 1)
            .unwrap_or_else(|error| panic!("state domain rejected: {error}"))
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

    fn expected_invocation(
        command: &GeneratedCommandEnvelope,
        context: &GeneratedContextEnvelope,
    ) -> ExpectedInvocationBindings {
        ExpectedInvocationBindings::try_new(command.commitment(), context.commitment())
            .unwrap_or_else(|error| panic!("expected invocation rejected: {error}"))
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

    #[test]
    fn generated_typed_effect_and_channel_stage_exact_catalog_values() {
        let project = generated_project();
        let envelope = project
            .admit_root::<RustCryptoSha256>(&minimal_state(), ValidationLimits::default())
            .unwrap_or_else(|error| panic!("catalog root admission failed: {error}"));
        let command = admitted_command(&project);
        let context = admitted_context(&project);
        let authority = NonZeroHash::try_new(Hash32::new([1; 32]))
            .unwrap_or_else(|error| panic!("nonzero authority rejected: {error}"));
        let mut transition = project
            .begin_transition::<RustCryptoSha256>(
                &envelope,
                state_domain(),
                &command,
                &context,
                BudgetUsed::default(),
                TransitionLimits::default(),
            )
            .unwrap_or_else(|error| panic!("transition start failed: {error}"));
        transition
            .emit_effect_20(7, authority, &Amount(9))
            .unwrap_or_else(|error| panic!("typed effect failed: {error}"))
            .enqueue_channel_30(8, &Label("destination".into()), &Event::Stop)
            .unwrap_or_else(|error| panic!("typed channel failed: {error}"));
        let decision = transition
            .seal()
            .unwrap_or_else(|error| panic!("transition seal failed: {error}"));
        let artifacts = match &decision {
            Decision::Accept(artifacts) => artifacts,
            other => panic!("expected accept, got {other:?}"),
        };
        let effects = artifacts.candidate().bundle().commit_plan().effects();
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0].ordinal(), 7);
        assert_eq!(effects[0].operation(), 20);
        assert_eq!(effects[0].authority(), authority.get());
        assert_eq!(effects[0].subject(), Hash32::ZERO);
        assert_eq!(effects[0].payload(), &zeno_fcis_value::Value::U128(9));
        let entries = artifacts.candidate().bundle().outbox_plan().entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].ordinal(), 8);
        assert_eq!(entries[0].channel(), 30);
        assert_eq!(
            entries[0].destination(),
            &zeno_fcis_value::Value::Text("destination".into())
        );
        assert_eq!(
            entries[0].payload(),
            &zeno_fcis_value::Value::Sum {
                type_id: 9,
                variant: 1,
                payload: None
            }
        );
        validate_transition_decision::<RustCryptoSha256>(
            &decision,
            project.catalog(),
            expected_invocation(&command, &context),
            envelope.value().value(),
            state_domain(),
        )
        .unwrap_or_else(|error| panic!("transition decision invalid: {error}"));
    }

    #[test]
    fn generated_typed_staging_rejects_invalid_values_without_staging() {
        assert!(NonZeroHash::try_new(Hash32::ZERO).is_err());
        let project = generated_project();
        let envelope = project
            .admit_root::<RustCryptoSha256>(&minimal_state(), ValidationLimits::default())
            .unwrap_or_else(|error| panic!("catalog root admission failed: {error}"));
        let command = admitted_command(&project);
        let context = admitted_context(&project);
        let authority = NonZeroHash::try_new(Hash32::new([1; 32]))
            .unwrap_or_else(|error| panic!("nonzero authority rejected: {error}"));
        let mut transition = project
            .begin_transition::<RustCryptoSha256>(
                &envelope,
                state_domain(),
                &command,
                &context,
                BudgetUsed::default(),
                TransitionLimits::default(),
            )
            .unwrap_or_else(|error| panic!("transition start failed: {error}"));
        assert!(matches!(
            transition.emit_effect_20(1, authority, &Amount(1_000_001)),
            Err(GeneratedProjectError::Adapter(AdapterError::IntegerRange))
        ));
        assert!(matches!(
            transition.enqueue_channel_30(2, &Label("".into()), &Event::Stop),
            Err(GeneratedProjectError::Adapter(AdapterError::Length))
        ));
        let decision = transition
            .seal()
            .unwrap_or_else(|error| panic!("transition seal failed: {error}"));
        let artifacts = match decision {
            Decision::Accept(artifacts) => artifacts,
            other => panic!("expected accept, got {other:?}"),
        };
        assert!(
            artifacts
                .candidate()
                .bundle()
                .commit_plan()
                .effects()
                .is_empty()
        );
        assert!(
            artifacts
                .candidate()
                .bundle()
                .outbox_plan()
                .entries()
                .is_empty()
        );
    }

    #[test]
    fn generated_typed_root_update_uses_exact_field_path_and_value() {
        let project = generated_project();
        let envelope = project
            .admit_root::<RustCryptoSha256>(&minimal_state(), ValidationLimits::default())
            .unwrap_or_else(|error| panic!("catalog root admission failed: {error}"));
        let command = admitted_command(&project);
        let context = admitted_context(&project);
        let mut transition = project
            .begin_transition::<RustCryptoSha256>(
                &envelope,
                state_domain(),
                &command,
                &context,
                BudgetUsed::default(),
                TransitionLimits::default(),
            )
            .unwrap_or_else(|error| panic!("transition start failed: {error}"));
        transition
            .update_amount(&Amount(9))
            .unwrap_or_else(|error| panic!("typed root update failed: {error}"));
        let decision = transition
            .seal()
            .unwrap_or_else(|error| panic!("transition seal failed: {error}"));
        let artifacts = match &decision {
            Decision::Accept(artifacts) => artifacts,
            other => panic!("expected accept, got {other:?}"),
        };
        let patch = artifacts.candidate().bundle().patch();
        assert_eq!(patch.operations().len(), 1);
        match &patch.operations()[0] {
            PatchOp::Update { path, value, .. } => {
                assert_eq!(path.segments(), &[PathSegment::Field(1)]);
                assert_eq!(value, &zeno_fcis_value::Value::U128(9));
            }
            other => panic!("expected root-field update, got {other:?}"),
        }
        let applied = artifacts
            .candidate()
            .bundle()
            .validate_and_apply::<RustCryptoSha256>(envelope.value().value(), state_domain())
            .unwrap_or_else(|error| panic!("updated bundle application failed: {error}"));
        assert_eq!(
            applied.state(),
            &BalanceState {
                amount: Amount(9),
                ..minimal_state()
            }
            .to_value()
            .unwrap_or_else(|error| panic!("updated state conversion failed: {error:?}"))
        );
        validate_transition_decision::<RustCryptoSha256>(
            &decision,
            project.catalog(),
            expected_invocation(&command, &context),
            envelope.value().value(),
            state_domain(),
        )
        .unwrap_or_else(|error| panic!("transition decision invalid: {error}"));
    }

    #[test]
    fn generated_typed_root_read_returns_exact_type_and_records_only_read_footprint() {
        let project = generated_project();
        let state = BalanceState {
            amount: Amount(9),
            ..minimal_state()
        };
        let envelope = project
            .admit_root::<RustCryptoSha256>(&state, ValidationLimits::default())
            .unwrap_or_else(|error| panic!("catalog root admission failed: {error}"));
        let command = admitted_command(&project);
        let context = admitted_context(&project);
        let mut transition = project
            .begin_transition::<RustCryptoSha256>(
                &envelope,
                state_domain(),
                &command,
                &context,
                BudgetUsed::default(),
                TransitionLimits::default(),
            )
            .unwrap_or_else(|error| panic!("transition start failed: {error}"));
        assert_eq!(
            transition
                .read_amount()
                .unwrap_or_else(|error| panic!("typed root read failed: {error}")),
            Amount(9)
        );
        let decision = transition
            .seal()
            .unwrap_or_else(|error| panic!("transition seal failed: {error}"));
        let artifacts = match &decision {
            Decision::Accept(artifacts) => artifacts,
            other => panic!("expected accept, got {other:?}"),
        };
        assert!(
            artifacts
                .candidate()
                .bundle()
                .patch()
                .operations()
                .is_empty()
        );
        assert_eq!(artifacts.candidate().footprint().reads().paths().len(), 1);
        let observed = &artifacts.candidate().footprint().reads().paths()[0];
        assert_eq!(
            observed.namespace(),
            crate::bootstrap_project::STATE_TYPE_ID
        );
        assert_eq!(observed.atoms(), &[PathAtom::Field(1)]);
        assert!(
            artifacts
                .candidate()
                .footprint()
                .writes()
                .paths()
                .is_empty()
        );
        validate_transition_decision::<RustCryptoSha256>(
            &decision,
            project.catalog(),
            expected_invocation(&command, &context),
            envelope.value().value(),
            state_domain(),
        )
        .unwrap_or_else(|error| panic!("transition decision invalid: {error}"));
    }

    #[test]
    fn generated_typed_context_field_observation_records_only_exact_context_footprint() {
        let project = generated_project();
        let envelope = project
            .admit_root::<RustCryptoSha256>(&minimal_state(), ValidationLimits::default())
            .unwrap_or_else(|error| panic!("catalog root admission failed: {error}"));
        let command = admitted_command(&project);
        let context = admitted_context(&project);
        let mut transition = project
            .begin_transition::<RustCryptoSha256>(
                &envelope,
                state_domain(),
                &command,
                &context,
                BudgetUsed::default(),
                TransitionLimits::default(),
            )
            .unwrap_or_else(|error| panic!("transition start failed: {error}"));
        transition
            .observe_context_flag()
            .unwrap_or_else(|error| panic!("typed context observation failed: {error}"));
        let decision = transition
            .seal()
            .unwrap_or_else(|error| panic!("transition seal failed: {error}"));
        let artifacts = match &decision {
            Decision::Accept(artifacts) => artifacts,
            other => panic!("expected accept, got {other:?}"),
        };
        let candidate = artifacts.candidate();
        assert!(candidate.bundle().patch().operations().is_empty());
        assert!(candidate.footprint().reads().paths().is_empty());
        assert!(candidate.footprint().writes().paths().is_empty());
        assert_eq!(candidate.footprint().contexts().paths().len(), 1);
        let observed = &candidate.footprint().contexts().paths()[0];
        assert_eq!(
            observed.namespace(),
            crate::bootstrap_project::CONTEXT_TYPE_ID
        );
        assert_eq!(observed.atoms(), &[PathAtom::Field(5)]);
        validate_transition_decision::<RustCryptoSha256>(
            &decision,
            project.catalog(),
            expected_invocation(&command, &context),
            envelope.value().value(),
            state_domain(),
        )
        .unwrap_or_else(|error| panic!("transition decision invalid: {error}"));
    }

    #[test]
    fn generated_typed_root_update_rejects_invalid_value_without_staging() {
        let project = generated_project();
        let envelope = project
            .admit_root::<RustCryptoSha256>(&minimal_state(), ValidationLimits::default())
            .unwrap_or_else(|error| panic!("catalog root admission failed: {error}"));
        let command = admitted_command(&project);
        let context = admitted_context(&project);
        let mut transition = project
            .begin_transition::<RustCryptoSha256>(
                &envelope,
                state_domain(),
                &command,
                &context,
                BudgetUsed::default(),
                TransitionLimits::default(),
            )
            .unwrap_or_else(|error| panic!("transition start failed: {error}"));
        assert!(matches!(
            transition.update_amount(&Amount(1_000_001)),
            Err(GeneratedProjectError::Adapter(AdapterError::IntegerRange))
        ));
        let decision = transition
            .seal()
            .unwrap_or_else(|error| panic!("transition seal failed: {error}"));
        let artifacts = match decision {
            Decision::Accept(artifacts) => artifacts,
            other => panic!("expected accept, got {other:?}"),
        };
        assert!(
            artifacts
                .candidate()
                .bundle()
                .patch()
                .operations()
                .is_empty()
        );
        let applied = artifacts
            .candidate()
            .bundle()
            .validate_and_apply::<RustCryptoSha256>(envelope.value().value(), state_domain())
            .unwrap_or_else(|error| panic!("unchanged bundle application failed: {error}"));
        assert_eq!(applied.state(), envelope.value().value());
    }

    #[test]
    fn generated_project_reconstructs_exact_catalog_profile_and_schema() {
        let project = generated_project();
        let expected = fixture_catalog(schema());
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
    fn generated_project_starts_and_seals_accept_from_root_envelope() {
        let project = generated_project();
        let envelope = project
            .admit_root::<RustCryptoSha256>(&minimal_state(), ValidationLimits::default())
            .unwrap_or_else(|error| panic!("catalog root admission failed: {error}"));
        let command = admitted_command(&project);
        let context = admitted_context(&project);
        let decision = project
            .begin_transition::<RustCryptoSha256>(
                &envelope,
                state_domain(),
                &command,
                &context,
                BudgetUsed::default(),
                TransitionLimits::default(),
            )
            .unwrap_or_else(|error| panic!("transition start failed: {error}"))
            .seal()
            .unwrap_or_else(|error| panic!("transition seal failed: {error}"));
        let artifacts = match &decision {
            Decision::Accept(artifacts) => artifacts,
            other => panic!("expected accept, got {other:?}"),
        };
        let bindings = artifacts.candidate().bundle().body().bindings();
        assert_eq!(bindings.command_hash, command.commitment());
        assert_eq!(bindings.context_hash, context.commitment());
        validate_transition_decision::<RustCryptoSha256>(
            &decision,
            project.catalog(),
            expected_invocation(&command, &context),
            envelope.value().value(),
            state_domain(),
        )
        .unwrap_or_else(|error| panic!("transition decision invalid: {error}"));
    }

    #[test]
    fn generated_typed_rejection_uses_exact_catalog_reason() {
        let project = generated_project();
        let envelope = project
            .admit_root::<RustCryptoSha256>(&minimal_state(), ValidationLimits::default())
            .unwrap_or_else(|error| panic!("catalog root admission failed: {error}"));
        let command = admitted_command(&project);
        let context = admitted_context(&project);
        assert_eq!(RejectReasonId::Reason10.reason_id(), ReasonId::Reason10);
        assert_eq!(ReasonId::Reason10.class(), ReasonClass::Reject);

        let mut transition = project
            .begin_transition::<RustCryptoSha256>(
                &envelope,
                state_domain(),
                &command,
                &context,
                BudgetUsed::default(),
                TransitionLimits::default(),
            )
            .unwrap_or_else(|error| panic!("transition start failed: {error}"));
        transition
            .require(false, RejectReasonId::Reason10)
            .unwrap_or_else(|error| panic!("typed rejection failed: {error}"));
        let decision = transition
            .seal()
            .unwrap_or_else(|error| panic!("transition seal failed: {error}"));
        let rejected = match &decision {
            Decision::Reject(rejected) => rejected.reason(),
            other => panic!("expected reject, got {other:?}"),
        };
        assert_eq!(rejected.reason_id().get(), 10);
        validate_transition_decision::<RustCryptoSha256>(
            &decision,
            project.catalog(),
            expected_invocation(&command, &context),
            envelope.value().value(),
            state_domain(),
        )
        .unwrap_or_else(|error| panic!("transition decision invalid: {error}"));
    }

    #[test]
    fn generated_typed_reason_condition_and_duplicates_preserve_semantics() {
        let project = generated_project();
        let envelope = project
            .admit_root::<RustCryptoSha256>(&minimal_state(), ValidationLimits::default())
            .unwrap_or_else(|error| panic!("catalog root admission failed: {error}"));
        let command = admitted_command(&project);
        let context = admitted_context(&project);
        let mut accepted = project
            .begin_transition::<RustCryptoSha256>(
                &envelope,
                state_domain(),
                &command,
                &context,
                BudgetUsed::default(),
                TransitionLimits::default(),
            )
            .unwrap_or_else(|error| panic!("transition start failed: {error}"));
        accepted
            .require(true, RejectReasonId::Reason10)
            .unwrap_or_else(|error| panic!("typed requirement failed: {error}"));
        let accepted = accepted
            .seal()
            .unwrap_or_else(|error| panic!("transition seal failed: {error}"));
        assert!(matches!(accepted, Decision::Accept(_)));

        let mut once = project
            .begin_transition::<RustCryptoSha256>(
                &envelope,
                state_domain(),
                &command,
                &context,
                BudgetUsed::default(),
                TransitionLimits::default(),
            )
            .unwrap_or_else(|error| panic!("transition start failed: {error}"));
        once.require(false, RejectReasonId::Reason10)
            .unwrap_or_else(|error| panic!("typed rejection failed: {error}"));
        let once = once
            .seal()
            .unwrap_or_else(|error| panic!("transition seal failed: {error}"));

        let mut twice = project
            .begin_transition::<RustCryptoSha256>(
                &envelope,
                state_domain(),
                &command,
                &context,
                BudgetUsed::default(),
                TransitionLimits::default(),
            )
            .unwrap_or_else(|error| panic!("transition start failed: {error}"));
        twice
            .require(false, RejectReasonId::Reason10)
            .unwrap_or_else(|error| panic!("first typed rejection failed: {error}"))
            .require(false, RejectReasonId::Reason10)
            .unwrap_or_else(|error| panic!("second typed rejection failed: {error}"));
        let twice = twice
            .seal()
            .unwrap_or_else(|error| panic!("transition seal failed: {error}"));
        assert_eq!(once, twice);
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

    #[test]
    fn generated_transition_rejects_wrong_schema_envelope() {
        let value = minimal_state()
            .to_value()
            .unwrap_or_else(|error| panic!("root conversion failed: {error:?}"));
        let wrong = SchemaAdmittedEnvelope::try_new::<WrongHash>(
            &schema(),
            value,
            ValidationLimits::default(),
        )
        .unwrap_or_else(|error| panic!("wrong-provider envelope failed: {error}"));
        let project = generated_project();
        let command = admitted_command(&project);
        let context = admitted_context(&project);
        let result = project.begin_transition::<RustCryptoSha256>(
            &wrong,
            state_domain(),
            &command,
            &context,
            BudgetUsed::default(),
            TransitionLimits::default(),
        );
        assert!(matches!(
            result,
            Err(GeneratedProjectError::SchemaHashMismatch { .. })
        ));
    }

    #[test]
    fn generated_transition_rejects_wrong_root_type_after_schema_binding() {
        let limits = SchemaLimits::default();
        let definition = TypeDef::try_new(TypeId::new(77), "OtherRoot", TypeKind::Unit, limits)
            .unwrap_or_else(|error| panic!("wrong-root type rejected: {error}"));
        let schema = Schema::try_new(
            "WrongRootSchema",
            1,
            TypeId::new(77),
            vec![definition],
            limits,
        )
        .unwrap_or_else(|error| panic!("wrong-root schema rejected: {error}"));
        let wrong = SchemaAdmittedEnvelope::try_new::<ExpectedSchemaHash>(
            &schema,
            zeno_fcis_value::Value::Unit,
            ValidationLimits::default(),
        )
        .unwrap_or_else(|error| panic!("wrong-root envelope failed: {error}"));
        let project = generated_project();
        let command = admitted_command(&project);
        let context = admitted_context(&project);
        let result = project.begin_transition::<RustCryptoSha256>(
            &wrong,
            state_domain(),
            &command,
            &context,
            BudgetUsed::default(),
            TransitionLimits::default(),
        );
        assert!(matches!(
            result,
            Err(GeneratedProjectError::RootTypeMismatch {
                expected: 12,
                actual: 77
            })
        ));
    }

    #[test]
    fn generated_binding_failures_precede_transition_input_failures() {
        let envelope = minimal_state()
            .to_root_envelope::<RustCryptoSha256>(ValidationLimits::default())
            .unwrap_or_else(|error| panic!("root envelope failed: {error:?}"));
        let project = generated_project();
        let command = admitted_command(&project);
        let context = admitted_context(&project);
        let wrong_provider = project.begin_transition::<WrongHash>(
            &envelope,
            state_domain(),
            &command,
            &context,
            BudgetUsed::default(),
            TransitionLimits::default(),
        );
        assert!(matches!(
            wrong_provider,
            Err(GeneratedProjectError::HashAlgorithmMismatch)
        ));
    }
}
