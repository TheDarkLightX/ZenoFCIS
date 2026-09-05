//! Independent, bounded checks over the exact invocation and complete decision.
//! The authored formulas are evaluated, not replaced by successful placeholders.

use crate::{generated::*, profile};
use zeno_fcis_codec::{CanonicalEncode, Hash32};
use zeno_fcis_evidence::EvidenceEnvelope;
use zeno_fcis_laws::*;
use zeno_fcis_spec::*;

pub struct CounterLaws {
    project: ProjectSpec,
}

impl Default for CounterLaws {
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
            "example/durable-counter/verifier",
            b"External proofs are not admitted",
        )
    }
    fn verify(&self, _: &LawProofSubject, _: &EvidenceEnvelope, _: &[u8]) -> LawProofDecision {
        LawProofDecision::Indeterminate
    }
}

fn observations(
    pre: &CounterState,
    post: &CounterState,
    command: &CounterCommand,
    context: &CounterContext,
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
        observe(ProjectionRoot::Pre, &[100, 110], pre.count.0),
        observe(ProjectionRoot::Pre, &[100, 111], pre.failures.0),
        observe(ProjectionRoot::Post, &[100, 110], post.count.0),
        observe(ProjectionRoot::Post, &[100, 111], post.failures.0),
        observe(
            ProjectionRoot::Command,
            &[101],
            match command {
                CounterCommand::Increment => 120,
                CounterCommand::RecordFailure => 121,
            },
        ),
        observe(ProjectionRoot::Context, &[102], i128::from(context.0)),
    ])
    .expect("distinct reviewed projections")
}

impl CounterLaws {
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
            profile::digest("example/durable-counter/observation", &witness),
        )
        .map_err(|_| LawEngineFailure::InvalidOutput)
    }
}

impl ProjectLawEngine for CounterLaws {
    fn evaluate(
        &self,
        input: &LawCheckInput<'_>,
        limits: LawLimits,
    ) -> Result<Vec<LawObservation>, LawEngineFailure> {
        if limits.max_observations < 2 {
            return Err(LawEngineFailure::Incomplete);
        }
        let pre = CounterState::try_from_value(input.pre_state().clone())
            .map_err(|_| LawEngineFailure::Unsupported)?;
        let command = CounterCommand::try_from_value(input.command().clone())
            .map_err(|_| LawEngineFailure::Unsupported)?;
        let context = CounterContext::try_from_value(input.context().clone())
            .map_err(|_| LawEngineFailure::Unsupported)?;
        let (post, commit, outbox, relation, reason_ok) = match input.decision() {
            LawDecisionView::Accept {
                post_state,
                commit_plan,
                outbox_plan,
                ..
            } => (post_state, commit_plan, outbox_plan, 501, true),
            LawDecisionView::CommittedFailure {
                reason_id,
                post_state,
                commit_plan,
                outbox_plan,
                ..
            } => (post_state, commit_plan, outbox_plan, 502, *reason_id == 202),
            LawDecisionView::Reject { reason_id } => {
                let full = match command {
                    CounterCommand::Increment => pre.count.0 == 3,
                    CounterCommand::RecordFailure => pre.failures.0 == 3,
                };
                let expected = if !context.0 {
                    Some(200)
                } else if full {
                    Some(201)
                } else {
                    None
                };
                return if expected == Some(*reason_id) {
                    Ok(vec![])
                } else {
                    Err(LawEngineFailure::InvalidOutput)
                };
            }
        };
        let post = CounterState::try_from_value((*post).clone())
            .map_err(|_| LawEngineFailure::Unsupported)?;
        let expected_payload = Notification {
            notified_count: post.count,
            notified_failures: post.failures,
        }
        .to_value()
        .map_err(|_| LawEngineFailure::Unsupported)?;
        let expected_destination = NotificationDestination("local-observer".into())
            .to_value()
            .map_err(|_| LawEngineFailure::Unsupported)?;
        let delivery_ok = match outbox.entries() {
            [entry] => {
                entry.ordinal() == 0
                    && entry.channel() == 300
                    && entry.destination() == &expected_destination
                    && entry.payload() == &expected_payload
            }
            _ => false,
        };
        let step = observations(&pre, &post, &command, &context);
        let bytes = input
            .canonical_bytes()
            .map_err(|_| LawEngineFailure::InvalidOutput)?;
        Ok(vec![
            self.check(500, &step, true, &bytes)?,
            self.check(
                relation,
                &step,
                reason_ok && delivery_ok && commit.effects().is_empty(),
                &bytes,
            )?,
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
        let state = CounterState::try_from_value(input.initial_state().clone())
            .map_err(|_| LawEngineFailure::Unsupported)?;
        let step = observations(
            &state,
            &state,
            &CounterCommand::Increment,
            &CounterContext(true),
        );
        let bytes = input
            .canonical_bytes()
            .map_err(|_| LawEngineFailure::InvalidOutput)?;
        Ok(vec![self.check(
            500,
            &step,
            state.count.0 == 0 && state.failures.0 == 0,
            &bytes,
        )?])
    }
}
