//! Checks the executed application against the decision examples and against
//! a reference model of the README's rules over a scaled finite domain.
//!
//! Every decision runs through the application: schema admission, the
//! authority, the adapter in `src/program.rs` with its synthesized core, the
//! law checker, the committed patch, and the outbox plan. Fields are read by
//! the numeric IDs in `project.zeno`, not through the generated name bindings,
//! so a binding that swapped two fields or variants fails here. On every
//! committed decision, the invariants of claims 600 and 601 are evaluated
//! before and after, through the law checker's own observer.
//!
//! The domain is small enough to cross every rule input: every proposal from
//! every idle balance and budget position, every callback from every pending
//! swap, every caller, model, and price age at every clock boundary, and a
//! search of every state one day can reach from genesis. The reference model
//! restates the README's rules; its author also wrote the program, so it
//! catches binding and adapter errors, not a misreading shared by both. The
//! examples file, once reviewed by the project's owner, is the check on that.

use agent_treasury_guard::{
    Authority, authority, bindings::GeneratedProject, command, context, generated::*,
    genesis_state, laws::trace_step, profile, program::least_min_out, synthesized, treasury,
};
use std::collections::{BTreeMap, BTreeSet};
use zeno_fcis_codec::Domain;
use zeno_fcis_core::Decision;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_schema::ValidationLimits;
use zeno_fcis_spec::{
    ClaimFormula, EvalLimits, EvalOutcome, EvaluationContext, Identifier, PredicateProvider,
    ProjectionRoot, RelExpr, StableId, TraceStep, evaluate_relational, invariant_at,
};
use zeno_fcis_value::Value;

const EXAMPLES: &str = include_str!("decision-examples.txt");
const NO_SWAP: u16 = 170;
const PENDING_BUY: u16 = 171;
const PENDING_SELL: u16 = 172;
const BUY: u16 = 173;
const SELL: u16 = 174;
const PROPOSE: u16 = 175;
const SETTLED: u16 = 176;
const FAILED: u16 = 177;
const AGENT: u16 = 178;
const DEX: u16 = 179;
const V2: u16 = 180;
const V1: u16 = 181;
const UNLISTED: u16 = 182;
const QUOTE: u16 = 183;
const BASE: u16 = 184;
/// The scaled domain's constants, as the README states them.
const DAY: i128 = 4;
const LAST_TICK: i128 = 11;
const PRICE_AGE: i128 = 1;
const TTL: i128 = 2;
const BUDGET: i128 = 4;
const CAP: i128 = 3;
const RESERVE: i128 = 2;
const BALANCE_BOUND: i128 = 20;
const AMOUNTS: [i128; 3] = [1, 2, 3];
const MIN_OUTS: [i128; 4] = [0, 1, 2, 3];
const AMOUNT_OUTS: [i128; 4] = [0, 1, 2, 3];
const PRICES: [i128; 2] = [1, 2];

/// Fields 130 `quote`, 131 `base`, 132 `spent_today`, 133 `last_seen`, 134
/// `pending` (a variant ID), 135 `pending_amount`, and 136 `pending_min_out`.
type State = (i128, i128, i128, i128, u16, i128, i128);
/// Command fields 140 `action` and 141 `direction` (variant IDs), 142
/// `amount`, 143 `min_out`, 144 `intent`, and 145 `amount_out`.
type Input = (u16, u16, i128, i128, i128, i128);
/// Context fields 150 `caller` (a variant ID), 151 `now`, 152 `price`, 153
/// `price_time`, and 154 `model` (a variant ID).
type Context = (u16, i128, i128, i128, u16);
/// A queued request: payload fields 160 `intent_number`, 161 `asset_in` and
/// 162 `asset_out` (variant IDs), 163 `amount_in`, 164 `min_amount_out`, and
/// 165 `deadline`.
type Request = (i128, u16, u16, i128, i128, i128);

/// One decision, in the numeric IDs of `project.zeno`.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Outcome {
    /// `accept`, `reject`, or `failure`.
    kind: &'static str,
    /// The rejection or committed-failure reason.
    reason: Option<u32>,
    /// The state after the decision.
    post: State,
    /// Each queued request, all on channel 300.
    requests: Vec<Request>,
}

fn record_field(value: &Value, id: u16) -> &Value {
    let Value::Record(fields) = value else {
        panic!("record expected, got {value:?}");
    };
    fields
        .iter()
        .find(|field| field.id() == id)
        .unwrap_or_else(|| panic!("field {id} missing from {value:?}"))
        .value()
}

fn int(value: &Value, id: u16) -> i128 {
    match record_field(value, id) {
        Value::I128(value) => *value,
        other => panic!("field {id} holds {other:?}"),
    }
}

/// The variant ID of an enum or sum value.
fn variant(value: &Value) -> u16 {
    match value {
        Value::Enum { variant, .. } | Value::Sum { variant, .. } => *variant,
        other => panic!("variant expected, got {other:?}"),
    }
}

