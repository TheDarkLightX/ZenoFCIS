//! One account, decided by the template's unchanged authority.
//!
//! A step follows the template's own `invoke`, with the SQLite shell replaced
//! by [`AuthorizedShellState`]: schema admission through the generated
//! bindings, `admit_invocation`, `execute`, then, for an accept or a committed
//! failure, the commit and the exact replay check. The page supplies every
//! context value, including the time; nothing here reads a clock.

#![forbid(unsafe_code)]

use account_lockout::{
    Authority, authority,
    bindings::GeneratedProject,
    context,
    delivery::Destination,
    generated::{Account, AccountCommand, AlertDestination, AlertKind, SecurityAlert},
    genesis_state,
    laws::AccountLaws,
    profile,
    program::AccountProgram,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fmt::Debug;
use zeno_fcis_authority::{
    AuthorizationDecodeLimits, AuthorizedShellState, CatalogAuthorizedTransition,
};
use zeno_fcis_codec::{CanonicalEncode, Hash32};
use zeno_fcis_core::Decision;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_laws::{LawEvaluation, LawStatus};
use zeno_fcis_schema::ValidationLimits;
use zeno_fcis_shell::{CommitStatus, OutboxRecord};

/// The in-memory shell, pinned to the same provider, program, laws, and
/// destination type as the template's `Authority`.
type Shell = AuthorizedShellState<RustCryptoSha256, AccountProgram, AccountLaws, Destination>;
/// A decision the authority authorized to commit.
type Transition =
    CatalogAuthorizedTransition<RustCryptoSha256, AccountProgram, AccountLaws, Destination>;

/// Which check refused a request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stage {
    /// The page's JSON was not a request.
    Input,
    /// The schema refused a value before any decision.
    Admission,
    /// The authority refused to admit or execute the invocation.
    Authority,
    /// The shell refused the publication or its exact replay.
    Commit,
}

impl Stage {
    const fn label(self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Admission => "admission",
            Self::Authority => "authority",
            Self::Commit => "commit",
        }
    }
}

/// A refused request: no decision was made, and nothing changed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Refusal {
    stage: Stage,
    message: String,
}

impl Refusal {
    pub(crate) fn new(stage: Stage, message: impl Into<String>) -> Self {
        Self {
            stage,
            message: message.into(),
        }
    }

    /// Which check refused the request.
    #[must_use]
    pub const fn stage(&self) -> Stage {
        self.stage
    }

    /// The refusal as the page receives it.
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({ "error": self.message, "stage": self.stage.label() })
    }
}

/// Renders an error at `stage` as the template renders its own: with `Debug`.
fn refused<E: Debug>(stage: Stage) -> impl Fn(E) -> Refusal {
    move |error| Refusal::new(stage, format!("{error:?}"))
}

/// A failed publication consumes the shell; only a reset restores one.
fn consumed() -> Refusal {
    Refusal::new(
        Stage::Commit,
        "the shell was consumed by a failed publication; reset the demo",
    )
}

/// A request as the page sends it.
struct Request {
    command: AccountCommand,
    now: i128,
    admin: bool,
}

/// Reads `{"command": NAME, "now": SECONDS, "admin": FLAG}` and nothing else.
fn parse_request(input: &str) -> Result<Request, Refusal> {
    let invalid = |message: String| Refusal::new(Stage::Input, message);
    let value: Value =
        serde_json::from_str(input).map_err(|error| invalid(format!("invalid JSON: {error}")))?;
    let Value::Object(mut fields) = value else {
        return Err(invalid("a request is a JSON object".into()));
    };
    let command = match fields.remove("command") {
        Some(Value::String(name)) => match name.as_str() {
            "LoginSucceeded" => AccountCommand::LoginSucceeded,
            "LoginFailed" => AccountCommand::LoginFailed,
            "AdminUnlock" => AccountCommand::AdminUnlock,
            other => return Err(invalid(format!("unknown command {other:?}"))),
        },
        _ => return Err(invalid("\"command\" must be a string".into())),
    };
    let Some(now) = fields.remove("now").as_ref().and_then(Value::as_i64) else {
        return Err(invalid("\"now\" must be an integer".into()));
    };
    let Some(admin) = fields.remove("admin").as_ref().and_then(Value::as_bool) else {
        return Err(invalid("\"admin\" must be true or false".into()));
    };
    if let Some(extra) = fields.keys().next() {
        return Err(invalid(format!("unexpected field {extra:?}")));
    }
    Ok(Request {
        command,
        now: i128::from(now),
        admin,
    })
}

