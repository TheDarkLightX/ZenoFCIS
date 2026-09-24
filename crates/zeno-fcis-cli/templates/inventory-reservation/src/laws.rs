//! Independent, bounded checks over the exact invocation and complete decision.
//!
//! The authored formulas in `project.zeno` are evaluated as written. Formulas
//! cannot see the decision's reason, its outbox, or its effects, so this file
//! checks those in Rust, from the README's rules rather than from the program
//! or the synthesized step.

use crate::{generated::*, profile};
use zeno_fcis_codec::{CanonicalEncode, Hash32};
use zeno_fcis_evidence::EvidenceEnvelope;
use zeno_fcis_laws::*;
use zeno_fcis_spec::*;

/// The most units either field may hold.
const CAPACITY: i128 = 5;
/// The only destination shipment requests may go to.
const SHIPMENTS_TO: &str = "warehouse";

pub struct StockLaws {
    project: ProjectSpec,
}

impl Default for StockLaws {
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
            "example/inventory-reservation/verifier",
            b"External proofs are not admitted",
        )
    }
    fn verify(&self, _: &LawProofSubject, _: &EvidenceEnvelope, _: &[u8]) -> LawProofDecision {
        LawProofDecision::Indeterminate
    }
}

/// The variant ID that `command.101.120` observes.
fn action_id(action: &StockAction) -> i128 {
    match action {
        StockAction::Reserve => 150,
        StockAction::Release => 151,
        StockAction::Ship => 152,
        StockAction::Restock => 153,
    }
}

fn observations(
    pre: &Stock,
    post: &Stock,
    command: &StockCommand,
    context: &StockContext,
) -> TraceStep {
    let observe = |root, raw: &[u32], value| {
        Observation::new(
            ProjectionPath::try_new(
                root,
                raw.iter()
                    .map(|n| StableId::new(*n).expect("static ID"))
                    .collect(),
            )
            .expect("static path"),
            value,
        )
    };
    TraceStep::try_new(vec![
        observe(ProjectionRoot::Pre, &[100, 110], pre.available.0),
        observe(ProjectionRoot::Pre, &[100, 111], pre.reserved.0),
        observe(ProjectionRoot::Post, &[100, 110], post.available.0),
        observe(ProjectionRoot::Post, &[100, 111], post.reserved.0),
        observe(
            ProjectionRoot::Command,
            &[101, 120],
            action_id(&command.action),
        ),
        observe(ProjectionRoot::Command, &[101, 121], command.quantity.0),
        observe(
            ProjectionRoot::Context,
            &[102, 130],
            i128::from(context.authorized.0),
        ),
    ])
    .expect("distinct reviewed projections")
}

/// The rejection reason the README requires, or `None` if the input must commit.
fn expected_rejection(pre: &Stock, command: &StockCommand, context: &StockContext) -> Option<u32> {
    let (available, reserved, quantity) = (pre.available.0, pre.reserved.0, command.quantity.0);
    if !context.authorized.0 {
        return Some(200);
    }
    match command.action {
        StockAction::Reserve if quantity > available => Some(201),
        StockAction::Reserve if reserved + quantity > CAPACITY => Some(203),
        StockAction::Release | StockAction::Ship if quantity > reserved => Some(202),
        StockAction::Release | StockAction::Restock if available + quantity > CAPACITY => Some(203),
        _ => None,
    }
}

impl StockLaws {
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
            profile::digest("example/inventory-reservation/observation", &witness),
        )
        .map_err(|_| LawEngineFailure::InvalidOutput)
    }
}

impl ProjectLawEngine for StockLaws {
    fn evaluate(
        &self,
        input: &LawCheckInput<'_>,
        limits: LawLimits,
    ) -> Result<Vec<LawObservation>, LawEngineFailure> {
        if limits.max_observations < 3 {
            return Err(LawEngineFailure::Incomplete);
        }
        let pre = Stock::try_from_value(input.pre_state().clone())
            .map_err(|_| LawEngineFailure::Unsupported)?;
        let command = StockCommand::try_from_value(input.command().clone())
            .map_err(|_| LawEngineFailure::Unsupported)?;
        let context = StockContext::try_from_value(input.context().clone())
            .map_err(|_| LawEngineFailure::Unsupported)?;
        let (post, commit, outbox) = match input.decision() {
            LawDecisionView::Accept {
                post_state,
                commit_plan,
                outbox_plan,
                ..
            } => (post_state, commit_plan, outbox_plan),
            // This application never commits a failure (law 508).
            LawDecisionView::CommittedFailure { .. } => {
                return Err(LawEngineFailure::InvalidOutput);
            }
            LawDecisionView::Reject { reason_id } => {
                return if expected_rejection(&pre, &command, &context) == Some(*reason_id) {
                    Ok(vec![])
                } else {
                    Err(LawEngineFailure::InvalidOutput)
                };
            }
        };
        let post =
            Stock::try_from_value((*post).clone()).map_err(|_| LawEngineFailure::Unsupported)?;
        let accepted_while_rejectable = expected_rejection(&pre, &command, &context).is_some();
        let delivery_ok = match (outbox.entries(), command.action == StockAction::Ship) {
            ([], false) => true,
            ([entry], true) => {
                let destination = WarehouseDestination(SHIPMENTS_TO.into())
                    .to_value()
                    .map_err(|_| LawEngineFailure::Unsupported)?;
                let payload = ShipmentRequest {
                    shipped_units: command.quantity,
                }
                .to_value()
                .map_err(|_| LawEngineFailure::Unsupported)?;
                entry.ordinal() == 0
                    && entry.channel() == 300
                    && entry.destination() == &destination
                    && entry.payload() == &payload
            }
            _ => false,
        };
        let extra = !accepted_while_rejectable && delivery_ok && commit.effects().is_empty();
        let step = observations(&pre, &post, &command, &context);
        let bytes = input
            .canonical_bytes()
            .map_err(|_| LawEngineFailure::InvalidOutput)?;
        Ok(vec![
            self.check(500, &step, true, &bytes)?,
            self.check(501, &step, extra, &bytes)?,
            self.check(502, &step, extra, &bytes)?,
        ])
    }

    fn evaluate_genesis(
        &self,
        input: &GenesisLawCheckInput<'_>,
        limits: LawLimits,
    ) -> Result<Vec<LawObservation>, LawEngineFailure> {
        if limits.max_observations == 0 {
            return Err(LawEngineFailure::Incomplete);
        }
        let state = Stock::try_from_value(input.initial_state().clone())
            .map_err(|_| LawEngineFailure::Unsupported)?;
        let step = observations(
            &state,
            &state,
            &StockCommand {
                action: StockAction::Restock,
                quantity: Quantity(1),
            },
            &StockContext {
                authorized: OperatorFlag(true),
            },
        );
        let bytes = input
            .canonical_bytes()
            .map_err(|_| LawEngineFailure::InvalidOutput)?;
        let empty = state.available.0 == 0 && state.reserved.0 == 0;
        Ok(vec![self.check(500, &step, empty, &bytes)?])
    }
}
