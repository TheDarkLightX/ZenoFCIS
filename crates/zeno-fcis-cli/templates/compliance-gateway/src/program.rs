//! Reviewed adapter around the synthesized screening step.
//!
//! A reinstatement is decided here. A screening is decided by
//! `synthesized::transition`, which was selected by exhaustive verification
//! against `synthesis.json`, the contract written from `rules.txt`, over all
//! 720 standings and transfer features. This adapter encodes the features,
//! then maps the step's output through the table below into typed staging.
//!
//! | `output[0]` | Decision |
//! | --- | --- |
//! | 0 | accept: the transfer is allowed; nothing changes and nothing is queued |
//! | 1 | accept: the transfer is held; one review ticket names rule `output[1]` |
//! | 2 | committed failure under the reason of rule `output[1]`: `output[2]` strikes, and one alert |

use crate::{bindings::*, generated::*, profile, synthesized};
use zeno_fcis_authority::{CatalogTransitionProgram, ReviewedTransitionInput};
use zeno_fcis_codec::Hash32;
use zeno_fcis_core::BudgetUsed;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_schema::ValidationLimits;
use zeno_fcis_transition::TransitionDecision;

/// Where review tickets are delivered.
pub const REVIEW_QUEUE: &str = "review-queue";
/// Where block alerts are delivered.
pub const COMPLIANCE_TEAM: &str = "compliance-team";

pub struct GatewayProgram;

#[derive(Debug)]
pub enum GatewayProgramError {
    Project(Box<GeneratedProjectError>),
    SynthesisDomain,
}
impl From<GeneratedProjectError> for GatewayProgramError {
    fn from(error: GeneratedProjectError) -> Self {
        Self::Project(Box::new(error))
    }
}
impl From<AdapterError> for GatewayProgramError {
    fn from(error: AdapterError) -> Self {
        GeneratedProjectError::from(error).into()
    }
}

/// The code the synthesized step takes for a region: its position in
/// `rules.txt`.
#[must_use]
pub fn region_code(region: &Region) -> i64 {
    match region {
        Region::Allowed => 0,
        Region::Restricted => 1,
        Region::Sanctioned => 2,
    }
}

/// The code the synthesized step takes for a counterparty risk: its position
/// in `rules.txt`.
#[must_use]
pub fn risk_code(risk: &CounterpartyRisk) -> i64 {
    match risk {
        CounterpartyRisk::Low => 0,
        CounterpartyRisk::Medium => 1,
        CounterpartyRisk::High => 2,
    }
}

/// The rule at each position of `rules.txt`.
#[must_use]
pub fn rule_id(position: i64) -> Option<RuleId> {
    Some(match position {
        0 => RuleId::SanctionedRegion,
        1 => RuleId::FrozenAccount,
        2 => RuleId::UnverifiedHighRisk,
        3 => RuleId::RestrictedLarge,
        4 => RuleId::UnverifiedLarge,
        5 => RuleId::HighRiskCounterparty,
        6 => RuleId::RestrictedRegion,
        7 => RuleId::UnverifiedTransfer,
        8 => RuleId::PartiallyVerifiedLarge,
        9 => RuleId::RepeatOffender,
        10 => RuleId::MediumRiskLarge,
        11 => RuleId::DefaultAllow,
        _ => return None,
    })
}

/// The committed-failure reason of each blocking rule.
#[must_use]
pub fn block_reason(rule: &RuleId) -> Option<CommittedFailureReasonId> {
    Some(match rule {
        RuleId::SanctionedRegion => CommittedFailureReasonId::Reason210,
        RuleId::FrozenAccount => CommittedFailureReasonId::Reason211,
        RuleId::UnverifiedHighRisk => CommittedFailureReasonId::Reason212,
        RuleId::RestrictedLarge => CommittedFailureReasonId::Reason213,
        RuleId::UnverifiedLarge => CommittedFailureReasonId::Reason214,
        _ => return None,
    })
}

impl CatalogTransitionProgram<RustCryptoSha256> for GatewayProgram {
    type Error = GatewayProgramError;

    fn transition_build_hash(&self) -> Hash32 {
        profile::program_hash()
    }

    fn execute(
        &self,
        input: ReviewedTransitionInput<'_>,
    ) -> Result<TransitionDecision, Self::Error> {
        let project = GeneratedProject::try_new::<RustCryptoSha256>()?;
        let command = GatewayCommand::try_from_value(input.command().value().value().clone())?;
        let context = CallerContext::try_from_value(input.context().value().value().clone())?;
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
        let strikes = transition.read_strikes()?;
        match command.action {
            GatewayAction::Reinstate => {
                transition.observe_context_reviewer()?;
                if !context.reviewer.0 {
                    transition.require(false, RejectReasonId::Reason200)?;
                } else if strikes.0 == 0 {
                    transition.require(false, RejectReasonId::Reason201)?;
                } else {
                    transition.update_strikes(&Strikes(0))?;
                }
            }
            GatewayAction::Screen => {
                transition.observe_context_identity_tier()?;
                let domain = |value: i128| {
                    i64::try_from(value).map_err(|_| GatewayProgramError::SynthesisDomain)
                };
                let output = synthesized::transition(&[
                    domain(strikes.0)?,
                    domain(context.identity_tier.0)?,
                    region_code(&command.region),
                    domain(command.amount_band.0)?,
                    risk_code(&command.counterparty_risk),
                ])
                .ok_or(GatewayProgramError::SynthesisDomain)?;
                let rule = rule_id(output[1]).ok_or(GatewayProgramError::SynthesisDomain)?;
                match output[0] {
                    0 => {}
                    1 => {
                        transition.enqueue_channel_300(
                            0,
                            &ReviewDesk(REVIEW_QUEUE.into()),
                            &ReviewTicket {
                                ticket_rule: rule,
                                ticket_amount_band: command.amount_band,
                            },
                        )?;
                    }
                    2 => {
                        // A blocked transfer is a fact worth keeping: it commits
                        // as a failure, and the strike stays on record.
                        let reason =
                            block_reason(&rule).ok_or(GatewayProgramError::SynthesisDomain)?;
                        let after = Strikes(i128::from(output[2]));
                        transition.update_strikes(&after)?;
                        transition.enqueue_channel_301(
                            0,
                            &ComplianceDesk(COMPLIANCE_TEAM.into()),
                            &BlockAlert {
                                alert_rule: rule,
                                alert_strikes: after,
                            },
                        )?;
                        transition.fail_if(true, reason)?;
                    }
                    _ => return Err(GatewayProgramError::SynthesisDomain),
                }
            }
        }
        Ok(transition.seal()?)
    }
}