/// An integer as JSON; a value beyond `i64`, which the schema never admits,
/// is rendered as a decimal string rather than lost.
fn number(value: i128) -> Value {
    i64::try_from(value).map_or_else(|_| Value::String(value.to_string()), Value::from)
}

fn account_json(account: &Account) -> Value {
    json!({
        "failed_attempts": number(account.failed_attempts.0),
        "locked_until": number(account.locked_until.0),
        "last_seen": number(account.last_seen.0),
    })
}

/// The account the shell holds, through the generated bindings.
fn account(shell: &Shell) -> Result<Account, Refusal> {
    Account::try_from_value(shell.reference_state().state().clone()).map_err(refused(Stage::Commit))
}

/// One committed alert, with its delivery identity and acknowledgement.
fn outbox_json(record: &OutboxRecord) -> Result<Value, Refusal> {
    let entry = record.entry();
    let destination = AlertDestination::try_from_value(entry.destination().clone())
        .map_err(refused(Stage::Commit))?;
    let alert =
        SecurityAlert::try_from_value(entry.payload().clone()).map_err(refused(Stage::Commit))?;
    let kind = match alert.alert_kind {
        AlertKind::Locked => "Locked",
        AlertKind::Unlocked => "Unlocked",
    };
    Ok(json!({
        "candidate_id": record.candidate_id().to_string(),
        "ordinal": record.ordinal(),
        "channel": entry.channel(),
        "destination": destination.0.to_string(),
        "alert_kind": kind,
        "alert_until": number(alert.alert_until.0),
        "delivery_id": record.delivery_id().to_string(),
        "entry_hash": record.entry_hash().to_string(),
        "acknowledged": record.acknowledged(),
    }))
}

/// The names `project.zeno` and `profile.rs` give to reasons and laws.
struct Names {
    reasons: BTreeMap<u32, String>,
    laws: BTreeMap<u32, String>,
}

impl Names {
    fn load() -> Self {
        let reasons = profile::project()
            .reasons()
            .iter()
            .map(|reason| (reason.id().get(), reason.name().as_str().to_owned()))
            .collect();
        let laws = profile::manifest()
            .definitions()
            .iter()
            .map(|law| (law.id().get(), law.name().as_str().to_owned()))
            .collect();
        Self { reasons, laws }
    }

    fn reason(&self, id: u32) -> Value {
        json!({ "id": id, "name": self.reasons.get(&id) })
    }

    /// Each law's status, as the authority's law evaluation reports it.
    fn laws(&self, evaluation: &LawEvaluation) -> Value {
        Value::Array(
            evaluation
                .observations()
                .iter()
                .map(|observation| {
                    let id = observation.law_id().get();
                    let status = match observation.status() {
                        LawStatus::Satisfied => "Satisfied",
                        LawStatus::Violated => "Violated",
                        LawStatus::Indeterminate => "Indeterminate",
                    };
                    json!({ "id": id, "name": self.laws.get(&id), "status": status })
                })
                .collect(),
        )
    }
}

/// The shell at the exact genesis the template's `create` writes.
fn genesis(authority: &Authority) -> Result<Shell, Refusal> {
    let project =
        GeneratedProject::try_new::<RustCryptoSha256>().map_err(refused(Stage::Authority))?;
    let initial = project
        .admit_root::<RustCryptoSha256>(&genesis_state(), ValidationLimits::default())
        .map_err(refused(Stage::Admission))?;
    let genesis = authority
        .authorize_genesis(initial)
        .map_err(refused(Stage::Authority))?;
    AuthorizedShellState::new(authority, genesis).map_err(refused(Stage::Commit))
}

