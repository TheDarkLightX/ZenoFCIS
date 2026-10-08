//! One persistent session: genuine genesis, publication, exact replay and
//! outbox delivery through the library Authority and the SQLite v2 shell; and
//! the operations on an existing store along the contract lineage: audit,
//! delivery, checked upgrade and the explicit schema migration.

use std::io::{Read as _, Write as _};
use std::path::Path;

use zeno_fcis_codec::{DecodeLimits, Domain, Hash32, commitment, decode_envelope};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_shell::CommitStatus;
use zeno_fcis_shell_sqlite::{
    MemoryDestination,
    v2::{
        Error, Lineage, Opened, Snapshot, Step, Store, V2SqliteShell, behaviour, migration, upgrade,
    },
};
use zeno_fcis_synthesis::finite::{
    v2_authority::{Authority, PublicationOutcome},
    v2_catalog::BoundCatalog,
    v2_composition as c, v2_laws as l,
};

use zeno_fcis_value::ValueRef;

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
    /// How the new version admitted the store's state: `program-successor`,
    /// `genesis-admission`, `behaviour-change`, `migration` or `rename`. An
    /// upgrade across a migration or rename records one hop per such step
    /// and one per run of other steps; this is the last hop's kind.
    pub kind: &'static str,
    /// For a program successor, the evidence that the store's shell
    /// established all five premises, including the number of input tuples
    /// on which it compared the two decision programs; `None` for a genesis
    /// admission.
    pub premises: Option<upgrade::Premises>,
    /// Position among the store's upgrades, from 1.
    pub ordinal: u64,
    /// The commit position the upgrade was recorded at.
    pub at_commit: u64,
    /// The hash chain tip after the upgrade, in hexadecimal.
    pub chain: String,
    /// The SHA-256 of each adoption receipt the record binds, in hexadecimal.
    pub receipts: Vec<String>,
    /// For a behaviour change, what held on the store's state and the owner
    /// reviews the record binds; `None` for any other kind.
    pub behaviour: Option<upgrade::Behaviour>,
    /// For a migration, the migration digest the record binds, what the
    /// store's shell compared in its forward simulation and the migrated
    /// state's root; `None` for any other kind.
    pub migration: Option<upgrade::Migrated>,
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
                    "{{\"policy_differs_only_in_program_and_step_limit\":true,\"decision_law_991_required\":true,\"programs_equal_on_input_tuples\":{},\"step_limits_never_bind\":true,\"step_usage_unobserved\":true}}",
                    premises.tuples()
                )
            },
        );
        let ids = |ids: &[u32]| ids.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
        // A behaviour change also names what held, what was not evaluated
        // and the reviews it binds; no other kind has these keys.
        let behaviour = self.behaviour.as_ref().map_or_else(String::new, |behaviour| {
            let reviews: Vec<String> = behaviour
                .reviews()
                .iter()
                .map(|review| format!("\"{}\"", hex(review.as_bytes())))
                .collect();
            format!(
                ",\"behaviour\":{{\"state_laws_held\":[{}],\"claims_held\":[{}],\"not_evaluated\":[{}],\"reviews\":[{}]}}",
                ids(behaviour.laws()),
                ids(behaviour.claims()),
                ids(behaviour.unevaluated()),
                reviews.join(",")
            )
        });
        // A migration names its digest, what the simulation compared and
        // the migrated state's root; no other kind has this key.
        let migration = self.migration.as_ref().map_or_else(String::new, |migrated| {
            let simulated = migrated.simulated();
            format!(
                ",\"migration\":{{\"sha256\":\"{}\",\"states\":{},\"states_satisfying_state_laws\":{},\"genesis_states\":{},\"tuples_compared\":{},\"observations\":[\"genesis\",\"new-state-laws\",\"decision-class-and-reason\",\"deliveries\",\"successor-state\"],\"state_root\":\"{}\",\"claims_held\":[{}]}}",
                hex(migrated.migration().as_bytes()),
                simulated.states(),
                simulated.admitted(),
                simulated.genesis(),
                simulated.tuples(),
                hex(migrated.root().as_bytes()),
                ids(migrated.claims())
            )
        });
        format!(
            "{{\"status\":\"upgraded\",\"kind\":\"{}\",\"premises\":{premises},\"ordinal\":{},\"at_commit\":{},\"chain\":\"{}\",\"receipts\":[{}]{behaviour}{migration},\"contract_version\":{},\"commits\":{},\"pending\":{},\"upgrades\":{}}}",
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

/// Each behaviour change as the generated `EVOLUTIONS` lists it: the
/// version it follows, the owner's review, and each claim's ID, law program
/// and root.
type Evolutions<'a> = &'a [(u32, &'a str, &'a [(u32, &'a [l::Op<'static>], usize)])];

/// Each migration or rename as the generated `STATE_STEPS` lists it: the
/// version it follows, and for a migration each new state field's ID, its
/// source's tag (0 an old field, 1 a value, 2 a table), the old field or the
/// value, and the table; `None` for a rename.
type StateSteps<'a> = &'a [(u32, Option<&'a [(u16, u8, i128, &'a [(i128, i128)])]>)];

/// What the generated `with_lineage` passes: the checked catalogs alone
/// while the contract has no adoption, then also each adoption's receipt
/// digest, after an evolution also the behaviour changes, and after a
/// migration or rename also those. Every application shares this source,
/// before and after it adopts or evolves.
trait Generated<'a, 'p> {
    fn parts(
        self,
    ) -> (
        &'a [&'a BoundCatalog<'p>],
        &'a [&'a str],
        Evolutions<'a>,
        StateSteps<'a>,
    );
}

impl<'a, 'b: 'a, 'p> Generated<'a, 'p> for &'a [&'b BoundCatalog<'p>] {
    fn parts(
        self,
    ) -> (
        &'a [&'a BoundCatalog<'p>],
        &'a [&'a str],
        Evolutions<'a>,
        StateSteps<'a>,
    ) {
        (self, &[], &[], &[])
    }
}

