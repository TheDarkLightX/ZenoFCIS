//! The withdrawal-queue example's authority, synthesized controller step,
//! and law checker, run in the browser over the library's in-memory
//! reference shell.
//!
//! The step, the report, and the C ABI are `zeno-fcis-site-common`'s. This
//! crate names the application's types, its exact genesis, and the mapping
//! from the page's request to the template's command and context, and
//! exports the module's `demo_reset`.

use withdrawal_queue::{
    authority,
    bindings::GeneratedProject,
    delivery::Destination,
    deposit,
    generated::{Caller, Flag, Lane, TickContext, Vault, VaultCommand},
    genesis_state,
    laws::VaultLaws,
    profile,
    program::VaultProgram,
    request, tick,
};
use zeno_fcis_site_common::{
    Application, Authority, Json, Map, Names, Request, RustCryptoSha256, SchemaAdmittedEnvelope,
    SchemaAdmittedTypeEnvelope, ValidationLimits, Value,
};

/// The template, as `zeno-fcis new --template withdrawal-queue` writes it.
pub struct WithdrawalQueue;

/// Renders an error as the template renders its own: with `Debug`.
fn checked<T, E: std::fmt::Debug>(value: Result<T, E>) -> Result<T, String> {
    value.map_err(|error| format!("{error:?}"))
}

fn admit_root(vault: &Vault) -> Result<SchemaAdmittedEnvelope, String> {
    let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
    checked(project.admit_root::<RustCryptoSha256>(vault, ValidationLimits::default()))
}

impl Application for WithdrawalQueue {
    const NAME: &'static str = "withdrawal-queue";
    type Program = VaultProgram;
    type Laws = VaultLaws;
    type Destination = Destination;
    type Command = VaultCommand;
    type Context = TickContext;

    fn authority() -> Result<Authority<Self>, String> {
        authority()
    }

    fn names() -> Result<Names, String> {
        Ok(Names::new(
            &checked(profile::project())?,
            &checked(profile::manifest())?,
        ))
    }

    fn genesis() -> Result<SchemaAdmittedEnvelope, String> {
        admit_root(&genesis_state())
    }

    fn admit_state(state: Value) -> Result<SchemaAdmittedEnvelope, String> {
        admit_root(&checked(Vault::try_from_value(state))?)
    }

    /// `command` is `Deposit` with its `amount`, `RequestWithdrawal` with
    /// its `lane` and `amount`, or `Tick`, which carries neither. The
    /// context is `caller` and `alarm`.
    fn parse(fields: &Map<String, Json>) -> Result<(VaultCommand, TickContext), String> {
        let mut fields = Request::new(fields);
        let command = match fields.choice(
            "command",
            &[("Deposit", 0), ("RequestWithdrawal", 1), ("Tick", 2)],
        )? {
            0 => deposit(fields.integer("amount")?),
            1 => request(
                fields.choice("lane", &[("A", Lane::A), ("B", Lane::B)])?,
                fields.integer("amount")?,
            ),
            _ => tick(),
        };
        let caller = fields.choice(
            "caller",
            &[
                ("Operator", Caller::Operator),
                ("OwnerA", Caller::OwnerA),
                ("OwnerB", Caller::OwnerB),
                ("Keeper", Caller::Keeper),
            ],
        )?;
        let alarm = Flag(fields.flag("alarm")?);
        fields.finish()?;
        Ok((command, TickContext { caller, alarm }))
    }

