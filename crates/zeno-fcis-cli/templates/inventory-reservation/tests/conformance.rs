//! Normal checked bindings, genuine genesis and original-schema/policy refusals.
use inventory_reservation::{profile, v2_contract as contract};
use zeno_fcis_codec::{CanonicalEncode, Envelope, Hash32};
use zeno_fcis_synthesis::finite::{
    canonical_v2::schema, v2_authority::PublicationOutcome, v2_catalog as catalog,
    v2_composition::Raw,
};
use zeno_fcis_value::{Field, Value};
fn initial(changed: bool) -> Vec<u8> {
    let fields = if changed {
        vec![
            Field::new(110, Value::signed(1)),
            Field::new(111, Value::signed(0)),
        ]
    } else {
        vec![
            Field::new(110, Value::signed(0)),
            Field::new(111, Value::signed(0)),
        ]
    };
    Envelope::new(
        100,
        Hash32::new(contract::FRAMING.state.schema),
        Value::record_canonical(fields).unwrap(),
    )
    .canonical_bytes()
    .unwrap()
}
#[test]
fn genuine_genesis_requires_the_complete_original_initial_state() {
    let data = contract::Contract::new();
    let definition = data.descriptor();
    let authority = contract::checked_authority(&definition).unwrap();
    let good = initial(false);
    let bad = initial(true);
    assert!(matches!(
        authority.publish_genesis(&good),
        PublicationOutcome::Commit(_)
    ));
    assert!(matches!(
        authority.publish_genesis(&bad),
        PublicationOutcome::Refused { .. }
    ));
    assert!(matches!(
        authority.publish(Raw {
            state: &good,
            command: &[],
            context: &[]
        }),
        PublicationOutcome::Refused { .. }
    ));
}
#[test]
fn original_named_laws_keep_their_scopes_and_complete_required_registration() {
    let data = contract::Contract::new();
    let definition = data.descriptor();
    let manifest = profile::manifest();
    manifest.check_declared_scopes(&profile::project()).unwrap();
    for old in manifest.definitions() {
        let current = definition
            .laws
            .iter()
            .find(|law| law.id == old.id().get())
            .expect("original named/framework law retained");
        assert!(definition.required.contains(&current.id));
        assert_eq!(format!("{:?}", current.kind), format!("{:?}", old.kind()));
        assert_eq!(format!("{:?}", current.scope), format!("{:?}", old.scope()));
    }
    assert_eq!(definition.required.len(), definition.laws.len());
}
#[test]
fn mismatched_original_schema_complete_policy_and_framing_are_refused() {
    let data = contract::Contract::new();
    let definition = data.descriptor();
    let limits = || catalog::Limits {
        schema: schema::Limits {
            bytes: contract::ORIGINAL_SCHEMA.len() as u64,
            types: 64,
            fields: 32,
            variants: 32,
        },
        contract_bytes: contract::ORIGINAL_POLICY.len() as u64 + 1,
    };
    let mut original = contract::ORIGINAL_SCHEMA.to_vec();
    original.push(0);
    assert!(
        catalog::bind_original(
            &original,
            &contract::DESCRIPTION,
            limits(),
            contract::ORIGINAL_POLICY,
            &definition,
            &contract::FRAMING,
            contract::CHANNEL_ROOTS
        )
        .is_err()
    );
    let mut policy = contract::ORIGINAL_POLICY.to_vec();
    policy[0] ^= 1;
    assert!(
        catalog::bind_original(
            contract::ORIGINAL_SCHEMA,
            &contract::DESCRIPTION,
            limits(),
            &policy,
            &definition,
            &contract::FRAMING,
            contract::CHANNEL_ROOTS
        )
        .is_err()
    );
    let mut framing = contract::FRAMING;
    framing.command.root = framing.state.root;
    assert!(
        catalog::bind_original(
            contract::ORIGINAL_SCHEMA,
            &contract::DESCRIPTION,
            limits(),
            contract::ORIGINAL_POLICY,
            &definition,
            &framing,
            contract::CHANNEL_ROOTS
        )
        .is_err()
    );
    let mut reduced = definition;
    reduced.required = &[];
    assert!(contract::checked_authority(&reduced).is_err());
}