impl<'a, 'b: 'a, 'p> Generated<'a, 'p> for (&'a [&'b BoundCatalog<'p>], &'a [&'a str]) {
    fn parts(
        self,
    ) -> (
        &'a [&'a BoundCatalog<'p>],
        &'a [&'a str],
        Evolutions<'a>,
        StateSteps<'a>,
    ) {
        (self.0, self.1, &[], &[])
    }
}

impl<'a, 'b: 'a, 'p> Generated<'a, 'p>
    for (&'a [&'b BoundCatalog<'p>], &'a [&'a str], Evolutions<'a>)
{
    fn parts(
        self,
    ) -> (
        &'a [&'a BoundCatalog<'p>],
        &'a [&'a str],
        Evolutions<'a>,
        StateSteps<'a>,
    ) {
        (self.0, self.1, self.2, &[])
    }
}

impl<'a, 'b: 'a, 'p> Generated<'a, 'p>
    for (
        &'a [&'b BoundCatalog<'p>],
        &'a [&'a str],
        Evolutions<'a>,
        StateSteps<'a>,
    )
{
    fn parts(
        self,
    ) -> (
        &'a [&'a BoundCatalog<'p>],
        &'a [&'a str],
        Evolutions<'a>,
        StateSteps<'a>,
    ) {
        self
    }
}

/// One generated migration field as the shell takes it.
fn target<'a>(
    (id, tag, operand, table): &'a (u16, u8, i128, &'a [(i128, i128)]),
) -> AppResult<migration::Target<'a>> {
    let field = || {
        u16::try_from(*operand)
            .map_err(|_| format!("catalog: migration field {id} names old field {operand}"))
    };
    let source = match tag {
        0 => migration::Source::Field(field()?),
        1 => migration::Source::Value(*operand),
        2 => migration::Source::Table {
            field: field()?,
            cases: table,
        },
        _ => {
            return Err(format!(
                "catalog: migration field {id} has source tag {tag}"
            ));
        }
    };
    Ok(migration::Target { id: *id, source })
}

/// Binds every version's catalog and Authority, oldest first, with the
/// step that made each later version: an adoption's receipt digest, a
/// behaviour change with its review and claims, a data migration, or a
/// rename, for `f`.
pub(crate) fn with_lineage<R>(f: impl FnOnce(&Lineage<'_, '_>) -> AppResult<R>) -> AppResult<R> {
    v2_contract::with_lineage(|generated| {
        let (catalogs, receipts, evolutions, state_steps) = generated.parts();
        let mut receipts = receipts
            .iter()
            .map(|receipt| digest(receipt))
            .collect::<AppResult<Vec<_>>>()?
            .into_iter();
        let claims: Vec<Vec<behaviour::Claim<'_>>> = evolutions
            .iter()
            .map(|(_, _, claims)| {
                claims
                    .iter()
                    .map(|(id, nodes, root)| behaviour::Claim {
                        id: *id,
                        nodes,
                        root: *root,
                    })
                    .collect()
            })
            .collect();
        let migrations: Vec<(u32, Option<Vec<migration::Target<'_>>>)> = state_steps
            .iter()
            .map(|(after, fields)| {
                let fields = fields
                    .map(|fields| fields.iter().map(target).collect::<AppResult<Vec<_>>>())
                    .transpose()?;
                Ok((*after, fields))
            })
            .collect::<AppResult<_>>()?;
        let mut steps = Vec::new();
        for version in 1..catalogs.len() {
            let after = |declared: u32| usize::try_from(declared) == Ok(version);
            let evolution = evolutions
                .iter()
                .zip(&claims)
                .find(|((declared, _, _), _)| after(*declared));
            let moved = migrations.iter().find(|(declared, _)| after(*declared));
            steps.push(match (moved, evolution) {
                (Some((_, Some(fields))), Some(((_, review, _), claims))) => Step::Migration {
                    migration: migration::Migration { fields },
                    claims,
                    review: review.as_bytes(),
                },
                (Some((_, Some(_))), None) => {
                    return Err(format!(
                        "catalog: no target claim metadata for migration to version {}: regenerate the application",
                        version + 1
                    ));
                }
                (Some((_, None)), _) => Step::Rename,
                (None, Some(((_, review, _), claims))) => Step::BehaviourChange {
                    review: review.as_bytes(),
                    claims,
                },
                (None, None) => Step::Adoption(receipts.next().ok_or_else(|| {
                    format!("catalog: no adoption receipt for version {}", version + 1)
                })?),
            });
        }
        if receipts.next().is_some() {
            return Err("catalog: more adoption receipts than adoptions".to_owned());
        }
        let lineage = Lineage::bind_steps(catalogs, &steps).map_err(store)?;
        f(&lineage)
    })
    .map_err(|error| format!("catalog: {error:?}"))?
}

/// Replays every history segment of the store at `path` under the contract
/// version that published it, and reports the audited head, without writing
/// to the store: the file is opened read-only and no checkpoint is saved. A
/// store that runs an earlier version of this application is audited under
/// the versions up to its own, so this build can check it before
/// `--upgrade`.
///
/// # Errors
/// Returns the store's refusal: a segment under no version of this
/// application, a schema that needs `--migrate`, or failed replay.
pub fn audit(path: &Path) -> AppResult<Head> {
    with_lineage(|lineage| {
        let (version, snapshot) = lineage.audit_read_only(path).map_err(store)?;
        Ok(Head::of(&snapshot, version))
    })
}

/// Records the checked upgrade of the store at `path` to this application's
/// contract version. Across a behaviour change that `zeno-fcis contract
/// evolve` recorded, every state law of this version and every inductive
/// claim it declares must hold on the store's state, and the record binds
/// the owner's review. Across a data migration that `contract evolve
/// --migration` recorded, the store's shell admits the migration by its own
/// forward simulation over the old version's whole input domain and moves
/// the state to the new layout, checking every target state law and declared
/// inductive claim on that mapped state; across a rename it checks that only names
/// differ and frames the state again. Otherwise, when the store's shell
/// establishes every premise of a program succession, including its own
/// comparison of the two decision programs on every input tuple, the upgrade
/// is admitted at any state; failing that, the new contract must admit the
/// current state through its genesis laws.
///
/// # Errors
/// Returns the store's refusal: a store already at this version, a
/// different state schema without a declared migration, a state law or
/// claim that does not hold on the store's state, a migration the
/// simulation refuses, a missing premise with the new contract's genesis
/// laws refusing the current state, a schema that needs `--migrate`, or
/// failed replay. A refusal writes nothing.
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
                behaviour: receipt.behaviour().cloned(),
                migration: receipt.migration().cloned(),
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
            while let Some(pending) = shell.next_pending().map_err(store)? {
                let delivered = pending.deliver(&mut destination).map_err(store)?;
                let id = delivered.delivery().delivery_id();
                delivered.acknowledge().map_err(store)?;
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
        let delivered = pending.deliver(&mut destination).map_err(store)?;
        drop(delivered);
        drop(shell);
        shell = V2SqliteShell::open(path, &authority).map_err(store)?;
        while let Some(pending) = shell.next_pending().map_err(store)? {
            let delivered = pending.deliver(&mut destination).map_err(store)?;
            delivered.acknowledge().map_err(store)?;
        }
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

// The operational commands: `init`, `submit`, `decide`, `state`, `history`,
// `pending`, `deliver` and `version` of the command line in `src/cli.rs`.

/// Why an operational command did not complete.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Failure {
    /// The store, the contract or the input refused the operation. A refusal
    /// before a commit writes nothing to the store.
    Refused(String),
    /// A file beside the store, such as the submission journal or a delivery
    /// file, could not be read or written.
    Io(String),
}

impl From<String> for Failure {
    fn from(message: String) -> Self {
        Self::Refused(message)
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused(message) | Self::Io(message) => f.write_str(message),
        }
    }
}

