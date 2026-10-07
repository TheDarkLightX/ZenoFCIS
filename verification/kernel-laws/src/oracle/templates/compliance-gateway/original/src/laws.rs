//! Independent, bounded checks over the exact invocation and complete decision.
//!
//! The authored formulas in `project.zeno` are evaluated as written. Formulas
//! cannot see the decision's reason, its outbox, or its effects, so this file
//! checks those in Rust. For a screening, it evaluates `rules.txt` directly,
//! independently of the synthesized step, and requires the decision to be the
//! one the rule that fired demands: the kind of decision, its reason, the
//! notice it queues, and the strikes it records.

use crate::{
    generated::*,
    profile,
    rules::{RULE_BASE, RuleBase, RuleBaseError, Verdict},
};
use zeno_fcis_codec::{CanonicalEncode, Hash32};
use zeno_fcis_evidence::EvidenceEnvelope;
use zeno_fcis_laws::*;
use zeno_fcis_plan::{OutboxEntry, OutboxPlan};
use zeno_fcis_spec::*;
use zeno_fcis_value::{Field, Value};

/// The state invariant, checked on every committing decision and at genesis.
const STRIKES_WITHIN_BOUNDS: u32 = 500;
/// The laws that constrain an accepted decision.
const ACCEPT_LAWS: [u32; 2] = [501, 502];
/// The laws that constrain a committed failure.
const COMMITTED_FAILURE_LAWS: [u32; 1] = [503];
/// The most observations one decision reports: the invariant and the accept
/// laws. A limit below this cannot hold a complete evaluation.
const OBSERVATIONS_PER_DECISION: u32 = 3;
/// The rejection reasons, in the order the README's rules apply them.
const NOT_REVIEWER: u32 = 200;
const NO_STRIKES: u32 = 201;
/// The variant ID of the first rule of `rules.txt` in `RuleId`; the variants
/// follow the file's order.
const FIRST_RULE_VARIANT: u16 = 170;
/// The committed-failure reason of the first blocking rule of `rules.txt`;
/// the reasons follow the file's order.
const FIRST_BLOCK_REASON: u32 = 210;

/// The channel a notice goes out on, the only desk it may go to, and the
/// payload fields that carry the rule and the notice's other value.
struct NoticeShape {
    channel: u32,
    desk: &'static str,
    rule_field: u16,
    extra_field: u16,
}

/// Channel 300: a review ticket carries the rule and the amount band.
const REVIEW_TICKET: NoticeShape = NoticeShape {
    channel: 300,
    desk: "review-queue",
    rule_field: 145,
    extra_field: 146,
};

/// Channel 301: a block alert carries the rule and the strikes on record.
const BLOCK_ALERT: NoticeShape = NoticeShape {
    channel: 301,
    desk: "compliance-team",
    rule_field: 147,
    extra_field: 148,
};

pub struct GatewayLaws {
    project: ProjectSpec,
    rules: RuleBase,
}

impl GatewayLaws {
    /// Loads the rule base. A rule base with a conflict or a gap is refused
    /// here, before any authority that would use this checker exists.
    ///
    /// # Errors
    ///
    /// The `RuleBaseError` names the line, the conflicting rules, or the
    /// uncovered transfer.
    pub fn try_new() -> Result<Self, RuleBaseError> {
        Ok(Self {
            project: profile::project(),
            rules: RuleBase::load(RULE_BASE)?,
        })
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
            "example/compliance-gateway/verifier",
            b"External proofs are not admitted",
        )
    }
    fn verify(&self, _: &LawProofSubject, _: &EvidenceEnvelope, _: &[u8]) -> LawProofDecision {
        LawProofDecision::Indeterminate
    }
}

/// The variant ID that `command.101.130` observes.
fn action_id(action: &GatewayAction) -> i128 {
    match action {
        GatewayAction::Screen => 150,
        GatewayAction::Reinstate => 151,
    }
}

/// The variant ID that `command.101.131` observes.
fn region_id(region: &Region) -> i128 {
    match region {
        Region::Allowed => 160,
        Region::Restricted => 161,
        Region::Sanctioned => 162,
    }
}

/// The variant ID that `command.101.133` observes.
fn risk_id(risk: &CounterpartyRisk) -> i128 {
    match risk {
        CounterpartyRisk::Low => 165,
        CounterpartyRisk::Medium => 166,
        CounterpartyRisk::High => 167,
    }
}

/// The position of a region among the values `rules.txt` declares for it.
fn region_position(region: &Region) -> i64 {
    match region {
        Region::Allowed => 0,
        Region::Restricted => 1,
        Region::Sanctioned => 2,
    }
}

