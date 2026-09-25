//! The account-lockout example's authority, program, and law checker, run in
//! the browser over the library's in-memory reference shell.
//!
//! The step, the report, and the C ABI are `zeno-fcis-site-common`'s. This
//! crate names the application's types, its exact genesis, and the mapping
//! from the page's request to the template's command and context, and
//! exports the module's `demo_reset`.

use account_lockout::{
    authority,
    bindings::GeneratedProject,
    context,
    delivery::Destination,
    generated::{Account, AccountCommand, RequestContext},
    genesis_state,
    laws::AccountLaws,
    profile,
    program::AccountProgram,
};
use zeno_fcis_site_common::{
    Application, Authority, Json, Map, Names, Request, RustCryptoSha256, SchemaAdmittedEnvelope,
    SchemaAdmittedTypeEnvelope, ValidationLimits, Value,
};

/// The template, as `zeno-fcis new --template account-lockout` writes it.
pub struct AccountLockout;

/// Renders an error as the template renders its own: with `Debug`.
fn checked<T, E: std::fmt::Debug>(value: Result<T, E>) -> Result<T, String> {
    value.map_err(|error| format!("{error:?}"))
}

fn admit_root(account: &Account) -> Result<SchemaAdmittedEnvelope, String> {
    let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
    checked(project.admit_root::<RustCryptoSha256>(account, ValidationLimits::default()))
}