fn io(what: &str, path: &Path, error: &std::io::Error) -> Failure {
    Failure::Io(format!("{what} {}: {error}", path.display()))
}

/// A decision of the library Authority, with the state and deliveries in
/// numbers: 0 or 1 for a boolean, the value of an integer, the variant ID of
/// a sum.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Decision {
    /// `Accept`, `Reject` or `CommittedFailure`.
    pub class: &'static str,
    /// The reason, if the decision has one.
    pub reason: Option<u32>,
    /// The state fields after the decision, in program order; a reject
    /// keeps the state it was made on.
    pub post: Vec<i128>,
    /// Each delivery's channel and its payload fields' IDs and numbers.
    pub outbox: Vec<(u32, Vec<(u16, i128)>)>,
    /// The commit position, when the decision was committed.
    pub commit: Option<u64>,
}

fn atom_number(atom: &c::Atom<'_>) -> AppResult<i128> {
    match atom {
        c::Atom::Bool(value) => Ok(i128::from(*value)),
        c::Atom::I128(value) => Ok(*value),
        c::Atom::Sum { variant, .. } => Ok(i128::from(*variant)),
        other => Err(format!("the value {other:?} has no number form")),
    }
}

fn value_number(view: ValueRef<'_>) -> AppResult<i128> {
    match view {
        ValueRef::Bool(value) => Ok(i128::from(value)),
        ValueRef::I128(value) => Ok(value),
        ValueRef::U128(value) => {
            i128::try_from(value).map_err(|_| format!("the stored value {value} is too large"))
        }
        ValueRef::Sum { variant, .. } | ValueRef::Enum { variant, .. } => Ok(i128::from(variant)),
        other => Err(format!("the stored value {other:?} has no number form")),
    }
}

/// The state fields of a framed state, in program order.
fn state_numbers(descriptor: &c::Descriptor<'_>, framed: &[u8]) -> AppResult<Vec<i128>> {
    root_numbers(descriptor.state, framed)
}

fn root_numbers(schema: c::Schema<'_>, framed: &[u8]) -> AppResult<Vec<i128>> {
    let envelope = decode_envelope(framed, DecodeLimits::default())
        .map_err(|error| format!("the stored state does not decode: {error:?}"))?;
    match (schema, envelope.value().view()) {
        (c::Schema::Record(fields), ValueRef::Record(values)) => fields
            .iter()
            .map(|field| {
                values
                    .iter()
                    .find(|value| value.id() == field.id)
                    .ok_or_else(|| format!("the stored state has no field {}", field.id))
                    .and_then(|value| value_number(value.value().view()))
            })
            .collect(),
        (c::Schema::Leaf(_), view) => Ok(vec![value_number(view)?]),
        _ => Err("the stored state does not have the contract's state schema".to_owned()),
    }
}

/// The Authority's decision on `candidate`, made on the state `pre`.
fn decision_of(
    descriptor: &c::Descriptor<'_>,
    candidate: &c::Candidate<'_>,
    pre: &[i128],
) -> AppResult<Decision> {
    let post = if candidate.class() == c::Class::Reject {
        pre.to_vec()
    } else {
        let c::Schema::Record(fields) = descriptor.state else {
            return Err("the state root must be a record".to_owned());
        };
        fields
            .iter()
            .map(|field| {
                candidate
                    .post()
                    .iter()
                    .find(|posted| posted.id == field.id)
                    .ok_or_else(|| format!("the decision has no state field {}", field.id))
                    .and_then(|posted| atom_number(&posted.value))
            })
            .collect::<AppResult<_>>()?
    };
    let outbox = candidate
        .outbox()
        .iter()
        .map(|delivery| {
            let payload = delivery
                .payload
                .iter()
                .map(|field| Ok((field.id, atom_number(&field.value)?)))
                .collect::<AppResult<_>>()?;
            Ok((delivery.channel, payload))
        })
        .collect::<AppResult<_>>()?;
    Ok(Decision {
        class: class_name(candidate.class()),
        reason: candidate.reason(),
        post,
        outbox,
        commit: None,
    })
}

/// The framed state, command and context for a command on the stored state
/// `stored`, whose fields are `pre`; the stored bytes must be the framing of
/// those fields.
fn framed(
    descriptor: &c::Descriptor<'_>,
    pre: &[i128],
    command: &[i128],
    context: &[i128],
    stored: &[u8],
) -> AppResult<[Vec<u8>; 3]> {
    let inputs: Vec<i128> = pre.iter().chain(command).chain(context).copied().collect();
    let raw = wire(descriptor, &inputs)?;
    if raw[0] != stored {
        return Err("the stored state is not in the contract's framing".to_owned());
    }
    Ok(raw)
}