    fn admit(
        command: &VaultCommand,
        context: &TickContext,
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
    zeno_fcis_site_common::abi::reset::<WithdrawalQueue>()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use zeno_fcis_site_common::serde_json::json;
    use zeno_fcis_site_common::{Demo, Stage};

    fn deposit_of(amount: i64, caller: &str) -> String {
        format!(r#"{{"command":"Deposit","amount":{amount},"caller":"{caller}","alarm":false}}"#)
    }

    fn withdrawal(lane: &str, amount: i64, caller: &str) -> String {
        format!(
            r#"{{"command":"RequestWithdrawal","lane":"{lane}","amount":{amount},"caller":"{caller}","alarm":false}}"#
        )
    }

    fn keeper_tick(alarm: bool) -> String {
        format!(r#"{{"command":"Tick","caller":"Keeper","alarm":{alarm}}}"#)
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
        let state = Demo::<WithdrawalQueue>::new().unwrap().state().unwrap();
        assert_eq!(
            state["state"],
            json!({
                "balance": 0, "lane_a": "Empty", "amount_a": 0, "lane_b": "Empty", "amount_b": 0,
                "pause": 0, "must_serve": false, "priority": "A"
            })
        );
        assert_eq!(state["bundles"], 0);
        assert_eq!(state["outbox"], json!([]));
    }

    #[test]
    fn a_tick_pays_the_due_lane_and_queues_the_payout() {
        let mut demo = Demo::<WithdrawalQueue>::new().unwrap();
        let deposited = demo.step(&deposit_of(2, "Operator")).unwrap();
        assert_eq!(deposited["decision"], "Accept");
        assert_eq!(deposited["after"]["balance"], 2);
        let requested = demo.step(&withdrawal("A", 2, "OwnerA")).unwrap();
        assert_eq!(requested["after"]["lane_a"], "Arrived");
        assert_eq!(requested["after"]["amount_a"], 2);
        let paid = demo.step(&keeper_tick(false)).unwrap();
        assert_eq!(paid["decision"], "Accept");
        assert_eq!(paid["after"]["balance"], 0);
        assert_eq!(paid["after"]["lane_a"], "Empty");
        assert_eq!(paid["after"]["priority"], "B");
        let queued = paid["outbox"].as_array().unwrap();
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0]["channel"], 300);
        assert_eq!(queued[0]["channel_name"], "payout");
        assert_eq!(queued[0]["destination"], "settlement");
        assert_eq!(
            queued[0]["payload"],
            json!({ "paid_lane": "A", "paid_amount": 2 })
        );
    }

    #[test]
    fn an_honored_alarm_pauses_and_a_wrong_caller_is_rejected() {
        let mut demo = Demo::<WithdrawalQueue>::new().unwrap();
        let paused = demo.step(&keeper_tick(true)).unwrap();
        assert_eq!(paused["decision"], "Accept");
        assert_eq!(paused["after"]["pause"], 2);
        assert_eq!(paused["outbox"], json!([]));
        let rejected = demo.step(&deposit_of(1, "Keeper")).unwrap();
        assert_eq!(rejected["decision"], "Reject");
        assert_eq!(
            rejected["reason"],
            json!({ "id": 200, "name": "wrong_caller" })
        );
        assert_eq!(rejected["after"], rejected["before"]);
    }

    #[test]
    fn law_statuses_follow_the_manifest_scopes() {
        let mut demo = Demo::<WithdrawalQueue>::new().unwrap();
        let accept = demo.step(&deposit_of(1, "Operator")).unwrap();
        assert_eq!(law_ids(&accept), [500, 501, 502, 503]);
        assert_eq!(accept["laws"][3]["name"], "tick_follows_the_controller");
        demo.step(&deposit_of(2, "Operator")).unwrap();
        // A fourth unit fits the capacity of 4; a fifth is rejected.
        let reject = demo.step(&deposit_of(2, "Operator")).unwrap();
        assert_eq!(reject["reason"]["name"], "over_capacity");
        assert_eq!(law_ids(&reject), [509]);
    }

    #[test]
    fn a_malformed_request_is_refused_before_any_decision() {
        let mut demo = Demo::<WithdrawalQueue>::new().unwrap();
        for (input, stage, message) in [
            (
                r#"{"command":"Deposit","lane":"A","amount":1,"caller":"Operator","alarm":false}"#,
                Stage::Input,
                "unexpected field \"lane\"",
            ),
            (
                r#"{"command":"Tick","amount":1,"caller":"Keeper","alarm":false}"#,
                Stage::Input,
                "unexpected field \"amount\"",
            ),
            (
                r#"{"command":"RequestWithdrawal","lane":"C","amount":1,"caller":"OwnerA","alarm":false}"#,
                Stage::Input,
                "\"lane\" must be one of A, B",
            ),
            (&deposit_of(3, "Operator"), Stage::Admission, "IntegerRange"),
        ] {
            let refusal = demo.step(input).unwrap_err();
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
