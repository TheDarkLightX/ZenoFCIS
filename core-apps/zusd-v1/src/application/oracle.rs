//! Exact native oracle composition. The returned tuple is one conditional
//! candidate for atomic publication; this module performs no external effects.
use super::*;
use crate::{Evaluation, Failure, Limits, Work, run, vaults::PriceStatus};
use zeno_fcis_synthesis::finite::v2_oracle as o;

/// Immutable source states and request for one composed decision.
pub struct ComposedInputs<'a> {
    /// Bound zUSD consumer policy.
    pub policy: &'a Policy,
    /// zUSD state before the invocation.
    pub zusd: &'a State,
    /// Native oracle state before the invocation.
    pub oracle: &'a o::LifecycleState,
    /// Optional native command belonging to this request.
    pub invocation: Option<&'a NativeInvocation>,
    /// Exact zUSD action and authenticated facts.
    pub request: &'a Request,
}
/// Owned native command and its exact actor and source context.
#[derive(Clone, Debug)]
pub struct NativeInvocation {
    /// Actor submitting the native oracle command.
    pub actor: o::Id,
    /// Exact pre-root, nonce and clock for the native command.
    pub context: o::LifecycleContext,
    /// Native command associated with the zUSD request.
    pub command: o::LifecycleCommand,
}
/// Owned oracle and zUSD successors for one conditional atomic publication.
#[derive(Debug)]
#[must_use]
pub struct ComposedDecision {
    oracle_pre: Id,
    oracle: o::LifecycleState,
    zusd: Prepared,
}
impl ComposedDecision {
    /// Return the native source root required before publication.
    pub fn oracle_pre_root(&self) -> Id {
        self.oracle_pre
    }
    /// Borrow the immutable native oracle successor.
    pub fn oracle(&self) -> &o::LifecycleState {
        &self.oracle
    }
    /// Borrow the checked zUSD successor and its effect plan.
    pub fn zusd(&self) -> &Prepared {
        &self.zusd
    }
}
// Preserve measured work even when native admission or projection refuses.
// Native tariffs map conservatively into the consumer's three budget units.
fn charge(w: &mut Work, u: o::Usage) -> Result<(), Failure> {
    use o::Resource::*;
    let sum = |rs: &[o::Resource]| {
        rs.iter().try_fold(0u64, |n, r| {
            n.checked_add(u.used(*r)).ok_or(Failure::Arithmetic)
        })
    };
    w.charge(
        sum(&[Step, Candidate, Effect, Depth])?,
        sum(&[Read, Write])?,
        sum(&[Byte, WitnessByte])?,
    )
}
fn native_error(_: o::Failure) -> Failure {
    Failure::Canonical
}
fn binding(p: &Policy, s: &o::LifecycleState) -> Result<(), Failure> {
    let r = p.config.oracle;
    let f = s.feed(&r.feed).ok_or(Failure::UnboundProfile)?;
    let c = &f.rules.clock;
    if f.base.bytes() != b"AGRS"
        || f.quote.bytes() != b"USD"
        || s.chain().bytes() != p.config.chain
        || f.rules.query != r.query
        || f.rules.profile != r.profile
        || s.profile_id() != r.profile
        || encoding::hash(o::BLS_POP_SCHEME.as_bytes()) != r.scheme
        || o::evaluator_identity() != r.evaluator
        || c.backend != p.config.clock_backend
        || c.max_age != r.max_age
        || c.max_time_lag != r.max_lag
        || c.confirmation != r.confirmation
        || c.dispute_open != r.dispute_open
        || f.rules.max_deviation_bps != r.max_deviation
    {
        return Err(Failure::UnboundProfile);
    }
    Ok(())
}
fn record(
    a: &o::Aggregate,
    published: u64,
    earliest: u64,
    dispute_until: u64,
    committed: u64,
) -> PriceRecord {
    let n = a.numbers();
    PriceRecord {
        aggregate: a.id(),
        round: a.round(),
        published_epoch: published,
        earliest_commit_epoch: earliest,
        dispute_until,
        value_e8: n.median.raw(),
        observed_epoch: n.oldest_epoch,
        median_seconds: n.median_seconds,
        committed_epoch: committed,
        deviation: n.deviation_bps,
    }
}
fn pending(p: &o::Pending, committed: u64) -> PriceRecord {
    record(
        &p.aggregate,
        p.published,
        p.earliest,
        p.dispute_until,
        committed,
    )
}
fn projection(
    p: &Policy,
    pre: &State,
    native_pre: &o::LifecycleState,
    native_post: &o::LifecycleState,
    clock: Clock,
    ol: o::Limits,
    w: &mut Work,
) -> Result<PriceView, Failure> {
    binding(p, native_post)?;
    let (before, u) = o::native_consumer_read(
        native_pre,
        p.config.oracle.feed,
        o::ClockView {
            backend: clock.backend,
            finality_root: clock.finality,
            epoch: clock.epoch,
        },
        ol,
    )
    .into_parts();
    charge(w, u)?;
    let before = before.map_err(native_error)?;
    let (read, u) = o::native_consumer_read(
        native_post,
        p.config.oracle.feed,
        o::ClockView {
            backend: clock.backend,
            finality_root: clock.finality,
            epoch: clock.epoch,
        },
        ol,
    )
    .into_parts();
    charge(w, u)?;
    let read = read.map_err(native_error)?;
    if read.watermark() > clock.watermark {
        return Err(Failure::Canonical);
    }
    let committed = if let Some(a) = read.committed() {
        let old = pre
            .oracle()
            .and_then(|v| v.committed)
            .filter(|r| r.aggregate == a.id());
        Some(if let Some(old) = old {
            let expected = record(
                a,
                old.published_epoch,
                old.earliest_commit_epoch,
                old.dispute_until,
                old.committed_epoch,
            );
            if old != expected {
                return Err(Failure::Canonical);
            }
            old
        } else {
            let old_pending = before
                .pending()
                .filter(|q| q.aggregate.id() == a.id())
                .ok_or(Failure::Canonical)?;
            pending(old_pending, clock.epoch)
        })
    } else {
        None
    };
    let status = match read.status() {
        Ok(_) => PriceStatus::Usable,
        Err(o::Reason::WrongPhase) => PriceStatus::Unavailable,
        Err(
            o::Reason::StaleObservation | o::Reason::FutureObservation | o::Reason::BadClockOrSync,
        ) => PriceStatus::Stale,
        Err(o::Reason::DisputedOrFrozen) => PriceStatus::Disputed,
        Err(o::Reason::ExcessUncertainty) => PriceStatus::Uncertain,
        Err(o::Reason::SecurityBudgetExceeded) => PriceStatus::OverBudget,
        Err(_) => return Err(Failure::Canonical),
    };
    Ok(PriceView {
        committed,
        pending: read.pending().map(|p| pending(p, 0)),
        pending_blocked: read.pending_blocked(),
        status,
        capacity: read.status().map_or(0, |v| v.capacity().0),
        capacity_root: read.root(),
        aggregate_bytes: read.evidence_bytes().to_vec(),
    })
}
/// Data preparation only. Signatures still bind the exact resulting request;
/// decide_composed independently re-extracts and compares the complete view.
pub fn project_oracle_view(
    p: &Policy,
    pre: &State,
    native_pre: &o::LifecycleState,
    native_post: &o::LifecycleState,
    clock: Clock,
    oracle_limits: o::Limits,
    limits: Limits,
) -> Evaluation<PriceView> {
    run(limits, |w| {
        projection(p, pre, native_pre, native_post, clock, oracle_limits, w)
    })
}
/// One owned state/effect candidate. Refusal returns neither candidate.
/// Install immutable verifiers satisfying both ports' complete contracts.
pub fn decide_composed(
    context: ComposedInputs<'_>,
    crypto: &impl o::BlsVerifier,
    receipts: &impl ReceiptVerifierV1,
    oracle_limits: o::Limits,
    limits: Limits,
) -> Evaluation<ComposedDecision> {
    let ComposedInputs {
        policy: p,
        zusd: pre,
        oracle: native_pre,
        invocation: native,
        request,
    } = context;
    run(limits, |w| {
        let root = native_pre.root().map_err(native_error)?;
        let post = if let Some(i) = native {
            if i.context.clock.epoch != request.clock.epoch
                || i.context.clock.backend != request.clock.backend
                || i.context.clock.finality_root != request.clock.finality
            {
                return Err(Failure::Canonical);
            }
            let (plan, u) = o::lifecycle_relation(
                crypto,
                native_pre,
                &i.actor,
                i.context,
                &i.command,
                oracle_limits,
            )
            .into_parts();
            charge(w, u)?;
            plan.map_err(native_error)?
                .map_err(|r| Failure::Reject(2000 + r as u16))?
                .into_state()
        } else {
            native_pre.clone()
        };
        if let Some(receipt) = &request.oracle {
            let expected = projection(p, pre, native_pre, &post, request.clock, oracle_limits, w)?;
            if receipt.before != pre.oracle_root()
                || receipt.after != post.root().map_err(native_error)?
                || receipt.view != expected
            {
                return Err(Failure::Canonical);
            }
            // A new commitment can only be installed by the exact native commit.
            if matches!(
                receipt.action,
                OracleAction::CommitMatured | OracleAction::InitializeFirstMatured
            ) && !native.is_some_and(|i| {
                matches!(i.command,o::LifecycleCommand::Price {
                    feed,command:o::NativePriceCommand::Commit } if feed==p.config.oracle.feed)
            }) {
                return Err(Failure::Canonical);
            }
        } else if native.is_some() {
            return Err(Failure::Canonical);
        }
        let zusd = w.invoke(|l| super::decide(p, pre, request, receipts, l))?;
        Ok(ComposedDecision {
            oracle_pre: root,
            oracle: post,
            zusd,
        })
    })
}