impl Application for AccountLockout {
    const NAME: &'static str = "account-lockout";
    type Program = AccountProgram;
    type Laws = AccountLaws;
    type Destination = Destination;
    type Command = AccountCommand;
    type Context = RequestContext;

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
        admit_root(&checked(Account::try_from_value(state))?)
    }

    /// `command` is `LoginSucceeded`, `LoginFailed`, or `AdminUnlock`; the
    /// context is `now`, the time in Unix seconds, and `admin`, whether the
    /// request is marked as an administrator's.
    fn parse(request: &Map<String, Json>) -> Result<(AccountCommand, RequestContext), String> {
        let mut request = Request::new(request);
        let command = request.choice(
            "command",
            &[
                ("LoginSucceeded", AccountCommand::LoginSucceeded),
                ("LoginFailed", AccountCommand::LoginFailed),
                ("AdminUnlock", AccountCommand::AdminUnlock),
            ],
        )?;
        let context = context(request.integer("now")?, request.flag("admin")?);
        request.finish()?;
        Ok((command, context))
    }

    fn admit(
        command: &AccountCommand,
        context: &RequestContext,
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
pub extern "C" fn demo_reset() -> *mut u8 {
    zeno_fcis_site_common::abi::reset::<AccountLockout>()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use zeno_fcis_site_common::serde_json::json;
    use zeno_fcis_site_common::{Demo, Stage};

    fn request(command: &str, now: i128, admin: bool) -> String {
        format!(r#"{{"command":"{command}","now":{now},"admin":{admin}}}"#)
    }

    fn account(failed_attempts: i64, locked_until: i64, last_seen: i64) -> Json {
        json!({ "failed_attempts": failed_attempts, "locked_until": locked_until, "last_seen": last_seen })
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
        let state = Demo::<AccountLockout>::new().unwrap().state().unwrap();
        assert_eq!(state["state"], account(0, 0, 0));
        assert_eq!(state["bundles"], 0);
        assert_eq!(state["outbox"], json!([]));
        assert_eq!(state["steps"], 0);
    }

    #[test]
    fn three_failed_logins_lock_the_account_and_queue_a_named_alert() {
        let mut demo = Demo::<AccountLockout>::new().unwrap();
        let first = demo.step(&request("LoginFailed", 1000, false)).unwrap();
        assert_eq!(first["decision"], "CommittedFailure");
        assert_eq!(
            first["reason"],
            json!({ "id": 203, "name": "login_failed" })
        );
        assert_eq!(first["before"], account(0, 0, 0));
        assert_eq!(first["after"], account(1, 0, 1000));
        demo.step(&request("LoginFailed", 1010, false)).unwrap();
        let third = demo.step(&request("LoginFailed", 1020, false)).unwrap();
        assert_eq!(third["after"], account(0, 1920, 1020));
        let outbox = third["outbox"].as_array().unwrap();
        assert_eq!(outbox.len(), 1);
        assert_eq!(outbox[0]["channel"], 300);
        assert_eq!(outbox[0]["channel_name"], "security_alert");
        assert_eq!(outbox[0]["destination"], "security-team");
        assert_eq!(
            outbox[0]["payload"],
            json!({ "alert_kind": "Locked", "alert_until": 1920 })
        );
        assert_eq!(outbox[0]["acknowledged"], false);
        let state = demo.state().unwrap();
        assert_eq!(state["bundles"], 3);
        assert_eq!(state["outbox"].as_array().unwrap().len(), 1);
        // A decision reports only the entries it queued; the state holds them all.
        demo.step(&request("LoginSucceeded", 1920, false)).unwrap();
        let unlock = demo.step(&request("AdminUnlock", 2010, true)).unwrap();
        let queued = unlock["outbox"].as_array().unwrap();
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0]["payload"]["alert_kind"], "Unlocked");
        let state = demo.state().unwrap();
        let kinds: Vec<&Json> = state["outbox"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| &entry["payload"]["alert_kind"])
            .collect();
        assert_eq!(kinds, [&json!("Locked"), &json!("Unlocked")]);
    }

    #[test]
    fn a_rejection_publishes_nothing_and_names_its_reason() {
        let mut demo = Demo::<AccountLockout>::new().unwrap();
        demo.step(&request("LoginSucceeded", 1000, false)).unwrap();
        let before = demo.state().unwrap();
        let report = demo.step(&request("LoginSucceeded", 999, false)).unwrap();
        assert_eq!(report["decision"], "Reject");
        assert_eq!(
            report["reason"],
            json!({ "id": 200, "name": "clock_regressed" })
        );
        assert_eq!(report["roots"]["before"], report["roots"]["after"]);
        assert_eq!(report["commit"], Json::Null);
        assert!(report["rejection_id"].is_string());
        let after = demo.state().unwrap();
        assert_eq!(after["state"], before["state"]);
        assert_eq!(after["root"], before["root"]);
        assert_eq!(after["bundles"], before["bundles"]);
    }

    #[test]
    fn law_statuses_follow_the_manifest_scopes() {
        let mut demo = Demo::<AccountLockout>::new().unwrap();
        let failure = demo.step(&request("LoginFailed", 1000, false)).unwrap();
        assert_eq!(law_ids(&failure), [500, 503]);
        let accept = demo.step(&request("LoginSucceeded", 1010, false)).unwrap();
        assert_eq!(law_ids(&accept), [500, 501, 502]);
        let reject = demo.step(&request("AdminUnlock", 1020, false)).unwrap();
        assert_eq!(law_ids(&reject), [509]);
        assert_eq!(accept["laws"][1]["name"], "login_clears_failures");
        assert_eq!(reject["laws"][0]["name"], "reject_publishes_nothing");
    }

    #[test]
    fn a_malformed_request_is_refused_before_any_decision() {
        let mut demo = Demo::<AccountLockout>::new().unwrap();
        for (input, stage, message) in [
            ("{", Stage::Input, "invalid JSON"),
            ("[]", Stage::Input, "a request is a JSON object"),
            (
                r#"{"command":"Nope","now":1,"admin":false}"#,
                Stage::Input,
                "\"command\" must be one of LoginSucceeded, LoginFailed, AdminUnlock",
            ),
            (
                r#"{"command":"LoginFailed","now":1.5,"admin":false}"#,
                Stage::Input,
                "\"now\" must be an integer",
            ),
            (
                r#"{"command":"LoginFailed","now":1}"#,
                Stage::Input,
                "\"admin\" is required",
            ),
            (
                r#"{"command":"LoginFailed","now":1,"admin":false,"extra":1}"#,
                Stage::Input,
                "unexpected field \"extra\"",
            ),
            (
                r#"{"command":"LoginFailed","now":-1,"admin":false}"#,
                Stage::Admission,
                "IntegerRange",
            ),
            (
                r#"{"command":"LoginFailed","now":4102444801,"admin":false}"#,
                Stage::Admission,
                "IntegerRange",
            ),
        ] {
            let refusal = demo.step(input).unwrap_err();
            assert_eq!(refusal.stage(), stage, "{input}: {refusal:?}");
            let json = refusal.to_json();
            assert_eq!(json["stage"], stage.label());
            assert!(
                json["error"].as_str().unwrap().contains(message),
                "{input}: {json}"
            );
        }
        let state = demo.state().unwrap();
        assert_eq!(state["bundles"], 0);
        assert_eq!(state["steps"], 0);
        let report = demo.step(&request("LoginFailed", 0, false)).unwrap();
        assert_eq!(report["decision"], "CommittedFailure");
    }
}
