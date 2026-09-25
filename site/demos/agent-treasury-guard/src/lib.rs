//! The agent-treasury-guard example's authority, adapter, synthesized core,
//! and law checker, run in the browser over the library's in-memory
//! reference shell.
//!
//! The step, the report, and the C ABI are `zeno-fcis-site-common`'s. This
//! crate names the application's types, its exact genesis, and the mapping
//! from the page's request to the template's command and context, and
//! exports the module's `demo_reset`.

use agent_treasury_guard::{
    authority,
    bindings::GeneratedProject,
    context,
    delivery::Destination,
    failed,
    generated::{Caller, Direction, GuardContext, ModelId, Treasury, TreasuryCommand},
    genesis_state,
    laws::GuardLaws,
    profile,
    program::GuardProgram,
    propose, settled,
};
use zeno_fcis_site_common::{
    Application, Authority, Json, Map, Names, Request, RustCryptoSha256, SchemaAdmittedEnvelope,
    SchemaAdmittedTypeEnvelope, ValidationLimits, Value,
};

/// The template, as `zeno-fcis new --template agent-treasury-guard` writes it.
pub struct AgentTreasuryGuard;

/// Renders an error as the template renders its own: with `Debug`.
fn checked<T, E: std::fmt::Debug>(value: Result<T, E>) -> Result<T, String> {
    value.map_err(|error| format!("{error:?}"))
}

fn admit_root(treasury: &Treasury) -> Result<SchemaAdmittedEnvelope, String> {
    let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
    checked(project.admit_root::<RustCryptoSha256>(treasury, ValidationLimits::default()))
}

impl Application for AgentTreasuryGuard {
    const NAME: &'static str = "agent-treasury-guard";
    type Program = GuardProgram;
    type Laws = GuardLaws;
    type Destination = Destination;
    type Command = TreasuryCommand;
    type Context = GuardContext;

    fn authority() -> Result<Authority<Self>, String> {
        authority()
    }

    fn names() -> Result<Names, String> {
        Ok(Names::new(&profile::project(), &profile::manifest()))
    }

    fn genesis() -> Result<SchemaAdmittedEnvelope, String> {
        admit_root(&genesis_state())
    }

    fn admit_state(state: Value) -> Result<SchemaAdmittedEnvelope, String> {
        admit_root(&checked(Treasury::try_from_value(state))?)
    }

    /// `command` is `ProposeSwap`, the agent's proposal, with `direction`,
    /// `amount`, and `min_out`; `SwapSettled`, the DEX's report, with
    /// `intent` and `amount_out`; or `SwapFailed`, with `intent`. The
    /// context is `caller`, `now`, the oracle `price` and its `price_time`,
    /// and the `model` the agent runs.
    fn parse(request: &Map<String, Json>) -> Result<(TreasuryCommand, GuardContext), String> {
        let mut request = Request::new(request);
        let command = match request.choice(
            "command",
            &[("ProposeSwap", 0), ("SwapSettled", 1), ("SwapFailed", 2)],
        )? {
            0 => propose(
                request.choice(
                    "direction",
                    &[
                        ("BuyBase", Direction::BuyBase),
                        ("SellBase", Direction::SellBase),
                    ],
                )?,
                request.integer("amount")?,
                request.integer("min_out")?,
            ),
            1 => settled(request.integer("intent")?, request.integer("amount_out")?),
            _ => failed(request.integer("intent")?),
        };
        let caller = request.choice("caller", &[("Agent", Caller::Agent), ("Dex", Caller::Dex)])?;
        let now = request.integer("now")?;
        let price = request.integer("price")?;
        let price_time = request.integer("price_time")?;
        let model = request.choice(
            "model",
            &[
                ("TreasuryAgentV2", ModelId::TreasuryAgentV2),
                ("TreasuryAgentV1", ModelId::TreasuryAgentV1),
                ("UnlistedModel", ModelId::UnlistedModel),
            ],
        )?;
        request.finish()?;
        Ok((command, context(caller, now, price, price_time, model)))
    }

