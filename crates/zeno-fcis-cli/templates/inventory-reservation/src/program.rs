//! Reviewed adapter around the synthesized stock step.
//!
//! `synthesized::transition` decides authorization, stock movement, rejection,
//! and shipment selection. It was selected by exhaustive verification against
//! `synthesis.json` over all 864 admitted inputs.
//! This adapter maps its output through the table below into typed staging.
//!
//! | `output[0]` | Decision |
//! | --- | --- |
//! | 0 | reject `insufficient_available` (201) |
//! | 1 | reject `insufficient_reserved` (202) |
//! | 2 | reject `over_capacity` (203) |
//! | 3 | accept: `output[1]` available, `output[2]` reserved, and a shipment request when `output[3]` is 1 |
//! | 4 | reject `not_authorized` (200) |

use crate::{bindings::*, generated::*, profile, synthesized};
use zeno_fcis_authority::{CatalogTransitionProgram, ReviewedTransitionInput};
use zeno_fcis_codec::Hash32;
use zeno_fcis_core::BudgetUsed;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_schema::ValidationLimits;
use zeno_fcis_transition::TransitionDecision;

/// Where shipment requests are delivered.
pub const WAREHOUSE: &str = "warehouse";

pub struct StockProgram;

#[derive(Debug)]
pub enum StockProgramError {
    Project(Box<GeneratedProjectError>),
    SynthesisDomain,
}
impl From<GeneratedProjectError> for StockProgramError {
    fn from(error: GeneratedProjectError) -> Self {
        Self::Project(Box::new(error))
    }
}
impl From<AdapterError> for StockProgramError {
    fn from(error: AdapterError) -> Self {
        GeneratedProjectError::from(error).into()
    }
}

/// The action code the synthesized step takes.
pub fn action_code(action: &StockAction) -> i64 {
    match action {
        StockAction::Reserve => 0,
        StockAction::Release => 1,
        StockAction::Ship => 2,
        StockAction::Restock => 3,
    }
}

impl CatalogTransitionProgram<RustCryptoSha256> for StockProgram {
    type Error = StockProgramError;

    fn transition_build_hash(&self) -> Hash32 {
        profile::program_hash()
    }

    fn execute(
        &self,
        input: ReviewedTransitionInput<'_>,
    ) -> Result<TransitionDecision, Self::Error> {
        let project = GeneratedProject::try_new::<RustCryptoSha256>()?;
        let command = StockCommand::try_from_value(input.command().value().value().clone())?;
        let context = StockContext::try_from_value(input.context().value().value().clone())?;
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
        transition.observe_context_authorized()?;
        let available = transition.read_available()?;
        let reserved = transition.read_reserved()?;
        let domain =
            |value: i128| i64::try_from(value).map_err(|_| StockProgramError::SynthesisDomain);
        let output = synthesized::transition(&[
            domain(available.0)?,
            domain(reserved.0)?,
            action_code(&command.action),
            domain(command.quantity.0)?,
            i64::from(context.authorized.0),
        ])
        .ok_or(StockProgramError::SynthesisDomain)?;
        match output[0] {
            0 => {
                transition.require(false, RejectReasonId::Reason201)?;
            }
            1 => {
                transition.require(false, RejectReasonId::Reason202)?;
            }
            2 => {
                transition.require(false, RejectReasonId::Reason203)?;
            }
            3 => {
                transition.update_available(&Units(i128::from(output[1])))?;
                transition.update_reserved(&Units(i128::from(output[2])))?;
                if output[3] == 1 {
                    transition.enqueue_channel_300(
                        0,
                        &WarehouseDestination(WAREHOUSE.into()),
                        &ShipmentRequest {
                            shipped_units: command.quantity,
                        },
                    )?;
                }
            }
            4 => {
                transition.require(false, RejectReasonId::Reason200)?;
            }
            _ => return Err(StockProgramError::SynthesisDomain),
        }
        Ok(transition.seal()?)
    }
}
