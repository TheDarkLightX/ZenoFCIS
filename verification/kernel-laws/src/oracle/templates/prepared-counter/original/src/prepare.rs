//! Private preparation followed by ordinary authorization and atomic publication.

use crate::{
    AppResult, Authority, Shell, bindings::GeneratedProject, checked, delivery::Destination,
    generated::*, laws::CounterLaws, profile, program::CounterProgram,
};
use zeno_fcis_authority::{CatalogAuthorizedTransition, InvocationWitness};
use zeno_fcis_codec::{CanonicalEncode, Domain, Hash32};
use zeno_fcis_core::{BudgetLimits, Decision, Resource};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_schema::ValidationLimits;
use zeno_fcis_shell::CommitStatus;
use zeno_fcis_shell_sqlite::{CrashPoint, StoredSnapshot};
use zeno_fcis_synthesis::finite::preparation::{
    PreparationContext, PreparationLimits, PreparedFold,
};
use zeno_fcis_synthesis::finite::{Domain as FiniteDomain, Op, Program};

pub type Invocation = InvocationWitness<RustCryptoSha256, CounterProgram, CounterLaws, Destination>;
pub type Authorized =
    CatalogAuthorizedTransition<RustCryptoSha256, CounterProgram, CounterLaws, Destination>;

/// Maximum aggregate bytes of raw command, successor, authorization and bundle.
/// The complete bundle includes the patch, receipt, commit plan and outbox.
/// This profile bound is not a limit on SQLite files, allocator overhead or time.
pub const MAX_PUBLICATION_BYTES: usize = 16_384;

pub fn command(deltas: [i128; 3]) -> CounterCommand {
    CounterCommand {
        first: CounterDelta(deltas[0]),
        second: CounterDelta(deltas[1]),
        third: CounterDelta(deltas[2]),
    }
}

fn context(witness: &Invocation, version: u64) -> PreparationContext {
    PreparationContext {
        state_root: witness.pre_root(),
        state_version: version,
        invocation_hash: witness.invocation_id(),
    }
}

pub fn admit(
    authority: &Authority,
    state: &CounterState,
    command: &CounterCommand,
    allowed: bool,
    replay: Hash32,
) -> AppResult<Invocation> {
    let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
    let pre = checked(project.admit_root::<RustCryptoSha256>(state, ValidationLimits::default()))?;
    let command =
        checked(project.admit_command::<RustCryptoSha256>(command, ValidationLimits::default()))?;
    let context =
        checked(project.admit_context::<RustCryptoSha256>(
            &CounterContext(allowed),
            ValidationLimits::default(),
        ))?;
    checked(authority.admit_invocation(
        pre,
        command.admitted().clone(),
        context.admitted().clone(),
        profile::digest(
            "example/prepared-counter/principal",
            b"local tutorial operator",
        ),
        profile::digest(
            "example/prepared-counter/authentication",
            b"trusted local input; no remote authentication",
        ),
        replay,
    ))
}

/// Owns the admitted invocation and inputs. Dropping this value cancels it.
/// Recovery repeats `start` and the original ordered input; no checkpoint is trusted.
pub struct PreparedBatch {
    witness: Invocation,
    fold: PreparedFold,
}

impl PreparedBatch {
    pub fn start(
        authority: &Authority,
        snapshot: &StoredSnapshot,
        command: &CounterCommand,
        allowed: bool,
        replay: &str,
    ) -> AppResult<Self> {
        let state = checked(CounterState::try_from_value(snapshot.state().clone()))?;
        let witness = admit(
            authority,
            &state,
            command,
            allowed,
            profile::digest("example/prepared-counter/replay", replay.as_bytes()),
        )?;
        if !allowed {
            return Err("denied context".into());
        }
        let scalar = FiniteDomain::Int { min: 0, max: 3 };
        let program = checked(Program::try_new(
            vec![scalar, FiniteDomain::Int { min: -1, max: 1 }],
            vec![scalar],
            vec![Op::Input(0), Op::Input(1), Op::Add(0, 1)],
            vec![2],
        ))?;
        let items = [command.first.0, command.second.0, command.third.0]
            .into_iter()
            .map(|delta| checked(i64::try_from(delta)).map(|value| vec![value]))
            .collect::<AppResult<Vec<_>>>()?;
        let fold = checked(PreparedFold::start(
            program,
            vec![checked(i64::try_from(state.count.0))?],
            items,
            context(&witness, snapshot.version()),
            PreparationLimits {
                max_items: 3,
                max_chunk_items: 3,
                max_input_bytes: 98,
                max_output_bytes: 22,
            },
            BudgetLimits::zero()
                .with_limit(Resource::Read, 6)
                .with_limit(Resource::Write, 3)
                .with_limit(Resource::Candidate, 3)
                .with_limit(Resource::Byte, 120),
        ))?;
        Ok(Self { witness, fold })
    }

