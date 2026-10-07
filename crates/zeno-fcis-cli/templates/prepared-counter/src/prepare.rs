//! Checked ordered continuation followed by complete original-byte publication.
//! The cursor result is untrusted data. Only the current admitted history grants
//! the distinct publication capability consumed by the schema9 SQLite shell.
use crate::{
    AppResult, Authority, Shell, bindings::GeneratedProject, checked, decode_state, generated::*,
    profile,
};
use zeno_fcis_codec::{CanonicalEncode, Hash32};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_schema::ValidationLimits;
use zeno_fcis_shell::CommitStatus;
use zeno_fcis_shell_sqlite::{
    CrashPoint,
    v2::{CommitReceipt, PublicationSizes, Snapshot, compact_publication, expand_publication},
};
use zeno_fcis_synthesis::finite::{
    Domain, Op, V2Resource as Resource, V2Usage,
    v2_authority::PublicationOutcome,
    v2_composition::{Class, Raw},
    v2_continuation as continuation, v2_zero_limits,
};
use zeno_fcis_value::Value;

/// Original aggregate: bare command, bare successor, complete R and complete B.
/// Receipt/outbox and every duplicate durable field are included in B. H is
/// admitted separately; its copying and hashing are outside this logical bound.
pub const MAX_PUBLICATION_BYTES: usize = 16_384;
/// Original complete canonical `(initial accumulator, item tuples)` capacity.
pub const MAX_PREPARATION_INPUT_BYTES: u64 = 98;
/// Original complete canonical final accumulator tuple capacity.
pub const MAX_PREPARATION_OUTPUT_BYTES: u64 = 22;
/// Aggregate actual instruction attempts, retained through failed chunks/retries.
pub const MAX_PREPARATION_STEPS: u64 = 64;

/// Exactly the original three scalar fields, with the original generated domains.
pub fn command(deltas: [i128; 3]) -> CounterCommand {
    CounterCommand {
        first: CounterDelta(deltas[0]),
        second: CounterDelta(deltas[1]),
        third: CounterDelta(deltas[2]),
    }
}

fn context(
    snapshot: &Snapshot,
    command: &[u8],
    admitted_context: &[u8],
    replay: Hash32,
) -> AppResult<continuation::Context> {
    let principal = profile::digest(
        "example/prepared-counter/principal",
        b"local tutorial operator",
    );
    let authentication = profile::digest(
        "example/prepared-counter/authentication",
        b"trusted local input; no remote authentication",
    );
    let mut original = b"ZFCIS-PREPARED-CONTEXT\0\x01".to_vec();
    for part in [
        snapshot.binding(),
        snapshot.state(),
        command,
        admitted_context,
        principal.as_bytes().as_slice(),
        authentication.as_bytes().as_slice(),
        replay.as_bytes().as_slice(),
    ] {
        original.extend_from_slice(&checked(u64::try_from(part.len()))?.to_be_bytes());
        original.extend_from_slice(part);
    }
    // Sealed shell SHA-256 association, not a proof of injectivity or authority.
    let invocation = profile::digest("example/prepared-counter/invocation", &original);
    Ok(continuation::Context {
        state_root: *snapshot.root().as_bytes(),
        state_version: snapshot.version(),
        invocation_hash: *invocation.as_bytes(),
    })
}

