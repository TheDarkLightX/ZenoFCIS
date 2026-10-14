//! Actual owned ledger types and pure accounting.
//! Host parsing, hashing and JSON methods remain in the parent module.

extern crate alloc;
use self::alloc::{string::String, vec::Vec};
use super::super::value_types::{CheckerIdentity, Cost};
use super::membership;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// What a reservation pays for.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Stage {
    /// One attempt, whatever it turns out to be.
    Attempt,
    /// Intake/admission services before proposal bytes are interpreted.
    Preparation { work: u64 },
    /// One model call at its worst-case tokens and money.
    ModelCall {
        input_tokens: u32,
        output_tokens: u32,
        money_micros: u64,
    },
    /// One candidate check at its precharged work.
    Check { work: u64 },
    /// One resume replay at its precharged work.
    Replay { work: u64 },
}

/// One ledger record.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Kind {
    /// The session opened for exactly this request under this checker.
    Opened {
        request_id: String,
        checker: CheckerIdentity,
    },
    /// An allowance reserved before work. `attempt` is `None` for replays.
    Reserved { attempt: Option<u8>, stage: Stage },
    /// An attempt's outcome. Reservations stay charged.
    Settled {
        attempt: u8,
        outcome: String,
        candidate_sha256: Option<String>,
        cost: Option<Cost>,
    },
    /// The incumbent became this checked replacement.
    Replaced {
        attempt: u8,
        candidate_sha256: String,
        receipt_sha256: String,
        cost: Cost,
    },
    /// The session closed with this stop reason.
    Closed { reason: String },
}

/// A chained entry: `digest` commits to `sequence`, `previous` and `kind`.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct Entry {
    pub(crate) sequence: u64,
    pub(crate) previous: String,
    pub(crate) digest: String,
    pub(crate) kind: Kind,
}

/// The position a stored ledger must be at: entry count and last digest.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct Head {
    pub(crate) entries: u64,
    pub(crate) digest: String,
}

/// Why stored ledger bytes are not this ledger.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum LedgerFault {
    /// A line is not a ledger entry of this schema.
    Unreadable { line: usize },
    /// Sequence numbers are not `0, 1, 2, ...`.
    Sequence { line: usize },
    /// `previous` does not name the preceding digest.
    Link { line: usize },
    /// The recorded digest is not the recomputed one.
    Digest { line: usize },
    /// The first entry is not `Opened`, or `Opened` recurs.
    Opened { line: usize },
    /// An entry follows `Closed`.
    AfterClose { line: usize },
}

/// Totals derived from a ledger.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct Accounting {
    pub(crate) attempts: u8,
    pub(crate) checks: u8,
    pub(crate) replays: u8,
    pub(crate) model_calls: u8,
    pub(crate) tokens_reserved: u64,
    pub(crate) money_reserved: u64,
    pub(crate) work_reserved: u64,
    pub(crate) settled: u8,
    /// Attempts reserved but never settled: crash-pending, still charged.
    pub(crate) unresolved: Vec<u8>,
    /// The latest replacement, if any.
    pub(crate) replaced: Option<(u8, String, String, Cost)>,
    pub(crate) closed: Option<String>,
}

/// The in-memory ledger: the shell persists each new entry in order.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug, Default, Eq, PartialEq)]
pub(crate) struct Ledger {
    pub(super) entries: Vec<Entry>,
}

