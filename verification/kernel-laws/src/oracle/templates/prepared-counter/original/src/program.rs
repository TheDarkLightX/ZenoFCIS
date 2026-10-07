//! Pure typed transition, independently evaluated again at authorization.

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
            // Preparation reserves its own modeled costs. These are not a
            // claim about the runtime cost of this separate native evaluator.
            BudgetUsed::default(),
            input.limits(),
        )?;
        transition.observe_context_root()?;
        let mut count = transition.read_count()?.0;
        if !context.0 {
            transition.require(false, RejectReasonId::Reason200)?;
            return transition.seal();
        }
        // The admitted domains bound this arithmetic to -1..4. Each prefix,
        // rather than only the final sum, must remain in the state domain.
        for delta in [command.first.0, command.second.0, command.third.0] {
            count += delta;
            if !(0..=3).contains(&count) {
                transition.require(false, RejectReasonId::Reason201)?;
                return transition.seal();
            }
        }
        transition.update_count(&CounterValue(count))?;
        transition.enqueue_channel_300(
            0,
            &NotificationDestination("local-observer".into()),
            &Notification {
                notified_count: CounterValue(count),
            },
        )?;
        transition.seal()
    }
}
