//! Independent, bounded checks over the exact invocation and complete decision.
//!
//! The authored formulas in `project.zeno` are evaluated as written. Formulas
//! cannot see the decision's reason, its outbox, or its effects, so this file
//! checks those in Rust, from the README's rules rather than from the program.

use crate::{generated::*, profile};
use zeno_fcis_codec::{CanonicalEncode, Hash32};
use zeno_fcis_evidence::EvidenceEnvelope;
use zeno_fcis_laws::*;
use zeno_fcis_spec::*;

/// The failure count at which the next failed login locks the account.
const COUNT_BEFORE_LOCK: i128 = 2;
/// The only destination alerts may go to.
const ALERTS_TO: &str = "security-team";

pub struct AccountLaws {
    project: ProjectSpec,
}

impl Default for AccountLaws {
    fn default() -> Self {
        Self {
            project: profile::project(),
        }
    }
}

struct NoPredicates;
impl PredicateProvider for NoPredicates {
    fn evaluate(&self, _: &Identifier, _: &[i128]) -> Option<bool> {
        None
    }
}

pub struct NoExternalProofs;
impl LawEvidenceVerifier for NoExternalProofs {
    fn verifier_identity(&self) -> Hash32 {
        profile::digest(
            "example/account-lockout/verifier",
            b"External proofs are not admitted",
        )
    }
    fn verify(&self, _: &LawProofSubject, _: &EvidenceEnvelope, _: &[u8]) -> LawProofDecision {
        LawProofDecision::Indeterminate
    }
}

/// The variant ID that `command.101` observes.
fn command_id(command: &AccountCommand) -> i128 {
    match command {
        AccountCommand::LoginSucceeded => 120,
        AccountCommand::LoginFailed => 121,
        AccountCommand::AdminUnlock => 122,
    }
}

/// One observation at a reviewed path. Only a path the library refuses can
/// fail, which the static IDs here never cause.
fn observe(
    root: ProjectionRoot,
    raw: &[u32],
    value: i128,
) -> Result<Observation, LawEngineFailure> {
    let segments = raw
        .iter()
        .map(|id| StableId::new(*id).ok_or(LawEngineFailure::Unsupported))
        .collect::<Result<Vec<_>, _>>()?;
    let path = ProjectionPath::try_new(root, segments).ok_or(LawEngineFailure::Unsupported)?;
    Ok(Observation::new(path, value))
}

/// Observes one account under `root`, at the numeric IDs of `project.zeno`.
///
/// The law checker observes the account before and after every decision
/// through this one mapping, so an invariant over `pre.` paths, such as
/// claim 600, reads the same fields at genesis, before a step, and after it,
/// which the induction in `tests/claims.rs` relies on.
///
/// # Errors
///
/// `LawEngineFailure::Unsupported` if a projection cannot be formed, which
/// the static IDs here never cause.
pub fn state_observations(
    root: ProjectionRoot,
    state: &Account,
) -> Result<Vec<Observation>, LawEngineFailure> {
    Ok(vec![
        observe(root, &[100, 110], state.failed_attempts.0)?,
        observe(root, &[100, 111], state.locked_until.0)?,
        observe(root, &[100, 112], state.last_seen.0)?,
    ])
}

/// The observations the formulas read from one decision.
fn trace_step(
    pre: &Account,
    post: &Account,
    command: &AccountCommand,
    context: &RequestContext,
) -> Result<TraceStep, LawEngineFailure> {
    let mut step = state_observations(ProjectionRoot::Pre, pre)?;
    step.extend(state_observations(ProjectionRoot::Post, post)?);
    step.extend([
        observe(ProjectionRoot::Command, &[101], command_id(command))?,
        observe(ProjectionRoot::Context, &[102, 130], context.now.0)?,
        observe(
            ProjectionRoot::Context,
            &[102, 131],
            i128::from(context.admin.0),
        )?,
    ]);
    TraceStep::try_new(step).ok_or(LawEngineFailure::Unsupported)
}

/// The rejection reason the README requires, or `None` if the input must commit.
fn expected_rejection(
    pre: &Account,
    command: &AccountCommand,
    context: &RequestContext,
) -> Option<u32> {
    let now = context.now.0;
    if now < pre.last_seen.0 {
        return Some(200);
    }
    match command {
        AccountCommand::LoginSucceeded | AccountCommand::LoginFailed
            if now < pre.locked_until.0 =>
        {
            Some(201)
        }
        AccountCommand::AdminUnlock if !context.admin.0 => Some(202),
        _ => None,
    }
}

/// The one alert a committing decision must queue, if any.
fn expected_alert(
    pre: &Account,
    post: &Account,
    command: &AccountCommand,
) -> Option<SecurityAlert> {
    match command {
        AccountCommand::LoginFailed if pre.failed_attempts.0 == COUNT_BEFORE_LOCK => {
            Some(SecurityAlert {
                alert_kind: AlertKind::Locked,
                alert_until: post.locked_until,
            })
        }
        AccountCommand::AdminUnlock => Some(SecurityAlert {
            alert_kind: AlertKind::Unlocked,
            alert_until: LockDeadline(0),
        }),
        _ => None,
    }
}