/// Owns the original admitted bytes and the library's private cursor/meter.
/// Drop cancels; recovery admits and recomputes the same complete original input.
#[must_use]
pub struct PreparedBatch {
    binding: Vec<u8>,
    state: Vec<u8>,
    command: Vec<u8>,
    context: Vec<u8>,
    root: Hash32,
    version: u64,
    replay: Hash32,
    fold: continuation::PreparedFold,
}
impl PreparedBatch {
    /// Admit the actual persisted H and original typed invocation, then reserve
    /// Read6/Write3/Candidate3/Byte120 independently of the genuine Step64 meter.
    /// # Errors
    /// Returns actual history/schema/domain/context or complete reservation refusal.
    pub fn start(
        authority: &Authority<'_>,
        snapshot: &Snapshot,
        command: &CounterCommand,
        allowed: bool,
        replay: &str,
    ) -> AppResult<Self> {
        if authority.identity() != snapshot.binding() {
            return Err("preparation identity differs".into());
        }
        let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
        let state = decode_state(snapshot.state())?;
        let original_state =
            checked(project.admit_root::<RustCryptoSha256>(&state, ValidationLimits::default()))?;
        if checked(original_state.envelope().canonical_bytes())? != snapshot.state() {
            return Err("original state frame differs".into());
        }
        let admitted_command = checked(
            project.admit_command::<RustCryptoSha256>(command, ValidationLimits::default()),
        )?;
        let admitted_context = checked(project.admit_context::<RustCryptoSha256>(
            &CounterContext(allowed),
            ValidationLimits::default(),
        ))?;
        if !allowed {
            return Err("denied context".into());
        }
        let command_bytes = checked(admitted_command.admitted().envelope().canonical_bytes())?;
        let context_bytes = checked(admitted_context.admitted().envelope().canonical_bytes())?;
        let replay = profile::digest("example/prepared-counter/replay", replay.as_bytes());
        let scalar = Domain::Int { min: 0, max: 3 };
        let graph = checked(continuation::admit_graph(
            vec![scalar, Domain::Int { min: -1, max: 1 }],
            vec![scalar],
            vec![Op::Input(0), Op::Input(1), Op::Add(0, 1)],
            vec![2],
        ))?;
        let items = [command.first.0, command.second.0, command.third.0]
            .into_iter()
            .map(|delta| checked(i64::try_from(delta)).map(|value| vec![value]))
            .collect::<AppResult<Vec<_>>>()?;
        let fold = checked(continuation::start(
            graph,
            vec![checked(i64::try_from(state.count.0))?],
            items,
            context(snapshot, &command_bytes, &context_bytes, replay)?,
            continuation::PreparationLimits {
                max_items: 3,
                max_chunk_items: 3,
                max_input_bytes: MAX_PREPARATION_INPUT_BYTES,
                max_output_bytes: MAX_PREPARATION_OUTPUT_BYTES,
            },
            v2_zero_limits()
                .with_limit(Resource::Read, 6)
                .with_limit(Resource::Write, 3)
                .with_limit(Resource::Candidate, 3)
                .with_limit(Resource::Byte, 120)
                .with_limit(Resource::Step, MAX_PREPARATION_STEPS),
        ))?;
        Ok(Self {
            binding: snapshot.binding().to_vec(),
            state: snapshot.state().to_vec(),
            command: command_bytes,
            context: context_bytes,
            root: snapshot.root(),
            version: snapshot.version(),
            replay,
            fold,
        })
    }
    /// Process exactly the next range through the same charged private cursor.
    /// Failed chunks retain attempted work while rolling back cursor/accumulator.
    /// # Errors
    /// Returns the actual library offset/range/evaluation or aggregate budget refusal.
    pub fn advance(&mut self, offset: u32, count: u32) -> AppResult<()> {
        checked(self.fold.advance(offset, count))
    }
    /// Successfully processed original ordered prefix; exposes no partial result.
    #[must_use]
    pub fn processed_items(&self) -> u32 {
        self.fold.processed_items()
    }
    /// Remaining original items; completion remains checked by the library.
    #[must_use]
    pub fn remaining_items(&self) -> u32 {
        self.fold.remaining_items()
    }
    /// Genuine complete logical reservation, before any instruction attempt.
    #[must_use]
    pub fn reserved_budget(&self) -> V2Usage {
        self.fold.reserved_budget()
    }
    /// Actual cumulative library usage, including failed chunks and retries.
    #[must_use]
    pub fn usage(&self) -> V2Usage {
        self.fold.usage()
    }
    /// Exact immutable original data and all eight budgets, independent of progress.
    #[must_use]
    pub fn same_operation(&self, other: &Self) -> bool {
        self.binding == other.binding
            && self.state == other.state
            && self.command == other.command
            && self.context == other.context
            && self.root == other.root
            && self.version == other.version
            && self.replay == other.replay
            && self.fold.same_operation(&other.fold)
    }

