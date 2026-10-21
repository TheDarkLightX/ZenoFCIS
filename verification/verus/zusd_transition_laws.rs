//! One-step accounting and finite-trace induction. The aggregate adapter is separate.
#![allow(dead_code)]
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene))]
#[path = "../../core-apps/zusd-v1/src/accounting.rs"]
mod accounting;
use vstd::prelude::*;
verus! {

pub open spec fn total(rows: Seq<int>) -> int
    decreases rows.len(),
{
    if rows.len() == 0 { 0 } else {
        total(rows.drop_last()) + rows.last()
    }
}

/// Every finite nonnegative u128 row sequence has its exact cardinality bound.
pub proof fn row_sum_bound(rows: Seq<int>)
    requires forall|i: int| 0 <= i < rows.len() ==> 0 <= #[trigger] rows[i] <= u128::MAX,
    ensures 0 <= total(rows) <= rows.len() as int * u128::MAX as int,
    decreases rows.len(),
{
    if rows.len() > 0 {
        row_sum_bound(rows.drop_last());
    }
}

/// A canonical ledger at its u32 capacity still needs fewer than 160 bits.
pub proof fn ledger_sum_fits_wide(rows: Seq<int>)
    requires rows.len() <= u32::MAX,
        forall|i: int| 0 <= i < rows.len() ==> 0 <= #[trigger] rows[i] <= u128::MAX,
    ensures 0 <= total(rows)
        < (u32::MAX as int + 1) * (u128::MAX as int + 1),
{
    row_sum_bound(rows);
}

/// Replacing one exact balance changes the whole sum by exactly that delta.
pub proof fn row_replacement(rows: Seq<int>, at: int, next: int)
    requires 0 <= at < rows.len(),
    ensures total(rows.update(at, next)) == total(rows) - rows[at] + next,
    decreases rows.len(),
{
    let changed = rows.update(at, next);
    if at == rows.len() - 1 {
        assert(changed.drop_last() =~= rows.drop_last());
    } else {
        row_replacement(rows.drop_last(), at, next);
        assert(changed.drop_last() =~= rows.drop_last().update(at, next));
        assert(changed.last() == rows.last());
    }
}

/// Debit then credit of one asset preserves its total, including self-transfers.
pub proof fn transfer_conserves(rows: Seq<int>, from: int, to: int, amount: int)
    requires 0 <= from < rows.len(), 0 <= to < rows.len(),
    ensures total(rows.update(from, rows[from] - amount).update(to,
        rows.update(from, rows[from] - amount)[to] + amount)) == total(rows),
{
    let debited = rows.update(from, rows[from] - amount);
    row_replacement(rows, from, rows[from] - amount);
    row_replacement(debited, to, debited[to] + amount);
}

/// Supply changes by the same delta as a one-sided zUSD balance leg.
pub proof fn issuance_conserves(rows: Seq<int>, at: int, delta: int, supply: int)
    requires 0 <= at < rows.len(), total(rows) == supply,
    ensures total(rows.update(at, rows[at] + delta)) == supply + delta,
{
    row_replacement(rows, at, rows[at] + delta);
}

/// Funding may add exactly the funded amount to its own external asset.
pub proof fn funding_is_exact(rows: Seq<int>, at: int, amount: int)
    requires 0 <= at < rows.len(),
    ensures total(rows.update(at, rows[at] + amount)) == total(rows) + amount,
{
    row_replacement(rows, at, rows[at] + amount);
}

/// Moving debt between active and default pools cannot change total debt.
pub proof fn redistribution_conserves(active: int, default: int, moved: int)
    ensures active - moved + (default + moved) == active + default,
{}

/// From valid balances, matched issuance/burn changes preserve debt equality.
pub proof fn supply_debt_step(balance: int, supply: int, active: int, default: int,
    minted: int, burned: int, redistributed: int)
    requires balance == supply, supply == active + default,
    ensures balance + minted - burned == supply + minted - burned,
        supply + minted - burned
            == (active + minted - burned - redistributed) + (default + redistributed),
{}

/// The actual checked u128 admission function establishes exact debt equality.
pub fn checked_debt_admission(supply: u128, active: u128, default: u128) -> (ok: bool)
    ensures ok == (supply as int == active as int + default as int),
{
    accounting::debt_matches(supply, active, default)
}

/// Exact raw quantity change connects executable arithmetic to the sum lemma.
pub fn checked_balance_step(old: u128, amount: u128, credit: bool) -> (out: Option<u128>)
    ensures match out {
        Some(next) => next as int == old as int
            + if credit { amount as int } else { -(amount as int) },
        None => if credit { old as int + amount as int > u128::MAX }
            else { old < amount },
    },
{
    accounting::balance_change(old, amount, credit)
}

pub struct Totals {
    pub zusd: int,
    pub active: int,
    pub default: int,
    pub agrs: int,
    pub zdex: int,
    pub funded_agrs: int,
    pub funded_zdex: int,
}
pub enum Step {
    Money { minted: int, burned: int, redistributed: int },
    FundAgrs { amount: int },
    FundZdex { amount: int },
    Reject,
}
pub open spec fn conserved(s: Totals, initial_agrs: int, initial_zdex: int) -> bool {
    s.zusd == s.active + s.default
        && s.agrs == initial_agrs + s.funded_agrs
        && s.zdex == initial_zdex + s.funded_zdex
}
pub open spec fn successor(s: Totals, step: Step) -> Totals {
    match step {
        Step::Money { minted, burned, redistributed } => Totals {
            zusd: s.zusd + minted - burned,
            active: s.active + minted - burned - redistributed,
            default: s.default + redistributed,
            ..s
        },
        Step::FundAgrs { amount } => Totals {
            agrs: s.agrs + amount, funded_agrs: s.funded_agrs + amount, ..s
        },
        Step::FundZdex { amount } => Totals {
            zdex: s.zdex + amount, funded_zdex: s.funded_zdex + amount, ..s
        },
        Step::Reject => s,
    }
}
/// This one-step theorem assumes the pre-invariant, never the post-invariant.
pub proof fn one_step_conserves(s: Totals, step: Step, initial_agrs: int, initial_zdex: int)
    requires conserved(s, initial_agrs, initial_zdex),
    ensures conserved(successor(s, step), initial_agrs, initial_zdex),
{
    match step {
        Step::Money { minted, burned, redistributed } => {},
        Step::FundAgrs { amount } => {},
        Step::FundZdex { amount } => {},
        Step::Reject => {},
    }
}
/// Arbitrary finite length follows from the checked one-step transition theorem.
/// The runtime-to-successor refinement remains a separate obligation.
pub proof fn finite_trace_conserves(trace: Seq<Totals>, steps: Seq<Step>,
    initial_agrs: int, initial_zdex: int)
    requires trace.len() > 0, steps.len() + 1 == trace.len(),
        conserved(trace[0], initial_agrs, initial_zdex),
        forall|i: int| 0 <= i < steps.len() ==>
            trace[i + 1] == successor(trace[i], steps[i]),
    ensures conserved(trace.last(), initial_agrs, initial_zdex),
    decreases trace.len(),
{
    if trace.len() > 1 {
        finite_trace_conserves(trace.drop_last(), steps.drop_last(), initial_agrs, initial_zdex);
        let i = (steps.len() - 1) as int;
        one_step_conserves(trace[i], steps[i], initial_agrs, initial_zdex);
    }
}
}