    pub fn advance(&mut self, offset: u32, count: u32) -> AppResult<()> {
        checked(self.fold.advance(offset, count))
    }

    pub fn processed_items(&self) -> u32 {
        self.fold.processed_items()
    }

    /// Holds the same exclusive shell handle from fresh snapshot through commit.
    /// Another database handle is checked by SQLite's existing version/root gate.
    /// A root that returns to an earlier value still has a different version.
    pub fn publish(
        self,
        shell: &mut Shell,
        authority: &Authority,
        current_allowed: bool,
        byte_limit: usize,
        crash: Option<CrashPoint>,
    ) -> AppResult<Publication> {
        let snapshot = checked(shell.snapshot())?;
        let state = checked(CounterState::try_from_value(snapshot.state().clone()))?;
        let command = checked(CounterCommand::try_from_value(
            self.witness.command().value().value().clone(),
        ))?;
        let current = admit(
            authority,
            &state,
            &command,
            current_allowed,
            self.witness.replay_id(),
        )?;
        let output = checked(self.fold.finish(context(&current, snapshot.version())))?;
        let candidate = match checked(authority.execute(current))? {
            Decision::Accept(accept) => accept.into_candidate(),
            Decision::Reject(_) | Decision::CommittedFailure(_) => {
                return Err("prepared operation was not accepted".into());
            }
        };
        let post = checked(candidate.bundle().validate_and_apply::<RustCryptoSha256>(
            snapshot.state(),
            checked(Domain::new("example/prepared-counter/state", 1))?,
        ))?;
        let expected = checked(
            CounterState {
                count: CounterValue(i128::from(output[0])),
            }
            .to_value(),
        )?;
        if post.state() != &expected {
            return Err("prepared result differs from authorized complete state".into());
        }
        let sizes = publication_sizes(&candidate, &expected)?;
        let limit = byte_limit.min(MAX_PUBLICATION_BYTES);
        if sizes.total() > limit {
            return Err(format!(
                "publication capacity: required={} declared={limit}",
                sizes.total()
            ));
        }
        let authorization = checked(candidate.canonical_bytes())?;
        let status = checked(shell.commit_with_crash_point(candidate, crash))?;
        Ok(Publication {
            status,
            authorization,
            sizes,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublicationSizes {
    pub command: usize,
    pub state: usize,
    pub authorization: usize,
    pub bundle: usize,
    /// Included in `bundle`, reported separately for inspection.
    pub receipt: usize,
    /// Included in `bundle`, reported separately for inspection.
    pub outbox: usize,
}
impl PublicationSizes {
    pub fn total(self) -> usize {
        self.command + self.state + self.authorization + self.bundle
    }
}

pub fn publication_sizes(
    candidate: &Authorized,
    post: &zeno_fcis_value::Value,
) -> AppResult<PublicationSizes> {
    Ok(PublicationSizes {
        command: checked(
            candidate
                .invocation()
                .command()
                .value()
                .value()
                .canonical_bytes(),
        )?
        .len(),
        state: checked(post.canonical_bytes())?.len(),
        authorization: checked(candidate.canonical_bytes())?.len(),
        bundle: checked(candidate.bundle().canonical_bytes())?.len(),
        receipt: checked(candidate.bundle().receipt().canonical_bytes())?.len(),
        outbox: checked(candidate.bundle().outbox_plan().canonical_bytes())?.len(),
    })
}

pub struct Publication {
    pub status: CommitStatus,
    pub authorization: Vec<u8>,
    pub sizes: PublicationSizes,
}