/// The position of a counterparty risk among the values `rules.txt`
/// declares for it.
fn risk_position(risk: &CounterpartyRisk) -> i64 {
    match risk {
        CounterpartyRisk::Low => 0,
        CounterpartyRisk::Medium => 1,
        CounterpartyRisk::High => 2,
    }
}

/// The features of one screening, in the order `rules.txt` declares them, or
/// `None` for a value the rule base cannot hold.
fn features(pre: &Standing, command: &GatewayCommand, context: &CallerContext) -> Option<[i64; 5]> {
    Some([
        i64::try_from(pre.strikes.0).ok()?,
        i64::try_from(context.identity_tier.0).ok()?,
        region_position(&command.region),
        i64::try_from(command.amount_band.0).ok()?,
        risk_position(&command.counterparty_risk),
    ])
}

/// The observations that the formulas of `project.zeno` read from one
/// decision, at the numeric IDs of its fields.
///
/// # Errors
///
/// `LawEngineFailure::Unsupported` if a projection cannot be formed, which
/// the static IDs below never cause.
pub fn trace_step(
    pre: &Standing,
    post: &Standing,
    command: &GatewayCommand,
    context: &CallerContext,
) -> Result<TraceStep, LawEngineFailure> {
    let observe = |root, ids: &[u32], value| {
        let segments = ids
            .iter()
            .map(|id| StableId::new(*id).ok_or(LawEngineFailure::Unsupported))
            .collect::<Result<Vec<_>, _>>()?;
        let path = ProjectionPath::try_new(root, segments).ok_or(LawEngineFailure::Unsupported)?;
        Ok(Observation::new(path, value))
    };
    TraceStep::try_new(vec![
        observe(ProjectionRoot::Pre, &[100, 120], pre.strikes.0)?,
        observe(ProjectionRoot::Post, &[100, 120], post.strikes.0)?,
        observe(
            ProjectionRoot::Command,
            &[101, 130],
            action_id(&command.action),
        )?,
        observe(
            ProjectionRoot::Command,
            &[101, 131],
            region_id(&command.region),
        )?,
        observe(ProjectionRoot::Command, &[101, 132], command.amount_band.0)?,
        observe(
            ProjectionRoot::Command,
            &[101, 133],
            risk_id(&command.counterparty_risk),
        )?,
        observe(
            ProjectionRoot::Context,
            &[102, 140],
            context.identity_tier.0,
        )?,
        observe(
            ProjectionRoot::Context,
            &[102, 141],
            i128::from(context.reviewer.0),
        )?,
    ])
    .ok_or(LawEngineFailure::Unsupported)
}

/// The rejection reason the README requires, or `None` if the input must commit.
fn expected_rejection(
    pre: &Standing,
    command: &GatewayCommand,
    context: &CallerContext,
) -> Option<u32> {
    match command.action {
        GatewayAction::Reinstate if !context.reviewer.0 => Some(NOT_REVIEWER),
        GatewayAction::Reinstate if pre.strikes.0 == 0 => Some(NO_STRIKES),
        _ => None,
    }
}

/// The field `id` of a record value.
fn field(value: &Value, id: u16) -> Option<&Value> {
    let Value::Record(fields) = value else {
        return None;
    };
    fields
        .iter()
        .find(|field| field.id() == id)
        .map(Field::value)
}

/// Whether `entry` is the one notice of `shape` that names the rule at
/// position `rule` of `rules.txt` and carries `extra`: read by field ID, so
/// the check does not depend on the generated payload bindings.
fn is_notice(
    entry: &OutboxEntry,
    shape: &NoticeShape,
    desk: &Value,
    rule: usize,
    extra: i128,
) -> bool {
    let Value::Record(fields) = entry.payload() else {
        return false;
    };
    let rule_variant = u16::try_from(rule)
        .ok()
        .and_then(|rule| rule.checked_add(FIRST_RULE_VARIANT));
    entry.ordinal() == 0
        && entry.channel() == shape.channel
        && entry.destination() == desk
        && fields.len() == 2
        && matches!(
            field(entry.payload(), shape.rule_field),
            Some(Value::Enum { variant, .. } | Value::Sum { variant, .. }) if Some(*variant) == rule_variant
        )
        && field(entry.payload(), shape.extra_field) == Some(&Value::I128(extra))
}