/// The template's authority over an in-memory shell.
pub struct Demo {
    authority: Authority,
    names: Names,
    shell: Option<Shell>,
    /// Decisions executed so far; numbers each invocation's replay identity.
    steps: u64,
    /// Candidates in the order they were published. The reference shell keeps
    /// its records in canonical order, by candidate identity, which is not
    /// the order a reader expects the outbox in.
    committed: Vec<Hash32>,
}

impl Demo {
    /// Builds the authority and a shell at the exact genesis.
    ///
    /// # Errors
    ///
    /// The template's own refusal to build its authority or to authorize its
    /// genesis, rendered as text.
    pub fn new() -> Result<Self, Refusal> {
        let authority = authority().map_err(|message| Refusal::new(Stage::Authority, message))?;
        let shell = genesis(&authority)?;
        Ok(Self {
            authority,
            names: Names::load(),
            shell: Some(shell),
            steps: 0,
            committed: Vec::new(),
        })
    }

    /// The account, the shell's root and counts, every alert in the outbox,
    /// and the identities the shell is pinned to.
    ///
    /// # Errors
    ///
    /// A shell consumed by a failed publication, or a value the generated
    /// bindings cannot read back.
    pub fn state(&self) -> Result<Value, Refusal> {
        let shell = self.shell.as_ref().ok_or_else(consumed)?;
        let reference = shell.reference_state();
        let mut records: Vec<&OutboxRecord> = reference.outbox_records().iter().collect();
        records.sort_by_key(|record| {
            let published = self
                .committed
                .iter()
                .position(|candidate| *candidate == record.candidate_id().hash());
            (published, record.ordinal())
        });
        let outbox = records
            .into_iter()
            .map(outbox_json)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(json!({
            "account": account_json(&account(shell)?),
            "root": reference.root().to_string(),
            "bundles": reference.bundles().len(),
            "outbox": outbox,
            "policy_id": shell.policy_id().to_string(),
            "genesis_id": shell.genesis_id().to_string(),
            "steps": self.steps,
        }))
    }

    /// Decides one request, publishes what commits, and reports the decision:
    /// its kind and reason, each law's status, the account before and after,
    /// the alerts it queued, and its authorization or rejection identity.
    ///
    /// # Errors
    ///
    /// A request the page mis-shaped, a value the schema refuses, an
    /// invocation the authority refuses, or a publication the shell refuses.
    /// The stage names which; none of them changes the account.
    pub fn step(&mut self, input: &str) -> Result<Value, Refusal> {
        let request = parse_request(input)?;
        let shell = self.shell.as_ref().ok_or_else(consumed)?;
        let limits = ValidationLimits::default();
        let project =
            GeneratedProject::try_new::<RustCryptoSha256>().map_err(refused(Stage::Authority))?;
        let before = account(shell)?;
        let root = project
            .admit_root::<RustCryptoSha256>(&before, limits)
            .map_err(refused(Stage::Admission))?;
        let command = project
            .admit_command::<RustCryptoSha256>(&request.command, limits)
            .map_err(refused(Stage::Admission))?;
        let context = project
            .admit_context::<RustCryptoSha256>(&context(request.now, request.admin), limits)
            .map_err(refused(Stage::Admission))?;
        let replay = format!("browser-demo-step-{}", self.steps);
        let witness = self
            .authority
            .admit_invocation(
                root,
                command.admitted().clone(),
                context.admitted().clone(),
                profile::digest(
                    "example/account-lockout/principal",
                    b"browser demo account holder",
                ),
                profile::digest(
                    "example/account-lockout/authentication",
                    b"trusted page input; no remote authentication",
                ),
                profile::digest("example/account-lockout/replay", replay.as_bytes()),
            )
            .map_err(refused(Stage::Authority))?;
        let decision = self
            .authority
            .execute(witness)
            .map_err(refused(Stage::Authority))?;
        self.steps += 1;
        let root_before = shell.reference_state().root().to_string();
        match decision {
            Decision::Reject(rejected) => {
                let reject = rejected.into_reason();
                Ok(json!({
                    "decision": "Reject",
                    "reason": self.names.reason(reject.rejection().reason_id().get()),
                    "laws": self.names.laws(reject.law_evaluation()),
                    "before": account_json(&before),
                    "after": account_json(&before),
                    "roots": { "before": root_before, "after": root_before },
                    "outbox": [],
                    "authorization_id": Value::Null,
                    "rejection_id": reject.rejection_id().to_string(),
                    "commit": Value::Null,
                    "bundles": shell.reference_state().bundles().len(),
                }))
            }
            Decision::Accept(accepted) => {
                self.commit(accepted.into_candidate(), None, &before, &root_before)
            }
            Decision::CommittedFailure(failed) => {
                let (transition, reason) = failed.into_parts();
                self.commit(transition, Some(reason.get()), &before, &root_before)
            }
        }
    }

