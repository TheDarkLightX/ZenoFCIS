//! One persistent session: genuine genesis, publication, exact replay and
//! outbox delivery through the library Authority and the SQLite v2 shell; and
//! the operations on an existing store along the contract lineage: audit,
//! delivery, checked upgrade and the explicit schema migration.

use std::path::Path;

use zeno_fcis_codec::{Domain, Hash32, commitment};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_shell::CommitStatus;
use zeno_fcis_shell_sqlite::{
    MemoryDestination,
    v2::{Error, Lineage, Opened, Snapshot, Store, V2SqliteShell, upgrade},
};
use zeno_fcis_synthesis::finite::{
    v2_authority::{Authority, PublicationOutcome},
    v2_catalog::BoundCatalog,
    v2_composition as c,
};

use crate::{AppResult, Example, authority, compare, genesis, genesis_numbers, v2_contract, wire};

/// The audited head of a store.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Head {
    /// The contract version the store runs: its position in the lineage.
    pub contract_version: usize,
    /// Committed transitions after genesis.
    pub commits: u64,
    /// Outbox entries still pending.
    pub pending: u64,
    /// Recorded contract upgrades.
    pub upgrades: u64,
}

impl Head {
    fn of(snapshot: &Snapshot, contract_version: usize) -> Self {
        Self {
            contract_version,
            commits: snapshot.version(),
            pending: snapshot.pending(),
            upgrades: snapshot.upgrades(),
        }
    }

    /// One JSON object.
    #[must_use]
    pub fn json(&self) -> String {
        format!(
            "{{\"status\":\"audited\",\"contract_version\":{},\"commits\":{},\"pending\":{},\"upgrades\":{}}}",
            self.contract_version, self.commits, self.pending, self.upgrades
        )
    }
}

/// A recorded contract upgrade.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Upgraded {
    /// How the new version admitted the store's state: `program-successor`
    /// or `genesis-admission`.
    pub kind: &'static str,
    /// For a program successor, the premises the store's record states, in
    /// addition to the policy comparison that makes it one.
    pub premises: Option<upgrade::Premises>,
    /// Position among the store's upgrades, from 1.
    pub ordinal: u64,
    /// The commit position the upgrade was recorded at.
    pub at_commit: u64,
    /// The hash chain tip after the upgrade, in hexadecimal.
    pub chain: String,
    /// The SHA-256 of each adoption receipt the record binds, in hexadecimal.
    pub receipts: Vec<String>,
    /// The contract version the store runs now.
    pub contract_version: usize,
    /// Outbox entries still pending, or `None` when the store could not be
    /// read again after the upgrade committed.
    pub pending: Option<u64>,
}

impl Upgraded {
    /// One JSON object.
    #[must_use]
    pub fn json(&self) -> String {
        let receipts: Vec<String> = self
            .receipts
            .iter()
            .map(|receipt| format!("\"{receipt}\""))
            .collect();
        let premises = self.premises.map_or_else(
            || "null".to_owned(),
            |premises| {
                format!(
                    "{{\"policy_differs_only_in_program_and_step_limit\":true,\"step_limits_never_bind\":{},\"step_usage_unobserved\":{}}}",
                    premises.step_limits_never_bind, premises.step_usage_unobserved
                )
            },
        );
        format!(
            "{{\"status\":\"upgraded\",\"kind\":\"{}\",\"premises\":{premises},\"ordinal\":{},\"at_commit\":{},\"chain\":\"{}\",\"receipts\":[{}],\"contract_version\":{},\"commits\":{},\"pending\":{},\"upgrades\":{}}}",
            self.kind,
            self.ordinal,
            self.at_commit,
            self.chain,
            receipts.join(","),
            self.contract_version,
            self.at_commit,
            self.pending
                .map_or_else(|| "null".to_owned(), |pending| pending.to_string()),
            self.ordinal
        )
    }
}

/// The outbox entries a delivery run sent, and the head after it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Delivered {
    /// Each delivered entry's ID, in hexadecimal, in delivery order.
    pub deliveries: Vec<String>,
    /// The audited head after the run.
    pub head: Head,
}

