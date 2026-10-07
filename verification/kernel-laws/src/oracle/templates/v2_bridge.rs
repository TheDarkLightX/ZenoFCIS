//! Additional complete V2 publication comparisons over every original conformance input.
//! Historical authorization/receipt bytes remain tested by the original private bodies.
//! This bridge cannot be linked from any production dependency.
use std::sync::{OnceLock, atomic::{AtomicUsize, Ordering}};
use crate::oracle::{authority::{CatalogAuthorizationDecision, CatalogTransitionProgram}, core::Decision, laws::ProjectLawEngine};
use zeno_fcis_codec::{CanonicalEncode, Domain, Envelope};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_synthesis::finite::{v2_authority::{Authority, PublicationOutcome}, v2_composition::{self as composition, Class}};
use zeno_fcis_value::Value;
#[path = "../../../../../crates/zeno-fcis-cli/templates/durable-counter/src/v2_contract.rs"]
mod durable_counter;
#[path = "../../../../../crates/zeno-fcis-cli/templates/account-lockout/src/v2_contract.rs"]
mod account_lockout;
#[path = "../../../../../crates/zeno-fcis-cli/templates/inventory-reservation/src/v2_contract.rs"]
mod inventory_reservation;
#[path = "../../../../../crates/zeno-fcis-cli/templates/order-fulfillment/src/v2_contract.rs"]
mod order_fulfillment;
#[path = "../../../../../crates/zeno-fcis-cli/templates/agent-treasury-guard/src/v2_contract.rs"]
mod agent_treasury_guard;
#[path = "../../../../../crates/zeno-fcis-cli/templates/withdrawal-queue/src/v2_contract.rs"]
mod withdrawal_queue;
#[path = "../../../../../crates/zeno-fcis-cli/templates/prepared-counter/src/v2_contract.rs"]
mod prepared_counter;
#[path = "../../../../../crates/zeno-fcis-cli/templates/compliance-gateway/src/v2_contract.rs"]
mod compliance_gateway;
static COUNTS: [AtomicUsize; 8] = [const { AtomicUsize::new(0) }; 8];
fn authority_durable_counter() -> &'static Authority<'static> {
    static CONTRACT: OnceLock<durable_counter::Contract> = OnceLock::new();
    static DESCRIPTOR: OnceLock<composition::Descriptor<'static>> = OnceLock::new();
    static AUTHORITY: OnceLock<Authority<'static>> = OnceLock::new();
    AUTHORITY.get_or_init(|| {
        let contract = CONTRACT.get_or_init(durable_counter::Contract::new);
        let descriptor = DESCRIPTOR.get_or_init(|| contract.descriptor());
        durable_counter::checked_authority(descriptor).expect("actual complete original-schema V2 binding")
    })
}
fn authority_account_lockout() -> &'static Authority<'static> {
    static CONTRACT: OnceLock<account_lockout::Contract> = OnceLock::new();
    static DESCRIPTOR: OnceLock<composition::Descriptor<'static>> = OnceLock::new();
    static AUTHORITY: OnceLock<Authority<'static>> = OnceLock::new();
    AUTHORITY.get_or_init(|| {
        let contract = CONTRACT.get_or_init(account_lockout::Contract::new);
        let descriptor = DESCRIPTOR.get_or_init(|| contract.descriptor());
        account_lockout::checked_authority(descriptor).expect("actual complete original-schema V2 binding")
    })
}
fn authority_inventory_reservation() -> &'static Authority<'static> {
    static CONTRACT: OnceLock<inventory_reservation::Contract> = OnceLock::new();
    static DESCRIPTOR: OnceLock<composition::Descriptor<'static>> = OnceLock::new();
    static AUTHORITY: OnceLock<Authority<'static>> = OnceLock::new();
    AUTHORITY.get_or_init(|| {
        let contract = CONTRACT.get_or_init(inventory_reservation::Contract::new);
        let descriptor = DESCRIPTOR.get_or_init(|| contract.descriptor());
        inventory_reservation::checked_authority(descriptor).expect("actual complete original-schema V2 binding")
    })
}
fn authority_order_fulfillment() -> &'static Authority<'static> {
    static CONTRACT: OnceLock<order_fulfillment::Contract> = OnceLock::new();
    static DESCRIPTOR: OnceLock<composition::Descriptor<'static>> = OnceLock::new();
    static AUTHORITY: OnceLock<Authority<'static>> = OnceLock::new();
    AUTHORITY.get_or_init(|| {
        let contract = CONTRACT.get_or_init(order_fulfillment::Contract::new);
        let descriptor = DESCRIPTOR.get_or_init(|| contract.descriptor());
        order_fulfillment::checked_authority(descriptor).expect("actual complete original-schema V2 binding")
    })
}
fn authority_agent_treasury_guard() -> &'static Authority<'static> {
    static CONTRACT: OnceLock<agent_treasury_guard::Contract> = OnceLock::new();
    static DESCRIPTOR: OnceLock<composition::Descriptor<'static>> = OnceLock::new();
    static AUTHORITY: OnceLock<Authority<'static>> = OnceLock::new();
    AUTHORITY.get_or_init(|| {
        let contract = CONTRACT.get_or_init(agent_treasury_guard::Contract::new);
        let descriptor = DESCRIPTOR.get_or_init(|| contract.descriptor());
        agent_treasury_guard::checked_authority(descriptor).expect("actual complete original-schema V2 binding")
    })
}
fn authority_withdrawal_queue() -> &'static Authority<'static> {
    static CONTRACT: OnceLock<withdrawal_queue::Contract> = OnceLock::new();
    static DESCRIPTOR: OnceLock<composition::Descriptor<'static>> = OnceLock::new();
    static AUTHORITY: OnceLock<Authority<'static>> = OnceLock::new();
    AUTHORITY.get_or_init(|| {
        let contract = CONTRACT.get_or_init(withdrawal_queue::Contract::new);
        let descriptor = DESCRIPTOR.get_or_init(|| contract.descriptor());
        withdrawal_queue::checked_authority(descriptor).expect("actual complete original-schema V2 binding")
    })
}
fn authority_prepared_counter() -> &'static Authority<'static> {
    static CONTRACT: OnceLock<prepared_counter::Contract> = OnceLock::new();
    static DESCRIPTOR: OnceLock<composition::Descriptor<'static>> = OnceLock::new();
    static AUTHORITY: OnceLock<Authority<'static>> = OnceLock::new();
    AUTHORITY.get_or_init(|| {
        let contract = CONTRACT.get_or_init(prepared_counter::Contract::new);
        let descriptor = DESCRIPTOR.get_or_init(|| contract.descriptor());
        prepared_counter::checked_authority(descriptor).expect("actual complete original-schema V2 binding")
    })
}
fn authority_compliance_gateway() -> &'static Authority<'static> {
    static CONTRACT: OnceLock<compliance_gateway::Contract> = OnceLock::new();
    static DESCRIPTOR: OnceLock<composition::Descriptor<'static>> = OnceLock::new();
    static AUTHORITY: OnceLock<Authority<'static>> = OnceLock::new();
    AUTHORITY.get_or_init(|| {
        let contract = CONTRACT.get_or_init(compliance_gateway::Contract::new);
        let descriptor = DESCRIPTOR.get_or_init(|| contract.descriptor());
        compliance_gateway::checked_authority(descriptor).expect("actual complete original-schema V2 binding")
    })
}
fn binding(template: usize) -> (&'static Authority<'static>, &'static [(u32,u32,u32)]) {
    match template {
        0 => (authority_durable_counter(), durable_counter::CHANNEL_ROOTS),
        1 => (authority_account_lockout(), account_lockout::CHANNEL_ROOTS),
        2 => (authority_inventory_reservation(), inventory_reservation::CHANNEL_ROOTS),
        3 => (authority_order_fulfillment(), order_fulfillment::CHANNEL_ROOTS),
        4 => (authority_agent_treasury_guard(), agent_treasury_guard::CHANNEL_ROOTS),
        5 => (authority_withdrawal_queue(), withdrawal_queue::CHANNEL_ROOTS),
        6 => (authority_prepared_counter(), prepared_counter::CHANNEL_ROOTS),
        7 => (authority_compliance_gateway(), compliance_gateway::CHANNEL_ROOTS),
        _ => panic!("unknown privately owned template"),
    }
}
/// Test-only observation span; it adds no decision or authority path.
pub(crate) struct Scope { template: usize, name: &'static str, before: usize }
impl Scope {
    pub(crate) fn new(template: usize, name: &'static str) -> Self {
        Self { template, name, before: COUNTS[template].load(Ordering::SeqCst) }
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        println!("V2_ORIGINAL_DOMAIN template={} test={} cases={} passed={}", self.template, self.name,
            COUNTS[self.template].load(Ordering::SeqCst)-self.before, !std::thread::panicking());
    }
}
/// Reevaluate exact original envelopes through the real library-owned authority.
/// Compare the complete successor envelope and every ordered delivery byte.
pub(crate) fn compare<P,L,I>(template: usize, decision: &CatalogAuthorizationDecision<RustCryptoSha256,P,L,I>, state_domain: Domain<'_>)
where P: CatalogTransitionProgram<RustCryptoSha256>, L: ProjectLawEngine {
    let (old_invocation, old_candidate, old_class, old_reason) = match decision {
        Decision::Accept(a) => (a.candidate().invocation(), Some(a.candidate()), Class::Accept, None),
        Decision::CommittedFailure(f) => (f.candidate().invocation(), Some(f.candidate()), Class::CommittedFailure, Some(f.reason().get())),
        Decision::Reject(r) => (r.reason().invocation(), None, Class::Reject, Some(r.reason().rejection().reason_id().get())),
    };
    let state = old_invocation.pre_state().envelope().canonical_bytes().unwrap();
    let command = old_invocation.command().envelope().canonical_bytes().unwrap();
    let context = old_invocation.context().envelope().canonical_bytes().unwrap();
    let original = composition::Raw { state: &state, command: &command, context: &context };
    let (authority, links) = binding(template);
    match authority.publish(original) {
        PublicationOutcome::Reject(evaluation) => {
            assert!(old_candidate.is_none(), "original commit became a V2 rejection");
            let result = evaluation.result().expect("business rejection must be checked");
            assert_eq!(result.class(), old_class);
            assert_eq!(result.reason(), old_reason);
            assert!(result.post().is_empty() && result.patch().is_empty() && result.effects().is_empty() && result.outbox().is_empty());
        },
        PublicationOutcome::Commit(publication) => {
            let old = old_candidate.expect("original rejection became a V2 publication");
            let result = publication.evaluation().result().expect("genuine publication must be checked");
            assert_eq!(result.class(), old_class);
            assert_eq!(result.reason(), old_reason);
            let pre = old_invocation.pre_state().value().value();
            let post = old.bundle().validate_and_apply::<RustCryptoSha256>(pre, state_domain).expect("complete original bundle validation");
            let root = old_invocation.pre_state().envelope();
            let expected = Envelope::new(root.type_id(), root.schema_hash(), post.state().clone()).canonical_bytes().unwrap();
            assert_eq!(publication.poststate(), expected, "complete successor envelope changed");
            assert!(old.bundle().commit_plan().effects().is_empty());
            assert!(publication.effects().is_empty());
            let entries = old.bundle().outbox_plan().entries();
            assert_eq!(publication.outbox().len(), entries.len());
            for (wire, entry) in publication.outbox().iter().zip(entries) {
                assert_eq!(wire.ordinal(), entry.ordinal());
                assert_eq!(wire.channel(), entry.channel());
                let (_, destination, payload) = links.iter().find(|link| link.0 == entry.channel()).expect("exact original channel linkage");
                assert_eq!(wire.destination_root(), *destination);
                assert_eq!(wire.payload_root(), *payload);
                assert_eq!(wire.destination(), entry.destination().canonical_bytes().unwrap());
                assert_eq!(wire.payload(), entry.payload().canonical_bytes().unwrap());
                assert_eq!(wire.idempotency(), Value::unsigned(0).canonical_bytes().unwrap());
            }
            assert!(!publication.subject().is_empty());
        },
        other => panic!("original supported decision failed actual V2 publication: {other:?}"),
    }
    COUNTS[template].fetch_add(1, Ordering::SeqCst);
}