    /// Publishes an authorized transition, then re-authorizes its canonical
    /// bytes and requires the second publication to be an idempotent replay,
    /// exactly as the template's `invoke` does.
    fn commit(
        &mut self,
        transition: Transition,
        reason: Option<u32>,
        before: &Account,
        root_before: &str,
    ) -> Result<Value, Refusal> {
        let shell = self.shell.take().ok_or_else(consumed)?;
        let candidate_id = transition.body().candidate_id();
        let authorization_id = transition.authorization_id().to_string();
        let laws = self.names.laws(transition.law_evaluation());
        let bytes = transition
            .canonical_bytes()
            .map_err(refused(Stage::Commit))?;
        let published = shell.commit(transition).map_err(refused(Stage::Commit))?;
        if published.status() != CommitStatus::Committed {
            return Err(Refusal::new(Stage::Commit, "expected first publication"));
        }
        let replayed = self
            .authority
            .reauthorize_canonical_transition(&bytes, AuthorizationDecodeLimits::default())
            .map_err(refused(Stage::Commit))?;
        let replayed = published
            .into_state()
            .commit(replayed)
            .map_err(refused(Stage::Commit))?;
        if replayed.status() != CommitStatus::IdempotentReplay {
            return Err(Refusal::new(
                Stage::Commit,
                "exact replay was not idempotent",
            ));
        }
        let shell = replayed.into_state();
        let after = account(&shell)?;
        let reference = shell.reference_state();
        let outbox = reference
            .outbox_records()
            .iter()
            .filter(|record| record.candidate_id() == candidate_id)
            .map(outbox_json)
            .collect::<Result<Vec<_>, _>>()?;
        let report = json!({
            "decision": if reason.is_some() { "CommittedFailure" } else { "Accept" },
            "reason": reason.map(|id| self.names.reason(id)),
            "laws": laws,
            "before": account_json(before),
            "after": account_json(&after),
            "roots": { "before": root_before, "after": reference.root().to_string() },
            "outbox": outbox,
            "authorization_id": authorization_id,
            "rejection_id": Value::Null,
            "commit": { "status": "Committed", "replay": "IdempotentReplay" },
            "bundles": reference.bundles().len(),
        });
        self.shell = Some(shell);
        self.committed.push(candidate_id.hash());
        Ok(report)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// The README's demonstration: the requests `journey` makes, and the
    /// decision each expects.
    const DEMONSTRATION: [(&str, i128, bool, &str); 9] = [
        ("LoginFailed", 1000, false, "CommittedFailure"),
        ("LoginFailed", 1010, false, "CommittedFailure"),
        ("LoginFailed", 1020, false, "CommittedFailure"),
        ("LoginSucceeded", 1500, false, "Reject"),
        ("LoginSucceeded", 1000, false, "Reject"),
        ("AdminUnlock", 1600, false, "Reject"),
        ("LoginSucceeded", 1920, false, "Accept"),
        ("LoginFailed", 2000, false, "CommittedFailure"),
        ("AdminUnlock", 2010, true, "Accept"),
    ];

    fn request(command: &str, now: i128, admin: bool) -> String {
        format!(r#"{{"command":"{command}","now":{now},"admin":{admin}}}"#)
    }

    fn fields(account: &Value) -> (i64, i64, i64) {
        (
            account["failed_attempts"].as_i64().unwrap(),
            account["locked_until"].as_i64().unwrap(),
            account["last_seen"].as_i64().unwrap(),
        )
    }

    fn law_ids(report: &Value) -> Vec<u64> {
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
        let state = Demo::new().unwrap().state().unwrap();
        assert_eq!(fields(&state["account"]), (0, 0, 0));
        assert_eq!(state["bundles"], 0);
        assert_eq!(state["outbox"], json!([]));
        assert_eq!(state["steps"], 0);
    }

    #[test]
    fn the_readme_demonstration_decides_as_the_gate_expects() {
        let mut demo = Demo::new().unwrap();
        for (command, now, admin, expected) in DEMONSTRATION {
            let report = demo.step(&request(command, now, admin)).unwrap();
            assert_eq!(report["decision"], expected, "{command} at {now}");
        }
        let state = demo.state().unwrap();
        assert_eq!(fields(&state["account"]), (0, 0, 2010));
        assert_eq!(state["bundles"], 6);
        let outbox = state["outbox"].as_array().unwrap();
        assert_eq!(
            outbox
                .iter()
                .map(|alert| (
                    alert["alert_kind"].as_str().unwrap(),
                    alert["alert_until"].as_i64().unwrap()
                ))
                .collect::<Vec<_>>(),
            [("Locked", 1920), ("Unlocked", 0)]
        );
        assert!(outbox.iter().all(|alert| alert["acknowledged"] == false));
    }

    #[test]
    fn a_rejection_publishes_nothing_and_names_its_reason() {
        let mut demo = Demo::new().unwrap();
        demo.step(&request("LoginSucceeded", 1000, false)).unwrap();
        let before = demo.state().unwrap();
        let report = demo.step(&request("LoginSucceeded", 999, false)).unwrap();
        assert_eq!(report["decision"], "Reject");
        assert_eq!(
            report["reason"],
            json!({ "id": 200, "name": "clock_regressed" })
        );
        assert_eq!(report["roots"]["before"], report["roots"]["after"]);
        assert_eq!(report["commit"], Value::Null);
        assert!(report["rejection_id"].is_string());
        let after = demo.state().unwrap();
        assert_eq!(after["account"], before["account"]);
        assert_eq!(after["root"], before["root"]);
        assert_eq!(after["bundles"], before["bundles"]);
    }

    #[test]
    fn law_statuses_follow_the_manifest_scopes() {
        let mut demo = Demo::new().unwrap();
        let failure = demo.step(&request("LoginFailed", 1000, false)).unwrap();
        assert_eq!(law_ids(&failure), [500, 503]);
        let accept = demo.step(&request("LoginSucceeded", 1010, false)).unwrap();
        assert_eq!(law_ids(&accept), [500, 501, 502]);
        let reject = demo.step(&request("AdminUnlock", 1020, false)).unwrap();
        assert_eq!(law_ids(&reject), [509]);
        assert_eq!(accept["laws"][1]["name"], "login_clears_failures");
    }

    #[test]
    fn a_malformed_request_is_refused_before_any_decision() {
        let mut demo = Demo::new().unwrap();
        for (input, stage) in [
            ("{", Stage::Input),
            ("[]", Stage::Input),
            (r#"{"command":"Nope","now":1,"admin":false}"#, Stage::Input),
            (
                r#"{"command":"LoginFailed","now":1.5,"admin":false}"#,
                Stage::Input,
            ),
            (
                r#"{"command":"LoginFailed","now":1,"admin":false,"extra":1}"#,
                Stage::Input,
            ),
            (
                r#"{"command":"LoginFailed","now":-1,"admin":false}"#,
                Stage::Admission,
            ),
            (
                r#"{"command":"LoginFailed","now":4102444801,"admin":false}"#,
                Stage::Admission,
            ),
        ] {
            let refusal = demo.step(input).unwrap_err();
            assert_eq!(refusal.stage(), stage, "{input}: {refusal:?}");
            assert_eq!(refusal.to_json()["stage"], stage.label());
        }
        let state = demo.state().unwrap();
        assert_eq!(state["bundles"], 0);
        assert_eq!(state["steps"], 0);
        let report = demo.step(&request("LoginFailed", 0, false)).unwrap();
        assert_eq!(report["decision"], "CommittedFailure");
    }
}