fn earlier(running: usize, current: usize, doing: &str) -> Failure {
    Failure::Refused(format!(
        "store: runs contract version {running}; upgrade it to version {current} before {doing}"
    ))
}

/// The submission journal of the store at `path`: the file beside it, named
/// after it with `.submissions` appended.
#[must_use]
pub fn journal_path(path: &Path) -> std::path::PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".submissions");
    std::path::PathBuf::from(name)
}

/// One journal line: `SEQUENCE CONTRACT_VERSION | COMMAND | CONTEXT`, the
/// command and context as numbers in program order.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Submission {
    sequence: u64,
    contract_version: usize,
    command: Vec<i128>,
    context: Vec<i128>,
}

fn joined(numbers: &[i128]) -> String {
    numbers
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ")
}

impl Submission {
    fn line(&self) -> String {
        format!(
            "{} {} | {} | {}\n",
            self.sequence,
            self.contract_version,
            joined(&self.command),
            joined(&self.context)
        )
    }

    fn parse(line: &str) -> Option<Self> {
        let mut sections = line.split('|');
        let mut head = sections.next()?.split_whitespace();
        let numbers = |text: &str| {
            text.split_whitespace()
                .map(|word| word.parse::<i128>().ok())
                .collect::<Option<Vec<_>>>()
        };
        let submission = Self {
            sequence: head.next()?.parse().ok()?,
            contract_version: head.next()?.parse().ok()?,
            command: numbers(sections.next()?)?,
            context: numbers(sections.next()?)?,
        };
        (head.next().is_none() && sections.next().is_none()).then_some(submission)
    }
}

/// The submission journal of the store at `path`, open and locked: an
/// exclusive lock for `submit`, which holds it from reading the store's head
/// until its commit returns, and a shared one for `history`. `None` when the
/// journal does not exist.
fn locked_journal(path: &Path, exclusive: bool) -> Result<Option<std::fs::File>, Failure> {
    let journal = journal_path(path);
    let file = match std::fs::OpenOptions::new()
        .read(true)
        .append(exclusive)
        .open(&journal)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io("open the submission journal", &journal, &error)),
    };
    if exclusive {
        file.lock()
    } else {
        file.lock_shared()
    }
    .map_err(|error| io("lock the submission journal", &journal, &error))?;
    Ok(Some(file))
}

/// The submissions of a journal's text for commits up to `head`, oldest
/// first, and the length of the text that holds them.
///
/// Under the journal's lock, `submit` writes the line for commit `head + 1`
/// before committing and removes it again when the commit does not happen,
/// so the lines are in strictly increasing commit order and each commit has
/// at most one. Only a process that stopped between its write and its
/// commit's outcome leaves a line past the head, or a last line without its
/// newline; neither describes a commit, so both end the scan, and the next
/// `submit` removes them.
fn scan_journal(text: &str, journal: &Path, head: u64) -> Result<(Vec<Submission>, u64), Failure> {
    let mut submissions: Vec<Submission> = Vec::new();
    let mut length = 0;
    for (index, line) in text.split_inclusive('\n').enumerate() {
        let Some(complete) = line.strip_suffix('\n') else {
            break;
        };
        let submission = Submission::parse(complete).ok_or_else(|| {
            Failure::Refused(format!(
                "submission journal {} line {} is malformed",
                journal.display(),
                index + 1
            ))
        })?;
        if submissions
            .last()
            .is_some_and(|last| submission.sequence <= last.sequence)
        {
            return Err(Failure::Refused(format!(
                "submission journal {} line {} records commit {} out of order",
                journal.display(),
                index + 1,
                submission.sequence
            )));
        }
        if submission.sequence > head {
            break;
        }
        submissions.push(submission);
        length += line.len();
    }
    Ok((submissions, length as u64))
}

/// Removes from the locked journal every line past the store's `head`, left
/// by a process that stopped before its commit, and returns the length kept.
fn settle_journal(file: &mut std::fs::File, path: &Path, head: u64) -> Result<u64, Failure> {
    use std::io::Seek as _;
    let journal = journal_path(path);
    let mut text = String::new();
    file.seek(std::io::SeekFrom::Start(0))
        .and_then(|_| file.read_to_string(&mut text))
        .map_err(|error| io("read the submission journal", &journal, &error))?;
    let (_, length) = scan_journal(&text, &journal, head)?;
    if length < text.len() as u64 {
        truncate_journal(file, path, length)?;
    }
    Ok(length)
}

fn truncate_journal(file: &std::fs::File, path: &Path, length: u64) -> Result<(), Failure> {
    file.set_len(length)
        .and_then(|()| file.sync_all())
        .map_err(|error| {
            io(
                "truncate the submission journal",
                &journal_path(path),
                &error,
            )
        })
}

/// Appends one line to the locked journal and returns once it is on disk.
fn append_journal(
    file: &mut std::fs::File,
    path: &Path,
    submission: &Submission,
) -> Result<(), Failure> {
    file.write_all(submission.line().as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|error| io("write the submission journal", &journal_path(path), &error))
}

/// The journal's submissions for commits up to `head`, by commit position.
fn read_journal(
    path: &Path,
    head: u64,
) -> Result<std::collections::BTreeMap<u64, Submission>, Failure> {
    let journal = journal_path(path);
    let text = std::fs::read_to_string(&journal)
        .map_err(|error| io("read the submission journal", &journal, &error))?;
    let (submissions, _) = scan_journal(&text, &journal, head)?;
    Ok(submissions
        .into_iter()
        .map(|submission| (submission.sequence, submission))
        .collect())
}

fn version_number() -> AppResult<usize> {
    usize::try_from(v2_contract::VERSION).map_err(|_| "contract version out of range".to_owned())
}

/// Creates the empty submission journal of the store at `path` and
/// synchronizes it and its directory to disk. Refuses a journal that exists.
fn create_journal(path: &Path) -> Result<(), Failure> {
    let journal = journal_path(path);
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&journal)
        .and_then(|file| file.sync_all())
        .map_err(|error| io("create the submission journal", &journal, &error))?;
    let directory = match journal.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    std::fs::File::open(directory)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| io("synchronize the directory", directory, &error))
}

