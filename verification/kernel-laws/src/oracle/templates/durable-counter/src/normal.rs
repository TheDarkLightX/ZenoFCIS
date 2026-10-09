#![forbid(unsafe_code)]

/// Complete original-domain policy and checked library Authority construction.
#[path = "v2_contract.rs"]
pub mod v2_contract;

pub use super::{bindings, generated, delivery, profile};

use bindings::GeneratedProject;
use delivery::Destination;
use generated::{CounterCommand, CounterContext};
use generated::{CounterState, CounterValue};
use std::path::Path;
use zeno_fcis_codec::{CanonicalEncode, DecodeLimits, decode_envelope};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_schema::ValidationLimits;
use zeno_fcis_shell::CommitStatus;
use zeno_fcis_synthesis::finite::v2_composition as composition;
use zeno_fcis_synthesis::finite::{v2_authority::PublicationOutcome, v2_composition::Class};

/// Library authority borrowing the exact checked policy descriptor.
pub type Authority<'p> = zeno_fcis_synthesis::finite::v2_authority::Authority<'p>;
/// Persistent shell consuming genuine library publication capabilities.
pub type Shell<'a, 'p> = zeno_fcis_shell_sqlite::v2::V2SqliteShell<'a, 'p>;
pub type AppResult<T> = Result<T, String>;

fn checked<T, E: std::fmt::Debug>(value: Result<T, E>) -> AppResult<T> {
    value.map_err(|error| format!("{error:?}"))
}

/// The exact reviewed initial counter state.
#[must_use]
pub fn genesis_state() -> CounterState {
    CounterState {
        count: CounterValue(0),
        failures: CounterValue(0),
    }
}

/// Bind only the checked complete original schema and retained reviewed policy.
/// The caller owns Contract, then Descriptor, then this borrowing Authority.
///
/// # Errors
/// Returns the actual catalog or authority refusal.
pub fn authority<'p>(descriptor: &'p composition::Descriptor<'p>) -> AppResult<Authority<'p>> {
    v2_contract::checked_authority(descriptor).map_err(|error| match error {
        v2_contract::BindFailure::Catalog(error) => format!("catalog: {error:?}"),
        v2_contract::BindFailure::Authority(error) => format!("authority: {error:?}"),
    })
}

/// Publish the actual reviewed initial state through genuine Genesis.
///
/// # Errors
/// Returns schema/framing/genesis or persistent-shell refusal. Existing history is refused.
pub fn create<'a, 'p>(path: &Path, authority: &'a Authority<'p>) -> AppResult<Shell<'a, 'p>> {
    let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
    let initial = checked(
        project.admit_root::<RustCryptoSha256>(&genesis_state(), ValidationLimits::default()),
    )?;
    let initial = checked(initial.envelope().canonical_bytes())?;
    let genesis = match authority.publish_genesis(&initial) {
        PublicationOutcome::Commit(genesis) => genesis,
        PublicationOutcome::Refused { evaluation, error } => {
            return Err(format!("genesis: {error:?}; evaluation: {evaluation:?}"));
        }
        other => return Err(format!("unsupported genesis outcome: {other:?}")),
    };
    checked(Shell::create(path, authority, genesis))
}