#[cfg(verus_keep_ghost)]
verus! {
/// Ghost view of the actual result, with full String values and ordered IDs.
/// This is neither an executable event representation nor a second evaluator.
pub(crate) struct AccountingView {
    pub attempts: u8,
    pub checks: u8,
    pub replays: u8,
    pub model_calls: u8,
    pub tokens_reserved: u64,
    pub money_reserved: u64,
    pub work_reserved: u64,
    pub settled: u8,
    pub unresolved: Seq<u8>,
    pub replaced: Option<(u8, String, String, Cost)>,
    pub closed: Option<String>,
}

pub closed spec fn view(t: Accounting) -> AccountingView {
    AccountingView {
        attempts: t.attempts, checks: t.checks, replays: t.replays,
        model_calls: t.model_calls, tokens_reserved: t.tokens_reserved,
        money_reserved: t.money_reserved, work_reserved: t.work_reserved,
        settled: t.settled, unresolved: t.unresolved@,
        replaced: t.replaced, closed: t.closed,
    }
}

pub(crate) open spec fn zero() -> AccountingView {
    AccountingView {
        attempts: 0, checks: 0, replays: 0, model_calls: 0,
        tokens_reserved: 0, money_reserved: 0, work_reserved: 0, settled: 0,
        unresolved: Seq::empty(), replaced: None, closed: None,
    }
}
spec fn add8(a: u8, b: u8) -> u8 {
    if a as int + b as int > u8::MAX { u8::MAX } else { (a + b) as u8 }
}
spec fn add64(a: u64, b: u64) -> u64 {
    if a as int + b as int > u64::MAX { u64::MAX } else { (a + b) as u64 }
}

/// Ordered observations before the global unsettled-membership filter.
spec fn prefix(entries: Seq<Entry>, n: int) -> AccountingView
    recommends 0 <= n <= entries.len(),
    decreases n,
{
    if n <= 0 { zero() }
    else {
        let p = prefix(entries, n - 1);
        match entries[n - 1].kind {
            Kind::Opened { .. } => p,
            Kind::Reserved { attempt: _, stage } => match stage {
                Stage::Attempt => AccountingView { attempts: add8(p.attempts, 1), ..p },
                Stage::ModelCall { input_tokens, output_tokens, money_micros } => AccountingView {
                    model_calls: add8(p.model_calls, 1),
                    tokens_reserved: add64(add64(p.tokens_reserved, input_tokens as u64), output_tokens as u64),
                    money_reserved: add64(p.money_reserved, money_micros), ..p
                },
                Stage::Check { work } => AccountingView {
                    checks: add8(p.checks, 1), work_reserved: add64(p.work_reserved, work), ..p
                },
                Stage::Preparation { work } => AccountingView {
                    work_reserved: add64(p.work_reserved, work), ..p
                },
                Stage::Replay { work } => AccountingView {
                    replays: add8(p.replays, 1), work_reserved: add64(p.work_reserved, work), ..p
                },
            },
            Kind::Settled { .. } => AccountingView { settled: add8(p.settled, 1), ..p },
            Kind::Replaced { attempt, candidate_sha256, receipt_sha256, cost } => AccountingView {
                replaced: Some((attempt, candidate_sha256, receipt_sha256, cost)), ..p
            },
            Kind::Closed { reason } => AccountingView { closed: Some(reason), ..p },
        }
    }
}

spec fn reserved_ids(entries: Seq<Entry>, n: int) -> Seq<u8>
    recommends 0 <= n <= entries.len(),
    decreases n,
{
    if n <= 0 { Seq::empty() }
    else {
        let p = reserved_ids(entries, n - 1);
        match entries[n - 1].kind {
            Kind::Reserved { attempt: Some(id), stage: Stage::Attempt } => p.push(id),
            _ => p,
        }
    }
}
spec fn settled_ids(entries: Seq<Entry>, n: int) -> Seq<u8>
    recommends 0 <= n <= entries.len(),
    decreases n,
{
    if n <= 0 { Seq::empty() }
    else {
        let p = settled_ids(entries, n - 1);
        match entries[n - 1].kind {
            Kind::Settled { attempt, .. } => p.push(attempt),
            _ => p,
        }
    }
}

/// UTF-8 payload requests, not character counts or allocator capacities.
spec fn utf8_payload(value: String) -> int {
    vstd::utf8::encode_utf8(value@).len() as int
}
spec fn replacement_payload(value: Option<(u8, String, String, Cost)>) -> int {
    match value {
        Some((_, candidate, receipt, _)) => utf8_payload(candidate) + utf8_payload(receipt),
        None => 0,
    }
}
spec fn closed_payload(value: Option<String>) -> int {
    match value { Some(reason) => utf8_payload(reason), None => 0 }
}

/// A named subtotal of actual requests. Other source operations are excluded;
/// this is not the full invocation tariff, CPU work or physical memory.
struct CallerRequests {
    saturating_adds: int,
    producer_pushes: int,
    clone_calls: int,
    clone_payload: int,
    discarded_payload: int,
}
spec fn no_requests() -> CallerRequests {
    CallerRequests {
        saturating_adds: 0, producer_pushes: 0, clone_calls: 0,
        clone_payload: 0, discarded_payload: 0,
    }
}
spec fn requests_prefix(entries: Seq<Entry>, n: int) -> CallerRequests
    recommends 0 <= n <= entries.len(),
    decreases n,
{
    if n <= 0 { no_requests() }
    else {
        let p = requests_prefix(entries, n - 1);
        let prior = prefix(entries, n - 1);
        match entries[n - 1].kind {
            Kind::Opened { .. } => p,
            Kind::Reserved { attempt, stage } => match stage {
                Stage::Attempt => CallerRequests {
                    saturating_adds: p.saturating_adds + 1,
                    producer_pushes: p.producer_pushes + if attempt.is_some() { 1int } else { 0int }, ..p
                },
                Stage::ModelCall { .. } => CallerRequests { saturating_adds: p.saturating_adds + 4, ..p },
                Stage::Check { .. } | Stage::Replay { .. } => CallerRequests { saturating_adds: p.saturating_adds + 2, ..p },
                Stage::Preparation { .. } => CallerRequests { saturating_adds: p.saturating_adds + 1, ..p },
            },
            Kind::Settled { .. } => CallerRequests {
                saturating_adds: p.saturating_adds + 1, producer_pushes: p.producer_pushes + 1, ..p
            },
            Kind::Replaced { candidate_sha256, receipt_sha256, .. } => CallerRequests {
                clone_calls: p.clone_calls + 2,
                clone_payload: p.clone_payload + utf8_payload(candidate_sha256) + utf8_payload(receipt_sha256),
                discarded_payload: p.discarded_payload + replacement_payload(prior.replaced), ..p
            },
            Kind::Closed { reason } => CallerRequests {
                clone_calls: p.clone_calls + 1,
                clone_payload: p.clone_payload + utf8_payload(reason),
                discarded_payload: p.discarded_payload + closed_payload(prior.closed), ..p
            },
        }
    }
}

pub closed spec fn entries_view(ledger: Ledger) -> Seq<Entry> { ledger.entries@ }

pub(crate) closed spec fn entry_count(ledger: Ledger) -> nat { ledger.entries@.len() }

pub(crate) closed spec fn derived(ledger: Ledger) -> AccountingView {
    let e = ledger.entries@;
    let r = reserved_ids(e, e.len() as int);
    let s = settled_ids(e, e.len() as int);
    AccountingView {
        unresolved: membership::retained(r, s, r.len() as int),
        ..prefix(e, e.len() as int)
    }
}
}