impl AccountLaws {
    fn check(
        &self,
        law_id: u32,
        step: &TraceStep,
        extra: bool,
        input: &[u8],
    ) -> Result<LawObservation, LawEngineFailure> {
        let law = self
            .project
            .laws()
            .iter()
            .find(|law| law.id().get() == law_id)
            .ok_or(LawEngineFailure::Unsupported)?;
        let result = evaluate_relational(
            law.formula(),
            EvaluationContext::new(step, &NoPredicates, EvalLimits::default()),
        );
        let status = match (result, extra) {
            (EvalOutcome::True, true) => LawStatus::Satisfied,
            (EvalOutcome::Indeterminate(_), _) => LawStatus::Indeterminate,
            _ => LawStatus::Violated,
        };
        let mut witness = input.to_vec();
        witness.extend_from_slice(&law_id.to_be_bytes());
        witness.push(status as u8);
        LawObservation::try_new(
            profile::id(law_id),
            status,
            profile::digest("example/account-lockout/observation", &witness),
        )
        .map_err(|_| LawEngineFailure::InvalidOutput)
    }
}

impl ProjectLawEngine for AccountLaws {
    fn evaluate(
        &self,
        input: &LawCheckInput<'_>,
        limits: LawLimits,
    ) -> Result<Vec<LawObservation>, LawEngineFailure> {
        if limits.max_observations < 3 {
            return Err(LawEngineFailure::Incomplete);
        }
        let pre = Account::try_from_value(input.pre_state().clone())
            .map_err(|_| LawEngineFailure::Unsupported)?;
        let command = AccountCommand::try_from_value(input.command().clone())
            .map_err(|_| LawEngineFailure::Unsupported)?;
        let context = RequestContext::try_from_value(input.context().clone())
            .map_err(|_| LawEngineFailure::Unsupported)?;
        let (post, commit, outbox, relations, reason_ok): (_, _, _, &[u32], _) =
            match input.decision() {
                LawDecisionView::Accept {
                    post_state,
                    commit_plan,
                    outbox_plan,
                    ..
                } => (post_state, commit_plan, outbox_plan, &[501, 502], true),
                LawDecisionView::CommittedFailure {
                    reason_id,
                    post_state,
                    commit_plan,
                    outbox_plan,
                    ..
                } => (
                    post_state,
                    commit_plan,
                    outbox_plan,
                    &[503],
                    *reason_id == 203,
                ),
                LawDecisionView::Reject { reason_id } => {
                    return if expected_rejection(&pre, &command, &context) == Some(*reason_id) {
                        Ok(vec![])
                    } else {
                        Err(LawEngineFailure::InvalidOutput)
                    };
                }
            };
        let post =
            Account::try_from_value((*post).clone()).map_err(|_| LawEngineFailure::Unsupported)?;
        let committed_while_rejectable = expected_rejection(&pre, &command, &context).is_some();
        let destination = AlertDestination(ALERTS_TO.into())
            .to_value()
            .map_err(|_| LawEngineFailure::Unsupported)?;
        let delivery_ok = match (outbox.entries(), expected_alert(&pre, &post, &command)) {
            ([], None) => true,
            ([entry], Some(alert)) => {
                let payload = alert
                    .to_value()
                    .map_err(|_| LawEngineFailure::Unsupported)?;
                entry.ordinal() == 0
                    && entry.channel() == 300
                    && entry.destination() == &destination
                    && entry.payload() == &payload
            }
            _ => false,
        };
        let extra =
            reason_ok && !committed_while_rejectable && delivery_ok && commit.effects().is_empty();
        let step = trace_step(&pre, &post, &command, &context)?;
        let bytes = input
            .canonical_bytes()
            .map_err(|_| LawEngineFailure::InvalidOutput)?;
        let mut checked = vec![self.check(500, &step, true, &bytes)?];
        for relation in relations {
            checked.push(self.check(*relation, &step, extra, &bytes)?);
        }
        Ok(checked)
    }

    fn evaluate_genesis(
        &self,
        input: &GenesisLawCheckInput<'_>,
        limits: LawLimits,
    ) -> Result<Vec<LawObservation>, LawEngineFailure> {
        if limits.max_observations == 0 {
            return Err(LawEngineFailure::Incomplete);
        }
        let state = Account::try_from_value(input.initial_state().clone())
            .map_err(|_| LawEngineFailure::Unsupported)?;
        let step = trace_step(
            &state,
            &state,
            &AccountCommand::LoginSucceeded,
            &RequestContext {
                now: UnixTime(0),
                admin: AdminFlag(false),
            },
        )?;
        let bytes = input
            .canonical_bytes()
            .map_err(|_| LawEngineFailure::InvalidOutput)?;
        let zero =
            state.failed_attempts.0 == 0 && state.locked_until.0 == 0 && state.last_seen.0 == 0;
        Ok(vec![self.check(500, &step, zero, &bytes)?])
    }
}
