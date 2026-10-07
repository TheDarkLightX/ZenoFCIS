//! Original generated fixture native callbacks, retained solely for private regressions.
#![allow(dead_code, unused_imports, missing_docs, clippy::all, clippy::pedantic)]

pub(crate) mod generated {
    #![allow(dead_code, unused_imports, missing_docs, clippy::all, clippy::pedantic)]
    include!("generated/schema.rs");
}
pub(crate) mod bootstrap_project {
    #![allow(dead_code, unused_imports, missing_docs, clippy::all, clippy::pedantic)]
    use crate::oracle::core as zeno_fcis_core;
    use crate::oracle::transition as zeno_fcis_transition;
    include!("generated/project.rs");
}
pub(crate) mod bootstrap_runtime {
    include!("generated/runtime.rs");
}
#[path = "test_catalog.rs"]
pub(crate) mod test_catalog;

mod tests {
    use crate::oracle::core as zeno_fcis_core;
    use crate::oracle::transition as zeno_fcis_transition;
    use super::generated::*;
    use super::generated::{VectorExpect, VectorKind};
    use zeno_fcis_catalog::NonZeroHash;
    use zeno_fcis_codec::{
        CommitmentHasher, DecodeLimits, Domain, Hash32, commitment, decode_value,
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
    fn generated_transition_preserves_complete_invocation_bindings() {
        let project = generated_project();
        let state = project
            .admit_root::<RustCryptoSha256>(&minimal_state(), ValidationLimits::default())
            .unwrap_or_else(|error| panic!("generated binding regression: {error:?}"));
        let command = admitted_command(&project);
        let context = admitted_context(&project);
        let expected = ExpectedInvocationBindings::try_new(
            command.commitment(),
            RustCryptoSha256::hash(b"complete authenticated invocation"),
        )
        .unwrap_or_else(|error| panic!("generated binding regression: {error:?}"));
        let decision = project
            .begin_bound_transition::<RustCryptoSha256>(
                &state,
                state_domain(),
                &command,
                &context,
                expected,
                BudgetUsed::default(),
                TransitionLimits::default(),
            )
            .unwrap_or_else(|error| panic!("generated binding regression: {error:?}"))
            .seal()
            .unwrap_or_else(|error| panic!("generated binding regression: {error:?}"));
        assert!(
            validate_transition_decision::<RustCryptoSha256>(
                &decision,
                project.catalog(),
                expected,
                state.value().value(),
                state_domain(),
            )
            .is_ok()
        );
        let wrong_command = ExpectedInvocationBindings::try_new(
            RustCryptoSha256::hash(b"another command"),
            expected.context_hash(),
        )
        .unwrap_or_else(|error| panic!("generated binding regression: {error:?}"));
        assert!(
            validate_transition_decision::<RustCryptoSha256>(
                &decision,
                project.catalog(),
                expected_invocation(&command, &context),
                state.value().value(),
                state_domain(),
            )
            .is_err(),
            "the context value alone cannot replace the complete invocation binding"
        );
        assert!(
            project
                .begin_bound_transition::<RustCryptoSha256>(
                    &state,
                    state_domain(),
                    &command,
                    &context,
                    wrong_command,
                    BudgetUsed::default(),
                    TransitionLimits::default(),
                )
                .is_err()
        );
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
        assert_eq!(effect.payload(), &zeno_fcis_value::Value::unsigned(9));
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
        assert_eq!(effects[0].payload(), &zeno_fcis_value::Value::unsigned(9));
        let entries = artifacts.candidate().bundle().outbox_plan().entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].ordinal(), 8);
        assert_eq!(entries[0].channel(), 30);
        assert_eq!(
            entries[0].destination(),
            &zeno_fcis_value::Value::text_ascii(String::from("destination")).unwrap_or_else(|error| panic!("value fixture: {error}"))
        );
        assert_eq!(
            entries[0].payload(),
            &zeno_fcis_value::Value::sum(9, 1, None)
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
                assert_eq!(value, &zeno_fcis_value::Value::unsigned(9));
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
    fn generated_transition_rejects_wrong_schema_envelope() {
        let limits = SchemaLimits::default();
        let definition = TypeDef::try_new(TypeId::new(77), "OtherRoot", TypeKind::Unit, limits)
            .unwrap_or_else(|error| panic!("wrong-root type: {error}"));
        let changed_schema = Schema::try_new("WrongRootSchema", 1, TypeId::new(77), vec![definition], limits)
            .unwrap_or_else(|error| panic!("wrong-root schema: {error}"));
        let wrong = SchemaAdmittedEnvelope::try_new::<RustCryptoSha256>(
            &changed_schema, zeno_fcis_value::Value::unit(), ValidationLimits::default(),
        ).unwrap_or_else(|error| panic!("changed-schema envelope: {error}"));
        assert_ne!(wrong.schema_hash(), crate::bootstrap_project::SCHEMA_HASH);
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