/// The commits after genesis of the store at `path`, which `submit` may
/// write: the store's own refusal for one it may not.
fn submittable_head(path: &Path) -> Result<u64, Failure> {
    with_lineage(|lineage| {
        Ok(match lineage.open(path).map_err(store)? {
            Opened::V10(Store::Current(mut shell)) => {
                Ok(shell.snapshot().map_err(store)?.version())
            }
            Opened::V10(Store::Superseded(superseded)) => Err(earlier(
                superseded.version(),
                lineage.versions(),
                "submitting",
            )),
            Opened::V9(_) => Err(Failure::Refused(store(Error::Schema(9)))),
        })
    })?
}

/// The submission journal of the store at `path`, locked for `submit`. A
/// missing journal of a store at genesis, such as one an `init` that stopped
/// part way left, is created empty, which is exactly its content; a missing
/// journal of a store with commits is refused, since the store does not hand
/// out the submissions it would have to hold.
fn submission_journal(path: &Path) -> Result<std::fs::File, Failure> {
    if let Some(journal) = locked_journal(path, true)? {
        return Ok(journal);
    }
    // The store's own refusal comes first, as for every other command.
    let head = submittable_head(path)?;
    let journal = journal_path(path);
    if head != 0 {
        return Err(Failure::Io(format!(
            "the submission journal {} does not exist, and the store has commits it would record ({head} after genesis); it cannot be rebuilt from the store: restore it from a backup, or create it empty to let submit continue, after which history refuses those commits",
            journal.display()
        )));
    }
    match create_journal(path) {
        // Another submit created it first.
        Err(_) if journal.exists() => {}
        result => result?,
    }
    locked_journal(path, true)?.ok_or_else(|| {
        Failure::Io(format!(
            "the submission journal {} was removed while submit created it",
            journal.display()
        ))
    })
}

/// Publishes the contract's genesis into a new store at `path` and creates
/// its empty submission journal.
///
/// # Errors
/// Refuses when `path` or the journal beside it already exists, and returns
/// the store's or the file system's failure.
pub fn init(path: &Path) -> Result<Head, Failure> {
    let journal = journal_path(path);
    for existing in [path, journal.as_path()] {
        if existing
            .try_exists()
            .map_err(|error| io("inspect", existing, &error))?
        {
            return Err(Failure::Refused(format!(
                "{} already exists: init creates a new store and its journal",
                existing.display()
            )));
        }
    }
    let contract = v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = authority(&descriptor)?;
    let initial = genesis()?;
    let genesis = match authority.publish_genesis(&initial) {
        PublicationOutcome::Commit(publication) => publication,
        other => return Err(Failure::Refused(format!("genesis: {other:?}"))),
    };
    let mut shell = V2SqliteShell::create(path, &authority, genesis).map_err(store)?;
    create_journal(path)?;
    Ok(Head::of(
        &shell.snapshot().map_err(store)?,
        version_number()?,
    ))
}

/// Decides `command` and `context` on the current state of the store at
/// `path` and, unless the decision is a reject, commits it, after recording
/// the submission in the journal. A reject writes nothing.
///
/// The journal's exclusive lock is held from before the store's head is read
/// until the commit's outcome, so submissions are serialized against each
/// other; a commit that does not happen, such as one another writer of the
/// store overtook, has its journal line removed again.
///
/// # Errors
/// Returns the store's refusal, including a store at an earlier version, an
/// input the contract's framing refuses, an Authority refusal, or a journal
/// that cannot be written or is missing from a store with commits; the
/// store and the journal are then unchanged, except when removing the line
/// itself fails, which the message says. A store at genesis whose journal is
/// missing has it created empty first, and keeps it whatever follows.
pub fn submit(path: &Path, command: &[i128], context: &[i128]) -> Result<Decision, Failure> {
    let mut journal = submission_journal(path)?;
    with_lineage(|lineage| {
        Ok(match lineage.open(path).map_err(store)? {
            Opened::V10(Store::Current(mut shell)) => commit_submission(
                path,
                &mut journal,
                &mut shell,
                lineage.versions(),
                command,
                context,
            ),
            Opened::V10(Store::Superseded(superseded)) => Err(earlier(
                superseded.version(),
                lineage.versions(),
                "submitting",
            )),
            Opened::V9(_) => Err(Failure::Refused(store(Error::Schema(9)))),
        })
    })?
}

fn commit_submission(
    path: &Path,
    journal: &mut std::fs::File,
    shell: &mut V2SqliteShell<'_, '_>,
    contract_version: usize,
    command: &[i128],
    context: &[i128],
) -> Result<Decision, Failure> {
    let authority = shell.authority();
    let descriptor = authority.descriptor();
    let snapshot = shell.snapshot().map_err(store)?;
    let pre = state_numbers(descriptor, snapshot.state())?;
    let raw = framed(descriptor, &pre, command, context, snapshot.state())?;
    let original = c::Raw {
        state: &raw[0],
        command: &raw[1],
        context: &raw[2],
    };
    match authority.publish(original) {
        PublicationOutcome::Commit(publication) => {
            let candidate = publication
                .evaluation()
                .result()
                .map_err(|refusal| format!("refused: {refusal:?}"))?;
            let mut decision = decision_of(descriptor, candidate, &pre)?;
            let sequence = snapshot
                .version()
                .checked_add(1)
                .ok_or_else(|| "commit position out of range".to_owned())?;
            let mut message = b"submit".to_vec();
            message.extend_from_slice(&snapshot.version().to_be_bytes());
            for part in &raw[1..] {
                message.extend_from_slice(&(part.len() as u64).to_be_bytes());
                message.extend_from_slice(part);
            }
            let key = replay_key(&message)?;
            let kept = settle_journal(journal, path, snapshot.version())?;
            append_journal(
                journal,
                path,
                &Submission {
                    sequence,
                    contract_version,
                    command: command.to_vec(),
                    context: context.to_vec(),
                },
            )?;
            let committed = match shell.commit_at(snapshot.version(), key, publication) {
                Ok(receipt) if receipt.status() == CommitStatus::Committed => Ok(receipt.version()),
                Ok(_) => Err(Failure::Refused(
                    "store: the submission was already committed".to_owned(),
                )),
                Err(error) => Err(Failure::Refused(store(error))),
            };
            match committed {
                Ok(version) => {
                    decision.commit = Some(version);
                    Ok(decision)
                }
                // The commit did not happen: its line is removed, so a
                // refused submission leaves the journal as it found it.
                Err(refusal) => match truncate_journal(journal, path, kept) {
                    Ok(()) => Err(refusal),
                    Err(Failure::Io(error) | Failure::Refused(error)) => Err(Failure::Io(format!(
                        "{refusal}; then {error}, so the journal keeps a line past the store's head, which the next submit removes"
                    ))),
                },
            }
        }
        PublicationOutcome::Reject(evaluation) => {
            let candidate = evaluation
                .result()
                .map_err(|refusal| format!("refused: {refusal:?}"))?;
            Ok(decision_of(descriptor, candidate, &pre)?)
        }
        other => Err(Failure::Refused(format!("refused: {other:?}"))),
    }
}