impl Ledger {
    /// Derives the totals. Reservations count whether or not they settled.
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures view(result) == derived(*self),
            result.unresolved@.len() <= entry_count(*self),
    ))]
    pub(crate) fn accounting(&self) -> Accounting {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(view); }
        let mut totals = empty_accounting();
        let mut reserved = Vec::new();
        let mut settled = Vec::new();
        #[cfg(verus_keep_ghost)]
        proof_decl! {
            let ghost mut saturating_adds: int = 0;
            let ghost mut producer_pushes: int = 0;
            let ghost mut clone_calls: int = 0;
            let ghost mut clone_payload: int = 0;
            let ghost mut discarded_payload: int = 0;
        }
        #[cfg_attr(verus_keep_ghost, verus_spec(iter =>
            invariant
                0 <= iter.index() <= self.entries@.len(),
                iter.seq() == self.entries@.as_ref(),
                view(totals) == prefix(self.entries@, iter.index()),
                reserved@ == reserved_ids(self.entries@, iter.index()),
                settled@ == settled_ids(self.entries@, iter.index()),
                reserved@.len() + settled@.len() <= iter.index(),
                saturating_adds == requests_prefix(self.entries@, iter.index()).saturating_adds,
                producer_pushes == requests_prefix(self.entries@, iter.index()).producer_pushes,
                clone_calls == requests_prefix(self.entries@, iter.index()).clone_calls,
                clone_payload == requests_prefix(self.entries@, iter.index()).clone_payload,
                discarded_payload == requests_prefix(self.entries@, iter.index()).discarded_payload,
                0 <= saturating_adds <= 4 * iter.index(),
                producer_pushes == reserved@.len() + settled@.len(),
                0 <= clone_calls <= 2 * iter.index(),
                0 <= discarded_payload <= clone_payload,
                clone_payload == discarded_payload
                    + replacement_payload(totals.replaced) + closed_payload(totals.closed),
        ))]
        for entry in &self.entries {
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(*entry == self.entries@[iter.index()]);
                reveal_with_fuel(prefix, 1);
                reveal_with_fuel(reserved_ids, 1);
                reveal_with_fuel(settled_ids, 1);
                reveal_with_fuel(requests_prefix, 1);
            }
            match &entry.kind {
                Kind::Opened { .. } => {}
                Kind::Reserved { attempt, stage } => match stage {
                    Stage::Attempt => {
                        #[cfg(verus_keep_ghost)]
                        proof! { saturating_adds = saturating_adds + 1; }
                        totals.attempts = totals.attempts.saturating_add(1);
                        if let Some(attempt) = attempt {
                            #[cfg(verus_keep_ghost)]
                            proof! { producer_pushes = producer_pushes + 1; }
                            reserved.push(*attempt);
                        }
                    }
                    Stage::ModelCall {
                        input_tokens,
                        output_tokens,
                        money_micros,
                    } => {
                        #[cfg(verus_keep_ghost)]
                        proof! { saturating_adds = saturating_adds + 4; }
                        totals.model_calls = totals.model_calls.saturating_add(1);
                        totals.tokens_reserved = totals
                            .tokens_reserved
                            .saturating_add(u64::from(*input_tokens))
                            .saturating_add(u64::from(*output_tokens));
                        totals.money_reserved = totals.money_reserved.saturating_add(*money_micros);
                    }
                    Stage::Check { work } => {
                        #[cfg(verus_keep_ghost)]
                        proof! { saturating_adds = saturating_adds + 2; }
                        totals.checks = totals.checks.saturating_add(1);
                        totals.work_reserved = totals.work_reserved.saturating_add(*work);
                    }
                    Stage::Preparation { work } => {
                        #[cfg(verus_keep_ghost)]
                        proof! { saturating_adds = saturating_adds + 1; }
                        totals.work_reserved = totals.work_reserved.saturating_add(*work);
                    }
                    Stage::Replay { work } => {
                        #[cfg(verus_keep_ghost)]
                        proof! { saturating_adds = saturating_adds + 2; }
                        totals.replays = totals.replays.saturating_add(1);
                        totals.work_reserved = totals.work_reserved.saturating_add(*work);
                    }
                },
                Kind::Settled { attempt, .. } => {
                    #[cfg(verus_keep_ghost)]
                    proof! { saturating_adds = saturating_adds + 1; }
                    totals.settled = totals.settled.saturating_add(1);
                    #[cfg(verus_keep_ghost)]
                    proof! { producer_pushes = producer_pushes + 1; }
                    settled.push(*attempt);
                }
                Kind::Replaced {
                    attempt,
                    candidate_sha256,
                    receipt_sha256,
                    cost,
                } => {
                    #[cfg(verus_keep_ghost)]
                    proof! {
                        discarded_payload = discarded_payload + replacement_payload(totals.replaced);
                        clone_calls = clone_calls + 2;
                        clone_payload = clone_payload
                            + utf8_payload(*candidate_sha256) + utf8_payload(*receipt_sha256);
                    }
                    totals.replaced = Some((
                        *attempt,
                        candidate_sha256.clone(),
                        receipt_sha256.clone(),
                        *cost,
                    ));
                }
                Kind::Closed { reason } => {
                    #[cfg(verus_keep_ghost)]
                    proof! {
                        discarded_payload = discarded_payload + closed_payload(totals.closed);
                        clone_calls = clone_calls + 1;
                        clone_payload = clone_payload + utf8_payload(*reason);
                    }
                    totals.closed = Some(reason.clone());
                }
            }
        }
        #[cfg(verus_keep_ghost)]
        proof! { reveal(derived); reveal(entry_count); }
        totals.unresolved = membership::unresolved_attempts(&reserved, &settled);
        #[cfg(verus_keep_ghost)]
        proof! {
            let requested_filter_payload = reserved@.len() as int;
            let producer_drop_payload = reserved@.len() as int + settled@.len() as int;
            let helper_services = membership::source_services(settled@.len() as int,
                reserved@.len() as int, totals.unresolved@.len() as int);
            membership::service_bound(settled@.len() as int, reserved@.len() as int,
                totals.unresolved@.len() as int);
            assert(producer_drop_payload == producer_pushes);
            assert(helper_services + requested_filter_payload
                <= 271 + 15 * self.entries@.len());
            // Only these named call/push services and the helper's subtotal.
            // Cloned payload, discarded payload and all excluded operations
            // remain separate; no total work or allocator bound is inferred.
            assert(saturating_adds + producer_pushes + clone_calls
                + helper_services + requested_filter_payload <= 271 + 22 * self.entries@.len());
            assert(clone_payload == discarded_payload
                + replacement_payload(totals.replaced) + closed_payload(totals.closed));
        }
        totals
    }
}

