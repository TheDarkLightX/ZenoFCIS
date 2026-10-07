//! Original template factories, algorithms and regressions; verification only.
#[path = "prepared-counter/src/lib.rs"]
pub(crate) mod prepared_counter;
#[path = "compliance-gateway/src/lib.rs"]
pub(crate) mod compliance_gateway;
#[path="durable-counter/src/lib.rs"]
pub(crate) mod durable_counter;
#[path="account-lockout/src/lib.rs"]
pub(crate) mod account_lockout;
#[path="inventory-reservation/src/lib.rs"]
pub(crate) mod inventory_reservation;
#[path="order-fulfillment/src/lib.rs"]
pub(crate) mod order_fulfillment;
#[path="agent-treasury-guard/src/lib.rs"]
pub(crate) mod agent_treasury_guard;
#[path="withdrawal-queue/src/lib.rs"]
pub(crate) mod withdrawal_queue;
pub(crate) mod v2_bridge;

/// Explicit regeneration of the six oracle policy files from their own declarations.
#[cfg(test)]
mod policy_artifacts {
    use zeno_fcis_synthesis::finite::{v2_authority as a, v2_composition as c};
    macro_rules! emit {
        ($module:path, $name:literal) => {{
            use $module as t;
            let contract = t::Contract::new();
            let d = contract.descriptor();
            assert!(c::bind(&d).is_ok(), concat!($name, " descriptor"));
            let bytes = a::policy_bytes(&d, t::ORIGINAL_SCHEMA, &t::FRAMING, t::CHANNEL_ROOTS)
                .expect("library policy encoding");
            let file = concat!(env!("CARGO_MANIFEST_DIR"), "/src/oracle/templates/", $name, "/v2/policy.zcve");
            std::fs::write(file, &bytes).unwrap();
        }};
    }
    #[test]
    #[ignore = "Explicit artifact generation: writes only six oracle policy files"]
    fn emit_oracle_policy_artifacts() {
        emit!(super::durable_counter::normal::v2_contract, "durable-counter");
        emit!(super::account_lockout::normal::v2_contract, "account-lockout");
        emit!(super::inventory_reservation::normal::v2_contract, "inventory-reservation");
        emit!(super::order_fulfillment::normal::v2_contract, "order-fulfillment");
        emit!(super::agent_treasury_guard::normal::v2_contract, "agent-treasury-guard");
        emit!(super::withdrawal_queue::normal::v2_contract, "withdrawal-queue");
    }
}
