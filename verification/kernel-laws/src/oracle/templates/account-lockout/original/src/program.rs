//! Reviewed, hand-written decision for one account.
//!
//! The current time and the administrator flag arrive in the request's
//! context, so the decision reads no clock, and re-executing it when the
//! database reopens sees the same time.

use crate::{bindings::*, generated::*, profile};
use zeno_fcis_authority::{CatalogTransitionProgram, ReviewedTransitionInput};
use zeno_fcis_codec::Hash32;
use zeno_fcis_core::BudgetUsed;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_schema::ValidationLimits;
use zeno_fcis_transition::TransitionDecision;

/// Consecutive failed logins that lock the account.
pub const LOCK_AFTER_FAILURES: i128 = 3;
/// Seconds a lock lasts.
pub const LOCK_SECONDS: i128 = 900;
/// Where security alerts are delivered.
pub const ALERT_DESTINATION: &str = "security-team";

pub struct AccountProgram;

impl CatalogTransitionProgram<RustCryptoSha256> for AccountProgram {
    type Error = GeneratedProjectError;

    fn transition_build_hash(&self) -> Hash32 {
        profile::program_hash()
    }

    fn execute(
        &self,
        input: ReviewedTransitionInput<'_>,
    ) -> Result<TransitionDecision, Self::Error> {
        let project = GeneratedProject::try_new::<RustCryptoSha256>()?;
        let command = AccountCommand::try_from_value(input.command().value().value().clone())?;
        let context = RequestContext::try_from_value(input.context().value().value().clone())?;
        let admitted_command =
            project.admit_command::<RustCryptoSha256>(&command, ValidationLimits::default())?;
        let admitted_context =
            project.admit_context::<RustCryptoSha256>(&context, ValidationLimits::default())?;
        let mut transition = project.begin_bound_transition::<RustCryptoSha256>(
            input.pre_state(),
            input.state_domain(),
            &admitted_command,
            &admitted_context,
            input.expected_bindings(),
            BudgetUsed::default(),
            input.limits(),
        )?;
        transition.observe_context_now()?;
        let now = context.now.0;
        let failed = transition.read_failed_attempts()?.0;
        let locked_until = transition.read_locked_until()?.0;
        let last_seen = transition.read_last_seen()?.0;

        // Time comes from the caller, so it must never move backwards;
        // otherwise a stale clock could end a lock early.
        if now < last_seen {
            transition.require(false, RejectReasonId::Reason200)?;
            return transition.seal();
        }
        match command {
            AccountCommand::LoginSucceeded | AccountCommand::LoginFailed if now < locked_until => {
                transition.require(false, RejectReasonId::Reason201)?;
            }
            AccountCommand::LoginSucceeded => {
                transition.update_failed_attempts(&Attempts(0))?;
                transition.update_last_seen(&UnixTime(now))?;
            }
            AccountCommand::LoginFailed => {
                // A failed login is still a fact worth keeping, so it commits
                // as a failure instead of being rejected.
                transition.update_last_seen(&UnixTime(now))?;
                if failed + 1 == LOCK_AFTER_FAILURES {
                    let until = now + LOCK_SECONDS;
                    transition.update_failed_attempts(&Attempts(0))?;
                    transition.update_locked_until(&LockDeadline(until))?;
                    transition.enqueue_channel_300(
                        0,
                        &AlertDestination(ALERT_DESTINATION.into()),
                        &SecurityAlert {
                            alert_kind: AlertKind::Locked,
                            alert_until: LockDeadline(until),
                        },
                    )?;
                } else {
                    transition.update_failed_attempts(&Attempts(failed + 1))?;
                }
                transition.fail_if(true, CommittedFailureReasonId::Reason203)?;
            }
            AccountCommand::AdminUnlock => {
                transition.observe_context_admin()?;
                if context.admin.0 {
                    transition.update_failed_attempts(&Attempts(0))?;
                    transition.update_locked_until(&LockDeadline(0))?;
                    transition.update_last_seen(&UnixTime(now))?;
                    transition.enqueue_channel_300(
                        0,
                        &AlertDestination(ALERT_DESTINATION.into()),
                        &SecurityAlert {
                            alert_kind: AlertKind::Unlocked,
                            alert_until: LockDeadline(0),
                        },
                    )?;
                } else {
                    transition.require(false, RejectReasonId::Reason202)?;
                }
            }
        }
        transition.seal()
    }
}
