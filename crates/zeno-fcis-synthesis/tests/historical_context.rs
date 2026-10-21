//! Actual released publication bytes bind every historical invocation field.
use zeno_fcis_synthesis::finite::{
    V2Resource,
    historical::{self, HistoricalContext, Refusal},
};
#[path = "../../zeno-fcis-cli/tests/fixtures/withdrawal-queue-adopted/src/v2_contract_v1.rs"]
/// Current metadata for the same original policy.
pub mod current;
#[path = "test-data/released_24_contract.rs"]
/// Original released declaration data for the retained policy.
pub mod declarations;

fn with_context(f: impl FnOnce(&HistoricalContext<'_>)) {
    let data = declarations::Contract::new();
    let descriptor = data.descriptor();
    let catalog = declarations::checked_catalog(&descriptor)
        .unwrap_or_else(|e| panic!("released catalog: {e:?}"));
    let context = historical::bind_released_24(&catalog)
        .unwrap_or_else(|e| panic!("released source binding: {e:?}"));
    f(&context);
}

const IDENTITY: &[u8] = include_bytes!("test-data/released-24/genesis-identity.bin");
const INITIAL: &[u8] = include_bytes!("test-data/released-24/genesis-initial.bin");
const GENESIS: &[u8] = include_bytes!("test-data/released-24/genesis-genesis.bin");

#[test]
fn arbitrary_digest_refused() {
    with_context(|context| {
        assert_eq!(context.verify_identity(IDENTITY), Ok(()));
        let mut forged = IDENTITY.to_vec();
        let last = forged.len() - 1;
        forged[last] ^= 1;
        assert_eq!(context.verify_identity(&forged), Err(Refusal::Identity));
        assert_eq!(context.verify_identity(IDENTITY), Ok(()));
    });
}

#[test]
fn current_runtime_cannot_impersonate_history() {
    with_context(|context| {
        let data = current::Contract::new();
        let descriptor = data.descriptor();
        let authority = current::checked_authority(&descriptor)
            .unwrap_or_else(|e| panic!("current binding: {e:?}"));
        assert_eq!(context.policy(), current::ORIGINAL_POLICY);
        assert_ne!(context.identity(), authority.identity());
        assert_eq!(
            context.verify_identity(authority.identity()),
            Err(Refusal::Identity)
        );
        assert!(
            context
                .verify_genesis(INITIAL, GENESIS)
                .into_result()
                .is_ok()
        );
        assert!(matches!(
            authority.replay_genesis_publication(INITIAL, GENESIS),
            zeno_fcis_synthesis::finite::v2_authority::PublicationOutcome::Refused { .. }
        ));
    });
}

#[test]
fn publication_subject_uses_checked_context() {
    with_context(|context| {
        assert_eq!(context.identity(), IDENTITY);
        assert!(
            GENESIS
                .windows(IDENTITY.len())
                .any(|bytes| bytes == IDENTITY)
        );
        let first = context.verify_genesis(INITIAL, GENESIS);
        let used = first.usage();
        assert!(used.used(V2Resource::Step) > 0);
        let checked = first
            .into_result()
            .unwrap_or_else(|e| panic!("genuine genesis: {e:?}"));
        assert_eq!(checked.subject(), GENESIS);
        assert_eq!(checked.poststate(), INITIAL);
        let refused = context.verify_genesis(INITIAL, &[]);
        assert_eq!(refused.usage(), used);
        assert_eq!(refused.into_result().err(), Some(Refusal::Publication));
        let again = context.verify_genesis(INITIAL, GENESIS);
        assert_eq!(again.usage(), used);
        assert_eq!(
            again
                .into_result()
                .unwrap_or_else(|e| panic!("repeat: {e:?}"))
                .subject(),
            checked.subject()
        );
    });
}

#[test]
fn every_retained_publication_field_is_bound() {
    with_context(|context| {
        for index in 0..GENESIS.len() {
            let mut changed = GENESIS.to_vec();
            changed[index] ^= 1;
            assert!(
                context
                    .verify_genesis(INITIAL, &changed)
                    .into_result()
                    .is_err(),
                "unbound publication byte {index}"
            );
        }
        assert!(
            context
                .verify_genesis(INITIAL, GENESIS)
                .into_result()
                .is_ok()
        );
    });
}