impl Delivered {
    /// One JSON object.
    #[must_use]
    pub fn json(&self) -> String {
        let deliveries: Vec<String> = self
            .deliveries
            .iter()
            .map(|id| format!("\"{id}\""))
            .collect();
        format!(
            "{{\"status\":\"delivered\",\"deliveries\":[{}],\"contract_version\":{},\"commits\":{},\"pending\":{},\"upgrades\":{}}}",
            deliveries.join(","),
            self.head.contract_version,
            self.head.commits,
            self.head.pending,
            self.head.upgrades
        )
    }
}

fn store(error: Error) -> String {
    format!("store: {error} ({error:?})")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// A receipt digest as the generated contract writes it.
fn digest(text: &str) -> AppResult<[u8; 32]> {
    let invalid = || format!("adoption receipt digest `{text}` is not 64 hexadecimal digits");
    if text.len() != 64 || !text.is_ascii() {
        return Err(invalid());
    }
    let mut bytes = [0; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[2 * index..2 * index + 2], 16).map_err(|_| invalid())?;
    }
    Ok(bytes)
}

/// What the generated `with_lineage` passes: the checked catalogs alone
/// while the contract has no adoption, then also each adoption's receipt
/// digest. Every application shares this source, before and after it adopts.
trait Generated<'a, 'p> {
    fn parts(self) -> (&'a [&'a BoundCatalog<'p>], &'a [&'a str]);
}

impl<'a, 'b: 'a, 'p> Generated<'a, 'p> for &'a [&'b BoundCatalog<'p>] {
    fn parts(self) -> (&'a [&'a BoundCatalog<'p>], &'a [&'a str]) {
        (self, &[])
    }
}

impl<'a, 'b: 'a, 'p> Generated<'a, 'p> for (&'a [&'b BoundCatalog<'p>], &'a [&'a str]) {
    fn parts(self) -> (&'a [&'a BoundCatalog<'p>], &'a [&'a str]) {
        self
    }
}

/// Binds every version's catalog and Authority, oldest first, with the
/// adoptions' receipt digests, for `f`.
fn with_lineage<R>(f: impl FnOnce(&Lineage<'_, '_>) -> AppResult<R>) -> AppResult<R> {
    v2_contract::with_lineage(|generated| {
        let (catalogs, receipts) = generated.parts();
        let receipts = receipts
            .iter()
            .map(|receipt| digest(receipt))
            .collect::<AppResult<Vec<_>>>()?;
        let lineage = Lineage::bind(catalogs, &receipts).map_err(store)?;
        f(&lineage)
    })
    .map_err(|error| format!("catalog: {error:?}"))?
}

/// Replays every history segment of the store at `path` under the contract
/// version that published it, and reports the audited head. A store that
/// runs an earlier version of this application is audited under the versions
/// up to its own.
///
/// # Errors
/// Returns the store's refusal: a segment under no version of this
/// application, a schema that needs `--migrate`, or failed replay.
pub fn audit(path: &Path) -> AppResult<Head> {
    with_lineage(|lineage| match lineage.open(path).map_err(store)? {
        Opened::V10(Store::Current(mut shell)) => {
            shell.audit().map_err(store)?;
            Ok(Head::of(
                &shell.snapshot().map_err(store)?,
                lineage.versions(),
            ))
        }
        Opened::V10(Store::Superseded(mut superseded)) => {
            superseded.audit().map_err(store)?;
            let version = superseded.version();
            Ok(Head::of(&superseded.snapshot().map_err(store)?, version))
        }
        Opened::V9(_) => Err(store(Error::Schema(9))),
    })
}

/// Records the checked upgrade of the store at `path` to this application's
/// contract version. A version adopted from the store's version admits any
/// state; another contract must admit the current state through its genesis
/// laws.
///
/// # Errors
/// Returns the store's refusal: a store already at this version, a
/// different state schema, the new contract's genesis laws refusing the
/// current state, a schema that needs `--migrate`, or failed replay. A
/// refusal writes nothing.
pub fn upgrade(path: &Path) -> AppResult<Upgraded> {
    with_lineage(|lineage| match lineage.open(path).map_err(store)? {
        Opened::V10(Store::Superseded(superseded)) => {
            let (mut shell, receipt) = superseded.upgrade().map_err(store)?;
            // The upgrade has committed; a failure to read the store again
            // is reported, not turned into a refusal.
            let pending = shell.snapshot().ok().map(|snapshot| snapshot.pending());
            Ok(Upgraded {
                kind: receipt.kind().tag(),
                premises: receipt.premises(),
                ordinal: receipt.ordinal(),
                at_commit: receipt.version(),
                chain: hex(receipt.chain().as_bytes()),
                receipts: receipt
                    .receipts()
                    .iter()
                    .map(|digest| hex(digest.as_bytes()))
                    .collect(),
                contract_version: lineage.versions(),
                pending,
            })
        }
        Opened::V10(Store::Current(_)) => {
            Err(store(Error::Upgrade(upgrade::Refusal::SameContract)))
        }
        Opened::V9(_) => Err(store(Error::Schema(9))),
    })
}

/// Delivers every pending outbox entry of the store at `path`, which must run
/// this application's contract version, oldest first, and acknowledges each.
/// Entries from before an upgrade keep the IDs their commits bound.
///
/// # Errors
/// Returns the store's refusal, including a store that runs an earlier
/// version and must be upgraded first.
pub fn deliver(path: &Path) -> AppResult<Delivered> {
    with_lineage(|lineage| match lineage.open(path).map_err(store)? {
        Opened::V10(Store::Current(mut shell)) => {
            let mut destination = MemoryDestination::default();
            let mut deliveries = Vec::new();
            while let Some((id, observed)) = shell
                .deliver_next_memory_unacknowledged(&mut destination)
                .map_err(store)?
            {
                shell.acknowledge(id, observed).map_err(store)?;
                deliveries.push(hex(id.as_bytes()));
            }
            Ok(Delivered {
                deliveries,
                head: Head::of(&shell.snapshot().map_err(store)?, lineage.versions()),
            })
        }
        Opened::V10(Store::Superseded(superseded)) => Err(format!(
            "store: runs contract version {}; upgrade it to version {} before delivering",
            superseded.version(),
            lineage.versions()
        )),
        Opened::V9(_) => Err(store(Error::Schema(9))),
    })
}

/// Converts a schema v9 store, created before upgrades were recorded, to the
/// current schema after a complete audit under the version that created it.
///
/// # Errors
/// Returns the store's refusal: not exactly a v9 store, or a store under no
/// version of this application. A refusal writes nothing.
pub fn migrate(path: &Path) -> AppResult<Head> {
    with_lineage(|lineage| match lineage.open(path).map_err(store)? {
        Opened::V9(old) => match old.migrate().map_err(store)? {
            Store::Current(mut shell) => Ok(Head::of(
                &shell.snapshot().map_err(store)?,
                lineage.versions(),
            )),
            Store::Superseded(mut superseded) => {
                let version = superseded.version();
                Ok(Head::of(&superseded.snapshot().map_err(store)?, version))
            }
        },
        Opened::V10(_) => Err(store(Error::Schema(10))),
    })
}

/// What a session did.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Summary {
    /// Each decision's class, in order.
    pub decisions: Vec<&'static str>,
    /// Committed decisions in the store; genesis is not counted.
    pub bundles: u64,
    /// Outbox entries still pending.
    pub pending: u64,
    /// Outbox entries the destination received.
    pub deliveries: usize,
}

impl Summary {
    /// One JSON object.
    #[must_use]
    pub fn json(&self) -> String {
        let decisions: Vec<String> = self
            .decisions
            .iter()
            .map(|decision| format!("\"{decision}\""))
            .collect();
        format!(
            "{{\"status\":\"passed\",\"decisions\":[{}],\"bundles\":{},\"pending\":{},\"deliveries\":{}}}",
            decisions.join(","),
            self.bundles,
            self.pending,
            self.deliveries
        )
    }
}

fn replay_id(example: &Example) -> AppResult<Hash32> {
    replay_key(format!("example-{}", example.line).as_bytes())
}

/// The replay key of an example decided on an existing store after
/// `commits` commits: a key of its own, whichever keys the store holds.
fn resumed_replay_id(example: &Example, commits: u64) -> AppResult<Hash32> {
    replay_key(format!("example-{}-after-{commits}", example.line).as_bytes())
}

fn replay_key(message: &[u8]) -> AppResult<Hash32> {
    let domain = Domain::new(concat!(env!("CARGO_PKG_NAME"), "/replay"), 1)
        .map_err(|error| format!("replay domain: {error:?}"))?;
    commitment::<RustCryptoSha256>(domain, message).map_err(|error| format!("replay id: {error:?}"))
}

fn class_name(class: c::Class) -> &'static str {
    match class {
        c::Class::Accept => "Accept",
        c::Class::Reject => "Reject",
        c::Class::CommittedFailure => "CommittedFailure",
        _ => "Unsupported",
    }
}

/// Publishes genesis into a new database at `path`, then takes, while one
/// remains, the first example whose pre-state is the current state:
/// rejects must publish nothing, and each other decision is committed, then
/// recomputed from the same inputs and committed again as an idempotent
/// replay. Returns the store and each decision's class.
fn session<'a, 'p>(
    path: &Path,
    authority: &'a Authority<'p>,
    examples: &[Example],
) -> AppResult<(V2SqliteShell<'a, 'p>, Vec<&'static str>)> {
    let initial = genesis()?;
    let genesis = match authority.publish_genesis(&initial) {
        PublicationOutcome::Commit(publication) => publication,
        other => return Err(format!("genesis: {other:?}")),
    };
    let mut shell = V2SqliteShell::create(path, authority, genesis).map_err(store)?;
    let mut state = genesis_numbers()?;
    let mut remaining: Vec<&Example> = examples.iter().collect();
    let mut decisions = Vec::new();
    while let Some(position) = remaining
        .iter()
        .position(|example| example.inputs.get(..state.len()) == Some(state.as_slice()))
    {
        let example = remaining.remove(position);
        if decide_example(&mut shell, authority, example, replay_id(example)?)? {
            state.clone_from(&example.post);
        }
        decisions.push(class_name(example.class));
    }
    Ok((shell, decisions))
}

/// Decides one example on the store, whose state must be the example's
/// pre-state: a reject must publish nothing, and another decision is
/// committed under `replay`, then recomputed from the same inputs and
/// committed again as an idempotent replay. Returns whether it committed.
fn decide_example(
    shell: &mut V2SqliteShell<'_, '_>,
    authority: &Authority<'_>,
    example: &Example,
    replay: Hash32,
) -> AppResult<bool> {
    let descriptor = authority.descriptor();
    let raw = wire(descriptor, &example.inputs)?;
    let original = c::Raw {
        state: &raw[0],
        command: &raw[1],
        context: &raw[2],
    };
    let before = shell.snapshot().map_err(store)?;
    if before.state() != original.state {
        return Err(format!("line {}: the stored state differs", example.line));
    }
    match authority.publish(original) {
        PublicationOutcome::Commit(publication) => {
            let candidate = publication
                .evaluation()
                .result()
                .map_err(|refusal| format!("line {}: {refusal:?}", example.line))?;
            compare(descriptor, candidate, example)?;
            let subject = publication.subject().to_vec();
            if shell.commit(replay, publication).map_err(store)?.status() != CommitStatus::Committed
            {
                return Err(format!("line {}: not a first publication", example.line));
            }
            let replayed = match authority.replay_publication(original, &subject) {
                PublicationOutcome::Commit(publication) => publication,
                other => return Err(format!("line {}: replay: {other:?}", example.line)),
            };
            if shell.commit(replay, replayed).map_err(store)?.status()
                != CommitStatus::IdempotentReplay
            {
                return Err(format!("line {}: replay was not idempotent", example.line));
            }
            Ok(true)
        }
        PublicationOutcome::Reject(evaluation) => {
            let candidate = evaluation
                .result()
                .map_err(|refusal| format!("line {}: {refusal:?}", example.line))?;
            compare(descriptor, candidate, example)?;
            if shell.snapshot().map_err(store)? != before {
                return Err(format!("line {}: a reject changed the store", example.line));
            }
            Ok(false)
        }
        other => Err(format!("line {}: {other:?}", example.line)),
    }
}

/// Runs the examples as one session in a new database at `path`, then
/// delivers the outbox, the first entry across a reopen of the database
/// before its acknowledgment.
///
/// # Errors
/// Returns the first refusal, difference or store failure.
pub fn journey(path: &Path, examples: &[Example]) -> AppResult<Summary> {
    let contract = v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = authority(&descriptor)?;
    let (mut shell, decisions) = session(path, &authority, examples)?;
    let mut destination = MemoryDestination::default();
    if let Some(pending) = shell.next_pending().map_err(store)? {
        // An interruption after the destination records the first delivery
        // but before the store acknowledges it.
        let delivered = shell
            .deliver_next_memory_unacknowledged(&mut destination)
            .map_err(store)?;
        if delivered != Some((pending.delivery_id(), pending.entry_hash())) {
            return Err("the destination received a different entry".to_owned());
        }
        drop(shell);
        shell = V2SqliteShell::open(path, &authority).map_err(store)?;
        while shell.deliver_next_memory(&mut destination).map_err(store)? {}
        shell
            .acknowledge(pending.delivery_id(), pending.entry_hash())
            .map_err(store)?;
    }
    let snapshot = shell.snapshot().map_err(store)?;
    Ok(Summary {
        decisions,
        bundles: snapshot.bundle_count(),
        pending: snapshot.pending(),
        deliveries: destination.delivered_count(),
    })
}

/// Runs the examples as one session and leaves every outbox entry pending,
/// for `--deliver` to send later. Without a file at `path`, the session
/// starts at genesis in a new database there. An existing store must run
/// this build's version, after `--upgrade` when it ran an earlier one; the
/// session then continues from the store's state: while one remains, the
/// first example whose pre-state is that state is decided, a commit and its
/// replay recorded under a key of its own.
///
/// # Errors
/// Returns the first refusal, difference or store failure, or a store at
/// an earlier version.
pub fn decide(path: &Path, examples: &[Example]) -> AppResult<Summary> {
    if path
        .try_exists()
        .map_err(|error| format!("{}: {error}", path.display()))?
    {
        return resume(path, examples);
    }
    let contract = v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = authority(&descriptor)?;
    let (mut shell, decisions) = session(path, &authority, examples)?;
    let snapshot = shell.snapshot().map_err(store)?;
    Ok(Summary {
        decisions,
        bundles: snapshot.bundle_count(),
        pending: snapshot.pending(),
        deliveries: 0,
    })
}

/// Continues a session on the existing store at `path`, which runs this
/// build's version.
fn resume(path: &Path, examples: &[Example]) -> AppResult<Summary> {
    with_lineage(|lineage| match lineage.open(path).map_err(store)? {
        Opened::V10(Store::Current(mut shell)) => {
            let authority = shell.authority();
            let mut remaining: Vec<&Example> = examples.iter().collect();
            let mut decisions = Vec::new();
            loop {
                let snapshot = shell.snapshot().map_err(store)?;
                let mut next = None;
                for (position, example) in remaining.iter().enumerate() {
                    if wire(authority.descriptor(), &example.inputs)?[0] == snapshot.state() {
                        next = Some(position);
                        break;
                    }
                }
                let Some(position) = next else { break };
                let example = remaining.remove(position);
                let replay = resumed_replay_id(example, snapshot.version())?;
                decide_example(&mut shell, authority, example, replay)?;
                decisions.push(class_name(example.class));
            }
            let snapshot = shell.snapshot().map_err(store)?;
            Ok(Summary {
                decisions,
                bundles: snapshot.bundle_count(),
                pending: snapshot.pending(),
                deliveries: 0,
            })
        }
        Opened::V10(Store::Superseded(superseded)) => Err(format!(
            "store: runs contract version {}; upgrade it to version {} before deciding",
            superseded.version(),
            lineage.versions()
        )),
        Opened::V9(_) => Err(store(Error::Schema(9))),
    })
}
