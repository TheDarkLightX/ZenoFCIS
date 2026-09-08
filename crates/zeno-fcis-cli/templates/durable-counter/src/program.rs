//! Pure reviewed transition. All state access and staging use generated typed methods.

use crate::{bindings::*, generated::*, profile, synthesized};
use zeno_fcis_authority::{CatalogTransitionProgram, ReviewedTransitionInput};
use zeno_fcis_codec::Hash32;
use zeno_fcis_core::BudgetUsed;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_schema::ValidationLimits;
use zeno_fcis_transition::TransitionDecision;

pub struct CounterProgram;

#[derive(Debug)]
pub enum CounterProgramError {
    Project(Box<GeneratedProjectError>),
    SynthesisDomain,
}
impl From<GeneratedProjectError> for CounterProgramError {
    fn from(error: GeneratedProjectError) -> Self {
        Self::Project(Box::new(error))
    }
}
impl From<AdapterError> for CounterProgramError {
    fn from(error: AdapterError) -> Self {
        GeneratedProjectError::from(error).into()
    }
}

impl CatalogTransitionProgram<RustCryptoSha256> for CounterProgram {
    type Error = CounterProgramError;

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
        let count = transition.read_count()?;
        let failures = transition.read_failures()?;
        let command_bit = i64::from(matches!(command, CounterCommand::RecordFailure));
        let output = synthesized::transition(&[
            i64::try_from(count.0).map_err(|_| CounterProgramError::SynthesisDomain)?,
            i64::try_from(failures.0).map_err(|_| CounterProgramError::SynthesisDomain)?,
            command_bit,
            i64::from(context.0),
        ])
        .ok_or(CounterProgramError::SynthesisDomain)?;
        // The generated function returns data. This reviewed adapter maps the
        // complete finite decision into existing typed staging operations.
        match output[0] {
            0 => {
                transition.require(false, RejectReasonId::Reason200)?;
                return Ok(transition.seal()?);
            }
            1 => {
                transition.require(false, RejectReasonId::Reason201)?;
                return Ok(transition.seal()?);
            }
            2 => {
                transition.update_count(&CounterValue(i128::from(output[1])))?;
            }
            3 => {
                transition.update_failures(&CounterValue(i128::from(output[2])))?;
                transition.fail_if(true, CommittedFailureReasonId::Reason202)?;
            }
            _ => return Err(CounterProgramError::SynthesisDomain),
        }
        if output[3] == 1 {
            transition.enqueue_channel_300(
                0,
                &NotificationDestination("local-observer".into()),
                &Notification {
                    notified_count: CounterValue(i128::from(output[4])),
                    notified_failures: CounterValue(i128::from(output[5])),
                },
            )?;
        }
        Ok(transition.seal()?)
    }
}