/// The decision `submit` would make on the store's current state, computed
/// by the same Authority; nothing is committed or recorded.
///
/// # Errors
/// As `submit`, apart from the journal.
pub fn preview(path: &Path, command: &[i128], context: &[i128]) -> Result<Decision, Failure> {
    with_lineage(|lineage| {
        Ok(match lineage.open(path).map_err(store)? {
            Opened::V10(Store::Current(mut shell)) => (|| {
                let authority = shell.authority();
                let descriptor = authority.descriptor();
                let snapshot = shell.snapshot().map_err(store)?;
                let pre = state_numbers(descriptor, snapshot.state())?;
                let raw = framed(descriptor, &pre, command, context, snapshot.state())?;
                let original = c::Raw {
                    state: &raw[0],
                    command: &raw[1],
                    context: &raw[2],
                };
                match authority.publish(original) {
                    PublicationOutcome::Commit(publication) => {
                        let candidate = publication
                            .evaluation()
                            .result()
                            .map_err(|refusal| format!("refused: {refusal:?}"))?;
                        Ok(decision_of(descriptor, candidate, &pre)?)
                    }
                    PublicationOutcome::Reject(evaluation) => {
                        let candidate = evaluation
                            .result()
                            .map_err(|refusal| format!("refused: {refusal:?}"))?;
                        Ok(decision_of(descriptor, candidate, &pre)?)
                    }
                    other => Err(Failure::Refused(format!("refused: {other:?}"))),
                }
            })(),
            Opened::V10(Store::Superseded(superseded)) => Err(earlier(
                superseded.version(),
                lineage.versions(),
                "deciding",
            )),
            Opened::V9(_) => Err(Failure::Refused(store(Error::Schema(9)))),
        })
    })?
}

/// The audited head of a store and its current state fields.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Current {
    /// The head, with the contract version the store runs.
    pub head: Head,
    /// The state fields, in program order.
    pub state: Vec<i128>,
}

/// The current state of the store at `path`, whichever version of this
/// application it runs.
///
/// # Errors
/// Returns the store's refusal.
pub fn current(path: &Path) -> Result<Current, Failure> {
    with_lineage(|lineage| {
        let descriptor = lineage
            .authorities()
            .last()
            .ok_or_else(|| "the lineage is empty".to_owned())?
            .descriptor();
        let (snapshot, version) = match lineage.open(path).map_err(store)? {
            Opened::V10(Store::Current(mut shell)) => {
                (shell.snapshot().map_err(store)?, lineage.versions())
            }
            Opened::V10(Store::Superseded(mut superseded)) => {
                let version = superseded.version();
                (superseded.snapshot().map_err(store)?, version)
            }
            Opened::V9(_) => return Err(store(Error::Schema(9))),
        };
        Ok(Current {
            head: Head::of(&snapshot, version),
            state: state_numbers(descriptor, snapshot.state())?,
        })
    })
    .map_err(Failure::from)
}

/// One committed submission, decided again from its recorded inputs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Entry {
    /// The contract version that decided it.
    pub contract_version: usize,
    /// The command fields, in program order.
    pub command: Vec<i128>,
    /// The context fields, in program order.
    pub context: Vec<i128>,
    /// The decision, with its commit position.
    pub decision: Decision,
}

/// The committed history of a store, from its submission journal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct History {
    /// The audited head.
    pub head: Head,
    /// The genesis state fields.
    pub genesis: Vec<i128>,
    /// The checked lineage version whose schema interprets genesis.
    pub genesis_contract_version: usize,
    /// One entry per commit, oldest first.
    pub entries: Vec<Entry>,
}