fn pending(id: u16) -> PendingSwap {
    match id {
        NO_SWAP => PendingSwap::NoSwap,
        PENDING_BUY => PendingSwap::PendingBuy,
        PENDING_SELL => PendingSwap::PendingSell,
        other => panic!("unknown pending variant {other}"),
    }
}

fn direction(id: u16) -> Direction {
    match id {
        BUY => Direction::BuyBase,
        SELL => Direction::SellBase,
        other => panic!("unknown direction variant {other}"),
    }
}

fn action(id: u16) -> TreasuryAction {
    match id {
        PROPOSE => TreasuryAction::ProposeSwap,
        SETTLED => TreasuryAction::SwapSettled,
        FAILED => TreasuryAction::SwapFailed,
        other => panic!("unknown action variant {other}"),
    }
}

fn caller(id: u16) -> Caller {
    match id {
        AGENT => Caller::Agent,
        DEX => Caller::Dex,
        other => panic!("unknown caller variant {other}"),
    }
}

fn model_id(id: u16) -> ModelId {
    match id {
        V2 => ModelId::TreasuryAgentV2,
        V1 => ModelId::TreasuryAgentV1,
        UNLISTED => ModelId::UnlistedModel,
        other => panic!("unknown model variant {other}"),
    }
}

fn state((quote, base, spent, seen, pending_id, amount, min_out): State) -> Treasury {
    treasury(
        quote,
        base,
        spent,
        seen,
        pending(pending_id),
        amount,
        min_out,
    )
}

fn state_of(value: &Value) -> State {
    (
        int(value, 130),
        int(value, 131),
        int(value, 132),
        int(value, 133),
        variant(record_field(value, 134)),
        int(value, 135),
        int(value, 136),
    )
}

/// Runs one command through the application from an admitted pre-state.
fn observe(authority: &Authority, pre: State, input: Input, ctx: Context) -> Outcome {
    let (action_id, direction_id, amount, min_out, intent, amount_out) = input;
    let (caller_id, now, price, price_time, model) = ctx;
    let project = GeneratedProject::try_new::<RustCryptoSha256>().unwrap();
    let limits = ValidationLimits::default();
    let root = project
        .admit_root::<RustCryptoSha256>(&state(pre), limits)
        .unwrap();
    let pre_value = root.value().value().clone();
    // The typed bindings must place each field and variant at its numeric ID.
    assert_eq!(state_of(&pre_value), pre);
    let admitted_command = project
        .admit_command::<RustCryptoSha256>(
            &command(
                action(action_id),
                direction(direction_id),
                amount,
                min_out,
                intent,
                amount_out,
            ),
            limits,
        )
        .unwrap();
    let command_value = admitted_command.admitted().value().value();
    assert_eq!(
        (
            variant(record_field(command_value, 140)),
            variant(record_field(command_value, 141)),
            int(command_value, 142),
            int(command_value, 143),
            int(command_value, 144),
            int(command_value, 145),
        ),
        input
    );
    let admitted_context = project
        .admit_context::<RustCryptoSha256>(
            &context(caller(caller_id), now, price, price_time, model_id(model)),
            limits,
        )
        .unwrap();
    let context_value = admitted_context.admitted().value().value();
    assert_eq!(
        (
            variant(record_field(context_value, 150)),
            int(context_value, 151),
            int(context_value, 152),
            int(context_value, 153),
            variant(record_field(context_value, 154)),
        ),
        ctx
    );
    let replay = format!("conformance {pre:?} {input:?} {ctx:?}");
    let witness = authority
        .admit_invocation(
            root,
            admitted_command.admitted().clone(),
            admitted_context.admitted().clone(),
            profile::digest("example/agent-treasury-guard/principal", b"conformance"),
            profile::digest(
                "example/agent-treasury-guard/authentication",
                b"conformance",
            ),
            profile::digest("example/agent-treasury-guard/replay", replay.as_bytes()),
        )
        .unwrap();
    let domain = Domain::new("example/agent-treasury-guard/state", 1).unwrap();
    macro_rules! committed {
        ($kind:expr, $reason:expr, $candidate:expr) => {{
            let bundle = $candidate.bundle();
            assert!(bundle.commit_plan().effects().is_empty());
            let applied = bundle
                .patch()
                .apply::<RustCryptoSha256>(&pre_value, domain)
                .unwrap();
            Outcome {
                kind: $kind,
                reason: $reason,
                post: state_of(applied.state()),
                requests: bundle
                    .outbox_plan()
                    .entries()
                    .iter()
                    .map(|entry| {
                        assert_eq!(entry.channel(), 300);
                        assert_eq!(entry.destination(), &Value::Text("zenodex".into()));
                        let payload = entry.payload();
                        (
                            int(payload, 160),
                            variant(record_field(payload, 161)),
                            variant(record_field(payload, 162)),
                            int(payload, 163),
                            int(payload, 164),
                            int(payload, 165),
                        )
                    })
                    .collect(),
            }
        }};
    }
    match authority.execute(witness).unwrap() {
        Decision::Reject(reject) => Outcome {
            kind: "reject",
            reason: Some(reject.reason().rejection().reason_id().get()),
            post: pre,
            requests: Vec::new(),
        },
        Decision::Accept(accepted) => {
            let candidate = accepted.into_candidate();
            committed!("accept", None, candidate)
        }
        Decision::CommittedFailure(failed) => {
            let (candidate, reason) = failed.into_parts();
            committed!("failure", Some(reason.get()), candidate)
        }
    }
}