/// Decide exact original inputs, publish a genuine capability, and recompute its exact replay.
/// Local principal/context authentication and input framing are host assumptions.
///
/// # Errors
/// Returns schema admission, actual technical refusal, or shell/replay failure.
pub fn invoke(
    shell: &mut Shell<'_, '_>,
    authority: &Authority<'_>,
    command: CounterCommand,
    allowed: bool,
    replay: &str,
) -> AppResult<&'static str> {
    let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
    let snapshot = checked(shell.snapshot())?;
    let command =
        checked(project.admit_command::<RustCryptoSha256>(&command, ValidationLimits::default()))?;
    let context =
        checked(project.admit_context::<RustCryptoSha256>(
            &CounterContext(allowed),
            ValidationLimits::default(),
        ))?;
    let command = checked(command.admitted().envelope().canonical_bytes())?;
    let context = checked(context.admitted().envelope().canonical_bytes())?;
    let original = composition::Raw {
        state: snapshot.state(),
        command: &command,
        context: &context,
    };
    let publication = match authority.publish(original) {
        PublicationOutcome::Commit(publication) => publication,
        PublicationOutcome::Reject(_) => return Ok("Reject"),
        PublicationOutcome::Refused { evaluation, error } => {
            return Err(format!(
                "publication: {error:?}; evaluation: {evaluation:?}"
            ));
        }
        other => return Err(format!("unsupported publication outcome: {other:?}")),
    };
    let outcome = match checked(publication.evaluation().result())?.class() {
        Class::Accept => "Accept",
        Class::CommittedFailure => "CommittedFailure",
        Class::Reject => return Err("reject yielded a publication capability".into()),
        other => return Err(format!("unsupported decision class: {other:?}")),
    };
    let subject = publication.subject().to_vec();
    let replay_id = profile::digest("example/durable-counter/replay", replay.as_bytes());
    if checked(shell.commit(replay_id, publication))?.status() != CommitStatus::Committed {
        return Err("expected first publication".into());
    }
    let replayed = match authority.replay_publication(original, &subject) {
        PublicationOutcome::Commit(publication) => publication,
        other => return Err(format!("exact publication replay refused: {other:?}")),
    };
    if checked(shell.commit(replay_id, replayed))?.status() != CommitStatus::IdempotentReplay {
        return Err("exact replay was not idempotent".into());
    }
    Ok(outcome)
}

/// Decode an already checked snapshot only for typed host presentation.
/// The decoded Value is never supplied to core evaluation.
///
/// # Errors
/// Returns malformed envelope or typed presentation failure.
pub fn decode_state(original: &[u8]) -> AppResult<CounterState> {
    let envelope = checked(decode_envelope(original, DecodeLimits::default()))?;
    checked(CounterState::try_from_value(envelope.into_value()))
}

/// Exercises commit, rejected input, intentional committing failure, restart, and delivery retry.
pub fn journey(path: &Path) -> AppResult<String> {
    let contract = v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = authority(&descriptor)?;
    let destination = Destination::default();
    let mut shell = create(path, &authority)?;
    if invoke(
        &mut shell,
        &authority,
        CounterCommand::Increment,
        true,
        "increment-1",
    )? != "Accept"
    {
        return Err("accept journey".into());
    }
    let before = checked(shell.snapshot())?;
    if invoke(
        &mut shell,
        &authority,
        CounterCommand::Increment,
        false,
        "denied-1",
    )? != "Reject"
    {
        return Err("reject journey".into());
    }
    let after = checked(shell.snapshot())?;
    if before != after {
        return Err("rejection published state, replay or outbox data".into());
    }
    if invoke(
        &mut shell,
        &authority,
        CounterCommand::RecordFailure,
        true,
        "failure-1",
    )? != "CommittedFailure"
    {
        return Err("failure journey".into());
    }
    let pending = checked(shell.next_pending())?.ok_or("missing pending delivery")?;
    // Model interruption after the destination records delivery but before SQLite acknowledges it.
    let delivered = checked(pending.deliver(&mut destination.memory()))?;
    drop(delivered);
    drop(shell);
    let mut shell = checked(Shell::open(path, &authority))?;
    while let Some(pending) = checked(shell.next_pending())? {
        let delivered = checked(pending.deliver(&mut destination.memory()))?;
        checked(delivered.acknowledge())?;
    }
    let snapshot = checked(shell.snapshot())?;
    let state = decode_state(snapshot.state())?;
    if (
        state.count.0,
        state.failures.0,
        snapshot.version(),
        snapshot.pending(),
        destination.delivered_count(),
    ) != (1, 1, 2, 0, 2)
    {
        return Err("lifecycle differs from the independent expected result".into());
    }
    Ok("Accept, Reject, CommittedFailure; replay and restart checked; count=1 failures=1 bundles=2 deliveries=2 pending=0".into())
}