    /// Finish against the actual current Snapshot, fully reevaluate the original
    /// command under its admitted persisted H, measure all original components,
    /// then commit with expected-version/root/chain CAS in the checked transaction.
    /// # Errors
    /// Returns incomplete/stale/history/decision/complete capacity or durable refusal.
    pub fn publish(
        self,
        shell: &mut Shell<'_, '_>,
        authority: &Authority<'_>,
        current_allowed: bool,
        byte_limit: usize,
        crash: Option<CrashPoint>,
    ) -> AppResult<Publication> {
        let snapshot = checked(shell.snapshot())?;
        if authority.identity() != snapshot.binding() {
            return Err("publication identity differs".into());
        }
        let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
        let fresh = checked(project.admit_context::<RustCryptoSha256>(
            &CounterContext(current_allowed),
            ValidationLimits::default(),
        ))?;
        let fresh_context = checked(fresh.admitted().envelope().canonical_bytes())?;
        let output = checked(self.fold.finish(context(
            &snapshot,
            &self.command,
            &fresh_context,
            self.replay,
        )?))?;
        if snapshot.binding() != self.binding
            || snapshot.state() != self.state
            || snapshot.root() != self.root
            || snapshot.version() != self.version
            || fresh_context != self.context
            || !current_allowed
        {
            return Err("stale preparation context".into());
        }
        let publication = match authority.publish(Raw {
            state: snapshot.state(),
            command: &self.command,
            context: &fresh_context,
        }) {
            PublicationOutcome::Commit(p) => p,
            PublicationOutcome::Reject(evaluation) => {
                return Err(format!(
                    "prepared operation rejected: {:?}",
                    evaluation.result()
                ));
            }
            other => return Err(format!("complete original decision refused: {other:?}")),
        };
        if checked(publication.evaluation().result())?.class() != Class::Accept {
            return Err("prepared operation was not accepted".into());
        }
        let [count] = output.as_slice() else {
            return Err("complete accumulator shape differs".into());
        };
        let expected = checked(project.admit_root::<RustCryptoSha256>(
            &CounterState {
                count: CounterValue(i128::from(*count)),
            },
            ValidationLimits::default(),
        ))?;
        if publication.poststate() != checked(expected.envelope().canonical_bytes())?
            || !publication.effects().is_empty()
            || publication.outbox().len() != 1
        {
            return Err("checked fold differs from complete genuine publication".into());
        }
        let entry = &publication.outbox()[0];
        let destination = checked(NotificationDestination("local-observer".into()).to_value())?;
        let payload = checked(
            Notification {
                notified_count: CounterValue(i128::from(*count)),
            }
            .to_value(),
        )?;
        if (
            entry.ordinal(),
            entry.channel(),
            entry.destination_root(),
            entry.payload_root(),
        ) != (0, 300, 103, 104)
            || entry.destination() != checked(destination.canonical_bytes())?
            || entry.payload() != checked(payload.canonical_bytes())?
            || entry.idempotency() != checked(Value::unsigned(0).canonical_bytes())?
        {
            return Err("complete notification differs".into());
        }
        let sizes =
            checked(shell.publication_bundle(snapshot.version(), self.replay, &publication))?
                .sizes();
        let required = checked(sizes.total())?;
        let limit = byte_limit.min(MAX_PUBLICATION_BYTES);
        if required > limit {
            return Err(format!(
                "publication capacity: required={required} declared={limit}"
            ));
        }
        let authorization = checked(compact_publication(
            publication.identity(),
            publication.subject(),
        ))?;
        let receipt = checked(shell.commit_bounded_at(
            snapshot.version(),
            self.replay,
            publication,
            limit,
            crash,
        ))?;
        Ok(Publication {
            status: receipt.status(),
            authorization,
            sizes,
            receipt,
            binding: self.binding,
            state: self.state,
            command: self.command,
            context: self.context,
            replay: self.replay,
        })
    }
}

/// Owned descriptive artifacts from an actual completed commit, not a capability.
#[derive(Debug)]
pub struct Publication {
    pub status: CommitStatus,
    pub authorization: Vec<u8>,
    pub sizes: PublicationSizes,
    pub receipt: CommitReceipt,
    binding: Vec<u8>,
    state: Vec<u8>,
    command: Vec<u8>,
    context: Vec<u8>,
    replay: Hash32,
}
impl Publication {
    /// Re-admit the original source H and recheck every original R byte before retry.
    /// A different destination binding cannot replace the retained source binding.
    /// Ordinary exact replay remains idempotent even after another head commits.
    /// # Errors
    /// Returns history, complete replay or durable identity refusal.
    pub fn replay(
        &self,
        shell: &mut Shell<'_, '_>,
        authority: &Authority<'_>,
    ) -> AppResult<CommitReceipt> {
        if authority.identity() != self.binding {
            return Err("publication identity differs".into());
        }
        let full = checked(expand_publication(&self.binding, &self.authorization))?;
        if checked(shell.binding())? != self.binding {
            return Err("publication belongs to a different history binding".into());
        }
        let PublicationOutcome::Commit(publication) = authority.replay_publication(
            Raw {
                state: &self.state,
                command: &self.command,
                context: &self.context,
            },
            &full,
        ) else {
            return Err("exact original replay refused".into());
        };
        checked(shell.commit(self.replay, publication))
    }
}