/// Three quarters of the oracle value, rounded up.
fn least_minimum(direction_id: u16, amount: i128, price: i128) -> i128 {
    let (numerator, denominator) = if direction_id == BUY {
        (amount * 3, 4 * price)
    } else {
        (amount * price * 3, 4)
    };
    (numerator + denominator - 1) / denominator
}

/// The README's rules, restated independently of `src/program.rs`.
fn model(pre: State, input: Input, ctx: Context) -> Outcome {
    let (quote, base, spent_today, last_seen, pending_id, held, held_min_out) = pre;
    let (action_id, direction_id, amount, min_out, intent, amount_out) = input;
    let (caller_id, now, price, price_time, model) = ctx;
    let reject = |reason| Outcome {
        kind: "reject",
        reason: Some(reason),
        post: pre,
        requests: Vec::new(),
    };
    let sender = if action_id == PROPOSE { AGENT } else { DEX };
    if caller_id != sender {
        return reject(200);
    }
    if now <= last_seen {
        return reject(201);
    }
    // A later day starts its budget from zero.
    let spent = if now / DAY > last_seen / DAY {
        0
    } else {
        spent_today
    };
    if action_id == PROPOSE {
        let value = if direction_id == BUY {
            amount
        } else {
            amount * price
        };
        if model != V2 {
            return reject(202);
        }
        if price_time > now || now - price_time > PRICE_AGE {
            return reject(203);
        }
        if pending_id != NO_SWAP {
            return reject(204);
        }
        if value > CAP {
            return reject(208);
        }
        if spent + value > BUDGET {
            return reject(209);
        }
        if min_out < least_minimum(direction_id, amount, price) {
            return reject(210);
        }
        let (post, assets) = if direction_id == BUY {
            if quote - amount < RESERVE {
                return reject(211);
            }
            (
                (
                    quote - amount,
                    base,
                    spent + value,
                    now,
                    PENDING_BUY,
                    amount,
                    min_out,
                ),
                (QUOTE, BASE),
            )
        } else {
            if base - amount < 0 {
                return reject(211);
            }
            (
                (
                    quote,
                    base - amount,
                    spent + value,
                    now,
                    PENDING_SELL,
                    amount,
                    min_out,
                ),
                (BASE, QUOTE),
            )
        };
        return Outcome {
            kind: "accept",
            reason: None,
            post,
            requests: vec![(now, assets.0, assets.1, amount, min_out, now + TTL)],
        };
    }
    if pending_id == NO_SWAP {
        return reject(205);
    }
    if intent != last_seen {
        return reject(206);
    }
    let bought_base = pending_id == PENDING_BUY;
    if action_id == SETTLED {
        if amount_out < held_min_out {
            return reject(207);
        }
        let (quote, base) = if bought_base {
            (quote, base + amount_out)
        } else {
            (quote + amount_out, base)
        };
        return Outcome {
            kind: "accept",
            reason: None,
            post: (quote, base, spent, now, NO_SWAP, 0, 0),
            requests: Vec::new(),
        };
    }
    // A failure refunds the held amount; the budget stays committed.
    let (quote, base) = if bought_base {
        (quote + held, base)
    } else {
        (quote, base + held)
    };
    Outcome {
        kind: "failure",
        reason: Some(212),
        post: (quote, base, spent, now, NO_SWAP, 0, 0),
        requests: Vec::new(),
    }
}

/// Every proposal: both directions, every amount, every minimum output.
fn proposals() -> Vec<Input> {
    let mut inputs = Vec::new();
    for direction_id in [BUY, SELL] {
        for amount in AMOUNTS {
            for min_out in MIN_OUTS {
                inputs.push((PROPOSE, direction_id, amount, min_out, 0, 0));
            }
        }
    }
    inputs
}

/// Every settlement and failure naming one of `intents`.
fn callbacks(intents: &[i128]) -> Vec<Input> {
    let mut inputs = Vec::new();
    for &intent in intents {
        for amount_out in AMOUNT_OUTS {
            inputs.push((SETTLED, BUY, 1, 0, intent, amount_out));
        }
        inputs.push((FAILED, BUY, 1, 0, intent, 0));
    }
    inputs
}

/// A fresh, approved request from `caller_id` at `now`.
fn fresh(caller_id: u16, now: i128, price: i128) -> Context {
    (caller_id, now, price, now, V2)
}