/// The history of the store at `path`, read from its submission journal and
/// checked against a full read-only store audit: every commit position has a
/// submission, its contract identity and command/context equal the actual
/// checked inputs, and the Authority decides those original inputs again.
/// The store's audit supplies each pre-state after preceding migrations;
/// the journal cannot select a different policy or omit an upgrade.
///
/// # Errors
/// Refuses a store with a commit no submission records, such as one made by
/// `--decide`, a journal that differs from the stored inputs, or the
/// store's refusal.
pub fn history(path: &Path) -> Result<History, Failure> {
    // Held while the store and the journal are read, so no submit is between
    // its journal line and its commit.
    let _journal = locked_journal(path, false)?;
    with_lineage(|lineage| {
        Ok((|| {
            let history = lineage.audit_history_read_only(path).map_err(store)?;
            let snapshot = &history.snapshot;
            let version = if lineage
                .authorities()
                .last()
                .is_some_and(|last| last.identity() == snapshot.binding())
            {
                lineage.versions()
            } else {
                history.contract_version
            };
            let submissions = read_journal(path, snapshot.version())?;
            let initial = lineage
                .authorities()
                .get(history.genesis_contract_version - 1)
                .ok_or_else(|| "the genesis contract is absent from the lineage".to_owned())?;
            let genesis = state_numbers(initial.descriptor(), &history.genesis)?;
            let mut entries = Vec::new();
            for recorded in &history.commits {
                let sequence = recorded.sequence;
                let submission = submissions.get(&sequence).ok_or_else(|| {
                    Failure::Refused(format!(
                        "history: the submission journal has no submission for commit {sequence}; it was committed by another program, such as --decide"
                    ))
                })?;
                let authority = submission
                    .contract_version
                    .checked_sub(1)
                    .filter(|_| submission.contract_version <= version)
                    .and_then(|index| lineage.authorities().get(index))
                    .ok_or_else(|| {
                        Failure::Refused(format!(
                            "history: commit {sequence} names contract version {}, which this store has not run",
                            submission.contract_version
                        ))
                    })?;
                let publishing = lineage
                    .authorities()
                    .get(recorded.contract_version - 1)
                    .ok_or_else(|| {
                        "the publishing contract is absent from the lineage".to_owned()
                    })?;
                let descriptor = publishing.descriptor();
                if authority.identity() != publishing.identity()
                    || root_numbers(descriptor.command, &recorded.command)? != submission.command
                    || root_numbers(descriptor.context, &recorded.context)? != submission.context
                {
                    return Err(Failure::Refused(format!(
                        "history: commit {sequence}: the submission journal differs from the stored contract or inputs"
                    )));
                }
                let state = state_numbers(descriptor, &recorded.prestate)?;
                let evaluation = publishing.evaluate(c::Raw {
                    state: &recorded.prestate,
                    command: &recorded.command,
                    context: &recorded.context,
                });
                let candidate = evaluation.result().map_err(|refusal| {
                    format!("history: commit {sequence}: refused: {refusal:?}")
                })?;
                let mut decision = decision_of(descriptor, candidate, &state)?;
                if decision.class == "Reject" {
                    return Err(Failure::Refused(format!(
                        "history: commit {sequence} decides again as a reject"
                    )));
                }
                if decision.post != state_numbers(descriptor, &recorded.poststate)? {
                    return Err(Failure::Refused(format!(
                        "history: commit {sequence}: the recomputed successor differs from the stored state"
                    )));
                }
                decision.commit = Some(sequence);
                entries.push(Entry {
                    contract_version: submission.contract_version,
                    command: submission.command.clone(),
                    context: submission.context.clone(),
                    decision,
                });
            }
            Ok(History {
                head: Head::of(snapshot, version),
                genesis,
                genesis_contract_version: history.genesis_contract_version,
                entries,
            })
        })())
    })?
}

/// A stored delivery as it leaves the store.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Outgoing {
    /// The delivery ID, in hexadecimal: the idempotency key every attempt
    /// carries.
    pub id: String,
    /// The hash of the outbox entry, in hexadecimal.
    pub entry_hash: String,
    /// The commit that bound it.
    pub commit: u64,
    /// Its lane within the commit.
    pub lane: u8,
    /// Its idempotency ordinal.
    pub ordinal: u32,
    /// The channel.
    pub channel: u32,
    /// The destination's type.
    pub destination_type: u32,
    /// The payload's type.
    pub payload_type: u32,
    /// The canonical destination value bytes.
    pub destination: Vec<u8>,
    /// The canonical payload value bytes.
    pub payload: Vec<u8>,
}

impl Outgoing {
    fn of(delivery: &zeno_fcis_shell_sqlite::v2::Delivery) -> Self {
        Self {
            id: hex(delivery.delivery_id().as_bytes()),
            entry_hash: hex(delivery.entry_hash().as_bytes()),
            commit: delivery.version(),
            lane: delivery.lane(),
            ordinal: delivery.ordinal(),
            channel: delivery.channel(),
            destination_type: delivery.destination_root(),
            payload_type: delivery.payload_root(),
            destination: delivery.destination().to_vec(),
            payload: delivery.payload().to_vec(),
        }
    }

    /// One JSON object on one line, with the destination and payload by the
    /// names the contract gives them. A destination compares lines to tell
    /// a repeat from a different delivery under the same ID.
    #[must_use]
    pub fn json(&self) -> String {
        crate::cli::delivery_json(self)
    }
}

/// What a destination did with a delivery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Sent {
    /// It now holds the delivery durably.
    Accepted,
    /// It already held the same delivery under the same ID, from an attempt
    /// whose acknowledgment did not reach the store.
    AlreadyHeld,
}

/// Where `deliver_to` sends each pending delivery: the extension point for
/// outbound transports. [`FileDestination`] is the implementation here; the
/// delivery relay of ZenoFCIS 2.2 adds its own.
///
/// `deliver_to` takes each pending delivery through the store's typed
/// lifecycle, `Pending` to `Delivered` through the library interpreter, then
/// calls `send`, and acknowledges the delivery in the store only after
/// `send` returns `Ok`. A failure, a crash or a lost acknowledgment leaves
/// the delivery pending, and the next run sends it again with the same ID,
/// so delivery is at least once. A destination that keeps one copy per ID
/// and refuses different content under a held ID, as `FileDestination`
/// does, holds each delivery exactly once.
pub trait Destination {
    /// Hands one delivery to the destination, returning only once the
    /// destination holds it durably.
    ///
    /// # Errors
    /// Returns why the destination does not hold it; the delivery then stays
    /// pending.
    fn send(&mut self, delivery: &Outgoing) -> Result<Sent, Failure>;
}

/// A file that receives each delivery as one JSON line, `Outgoing::json`,
/// appended and synchronized to disk before `send` returns.
///
/// It keeps one line per delivery ID: a delivery whose line it already
/// holds is not appended again, and a different line under a held ID is
/// refused. A last line without its newline, left by an interrupted append,
/// is removed before the next append.
#[derive(Clone, Debug)]
pub struct FileDestination {
    path: std::path::PathBuf,
}

impl FileDestination {
    /// The destination appending to the file at `path`, created on first use.
    #[must_use]
    pub fn new(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
        }
    }
}