/// Owned exact accounting cache. No admission or receipt authority is implied.
/// Once consumed, later mutation of Ledger does not update the returned totals.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug)]
pub(crate) struct AccountedLedger {
    ledger: Ledger,
    totals: Accounting,
}

#[cfg(verus_keep_ghost)]
verus! {
impl AccountedLedger {
    #[verifier::type_invariant]
    spec fn well_formed(&self) -> bool {
        view(self.totals) == derived(self.ledger)
    }
    pub(crate) closed spec fn ledger_view(&self) -> Ledger { self.ledger }
    pub(crate) closed spec fn totals_view(&self) -> Accounting { self.totals }
}
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result.ledger_view() == ledger,
        view(result.totals_view()) == derived(ledger),
))]
pub(crate) fn account_ledger(ledger: Ledger) -> AccountedLedger {
    let totals = ledger.accounting();
    #[cfg(verus_keep_ghost)]
    proof! {
        reveal(AccountedLedger::ledger_view);
        reveal(AccountedLedger::totals_view);
    }
    AccountedLedger { ledger, totals }
}

impl AccountedLedger {
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures *result == self.ledger_view(),
    ))]
    pub(crate) fn ledger(&self) -> &Ledger {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(AccountedLedger::ledger_view); }
        &self.ledger
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures *result == self.totals_view(),
            view(*result) == derived(self.ledger_view()),
    ))]
    pub(crate) fn totals(&self) -> &Accounting {
        #[cfg(verus_keep_ghost)]
        proof! {
            use_type_invariant(self);
            reveal(AccountedLedger::ledger_view);
            reveal(AccountedLedger::totals_view);
        }
        &self.totals
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures result.0 == self.ledger_view(), result.1 == self.totals_view(),
            view(result.1) == derived(result.0),
    ))]
    pub(crate) fn into_parts(self) -> (Ledger, Accounting) {
        #[cfg(verus_keep_ghost)]
        proof! {
            use_type_invariant(&self);
            reveal(AccountedLedger::ledger_view);
            reveal(AccountedLedger::totals_view);
        }
        (self.ledger, self.totals)
    }
}

