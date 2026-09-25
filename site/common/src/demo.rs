//! One application, decided by its unchanged authority.
//!
//! A step follows the template's own `invoke`, with the SQLite shell replaced
//! by the library's in-memory [`AuthorizedShellState`]: schema admission
//! through the generated bindings, `admit_invocation`, `execute`, then, for
//! an accept or a committed failure, the commit and the exact replay check.
//! The page supplies every context value, including any time; nothing here
//! reads a clock.

#![forbid(unsafe_code)]

use crate::application::{Application, Authority, Shell, Transition};
use crate::render::Names;
use serde_json::{Value as Json, json};
use std::fmt::Debug;
use zeno_fcis_authority::{AuthorizationDecodeLimits, AuthorizedShellState};
use zeno_fcis_codec::{CanonicalEncode, Domain, Hash32, commitment};
use zeno_fcis_core::Decision;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_shell::{CommitStatus, OutboxRecord};

/// Which check refused a request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stage {
    /// The page's JSON was not a request of this template.
    Input,
    /// The schema refused a value before any decision.
    Admission,
    /// The authority refused to admit or execute the invocation.
    Authority,
    /// The shell refused the publication or its exact replay.
    Commit,
}

impl Stage {
    /// The stage's name in a refusal.
    #[must_use]
    pub const fn label(self) -> &'static str {
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
    /// A refusal at `stage`, with the text the page shows.
    #[must_use]
    pub fn new(stage: Stage, message: impl Into<String>) -> Self {
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
    pub fn to_json(&self) -> Json {
        json!({ "error": self.message, "stage": self.stage.label() })
    }
}

/// Renders an error at `stage` as the templates render their own: with `Debug`.
fn refused<E: Debug>(stage: Stage) -> impl Fn(E) -> Refusal {
    move |error| Refusal::new(stage, format!("{error:?}"))
}

/// A template's own text, already rendered, at `stage`.
fn refused_text(stage: Stage) -> impl Fn(String) -> Refusal {
    move |message| Refusal::new(stage, message)
}

/// A failed publication consumes the shell; only a reset restores one.
fn consumed() -> Refusal {
    Refusal::new(
        Stage::Commit,
        "the shell was consumed by a failed publication; reset the demo",
    )
}

/// What the ABI drives: one demo, whichever application it runs.
pub trait DemoInstance {
    /// Decides one request; see [`Demo::step`].
    ///
    /// # Errors
    ///
    /// A refused request; see [`Demo::step`].
    fn step(&mut self, input: &str) -> Result<Json, Refusal>;

    /// The current state; see [`Demo::state`].
    ///
    /// # Errors
    ///
    /// A consumed shell; see [`Demo::state`].
    fn state(&self) -> Result<Json, Refusal>;
}

/// A template's authority over an in-memory shell.
pub struct Demo<A: Application> {
    authority: Authority<A>,
    names: Names,
    shell: Option<Shell<A>>,
    /// Decisions executed so far; numbers each invocation's replay identity.
    steps: u64,
    /// Candidates in the order they were published. The reference shell keeps
    /// its records in canonical order, by candidate identity, which is not
    /// the order a reader expects the outbox in.
    committed: Vec<Hash32>,
}

impl<A: Application> Demo<A> {
    /// Builds the authority and a shell at the exact genesis.
    ///
    /// # Errors
    ///
    /// The template's own refusal to build its authority, its names, or its
    /// genesis, or the authority's refusal to authorize that genesis.
    pub fn new() -> Result<Self, Refusal> {
        let authority = A::authority().map_err(refused_text(Stage::Authority))?;
        let names = A::names().map_err(refused_text(Stage::Authority))?;
        let genesis = A::genesis().map_err(refused_text(Stage::Admission))?;
        let genesis = authority
            .authorize_genesis(genesis)
            .map_err(refused(Stage::Authority))?;
        let shell =
            AuthorizedShellState::new(&authority, genesis).map_err(refused(Stage::Commit))?;
        Ok(Self {
            authority,
            names,
            shell: Some(shell),
            steps: 0,
            committed: Vec::new(),
        })
    }

    /// A domain-separated commitment under `example/<NAME>/<label>`, as the
    /// template's `profile::digest` makes it.
    fn digest(label: &str, bytes: &[u8]) -> Result<Hash32, Refusal> {
        let label = format!("example/{}/{label}", A::NAME);
        let domain = Domain::new(&label, 1).map_err(refused(Stage::Authority))?;
        commitment::<RustCryptoSha256>(domain, bytes).map_err(refused(Stage::Authority))
    }