impl Destination for FileDestination {
    fn send(&mut self, delivery: &Outgoing) -> Result<Sent, Failure> {
        let path = self.path.as_path();
        let created = !path
            .try_exists()
            .map_err(|error| io("inspect the delivery file", path, &error))?;
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(path)
            .map_err(|error| io("open the delivery file", path, &error))?;
        let mut text = String::new();
        file.read_to_string(&mut text)
            .map_err(|error| io("read the delivery file", path, &error))?;
        if !text.is_empty() && !text.ends_with('\n') {
            let complete = text.rfind('\n').map_or(0, |index| index + 1);
            text.truncate(complete);
            file.set_len(complete as u64)
                .and_then(|()| file.sync_all())
                .map_err(|error| io("truncate the delivery file", path, &error))?;
        }
        let line = delivery.json();
        let key = format!("{{\"delivery_id\":\"{}\",", delivery.id);
        if let Some(held) = text.lines().find(|held| held.starts_with(&key)) {
            return if held == line {
                Ok(Sent::AlreadyHeld)
            } else {
                Err(Failure::Refused(format!(
                    "the delivery file {} holds delivery {} with other content",
                    path.display(),
                    delivery.id
                )))
            };
        }
        file.write_all(format!("{line}\n").as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(|error| io("append to the delivery file", path, &error))?;
        if created && let Some(parent) = path.parent() {
            let parent = if parent.as_os_str().is_empty() {
                Path::new(".")
            } else {
                parent
            };
            std::fs::File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(|error| io("synchronize the directory of", path, &error))?;
        }
        Ok(Sent::Accepted)
    }
}

/// The pending deliveries of a store: how many, and the one `deliver` sends
/// next.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Waiting {
    /// The audited head; `pending` counts the deliveries.
    pub head: Head,
    /// The oldest pending delivery.
    pub next: Option<Outgoing>,
}

/// The pending deliveries of the store at `path`, which must run this
/// application's version. Nothing is delivered.
///
/// # Errors
/// Returns the store's refusal, including a store at an earlier version.
pub fn pending(path: &Path) -> Result<Waiting, Failure> {
    with_lineage(|lineage| {
        Ok(match lineage.open(path).map_err(store)? {
            Opened::V10(Store::Current(mut shell)) => (|| {
                let next = shell
                    .next_pending()
                    .map_err(store)?
                    .map(|pending| Outgoing::of(pending.delivery()));
                Ok(Waiting {
                    head: Head::of(&shell.snapshot().map_err(store)?, lineage.versions()),
                    next,
                })
            })(),
            Opened::V10(Store::Superseded(superseded)) => Err(earlier(
                superseded.version(),
                lineage.versions(),
                "delivering",
            )),
            Opened::V9(_) => Err(Failure::Refused(store(Error::Schema(9)))),
        })
    })?
}

/// A failure of `deliver_to` that names the deliveries it completed first:
/// those are held by the destination and acknowledged in the store, and stay
/// so.
fn after_delivering(delivered: &[String], failure: Failure) -> Failure {
    if delivered.is_empty() {
        return failure;
    }
    let done = format!(
        "after {} deliveries were sent and acknowledged ({}), which stay delivered: ",
        delivered.len(),
        delivered.join(", ")
    );
    match failure {
        Failure::Refused(message) => Failure::Refused(done + &message),
        Failure::Io(message) => Failure::Io(done + &message),
    }
}

/// Sends every pending delivery of the store at `path`, which must run this
/// application's version, to `destination`, oldest first, and acknowledges
/// each one after the destination holds it.
///
/// # Errors
/// Returns the store's refusal or the destination's failure; every delivery
/// not yet acknowledged stays pending. The deliveries acknowledged before
/// the failure stay delivered, and the failure's message names them.
pub fn deliver_to(path: &Path, destination: &mut dyn Destination) -> Result<Delivered, Failure> {
    with_lineage(|lineage| {
        Ok(match lineage.open(path).map_err(store)? {
            Opened::V10(Store::Current(mut shell)) => (|| {
                let mut interpreter = MemoryDestination::default();
                let mut deliveries = Vec::new();
                loop {
                    let step = (|| {
                        let Some(pending) = shell.next_pending().map_err(store)? else {
                            return Ok(None);
                        };
                        let delivered = pending.deliver(&mut interpreter).map_err(store)?;
                        let outgoing = Outgoing::of(delivered.delivery());
                        destination.send(&outgoing)?;
                        delivered.acknowledge().map_err(store)?;
                        Ok(Some(outgoing.id))
                    })();
                    match step {
                        Ok(Some(id)) => deliveries.push(id),
                        Ok(None) => break,
                        Err(failure) => return Err(after_delivering(&deliveries, failure)),
                    }
                }
                Ok(Delivered {
                    deliveries,
                    head: Head::of(&shell.snapshot().map_err(store)?, lineage.versions()),
                })
            })(),
            Opened::V10(Store::Superseded(superseded)) => Err(earlier(
                superseded.version(),
                lineage.versions(),
                "delivering",
            )),
            Opened::V9(_) => Err(Failure::Refused(store(Error::Schema(9)))),
        })
    })?
}

/// The contract identity of this build: a domain-separated SHA-256
/// commitment, in hexadecimal, to the identity bytes of the library
/// Authority bound to its current contract version. A store records the
/// identity of each version it runs, so two builds with different
/// identities never open each other's stores as their own.
///
/// # Errors
/// Returns a catalog refusal.
pub fn identity() -> AppResult<String> {
    let contract = v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = authority(&descriptor)?;
    let domain = Domain::new("zeno-fcis-app/contract-identity", 1)
        .map_err(|error| format!("identity domain: {error:?}"))?;
    commitment::<RustCryptoSha256>(domain, authority.identity())
        .map(|digest| hex(digest.as_bytes()))
        .map_err(|error| format!("identity: {error:?}"))
}

#[cfg(test)]
mod tests {
    use super::{Failure, after_delivering};

    #[test]
    fn a_failure_after_deliveries_names_them_and_keeps_its_kind() {
        let delivered = ["aa".to_owned(), "bb".to_owned()];
        let refused = after_delivering(&delivered, Failure::Refused("held".to_owned()));
        assert!(matches!(&refused, Failure::Refused(message)
            if message.contains("after 2 deliveries") && message.contains("aa, bb")
                && message.contains("stay delivered") && message.ends_with("held")));
        let io = after_delivering(&delivered[..1], Failure::Io("disk".to_owned()));
        assert!(
            matches!(&io, Failure::Io(message) if message.contains("aa") && message.ends_with("disk"))
        );
        assert!(
            matches!(after_delivering(&[], Failure::Io("disk".to_owned())),
            Failure::Io(message) if message == "disk")
        );
    }
}
