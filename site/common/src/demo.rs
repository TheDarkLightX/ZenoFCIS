//! Browser-owned state updated only from genuine V2 publications.
//!
//! Every invocation binds the template's checked contract and replays its
//! exact original inputs and subject. The browser shell is ephemeral: it
//! provides neither durable storage, authentication nor external delivery.

#![forbid(unsafe_code)]

use crate::application::Application;
use crate::render::Names;
use serde_json::{Value as Json, json};
use std::fmt::Debug;
use std::marker::PhantomData;
use zeno_fcis_authority::{Publication, PublicationOutcome, WireDelivery};
use zeno_fcis_codec::{
    CanonicalEncode, DecodeLimits, Domain, Hash32, commitment, decode_envelope, decode_value,
};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_synthesis::finite::v2_composition::{Class, Kind, Raw};
use zeno_fcis_value::Value;

/// Which check refused a request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stage {
    /// The page's JSON was not a request of this template.
    Input,
    /// The schema refused a value before any decision.
    Admission,
    /// The authority refused the invocation or publication.
    Authority,
    /// The browser shell refused publication or its exact replay.
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
/// A refused request; browser state has not changed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Refusal {
    stage: Stage,
    message: String,
}
impl Refusal {
    /// A refusal at the indicated stage.
    #[must_use]
    pub fn new(stage: Stage, message: impl Into<String>) -> Self {
        Self {
            stage,
            message: message.into(),
        }
    }
    /// The check that refused the request.
    #[must_use]
    pub const fn stage(&self) -> Stage {
        self.stage
    }
    /// The page's refusal report.
    #[must_use]
    pub fn to_json(&self) -> Json {
        json!({"error":self.message,"stage":self.stage.label()})
    }
}
fn refused<E: Debug>(stage: Stage) -> impl Fn(E) -> Refusal {
    move |error| Refusal::new(stage, format!("{error:?}"))
}
fn refused_text(stage: Stage) -> impl Fn(String) -> Refusal {
    move |message| Refusal::new(stage, message)
}
/// The bounded C ABI drives any one template's demo.
pub trait DemoInstance {
    /// Decide a request and publish its committing result.
    ///
    /// # Errors
    /// Returns an input, admission, authority or commit refusal.
    fn step(&mut self, input: &str) -> Result<Json, Refusal>;
    /// Read the browser shell's current state.
    ///
    /// # Errors
    /// Returns a malformed retained-envelope refusal.
    fn state(&self) -> Result<Json, Refusal>;
}
/// One ephemeral shell, cloned to stage an atomic first commit and replay.
#[derive(Clone)]
struct BrowserState {
    identity: Vec<u8>,
    state: Vec<u8>,
    root: Hash32,
    subjects: Vec<(Hash32, Vec<u8>)>,
    outbox: Vec<Json>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Commit {
    Committed,
    IdempotentReplay,
}
/// A template's genuine V2 publications over browser-owned state.
pub struct Demo<A: Application> {
    names: Names,
    shell: BrowserState,
    policy_id: Hash32,
    genesis_id: Hash32,
    steps: u64,
    application: PhantomData<A>,
}
impl<A: Application> Demo<A> {
    /// Check the exact template genesis before installing browser state.
    ///
    /// # Errors
    /// Returns a template binding, genesis or encoding refusal.
    pub fn new() -> Result<Self, Refusal> {
        let names = A::names().map_err(refused_text(Stage::Authority))?;
        let initial = A::genesis().map_err(refused_text(Stage::Admission))?;
        let initial = initial
            .envelope()
            .canonical_bytes()
            .map_err(refused(Stage::Admission))?;
        let (identity, state, genesis_id) =
            A::with_authority(|authority| match authority.publish_genesis(&initial) {
                PublicationOutcome::Commit(publication) => {
                    if publication.evaluation().kind() != Kind::Genesis
                        || publication.poststate() != initial
                    {
                        return Err(Refusal::new(
                            Stage::Authority,
                            "genesis changed the initial state",
                        ));
                    }
                    Ok((
                        authority.identity().to_vec(),
                        publication.poststate().to_vec(),
                        Self::digest("genesis", publication.subject())?,
                    ))
                }
                other => Err(Refusal::new(
                    Stage::Authority,
                    format!("genesis: {other:?}"),
                )),
            })
            .map_err(refused_text(Stage::Authority))??;
        decode_envelope(&state, DecodeLimits::default()).map_err(refused(Stage::Commit))?;
        let policy_id = Self::digest("policy", &identity)?;
        let root = Self::digest("state", &state)?;
        Ok(Self {
            names,
            shell: BrowserState {
                identity,
                state,
                root,
                subjects: Vec::new(),
                outbox: Vec::new(),
            },
            policy_id,
            genesis_id,
            steps: 0,
            application: PhantomData,
        })
    }
    fn digest(label: &str, bytes: &[u8]) -> Result<Hash32, Refusal> {
        let name = format!("example/{}/{label}", A::NAME);
        let domain = Domain::new(&name, 1).map_err(refused(Stage::Authority))?;
        commitment::<RustCryptoSha256>(domain, bytes).map_err(refused(Stage::Authority))
    }
    fn render_state(&self, state: &[u8]) -> Result<Json, Refusal> {
        let envelope =
            decode_envelope(state, DecodeLimits::default()).map_err(refused(Stage::Commit))?;
        Ok(self.names.render(&envelope.into_value()))
    }
    fn outbox_json(&self, candidate: Hash32, delivery: &WireDelivery) -> Result<Json, Refusal> {
        let destination = decode_value(delivery.destination(), DecodeLimits::default())
            .map_err(refused(Stage::Commit))?;
        let payload = decode_value(delivery.payload(), DecodeLimits::default())
            .map_err(refused(Stage::Commit))?;
        let entry = Value::tuple(vec![
            Value::unsigned(u128::from(delivery.ordinal())),
            Value::unsigned(u128::from(delivery.channel())),
            Value::unsigned(u128::from(delivery.destination_root())),
            Value::unsigned(u128::from(delivery.payload_root())),
            Value::bytes(delivery.destination().to_vec()).map_err(refused(Stage::Commit))?,
            Value::bytes(delivery.payload().to_vec()).map_err(refused(Stage::Commit))?,
            Value::bytes(delivery.idempotency().to_vec()).map_err(refused(Stage::Commit))?,
        ])
        .map_err(refused(Stage::Commit))?;
        let entry_hash = Self::digest(
            "entry",
            &entry.canonical_bytes().map_err(refused(Stage::Commit))?,
        )?;
        let mut delivery_bytes = candidate.as_bytes().to_vec();
        delivery_bytes.extend_from_slice(entry_hash.as_bytes());
        let delivery_id = Self::digest("delivery", &delivery_bytes)?;
        Ok(
            json!({"candidate_id":candidate.to_string(),"ordinal":delivery.ordinal(),"channel":delivery.channel(),
            "channel_name":self.names.channel(delivery.channel()),"destination":self.names.render(&destination),"payload":self.names.render(&payload),
            "delivery_id":delivery_id.to_string(),"entry_hash":entry_hash.to_string(),"acknowledged":false}),
        )
    }
    /// Consume a genuine capability. Replay is checked before comparing the
    /// current state because its original state predates the first commit.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "The shell consumes ownership of each non-Clone publication capability."
    )]
    fn commit(
        &self,
        shell: &mut BrowserState,
        replay: Hash32,
        publication: Publication<'_>,
    ) -> Result<(Commit, Vec<Json>), Refusal> {
        if publication.identity() != shell.identity
            || publication.evaluation().kind() != Kind::Transition
        {
            return Err(Refusal::new(
                Stage::Commit,
                "publication identity or kind mismatch",
            ));
        }
        if let Some((_, subject)) = shell.subjects.iter().find(|(key, _)| *key == replay) {
            return if subject == publication.subject() {
                Ok((Commit::IdempotentReplay, Vec::new()))
            } else {
                Err(Refusal::new(
                    Stage::Commit,
                    "replay key refers to a different publication",
                ))
            };
        }
        if publication.evaluation().raw().state != shell.state {
            return Err(Refusal::new(Stage::Commit, "publication prestate mismatch"));
        }
        let id = Self::publication_id(replay, publication.subject())?;
        let post = publication.poststate().to_vec();
        self.render_state(&post)?;
        let root = Self::digest("state", &post)?;
        let outbox = publication
            .outbox()
            .iter()
            .map(|delivery| self.outbox_json(id, delivery))
            .collect::<Result<Vec<_>, _>>()?;
        // All fallible validation and encoding precedes publication into
        // the staging shell.
        let subject = publication.subject().to_vec();
        shell.state = post;
        shell.root = root;
        shell.subjects.push((replay, subject));
        shell.outbox.extend(outbox.clone());
        Ok((Commit::Committed, outbox))
    }
    fn publication_id(replay: Hash32, subject: &[u8]) -> Result<Hash32, Refusal> {
        let bytes = Value::tuple(vec![
            Value::bytes(replay.as_bytes().to_vec()).map_err(refused(Stage::Commit))?,
            Value::bytes(subject.to_vec()).map_err(refused(Stage::Commit))?,
        ])
        .map_err(refused(Stage::Commit))?
        .canonical_bytes()
        .map_err(refused(Stage::Commit))?;
        Self::digest("publication", &bytes)
    }
    /// Read the state, local history counts and queued entries.
    ///
    /// # Errors
    /// Returns a malformed retained-state envelope refusal.
    pub fn state(&self) -> Result<Json, Refusal> {
        Ok(
            json!({"state":self.render_state(&self.shell.state)?,"root":self.shell.root.to_string(),"bundles":self.shell.subjects.len(),
            "outbox":self.shell.outbox,"policy_id":self.policy_id.to_string(),"genesis_id":self.genesis_id.to_string(),"steps":self.steps}),
        )
    }
    /// Publish actual original inputs and replay the same capability's subject.
    /// All fallible work precedes replacement of the browser shell.
    ///
    /// # Errors
    /// Returns a refusal without changing browser state, history or outbox.
    pub fn step(&mut self, input: &str) -> Result<Json, Refusal> {
        let request: Json = serde_json::from_str(input)
            .map_err(|error| Refusal::new(Stage::Input, format!("invalid JSON: {error}")))?;
        let Json::Object(fields) = request else {
            return Err(Refusal::new(Stage::Input, "a request is a JSON object"));
        };
        let (command, context) = A::parse(&fields).map_err(refused_text(Stage::Input))?;
        let (command, context) =
            A::admit(&command, &context).map_err(refused_text(Stage::Admission))?;
        let command = command
            .envelope()
            .canonical_bytes()
            .map_err(refused(Stage::Admission))?;
        let context = context
            .envelope()
            .canonical_bytes()
            .map_err(refused(Stage::Admission))?;
        let before = self.render_state(&self.shell.state)?;
        let next_step = self
            .steps
            .checked_add(1)
            .ok_or_else(|| Refusal::new(Stage::Commit, "step counter overflow"))?;
        let replay = Self::digest("replay", &self.steps.to_be_bytes())?;
        let (report, staged) = A::with_authority(|authority| {
            if authority.identity() != self.shell.identity { return Err(Refusal::new(Stage::Authority,"authority identity changed")); }
            let raw = Raw { state: &self.shell.state, command: &command, context: &context };
            match authority.publish(raw) {
                PublicationOutcome::Reject(evaluation) => {
                    let candidate = evaluation.result().map_err(refused(Stage::Authority))?;
                    if candidate.class() != Class::Reject { return Err(Refusal::new(Stage::Authority,"unexpected rejection class")); }
                    let subject = evaluation.subject().map_err(refused(Stage::Authority))?;
                    let rejection = Self::digest("rejection", subject)?;
                    Ok((json!({"decision":"Reject","reason":candidate.reason().map(|id|self.names.reason(id)),
                        "laws":self.names.laws(evaluation.diagnostics()),"before":before,"after":before,
                        "roots":{"before":self.shell.root.to_string(),"after":self.shell.root.to_string()},"outbox":[],
                        "authorization_id":Json::Null,"rejection_id":rejection.to_string(),"commit":Json::Null,"bundles":self.shell.subjects.len()}),None))
                }
                PublicationOutcome::Commit(publication) => {
                    let candidate = publication.evaluation().result().map_err(refused(Stage::Authority))?;
                    let decision = match candidate.class() { Class::Accept => "Accept", Class::CommittedFailure => "CommittedFailure",
                        _ => return Err(Refusal::new(Stage::Authority,"unexpected committing class")) };
                    let reason = candidate.reason().map(|id|self.names.reason(id));
                    let laws = self.names.laws(publication.evaluation().diagnostics());
                    let subject = publication.subject().to_vec();
                    let id = Self::publication_id(replay, &subject)?;
                    let mut staged = self.shell.clone();
                    let (status,outbox) = self.commit(&mut staged,replay,publication)?;
                    if status != Commit::Committed { return Err(Refusal::new(Stage::Commit,"expected first publication")); }
                    let replayed = match authority.replay_publication(raw,&subject) {
                        PublicationOutcome::Commit(replayed) => replayed,
                        other => return Err(Refusal::new(Stage::Commit,format!("exact replay: {other:?}"))),
                    };
                    let committed_state = staged.state.clone();
                    let committed_root = staged.root;
                    let committed_counts = (staged.subjects.len(),staged.outbox.len());
                    let (status,repeated_entries) = self.commit(&mut staged,replay,replayed)?;
                    if status != Commit::IdempotentReplay || !repeated_entries.is_empty() || staged.state != committed_state || staged.root != committed_root
                        || (staged.subjects.len(),staged.outbox.len()) != committed_counts {
                        return Err(Refusal::new(Stage::Commit,"exact replay was not idempotent"));
                    }
                    let report = json!({"decision":decision,"reason":reason,"laws":laws,"before":before,"after":self.render_state(&staged.state)?,
                        "roots":{"before":self.shell.root.to_string(),"after":staged.root.to_string()},"outbox":outbox,
                        "authorization_id":id.to_string(),"rejection_id":Json::Null,"commit":{"status":"Committed","replay":"IdempotentReplay"},"bundles":staged.subjects.len()});
                    Ok((report,Some(staged)))
                }
                other => Err(Refusal::new(Stage::Authority,format!("publication: {other:?}"))),
            }
        }).map_err(refused_text(Stage::Authority))??;
        if let Some(shell) = staged {
            self.shell = shell;
        }
        self.steps = next_step;
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