// The actual zero constructor and explicit Clone implementations.
// Host Default compatibility delegates here; the accounting loop calls directly.
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures view(result) == zero(),))]
pub(super) fn empty_accounting() -> Accounting {
    #[cfg(verus_keep_ghost)]
    proof! { reveal(view); }
    Accounting {
        attempts: 0,
        checks: 0,
        replays: 0,
        model_calls: 0,
        tokens_reserved: 0,
        money_reserved: 0,
        work_reserved: 0,
        settled: 0,
        unresolved: Vec::new(),
        replaced: None,
        closed: None,
    }
}

impl Clone for Stage {
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == *self,))]
    fn clone(&self) -> Self {
        match self {
            Self::Attempt => Self::Attempt,
            Self::Preparation { work } => Self::Preparation { work: *work },
            Self::ModelCall {
                input_tokens,
                output_tokens,
                money_micros,
            } => Self::ModelCall {
                input_tokens: *input_tokens,
                output_tokens: *output_tokens,
                money_micros: *money_micros,
            },
            Self::Check { work } => Self::Check { work: *work },
            Self::Replay { work } => Self::Replay { work: *work },
        }
    }
}

impl Clone for Kind {
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == *self,))]
    fn clone(&self) -> Self {
        match self {
            Self::Opened {
                request_id,
                checker,
            } => Self::Opened {
                request_id: request_id.clone(),
                checker: checker.clone(),
            },
            Self::Reserved { attempt, stage } => Self::Reserved {
                attempt: *attempt,
                stage: stage.clone(),
            },
            Self::Settled {
                attempt,
                outcome,
                candidate_sha256,
                cost,
            } => Self::Settled {
                attempt: *attempt,
                outcome: outcome.clone(),
                candidate_sha256: candidate_sha256.clone(),
                cost: *cost,
            },
            Self::Replaced {
                attempt,
                candidate_sha256,
                receipt_sha256,
                cost,
            } => Self::Replaced {
                attempt: *attempt,
                candidate_sha256: candidate_sha256.clone(),
                receipt_sha256: receipt_sha256.clone(),
                cost: *cost,
            },
            Self::Closed { reason } => Self::Closed {
                reason: reason.clone(),
            },
        }
    }
}