    /// One published entry, with its delivery identity and acknowledgement.
    fn outbox_json(&self, record: &OutboxRecord) -> Json {
        let entry = record.entry();
        json!({
            "candidate_id": record.candidate_id().to_string(),
            "ordinal": record.ordinal(),
            "channel": entry.channel(),
            "channel_name": self.names.channel(entry.channel()),
            "destination": self.names.render(entry.destination()),
            "payload": self.names.render(entry.payload()),
            "delivery_id": record.delivery_id().to_string(),
            "entry_hash": record.entry_hash().to_string(),
            "acknowledged": record.acknowledged(),
        })
    }

    /// The state, the shell's root and counts, every entry in the outbox in
    /// the order it was published, and the identities the shell is pinned to.
    ///
    /// # Errors
    ///
    /// A shell consumed by a failed publication.
    pub fn state(&self) -> Result<Json, Refusal> {
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
        Ok(json!({
            "state": self.names.render(reference.state()),
            "root": reference.root().to_string(),
            "bundles": reference.bundles().len(),
            "outbox": records.into_iter().map(|record| self.outbox_json(record)).collect::<Vec<_>>(),
            "policy_id": shell.policy_id().to_string(),
            "genesis_id": shell.genesis_id().to_string(),
            "steps": self.steps,
        }))
    }

    /// Decides one request, publishes what commits, and reports the decision:
    /// its kind and reason, each law's status, the state before and after,
    /// the entries it queued, and its authorization or rejection identity.
    ///
    /// # Errors
    ///
    /// A request the page mis-shaped, a value the schema refuses, an
    /// invocation the authority refuses, or a publication the shell refuses.
    /// The stage names which; none of them changes the state.
    pub fn step(&mut self, input: &str) -> Result<Json, Refusal> {
        let request: Json = serde_json::from_str(input)
            .map_err(|error| Refusal::new(Stage::Input, format!("invalid JSON: {error}")))?;
        let Json::Object(fields) = request else {
            return Err(Refusal::new(Stage::Input, "a request is a JSON object"));
        };
        let (command, context) = A::parse(&fields).map_err(refused_text(Stage::Input))?;
        let shell = self.shell.as_ref().ok_or_else(consumed)?;
        let before = shell.reference_state().state().clone();
        let root = A::admit_state(before.clone()).map_err(refused_text(Stage::Admission))?;
        let (command, context) =
            A::admit(&command, &context).map_err(refused_text(Stage::Admission))?;
        let replay = format!("browser-demo-step-{}", self.steps);
        let witness = self
            .authority
            .admit_invocation(
                root,
                command,
                context,
                Self::digest("principal", b"browser demo principal")?,
                Self::digest(
                    "authentication",
                    b"trusted page input; no remote authentication",
                )?,
                Self::digest("replay", replay.as_bytes())?,
            )
            .map_err(refused(Stage::Authority))?;
        let decision = self
            .authority
            .execute(witness)
            .map_err(refused(Stage::Authority))?;
        self.steps += 1;
        let before = self.names.render(&before);
        let root_before = shell.reference_state().root().to_string();
        match decision {
            Decision::Reject(rejected) => {
                let reject = rejected.into_reason();
                Ok(json!({
                    "decision": "Reject",
                    "reason": self.names.reason(reject.rejection().reason_id().get()),
                    "laws": self.names.laws(reject.law_evaluation()),
                    "before": before,
                    "after": before,
                    "roots": { "before": root_before, "after": root_before },
                    "outbox": [],
                    "authorization_id": Json::Null,
                    "rejection_id": reject.rejection_id().to_string(),
                    "commit": Json::Null,
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
    /// exactly as the templates' `invoke` does.
    fn commit(
        &mut self,
        transition: Transition<A>,
        reason: Option<u32>,
        before: &Json,
        root_before: &str,
    ) -> Result<Json, Refusal> {
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
        let reference = shell.reference_state();
        let outbox: Vec<Json> = reference
            .outbox_records()
            .iter()
            .filter(|record| record.candidate_id() == candidate_id)
            .map(|record| self.outbox_json(record))
            .collect();
        let report = json!({
            "decision": if reason.is_some() { "CommittedFailure" } else { "Accept" },
            "reason": reason.map(|id| self.names.reason(id)),
            "laws": laws,
            "before": before,
            "after": self.names.render(reference.state()),
            "roots": { "before": root_before, "after": reference.root().to_string() },
            "outbox": outbox,
            "authorization_id": authorization_id,
            "rejection_id": Json::Null,
            "commit": { "status": "Committed", "replay": "IdempotentReplay" },
            "bundles": reference.bundles().len(),
        });
        self.shell = Some(shell);
        self.committed.push(candidate_id.hash());
        Ok(report)
    }
}

impl<A: Application> DemoInstance for Demo<A> {
    fn step(&mut self, input: &str) -> Result<Json, Refusal> {
        Self::step(self, input)
    }

    fn state(&self) -> Result<Json, Refusal> {
        Self::state(self)
    }
}
