//! Exact original compound declarations and named refusals, without a publication claim.
#![allow(missing_docs, clippy::all, clippy::pedantic)]
use zeno_fcis_catalog::NonZeroHash;
use zeno_fcis_codec::{CanonicalEncode, Hash32};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_generated_code_tests::{bootstrap_project as g, fixture_schema, generated::*};
use zeno_fcis_schema::ValidationLimits;
use zeno_fcis_synthesis::finite::{
    Domain, Op, V2InputField, V2InputLeaf, V2ScalarProgram, canonical_v2::schema as s,
    v2_catalog as k, v2_composition as c, v2_zero_limits,
};

fn checked<T, E: core::fmt::Debug>(result: Result<T, E>) -> T {
    result.unwrap_or_else(|error| panic!("original fixture: {error:?}"))
}
fn state() -> BalanceState {
    BalanceState {
        amount: Amount(9),
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
fn original_declarations_keep_every_field_identifier_and_unsigned_or_compound_domain() {
    let reads = [
        g::GeneratedDeclarations::read_amount(),
        g::GeneratedDeclarations::read_signed(),
        g::GeneratedDeclarations::read_label(),
        g::GeneratedDeclarations::read_blob(),
        g::GeneratedDeclarations::read_flag(),
        g::GeneratedDeclarations::read_nil(),
        g::GeneratedDeclarations::read_tag(),
        g::GeneratedDeclarations::read_point(),
        g::GeneratedDeclarations::read_event(),
        g::GeneratedDeclarations::read_labels(),
        g::GeneratedDeclarations::read_scores(),
    ];
    for (index, read) in reads.into_iter().enumerate() {
        assert!(
            matches!(checked(read), c::Expr::Input(c::Source::State, id) if id==index as u16+1)
        );
    }
    let contexts = [
        g::GeneratedDeclarations::context_amount_binding(),
        g::GeneratedDeclarations::context_signed_binding(),
        g::GeneratedDeclarations::context_label_binding(),
        g::GeneratedDeclarations::context_blob_binding(),
        g::GeneratedDeclarations::context_flag_binding(),
        g::GeneratedDeclarations::context_nil_binding(),
        g::GeneratedDeclarations::context_tag_binding(),
        g::GeneratedDeclarations::context_point_binding(),
        g::GeneratedDeclarations::context_event_binding(),
        g::GeneratedDeclarations::context_labels_binding(),
        g::GeneratedDeclarations::context_scores_binding(),
    ];
    for (index, binding) in contexts.into_iter().enumerate() {
        assert!(matches!(binding.source, c::Source::Context));
        assert!(matches!(binding.selector, c::Selector::Field(id) if id==index as u16+1));
    }
    let amount = checked(g::GeneratedDeclarations::update_amount(c::Expr::Constant(
        c::Atom::U128(9),
    )));
    assert_eq!(amount.field, 1);
    assert!(matches!(
        amount.domain,
        c::Domain::U128 {
            min: 0,
            max: 1_000_000
        }
    ));
    let signed = checked(g::GeneratedDeclarations::update_signed(c::Expr::Constant(
        c::Atom::I128(-1000),
    )));
    assert_eq!(signed.field, 2);
    assert!(matches!(
        signed.domain,
        c::Domain::I128 {
            min: -1000,
            max: 1000
        }
    ));
    let tag = checked(g::GeneratedDeclarations::update_tag(c::Expr::Constant(
        c::Atom::Enum {
            type_id: 7,
            variant: 2,
        },
    )));
    assert_eq!(tag.field, 7);
    assert!(matches!(
        tag.domain,
        c::Domain::Enum {
            type_id: 7,
            variants: &[1, 2]
        }
    ));
    for (id, result) in [
        (6, g::GeneratedDeclarations::update_nil(c::Expr::Output(0))),
        (
            8,
            g::GeneratedDeclarations::update_point(c::Expr::Output(0)),
        ),
        (
            9,
            g::GeneratedDeclarations::update_event(c::Expr::Output(0)),
        ),
        (
            10,
            g::GeneratedDeclarations::update_labels(c::Expr::Output(0)),
        ),
        (
            11,
            g::GeneratedDeclarations::update_scores(c::Expr::Output(0)),
        ),
    ] {
        assert!(
            matches!(result, Err(g::GeneratedProjectError::UnsupportedStateField(actual)) if actual==id)
        );
    }
    // A proposal is data. An out-of-domain U128 remains U128 until actual admission refuses it.
    let proposed = checked(g::GeneratedDeclarations::update_amount(c::Expr::Constant(
        c::Atom::U128(1_000_001),
    )));
    assert!(matches!(
        proposed.value,
        c::Expr::Constant(c::Atom::U128(1_000_001))
    ));
    assert!(Amount(1_000_001).to_value().is_err());
}

#[test]
fn original_generated_invocation_and_inert_delivery_keep_complete_compound_wire() {
    let project = checked(g::GeneratedProject::try_new::<RustCryptoSha256>());
    let state_value = state();
    let context_value = state();
    let command_value = Event::Stop;
    let state =
        checked(project.admit_root::<RustCryptoSha256>(&state_value, ValidationLimits::default()));
    let command = checked(
        project.admit_command::<RustCryptoSha256>(&command_value, ValidationLimits::default()),
    );
    let context = checked(
        project.admit_context::<RustCryptoSha256>(&context_value, ValidationLimits::default()),
    );
    let invocation = checked(project.invocation::<RustCryptoSha256>(&state, &command, &context));
    assert_eq!(
        invocation.original().state,
        checked(state.envelope().canonical_bytes())
    );
    assert_eq!(
        invocation.original().command,
        checked(command.admitted().envelope().canonical_bytes())
    );
    assert_eq!(
        invocation.original().context,
        checked(context.admitted().envelope().canonical_bytes())
    );
    assert_ne!(command.commitment(), context.commitment());
    let authority = checked(NonZeroHash::try_new(Hash32::new([1; 32])));
    let effect = checked(g::effect_20(7, authority, &Amount(9)));
    assert_eq!(effect.ordinal(), 7);
    assert_eq!(effect.operation(), 20);
    assert_eq!(effect.authority(), Hash32::new([1; 32]));
    assert_eq!(effect.subject(), Hash32::ZERO);
    assert_eq!(effect.payload(), &zeno_fcis_value::Value::unsigned(9));
    let outbox = checked(g::channel_30(8, &Label("destination".into()), &Event::Stop));
    assert_eq!(outbox.ordinal(), 8);
    assert_eq!(outbox.channel(), 30);
    assert_eq!(
        outbox.destination(),
        &checked(Label("destination".into()).to_value())
    );
    assert_eq!(outbox.payload(), &checked(Event::Stop.to_value()));
    assert_eq!(
        checked(outbox.payload().canonical_bytes()),
        vec![10, 0, 0, 0, 9, 0, 1, 0]
    );
    assert!(g::effect_20(7, authority, &Amount(1_000_001)).is_err());
    assert!(g::channel_30(8, &Label("".into()), &Event::Stop).is_err());
}

// This deliberately incomplete hostile proposal cannot represent the original
// Tuple/Vector/Map/payload-bearing Sum. It is a refusal control, never a replacement.
fn with_incomplete_flat_proposal(
    run: impl FnOnce(&g::GeneratedProject, &mut g::GeneratedProgramProposal<'_>),
) {
    let project = checked(g::GeneratedProject::try_new::<RustCryptoSha256>());
    let original = checked(project.catalog().schema().canonical_bytes());
    let description = s::Description {
        profile: b"CodegenFixture",
        version: 1,
        root: 12,
        definitions: &[],
    };
    let flag = [V2InputField {
        id: 5,
        leaf: V2InputLeaf::Bool,
    }];
    let command = V2InputLeaf::Bool;
    let outputs = [Domain::Int { min: 1, max: 1 }];
    let output_types = [V2InputLeaf::I128 { min: 1, max: 1 }];
    let nodes = [Op::Int(1)];
    let roots = [0];
    let reasons = [c::Reason {
        id: 10,
        class: c::Class::Reject,
    }];
    let branches = [c::Branch {
        code: 1,
        class: c::Class::Reject,
        reason: Some(10),
        assignments: &[],
        effects: &[],
        outbox: &[],
    }];
    let definition = c::Descriptor {
        state: c::Schema::Record(&flag),
        command: c::Schema::Leaf(&command),
        context: c::Schema::Record(&flag),
        program: V2ScalarProgram {
            inputs: &[],
            outputs: &outputs,
            nodes: &nodes,
            roots: &roots,
        },
        bindings: &[],
        output_types: &output_types,
        decision_output: 0,
        branches: &branches,
        reasons: &reasons,
        channels: &[],
        laws: &[],
        required: &[],
        limits: v2_zero_limits(),
    };
    let frame = |root| c::FrameBinding {
        root,
        schema: *g::SCHEMA_HASH.as_bytes(),
        max_bytes: 8192,
    };
    let framing = c::Framing {
        state: frame(12),
        command: frame(9),
        context: frame(12),
    };
    let mut proposal = g::GeneratedProgramProposal {
        original_schema: &original,
        description: &description,
        catalog_limits: k::Limits {
            schema: s::Limits {
                bytes: 8192,
                types: 32,
                fields: 32,
                variants: 32,
            },
            contract_bytes: 8192,
        },
        original_policy: b"",
        definition: &definition,
        framing: &framing,
        channel_roots: &[(30, 3, 9)],
    };
    run(&project, &mut proposal);
}

#[test]
fn original_complete_schema_refuses_flat_approximation_after_exact_generated_metadata_checks() {
    with_incomplete_flat_proposal(|project, proposal| {
        assert!(!s::encoding_matches(
            proposal.original_schema,
            proposal.description,
            8192
        ));
        let result = project.bind_program::<RustCryptoSha256>(g::GeneratedProgramProposal {
            original_schema: proposal.original_schema,
            description: proposal.description,
            catalog_limits: k::Limits {
                schema: proposal.catalog_limits.schema,
                contract_bytes: proposal.catalog_limits.contract_bytes,
            },
            original_policy: proposal.original_policy,
            definition: proposal.definition,
            framing: proposal.framing,
            channel_roots: proposal.channel_roots,
        });
        assert!(matches!(
            result,
            Err(g::GeneratedProjectError::CheckedCatalog(
                k::Failure::Schema(s::Failure::Metadata)
            ))
        ));
    });
    // Full original metadata is still reconstructed and bound by the generated project.
    let schema = checked(fixture_schema());
    assert_eq!(schema.types().len(), 12);
    let project = checked(g::GeneratedProject::try_new::<RustCryptoSha256>());
    assert_eq!(
        checked(schema.canonical_bytes()),
        checked(project.catalog().schema().canonical_bytes())
    );
    assert_eq!(project.catalog().profile().state_type().get(), 12);
    assert_eq!(project.catalog().profile().command_type().get(), 9);
    assert_eq!(project.catalog().profile().context_type().get(), 12);
}