impl Clone for Entry {
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == *self,))]
    fn clone(&self) -> Self {
        Self {
            sequence: self.sequence,
            previous: self.previous.clone(),
            digest: self.digest.clone(),
            kind: self.kind.clone(),
        }
    }
}

impl Clone for Head {
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == *self,))]
    fn clone(&self) -> Self {
        Self {
            entries: self.entries,
            digest: self.digest.clone(),
        }
    }
}

impl Clone for LedgerFault {
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == *self,))]
    fn clone(&self) -> Self {
        match self {
            Self::Unreadable { line } => Self::Unreadable { line: *line },
            Self::Sequence { line } => Self::Sequence { line: *line },
            Self::Link { line } => Self::Link { line: *line },
            Self::Digest { line } => Self::Digest { line: *line },
            Self::Opened { line } => Self::Opened { line: *line },
            Self::AfterClose { line } => Self::AfterClose { line: *line },
        }
    }
}

impl Clone for Accounting {
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures view(result) == view(*self),))]
    fn clone(&self) -> Self {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(view); }
        Self {
            attempts: self.attempts,
            checks: self.checks,
            replays: self.replays,
            model_calls: self.model_calls,
            tokens_reserved: self.tokens_reserved,
            money_reserved: self.money_reserved,
            work_reserved: self.work_reserved,
            settled: self.settled,
            unresolved: self.unresolved.clone(),
            replaced: match &self.replaced {
                Some((attempt, candidate, receipt, cost)) => {
                    let candidate = candidate.clone();
                    let receipt = receipt.clone();
                    Some((*attempt, candidate, receipt, *cost))
                }
                None => None,
            },
            closed: self.closed.clone(),
        }
    }
}

impl Clone for Ledger {
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures entries_view(result) == entries_view(*self),
    ))]
    fn clone(&self) -> Self {
        let entries = self.entries.clone();
        #[cfg(verus_keep_ghost)]
        proof! {
            reveal(entries_view);
            assert forall|i: int| 0 <= i < self.entries@.len()
                implies entries@[i] == self.entries@[i] by {
                assert(vstd::pervasive::cloned::<Entry>(self.entries@[i], entries@[i]));
            }
            assert(entries@ =~= self.entries@);
        }
        Self { entries }
    }
}