struct NoPredicates;
impl PredicateProvider for NoPredicates {
    fn evaluate(&self, _: &Identifier, _: &[i128]) -> Option<bool> {
        None
    }
}

fn holds(formula: &RelExpr, step: &TraceStep) -> bool {
    matches!(
        evaluate_relational(
            formula,
            EvaluationContext::new(step, &NoPredicates, EvalLimits::default())
        ),
        EvalOutcome::True
    )
}

/// The invariants of claims 600 and 601, each over the treasury before and
/// after a decision. Their induction steps are attested for every integer;
/// the tests evaluate them on each committed decision's actual transition, as
/// the law checker observes it.
struct Invariants(Vec<(u32, RelExpr, RelExpr)>);

impl Invariants {
    fn load() -> Self {
        let spec = profile::project();
        Self(
            [600, 601]
                .into_iter()
                .map(|id| {
                    let claim = spec
                        .claim(StableId::new(id).unwrap())
                        .unwrap_or_else(|| panic!("claim {id} is declared"));
                    let ClaimFormula::Relational(before) = claim.formula() else {
                        panic!("claim {id} states a relational invariant");
                    };
                    let after = invariant_at(before, ProjectionRoot::Post)
                        .expect("an invariant over pre. paths");
                    (id, before.clone(), after)
                })
                .collect(),
        )
    }

    /// Checks every invariant before and after a decision that committed,
    /// and returns whether it did.
    fn hold_across(&self, pre: State, outcome: &Outcome, input: Input, ctx: Context) -> bool {
        if outcome.kind == "reject" {
            return false;
        }
        let step = trace_step(
            &state(pre),
            &state(outcome.post),
            &command(
                action(input.0),
                direction(input.1),
                input.2,
                input.3,
                input.4,
                input.5,
            ),
            &context(caller(ctx.0), ctx.1, ctx.2, ctx.3, model_id(ctx.4)),
        )
        .unwrap();
        for (id, before, after) in &self.0 {
            assert!(
                holds(before, &step),
                "claim {id} before {pre:?} {input:?} {ctx:?}"
            );
            assert!(
                holds(after, &step),
                "claim {id} after {pre:?} {input:?} {ctx:?}"
            );
        }
        true
    }
}

