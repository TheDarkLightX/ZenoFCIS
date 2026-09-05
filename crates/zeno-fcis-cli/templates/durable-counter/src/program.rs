//! Pure reviewed transition. All state access and staging use generated typed methods.

use crate::{bindings::*, generated::*, profile};
use zeno_fcis_authority::{CatalogTransitionProgram, ReviewedTransitionInput};
use zeno_fcis_codec::Hash32;
use zeno_fcis_core::BudgetUsed;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_schema::ValidationLimits;
use zeno_fcis_transition::TransitionDecision;

pub struct CounterProgram;

impl CatalogTransitionProgram<RustCryptoSha256> for CounterProgram {
    type Error = GeneratedProjectError;

    fn transition_build_hash(&self) -> Hash32 {
        profile::program_hash()
    }

    fn execute(
        &self,
        input: ReviewedTransitionInput<'_>,
    ) -> Result<TransitionDecision, Self::Error> {
        let project = GeneratedProject::try_new::<RustCryptoSha256>()?;
        let command = CounterCommand::try_from_value(input.command().value().value().clone())?;
        let context = CounterContext::try_from_value(input.context().value().value().clone())?;
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
        transition.observe_context_root()?;
        transition.require(context.0, RejectReasonId::Reason200)?;
        let count = transition.read_count()?;
        let failures = transition.read_failures()?;
        let selected = match command {
            CounterCommand::Increment => count.0,
            CounterCommand::RecordFailure => failures.0,
        };
        transition.require(selected < 3, RejectReasonId::Reason201)?;
        // Rejection seals without staging out-of-range values or delivery obligations.
        if !context.0 || selected >= 3 {
            return transition.seal();
        }
        let (count, failures) = match command {
            CounterCommand::Increment => {
                let count = CounterValue(count.0 + 1);
                transition.update_count(&count)?;
                (count, failures)
            }
            CounterCommand::RecordFailure => {
                let failures = CounterValue(failures.0 + 1);
                transition.update_failures(&failures)?;
                transition.fail_if(true, CommittedFailureReasonId::Reason202)?;
                (count, failures)
            }
        };
        transition.enqueue_channel_300(
            0,
            &NotificationDestination("local-observer".into()),
            &Notification {
                notified_count: count,
                notified_failures: failures,
            },
        )?;
        transition.seal()
    }
}