    fn admit(
        command: &TreasuryCommand,
        context: &GuardContext,
    ) -> Result<(SchemaAdmittedTypeEnvelope, SchemaAdmittedTypeEnvelope), String> {
        let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
        let limits = ValidationLimits::default();
        let command = checked(project.admit_command::<RustCryptoSha256>(command, limits))?;
        let context = checked(project.admit_context::<RustCryptoSha256>(context, limits))?;
        Ok((command.admitted().clone(), context.admitted().clone()))
    }
}

/// Builds the demo at the exact genesis and returns its state.
// SAFETY: the `demo_` prefix keeps every exported symbol unique.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn demo_reset() -> u32 {
    zeno_fcis_site_common::abi::reset::<AgentTreasuryGuard>()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use zeno_fcis_site_common::serde_json::json;
    use zeno_fcis_site_common::{Demo, Stage};

    fn context_json(caller: &str, now: i64, price: i64) -> String {
        format!(
            r#""caller":"{caller}","now":{now},"price":{price},"price_time":{now},"model":"TreasuryAgentV2""#
        )
    }

    fn proposal(
        direction: &str,
        amount: i64,
        min_out: i64,
        caller: &str,
        now: i64,
        price: i64,
    ) -> String {
        format!(
            r#"{{"command":"ProposeSwap","direction":"{direction}","amount":{amount},"min_out":{min_out},{}}}"#,
            context_json(caller, now, price)
        )
    }

    fn settlement(intent: i64, amount_out: i64, caller: &str, now: i64) -> String {
        format!(
            r#"{{"command":"SwapSettled","intent":{intent},"amount_out":{amount_out},{}}}"#,
            context_json(caller, now, 1)
        )
    }

    fn failure(intent: i64, caller: &str, now: i64) -> String {
        format!(
            r#"{{"command":"SwapFailed","intent":{intent},{}}}"#,
            context_json(caller, now, 1)
        )
    }

    fn law_ids(report: &Json) -> Vec<u64> {
        report["laws"]
            .as_array()
            .unwrap()
            .iter()
            .map(|law| {
                assert_eq!(law["status"], "Satisfied", "{law}");
                law["id"].as_u64().unwrap()
            })
            .collect()
    }

    #[test]
    fn a_new_demo_holds_the_exact_genesis() {
        let state = Demo::<AgentTreasuryGuard>::new().unwrap().state().unwrap();
        assert_eq!(
            state["state"],
            json!({
                "quote": 6, "base": 1, "spent_today": 0, "last_seen": 0,
                "pending": "NoSwap", "pending_amount": 0, "pending_min_out": 0
            })
        );
        assert_eq!(state["bundles"], 0);
        assert_eq!(state["outbox"], json!([]));
    }

    #[test]
    fn an_accepted_proposal_queues_the_swap_it_records_and_settles() {
        let mut demo = Demo::<AgentTreasuryGuard>::new().unwrap();
        let proposed = demo
            .step(&proposal("BuyBase", 2, 2, "Agent", 1, 1))
            .unwrap();
        assert_eq!(proposed["decision"], "Accept");
        assert_eq!(proposed["after"]["quote"], 4);
        assert_eq!(proposed["after"]["spent_today"], 2);
        assert_eq!(proposed["after"]["pending"], "PendingBuy");
        assert_eq!(proposed["after"]["pending_amount"], 2);
        let queued = proposed["outbox"].as_array().unwrap();
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0]["channel"], 300);
        assert_eq!(queued[0]["channel_name"], "swap_request");
        assert_eq!(queued[0]["destination"], "zenodex");
        assert_eq!(
            queued[0]["payload"],
            json!({
                "intent_number": 1, "asset_in": "Quote", "asset_out": "Base",
                "amount_in": 2, "min_amount_out": 2, "deadline": 3
            })
        );
        let settled = demo.step(&settlement(1, 2, "Dex", 2)).unwrap();
        assert_eq!(settled["decision"], "Accept");
        assert_eq!(settled["after"]["base"], 3);
        assert_eq!(settled["after"]["pending"], "NoSwap");
    }

    #[test]
    fn the_guard_refuses_by_the_first_rule_that_applies_and_records_a_failure() {
        let mut demo = Demo::<AgentTreasuryGuard>::new().unwrap();
        let wrong_caller = demo.step(&proposal("BuyBase", 1, 1, "Dex", 1, 1)).unwrap();
        assert_eq!(wrong_caller["decision"], "Reject");
        assert_eq!(
            wrong_caller["reason"],
            json!({ "id": 200, "name": "wrong_caller" })
        );
        let nothing_outstanding = demo.step(&failure(0, "Dex", 1)).unwrap();
        assert_eq!(
            nothing_outstanding["reason"],
            json!({ "id": 205, "name": "no_swap_outstanding" })
        );
        let over_cap = demo
            .step(&proposal("SellBase", 2, 3, "Agent", 1, 2))
            .unwrap();
        assert_eq!(
            over_cap["reason"],
            json!({ "id": 208, "name": "over_trade_cap" })
        );
        demo.step(&proposal("BuyBase", 2, 2, "Agent", 1, 1))
            .unwrap();
        let failed = demo.step(&failure(1, "Dex", 2)).unwrap();
        assert_eq!(failed["decision"], "CommittedFailure");
        assert_eq!(
            failed["reason"],
            json!({ "id": 212, "name": "swap_failed" })
        );
        assert_eq!(failed["after"]["quote"], 6);
        assert_eq!(failed["after"]["spent_today"], 2);
        assert_eq!(failed["after"]["pending"], "NoSwap");
    }

    #[test]
    fn law_statuses_follow_the_manifest_scopes() {
        let mut demo = Demo::<AgentTreasuryGuard>::new().unwrap();
        let accept = demo
            .step(&proposal("BuyBase", 2, 2, "Agent", 1, 1))
            .unwrap();
        assert_eq!(law_ids(&accept), [500, 501, 502, 503, 504, 505, 506, 507]);
        let failed_swap = demo.step(&failure(1, "Dex", 2)).unwrap();
        assert_eq!(law_ids(&failed_swap), [500, 501, 502, 508]);
        let reject = demo.step(&failure(1, "Dex", 3)).unwrap();
        assert_eq!(law_ids(&reject), [509]);
    }

    #[test]
    fn a_malformed_request_is_refused_before_any_decision() {
        let mut demo = Demo::<AgentTreasuryGuard>::new().unwrap();
        for (input, stage, message) in [
            (
                format!(
                    r#"{{"command":"SwapSettled","direction":"BuyBase","intent":1,"amount_out":1,{}}}"#,
                    context_json("Dex", 1, 1)
                ),
                Stage::Input,
                "unexpected field \"direction\"",
            ),
            (
                r#"{"command":"ProposeSwap","direction":"BuyBase","amount":1,"min_out":1,"caller":"Agent","now":1,"price":1,"price_time":1,"model":"Nope"}"#.to_string(),
                Stage::Input,
                "\"model\" must be one of TreasuryAgentV2, TreasuryAgentV1, UnlistedModel",
            ),
            (
                proposal("BuyBase", 4, 1, "Agent", 1, 1),
                Stage::Admission,
                "IntegerRange",
            ),
            (
                proposal("BuyBase", 1, 1, "Agent", 12, 1),
                Stage::Admission,
                "IntegerRange",
            ),
        ] {
            let refusal = demo.step(&input).unwrap_err();
            assert_eq!(refusal.stage(), stage, "{input}: {refusal:?}");
            assert!(
                refusal.to_json()["error"]
                    .as_str()
                    .unwrap()
                    .contains(message),
                "{input}: {refusal:?}"
            );
        }
        assert_eq!(demo.state().unwrap()["steps"], 0);
    }
}