impl GatewayLaws {
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
            profile::digest("example/compliance-gateway/observation", &witness),
        )
        .map_err(|_| LawEngineFailure::InvalidOutput)
    }

    /// Whether a committing screening is the decision `rules.txt` demands.
    fn screening_ok(
        &self,
        pre: &Standing,
        post: &Standing,
        command: &GatewayCommand,
        context: &CallerContext,
        failure: Option<u32>,
        outbox: &OutboxPlan,
    ) -> Result<bool, LawEngineFailure> {
        let Some(fired) =
            features(pre, command, context).and_then(|features| self.rules.decide(&features))
        else {
            return Ok(false);
        };
        let unsupported = |_| LawEngineFailure::Unsupported;
        Ok(match fired.verdict {
            Verdict::Allow => failure.is_none() && outbox.entries().is_empty(),
            Verdict::Hold => {
                let desk = ReviewDesk(REVIEW_TICKET.desk.into())
                    .to_value()
                    .map_err(unsupported)?;
                failure.is_none()
                    && matches!(
                        outbox.entries(),
                        [entry] if is_notice(entry, &REVIEW_TICKET, &desk, fired.rule, command.amount_band.0)
                    )
            }
            Verdict::Block => {
                let desk = ComplianceDesk(BLOCK_ALERT.desk.into())
                    .to_value()
                    .map_err(unsupported)?;
                let reason = self
                    .rules
                    .block_ordinal(fired.rule)
                    .and_then(|ordinal| u32::try_from(ordinal).ok())
                    .map(|ordinal| FIRST_BLOCK_REASON + ordinal);
                failure.is_some()
                    && failure == reason
                    && matches!(
                        outbox.entries(),
                        [entry] if is_notice(entry, &BLOCK_ALERT, &desk, fired.rule, post.strikes.0)
                    )
            }
        })
    }
}

impl ProjectLawEngine for GatewayLaws {
    fn evaluate(
        &self,
        input: &LawCheckInput<'_>,
        limits: LawLimits,
    ) -> Result<Vec<LawObservation>, LawEngineFailure> {
        if limits.max_observations < OBSERVATIONS_PER_DECISION {
            return Err(LawEngineFailure::Incomplete);
        }
        let pre = Standing::try_from_value(input.pre_state().clone())
            .map_err(|_| LawEngineFailure::Unsupported)?;
        let command = GatewayCommand::try_from_value(input.command().clone())
            .map_err(|_| LawEngineFailure::Unsupported)?;
        let context = CallerContext::try_from_value(input.context().clone())
            .map_err(|_| LawEngineFailure::Unsupported)?;
        let (post, commit, outbox, relations, failure): (_, _, _, &[u32], Option<u32>) =
            match input.decision() {
                LawDecisionView::Accept {
                    post_state,
                    commit_plan,
                    outbox_plan,
                    ..
                } => (post_state, commit_plan, outbox_plan, &ACCEPT_LAWS, None),
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
                    &COMMITTED_FAILURE_LAWS,
                    Some(*reason_id),
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
            Standing::try_from_value((*post).clone()).map_err(|_| LawEngineFailure::Unsupported)?;
        let committed_while_rejectable = expected_rejection(&pre, &command, &context).is_some();
        let decision_ok = match command.action {
            // A reinstatement never fails and queues nothing; law 502 checks
            // the rest.
            GatewayAction::Reinstate => failure.is_none() && outbox.entries().is_empty(),
            GatewayAction::Screen => {
                self.screening_ok(&pre, &post, &command, &context, failure, outbox)?
            }
        };
        let extra = decision_ok && !committed_while_rejectable && commit.effects().is_empty();
        let step = trace_step(&pre, &post, &command, &context)?;
        let bytes = input
            .canonical_bytes()
            .map_err(|_| LawEngineFailure::InvalidOutput)?;
        let mut checked = vec![self.check(STRIKES_WITHIN_BOUNDS, &step, true, &bytes)?];
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
        let state = Standing::try_from_value(input.initial_state().clone())
            .map_err(|_| LawEngineFailure::Unsupported)?;
        // Law 500 reads only the state, so the request in the step is a
        // placeholder that no formula reads.
        let step = trace_step(
            &state,
            &state,
            &GatewayCommand {
                action: GatewayAction::Screen,
                region: Region::Allowed,
                amount_band: AmountBand(0),
                counterparty_risk: CounterpartyRisk::Low,
            },
            &CallerContext {
                identity_tier: IdentityTier(0),
                reviewer: ReviewerFlag(false),
            },
        )?;
        let bytes = input
            .canonical_bytes()
            .map_err(|_| LawEngineFailure::InvalidOutput)?;
        Ok(vec![self.check(
            STRIKES_WITHIN_BOUNDS,
            &step,
            state.strikes.0 == 0,
            &bytes,
        )?])
    }
}