#[test]
fn every_proposal_matches_the_reference_model() {
    let authority = authority().unwrap();
    let invariants = Invariants::load();
    let (mut checked, mut committed) = (0, 0);
    // Every idle treasury that keeps the reserve, from the reserve to one
    // above it plus the largest amount, with every base balance a sell could
    // need and every budget position, at the start of day 1.
    for quote in 2..=6 {
        for base in 0..=3 {
            for spent in 0..=BUDGET {
                let pre = (quote, base, spent, 4, NO_SWAP, 0, 0);
                // The same day, and the next day, which restarts the budget.
                for now in [5, 8] {
                    for price in PRICES {
                        for input in proposals() {
                            let ctx = fresh(AGENT, now, price);
                            let outcome = observe(&authority, pre, input, ctx);
                            assert_eq!(
                                outcome,
                                model(pre, input, ctx),
                                "state {pre:?} command {input:?} context {ctx:?}"
                            );
                            committed +=
                                usize::from(invariants.hold_across(pre, &outcome, input, ctx));
                            checked += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!((checked, committed), (9_600, 2_352));
}

#[test]
fn every_callback_matches_the_reference_model() {
    let authority = authority().unwrap();
    let mut states = Vec::new();
    // Every pending swap: both directions, every held amount and minimum,
    // with balances at the reserve and above it, and both budget extremes.
    for pending_id in [PENDING_BUY, PENDING_SELL] {
        for held in AMOUNTS {
            for held_min_out in MIN_OUTS {
                for quote in [2, 6] {
                    for base in [0, 3] {
                        for spent in [0, BUDGET] {
                            states.push((quote, base, spent, 5, pending_id, held, held_min_out));
                        }
                    }
                }
            }
        }
    }
    for spent in [0, BUDGET] {
        states.push((6, 1, spent, 5, NO_SWAP, 0, 0));
    }
    assert_eq!(states.len(), 194);
    let invariants = Invariants::load();
    let (mut checked, mut committed) = (0, 0);
    for pre in states {
        // Intents before, at, and after the outstanding one, on the same day
        // and on the next.
        for now in [6, 8] {
            for input in callbacks(&[0, 4, 5, 6]) {
                let ctx = fresh(DEX, now, 1);
                let outcome = observe(&authority, pre, input, ctx);
                assert_eq!(
                    outcome,
                    model(pre, input, ctx),
                    "state {pre:?} command {input:?} context {ctx:?}"
                );
                committed += usize::from(invariants.hold_across(pre, &outcome, input, ctx));
                checked += 1;
            }
        }
    }
    assert_eq!((checked, committed), (7_760, 1_344));
}

#[test]
fn every_caller_model_and_clock_boundary_matches_the_reference_model() {
    let authority = authority().unwrap();
    let states = [
        (6, 1, 0, 5, NO_SWAP, 0, 0),
        (4, 1, 2, 5, PENDING_BUY, 2, 2),
        (6, 0, 2, 5, PENDING_SELL, 1, 2),
    ];
    let inputs = [
        (PROPOSE, BUY, 1, 1, 0, 0),
        (SETTLED, BUY, 1, 0, 5, 2),
        (FAILED, BUY, 1, 0, 5, 0),
    ];
    let invariants = Invariants::load();
    let (mut checked, mut committed) = (0, 0);
    for pre in states {
        for input in inputs {
            for caller_id in [AGENT, DEX] {
                for model_variant in [V2, V1, UNLISTED] {
                    for price in PRICES {
                        // Before, at, and after the last commit, the next
                        // day, and the last tick; prices two ticks old, one
                        // tick old, current, and from the future.
                        for now in [4, 5, 6, 7, 8, LAST_TICK] {
                            for price_time in [now - 2, now - 1, now, now + 1] {
                                if !(0..=LAST_TICK).contains(&price_time) {
                                    continue;
                                }
                                let ctx = (caller_id, now, price, price_time, model_variant);
                                let outcome = observe(&authority, pre, input, ctx);
                                assert_eq!(
                                    outcome,
                                    model(pre, input, ctx),
                                    "state {pre:?} command {input:?} context {ctx:?}"
                                );
                                committed +=
                                    usize::from(invariants.hold_across(pre, &outcome, input, ctx));
                                checked += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!((checked, committed), (2_484, 376));
}

/// Explores every state the application can reach from genesis within day 0,
/// through every proposal, every settlement and failure naming a tick of the
/// day, at every remaining tick and price, checking the invariants across
/// each committed decision. Returns the committing edges, each with the quote
/// value its proposal committed, and the number of decisions.
fn day_zero_reach(
    authority: &Authority,
    invariants: &Invariants,
) -> (Vec<(State, State, i128)>, usize) {
    let genesis = (6, 1, 0, 0, NO_SWAP, 0, 0);
    let mut reached = BTreeSet::from([genesis]);
    let mut pending = vec![genesis];
    let mut edges = Vec::new();
    let mut decided = 0;
    let inputs: Vec<Input> = proposals()
        .into_iter()
        .chain(callbacks(&[0, 1, 2, 3]))
        .collect();
    while let Some(pre) = pending.pop() {
        for now in (pre.3 + 1)..DAY {
            for price in PRICES {
                for input in &inputs {
                    let sender = if input.0 == PROPOSE { AGENT } else { DEX };
                    let ctx = fresh(sender, now, price);
                    let outcome = observe(authority, pre, *input, ctx);
                    decided += 1;
                    if !invariants.hold_across(pre, &outcome, *input, ctx) {
                        continue;
                    }
                    let value = match (input.0, input.1) {
                        (PROPOSE, BUY) => input.2,
                        (PROPOSE, SELL) => input.2 * price,
                        _ => 0,
                    };
                    edges.push((pre, outcome.post, value));
                    if reached.insert(outcome.post) {
                        pending.push(outcome.post);
                    }
                }
            }
        }
    }
    (edges, decided)
}

#[test]
fn no_sequence_within_a_day_commits_more_than_the_budget() {
    let authority = authority().unwrap();
    let (edges, decided) = day_zero_reach(&authority, &Invariants::load());
    let states: BTreeSet<State> = edges
        .iter()
        .flat_map(|(pre, post, _)| [*pre, *post])
        .collect();
    assert_eq!(states.len(), 230);
    assert_eq!((decided, edges.len()), (5_104, 524));
    for state in &states {
        assert!(state.0 >= RESERVE && state.1 >= 0 && state.2 <= BUDGET);
        assert!(state.0 <= BALANCE_BOUND && state.1 <= BALANCE_BOUND);
    }
    // Every commit advances the tick, so the graph has no cycles: relaxing the
    // edges in tick order finds, for each state, the largest sum of proposal
    // values along any path from genesis to it.
    let mut committed: BTreeMap<State, i128> = states.iter().map(|state| (*state, 0)).collect();
    let mut ordered = edges.clone();
    ordered.sort_by_key(|(pre, _, _)| pre.3);
    for (pre, post, value) in &ordered {
        let total = committed[pre] + value;
        if total > committed[post] {
            committed.insert(*post, total);
        }
    }
    // Along every path, whether or not a swap failed in between, the accepted
    // proposals commit at most the budget; the budget is reached, so the
    // bound is tight.
    assert_eq!(committed.values().copied().max(), Some(BUDGET));
    for (state, total) in &committed {
        assert!(*total <= BUDGET, "{state:?} committed {total}");
    }
}

/// Explores the reference model over all three days, through every command
/// at every later tick and price, from both callers.
fn model_reach() -> BTreeSet<State> {
    let genesis = (6, 1, 0, 0, NO_SWAP, 0, 0);
    let mut reached = BTreeSet::from([genesis]);
    let mut pending = vec![genesis];
    let inputs: Vec<Input> = proposals()
        .into_iter()
        .chain(callbacks(&(0..=LAST_TICK).collect::<Vec<_>>()))
        .collect();
    while let Some(pre) = pending.pop() {
        for now in (pre.3 + 1)..=LAST_TICK {
            for price in PRICES {
                for caller_id in [AGENT, DEX] {
                    for input in &inputs {
                        let outcome = model(pre, *input, fresh(caller_id, now, price));
                        if outcome.kind != "reject" && reached.insert(outcome.post) {
                            pending.push(outcome.post);
                        }
                    }
                }
            }
        }
    }
    reached
}

#[test]
fn the_reference_model_stays_within_the_balance_bound() {
    let reached = model_reach();
    assert_eq!(reached.len(), 13_893);
    let largest_quote = reached.iter().map(|state| state.0).max().unwrap();
    let largest_base = reached.iter().map(|state| state.1).max().unwrap();
    assert_eq!((largest_quote, largest_base), (17, 13));
    // No settlement or refund that the model can reach would take a balance
    // past the schema bound, so the program never fails to stage one.
    assert!(largest_quote < BALANCE_BOUND && largest_base < BALANCE_BOUND);
    for state in &reached {
        assert!(
            state.0 >= RESERVE && state.1 >= 0 && state.2 <= BUDGET,
            "{state:?}"
        );
        assert_eq!(
            state.4 == NO_SWAP,
            state.5 == 0 && state.6 == 0,
            "{state:?}"
        );
    }
}

/// Parses `state | command | context | outcome reason post | request`. The
/// state and post are the seven fields in order, the command its six, and the
/// context its five; the request is `-` or the six payload fields.
fn parse_example(line: &str) -> ((State, Input, Context), Outcome) {
    let parts: Vec<&str> = line.split('|').map(str::trim).collect();
    let [pre, input, ctx, decision, request] = parts.as_slice() else {
        panic!("five columns expected: {line}");
    };
    let numbers = |text: &str| -> Vec<i128> {
        text.split_whitespace()
            .map(|word| word.parse().unwrap())
            .collect()
    };
    let id = |raw: i128| u16::try_from(raw).unwrap();
    let state_of = |values: &[i128]| -> State {
        let [quote, base, spent, seen, pending_id, held, held_min_out] = values else {
            panic!("seven state fields expected: {line}");
        };
        (
            *quote,
            *base,
            *spent,
            *seen,
            id(*pending_id),
            *held,
            *held_min_out,
        )
    };
    let pre = state_of(&numbers(pre));
    let input = numbers(input);
    let [action_id, direction_id, amount, min_out, intent, amount_out] = input.as_slice() else {
        panic!("six command fields expected: {line}");
    };
    let input = (
        id(*action_id),
        id(*direction_id),
        *amount,
        *min_out,
        *intent,
        *amount_out,
    );
    let ctx = numbers(ctx);
    let [caller_id, now, price, price_time, model] = ctx.as_slice() else {
        panic!("five context fields expected: {line}");
    };
    let ctx = (id(*caller_id), *now, *price, *price_time, id(*model));
    let decision: Vec<&str> = decision.split_whitespace().collect();
    let [kind, reason, post @ ..] = decision.as_slice() else {
        panic!("decision fields expected: {line}");
    };
    let kind = match *kind {
        "accept" => "accept",
        "reject" => "reject",
        "failure" => "failure",
        other => panic!("unknown outcome {other}"),
    };
    let reason = (*reason != "-").then(|| reason.parse().unwrap());
    let post: Vec<i128> = post.iter().map(|word| word.parse().unwrap()).collect();
    let requests = if *request == "-" {
        Vec::new()
    } else {
        let request = numbers(request);
        let [
            intent_number,
            asset_in,
            asset_out,
            amount_in,
            min_amount_out,
            deadline,
        ] = request.as_slice()
        else {
            panic!("six request fields expected: {line}");
        };
        vec![(
            *intent_number,
            id(*asset_in),
            id(*asset_out),
            *amount_in,
            *min_amount_out,
            *deadline,
        )]
    };
    (
        (pre, input, ctx),
        Outcome {
            kind,
            reason,
            post: state_of(&post),
            requests,
        },
    )
}

#[test]
fn decision_examples_match_the_executed_application() {
    let authority = authority().unwrap();
    let mut covered = BTreeSet::new();
    let mut count = 0;
    for line in EXAMPLES
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let ((pre, input, ctx), expected) = parse_example(line);
        assert_eq!(
            observe(&authority, pre, input, ctx),
            expected,
            "example: {line}"
        );
        covered.insert((expected.kind, expected.reason));
        count += 1;
    }
    assert_eq!(count, 30);
    let mut expected = BTreeSet::from([("accept", None), ("failure", Some(212))]);
    expected.extend((200..=211).map(|reason| ("reject", Some(reason))));
    assert_eq!(covered, expected);
}

#[test]
fn schema_admission_enforces_the_declared_bounds() {
    let project = GeneratedProject::try_new::<RustCryptoSha256>().unwrap();
    let limits = ValidationLimits::default();
    let admits_state = |state: State| {
        project
            .admit_root::<RustCryptoSha256>(
                &agent_treasury_guard::treasury(
                    state.0,
                    state.1,
                    state.2,
                    state.3,
                    pending(state.4),
                    state.5,
                    state.6,
                ),
                limits,
            )
            .is_ok()
    };
    assert!(admits_state((
        BALANCE_BOUND,
        BALANCE_BOUND,
        BUDGET,
        LAST_TICK,
        PENDING_SELL,
        3,
        3
    )));
    for bad in [
        (BALANCE_BOUND + 1, 0, 0, 0, NO_SWAP, 0, 0),
        (0, BALANCE_BOUND + 1, 0, 0, NO_SWAP, 0, 0),
        (-1, 0, 0, 0, NO_SWAP, 0, 0),
        (0, 0, BUDGET + 1, 0, NO_SWAP, 0, 0),
        (0, 0, 0, LAST_TICK + 1, NO_SWAP, 0, 0),
        (0, 0, 0, 0, NO_SWAP, 4, 0),
        (0, 0, 0, 0, NO_SWAP, 0, 4),
    ] {
        assert!(!admits_state(bad), "state {bad:?} must be refused");
    }
    let admits_command = |input: Input| {
        project
            .admit_command::<RustCryptoSha256>(
                &command(
                    action(input.0),
                    direction(input.1),
                    input.2,
                    input.3,
                    input.4,
                    input.5,
                ),
                limits,
            )
            .is_ok()
    };
    assert!(admits_command((PROPOSE, SELL, 3, 3, LAST_TICK, 3)));
    for bad in [
        (PROPOSE, BUY, 0, 0, 0, 0),
        (PROPOSE, BUY, 4, 0, 0, 0),
        (PROPOSE, BUY, 1, 4, 0, 0),
        (SETTLED, BUY, 1, 0, LAST_TICK + 1, 0),
        (SETTLED, BUY, 1, 0, 0, 4),
    ] {
        assert!(!admits_command(bad), "command {bad:?} must be refused");
    }
    let admits_context = |ctx: Context| {
        project
            .admit_context::<RustCryptoSha256>(
                &context(caller(ctx.0), ctx.1, ctx.2, ctx.3, model_id(ctx.4)),
                limits,
            )
            .is_ok()
    };
    assert!(admits_context((AGENT, LAST_TICK, 2, 0, UNLISTED)));
    for bad in [
        (AGENT, LAST_TICK + 1, 1, 0, V2),
        (AGENT, 0, 0, 0, V2),
        (AGENT, 0, 3, 0, V2),
        (AGENT, 0, 1, -1, V2),
    ] {
        assert!(!admits_context(bad), "context {bad:?} must be refused");
    }
}

#[test]
fn only_the_reviewed_genesis_is_accepted() {
    let authority = authority().unwrap();
    let project = GeneratedProject::try_new::<RustCryptoSha256>().unwrap();
    let limits = ValidationLimits::default();
    assert_eq!(genesis_state(), state((6, 1, 0, 0, NO_SWAP, 0, 0)));
    for other in [
        (7, 1, 0, 0, NO_SWAP, 0, 0),
        (6, 0, 0, 0, NO_SWAP, 0, 0),
        (6, 1, 1, 0, NO_SWAP, 0, 0),
        (6, 1, 0, 1, NO_SWAP, 0, 0),
        (6, 1, 0, 0, PENDING_BUY, 1, 1),
    ] {
        let root = project
            .admit_root::<RustCryptoSha256>(&state(other), limits)
            .unwrap();
        assert!(authority.authorize_genesis(root).is_err(), "{other:?}");
    }
    let root = project
        .admit_root::<RustCryptoSha256>(&genesis_state(), limits)
        .unwrap();
    assert!(authority.authorize_genesis(root).is_ok());
}

/// The core's decision code for an action and the eleven facts, as the README
/// orders the rules: 0 accepts, 1 through 12 are the rejection reasons in
/// order, and 13 is the committed failure.
fn precedence(action_id: u16, facts: [bool; 11]) -> i64 {
    let [
        caller_ok,
        clock_ok,
        model_ok,
        price_fresh,
        swap_pending,
        intent_current,
        settlement_sufficient,
        within_cap,
        within_budget,
        slippage_ok,
        reserve_ok,
    ] = facts;
    if !caller_ok {
        return 1;
    }
    if !clock_ok {
        return 2;
    }
    if action_id == PROPOSE {
        return if !model_ok {
            3
        } else if !price_fresh {
            4
        } else if swap_pending {
            5
        } else if !within_cap {
            9
        } else if !within_budget {
            10
        } else if !slippage_ok {
            11
        } else if !reserve_ok {
            12
        } else {
            0
        };
    }
    if !swap_pending {
        6
    } else if !intent_current {
        7
    } else if action_id == SETTLED {
        i64::from(!settlement_sufficient) * 8
    } else {
        13
    }
}

#[test]
fn the_synthesized_core_decides_every_fact_tuple_as_the_readme_orders() {
    let mut checked = 0;
    for (code, action_id) in [(0, PROPOSE), (1, SETTLED), (2, FAILED)] {
        for bits in 0..(1u32 << 11) {
            let facts: [bool; 11] = std::array::from_fn(|index| bits & (1 << index) != 0);
            let mut inputs = [code; 12];
            for (slot, fact) in inputs[1..].iter_mut().zip(facts) {
                *slot = i64::from(fact);
            }
            assert_eq!(
                synthesized::transition(&inputs),
                Some([precedence(action_id, facts)]),
                "action {action_id} facts {facts:?}"
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 6_144);
    // Inputs outside the declared domain are refused, never decided.
    assert_eq!(
        synthesized::transition(&[3, 1, 1, 1, 1, 0, 0, 0, 1, 1, 1, 1]),
        None
    );
    assert_eq!(
        synthesized::transition(&[0, 2, 1, 1, 1, 0, 0, 0, 1, 1, 1, 1]),
        None
    );
    assert_eq!(synthesized::transition(&[0; 11]), None);
}

#[test]
fn fields_an_action_ignores_do_not_change_its_decision() {
    let authority = authority().unwrap();
    let idle = (6, 1, 0, 4, NO_SWAP, 0, 0);
    let pending_buy = (4, 1, 2, 5, PENDING_BUY, 2, 2);
    // A proposal ignores `intent` and `amount_out`.
    for pre in [idle, pending_buy] {
        let ctx = fresh(AGENT, 6, 1);
        let reference = observe(&authority, pre, (PROPOSE, BUY, 2, 2, 0, 0), ctx);
        for intent in [0, 5, LAST_TICK] {
            for amount_out in AMOUNT_OUTS {
                let input = (PROPOSE, BUY, 2, 2, intent, amount_out);
                assert_eq!(observe(&authority, pre, input, ctx), reference, "{input:?}");
            }
        }
    }
    // A settlement and a failure ignore `direction`, `amount`, and `min_out`.
    for pre in [idle, pending_buy] {
        let ctx = fresh(DEX, 6, 1);
        for (action_id, amount_out) in [(SETTLED, 3), (FAILED, 0)] {
            let reference = observe(&authority, pre, (action_id, BUY, 1, 0, 5, amount_out), ctx);
            for direction_id in [BUY, SELL] {
                for amount in AMOUNTS {
                    for min_out in MIN_OUTS {
                        let input = (action_id, direction_id, amount, min_out, 5, amount_out);
                        assert_eq!(observe(&authority, pre, input, ctx), reference, "{input:?}");
                    }
                }
            }
        }
    }
}

#[test]
fn the_least_minimum_keeps_three_quarters_of_the_oracle_value_and_one_less_does_not() {
    // For every direction, amount, and price, the adapter's least minimum is
    // the README's `div_ceil`; it keeps three quarters of the oracle value,
    // and one less does not, so law 507's bound is tight.
    for direction_id in [BUY, SELL] {
        for amount in AMOUNTS {
            for price in PRICES {
                let least = least_minimum(direction_id, amount, price);
                assert_eq!(
                    least_min_out(&direction(direction_id), amount, price),
                    Some(least)
                );
                let keeps = |min_out: i128| {
                    if direction_id == BUY {
                        4 * price * min_out >= 3 * amount
                    } else {
                        4 * min_out >= 3 * amount * price
                    }
                };
                assert!(
                    keeps(least) && !keeps(least - 1),
                    "direction {direction_id} amount {amount} price {price}"
                );
            }
        }
    }
    // Through the application, for every swap within the cap: from an idle
    // treasury with room, a proposal at the least minimum is accepted and
    // records it, and one below it is rejected as `slippage_too_wide`.
    let authority = authority().unwrap();
    let pre = (6, 3, 0, 4, NO_SWAP, 0, 0);
    let mut checked = 0;
    for direction_id in [BUY, SELL] {
        for amount in AMOUNTS {
            for price in PRICES {
                let value = if direction_id == BUY {
                    amount
                } else {
                    amount * price
                };
                if value > CAP {
                    continue;
                }
                let least = least_minimum(direction_id, amount, price);
                let ctx = fresh(AGENT, 5, price);
                let accepted = observe(
                    &authority,
                    pre,
                    (PROPOSE, direction_id, amount, least, 0, 0),
                    ctx,
                );
                assert_eq!((accepted.kind, accepted.post.6), ("accept", least));
                let short = observe(
                    &authority,
                    pre,
                    (PROPOSE, direction_id, amount, least - 1, 0, 0),
                    ctx,
                );
                assert_eq!((short.kind, short.reason), ("reject", Some(210)));
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 10);
}
