//! Checks the executed application against a reference model of the README's
//! rules on every decision reachable in a grid, and against the decision
//! examples.
//!
//! Every decision runs through the application: schema admission, the
//! authority, the program in `src/program.rs`, the law checker, the committed
//! patch, and the outbox plan. Fields are read by the numeric IDs in
//! `project.zeno`, not through the generated name bindings, so a binding that
//! swapped two fields or variants fails here.
//!
//! The reference model restates the README's rules in words, including the
//! controller's choices, without reading the synthesized step or the tables
//! in `src/controller.rs`. Its author also wrote the program, so it catches
//! binding, adapter, and table errors, not a misreading shared by both. The
//! examples file, once reviewed by the project's owner, is the check on that.
//!
//! The grid is the set of states reachable from genesis under deposits of
//! each amount, requests of each lane and amount, and ticks with and without
//! the alarm, each from its caller. The same exploration gives the graph on
//! which the response bound is checked.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;
use withdrawal_queue::{
    Authority, authority, bindings::GeneratedProject, command, generated::*, profile,
};
use zeno_fcis_codec::Domain;
use zeno_fcis_core::Decision;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_schema::ValidationLimits;
use zeno_fcis_value::Value;

const EXAMPLES: &str = include_str!("decision-examples.txt");
const DEPOSIT: u16 = 160;
const REQUEST: u16 = 161;
const TICK: u16 = 162;
const LANE_A: u16 = 170;
const LANE_B: u16 = 171;
const EMPTY: u16 = 180;
const ARRIVED: u16 = 181;
const PENDING: u16 = 182;
const OPERATOR: u16 = 190;
const OWNER_A: u16 = 191;
const OWNER_B: u16 = 192;
const KEEPER: u16 = 193;
const CALLERS: [u16; 4] = [OPERATOR, OWNER_A, OWNER_B, KEEPER];
const MAX_BALANCE: i128 = 4;
const MAX_AMOUNT: i128 = 2;
const PAUSE_TICKS: i128 = 2;
/// The checked response bound: ticks from the one that presents a lane to
/// the one that pays it, inclusive.
const RESPONSE_BOUND: usize = 8;

/// The vault, fields 120 through 127, with variant IDs for the lanes and the
/// priority.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct State {
    balance: i128,
    lane_a: u16,
    amount_a: i128,
    lane_b: u16,
    amount_b: i128,
    pause: i128,
    must_serve: bool,
    priority: u16,
}

/// Command fields 130 action, 131 lane, and 132 amount.
type Input = (u16, u16, i128);

/// One decision, in the numeric IDs of `project.zeno`.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Outcome {
    /// `accept` or `reject`.
    kind: &'static str,
    /// The rejection reason.
    reason: Option<u32>,
    /// The vault after the decision.
    post: State,
    /// Each queued payout: channel, paid lane (field 150), and amount (151).
    payouts: Vec<(u32, u16, i128)>,
}

const GENESIS: State = State {
    balance: 0,
    lane_a: EMPTY,
    amount_a: 0,
    lane_b: EMPTY,
    amount_b: 0,
    pause: 0,
    must_serve: false,
    priority: LANE_A,
};

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

fn flag(value: &Value, id: u16) -> bool {
    match record_field(value, id) {
        Value::Bool(value) => *value,
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

fn state_of(value: &Value) -> State {
    State {
        balance: int(value, 120),
        lane_a: variant(record_field(value, 121)),
        amount_a: int(value, 122),
        lane_b: variant(record_field(value, 123)),
        amount_b: int(value, 124),
        pause: int(value, 125),
        must_serve: flag(value, 126),
        priority: variant(record_field(value, 127)),
    }
}

fn action(id: u16) -> VaultAction {
    match id {
        DEPOSIT => VaultAction::Deposit,
        REQUEST => VaultAction::RequestWithdrawal,
        TICK => VaultAction::Tick,
        other => panic!("unknown action variant {other}"),
    }
}

fn lane(id: u16) -> Lane {
    match id {
        LANE_A => Lane::A,
        LANE_B => Lane::B,
        other => panic!("unknown lane variant {other}"),
    }
}

fn status(id: u16) -> LaneStatus {
    match id {
        EMPTY => LaneStatus::Empty,
        ARRIVED => LaneStatus::Arrived,
        PENDING => LaneStatus::Pending,
        other => panic!("unknown lane status variant {other}"),
    }
}

fn caller(id: u16) -> Caller {
    match id {
        OPERATOR => Caller::Operator,
        OWNER_A => Caller::OwnerA,
        OWNER_B => Caller::OwnerB,
        KEEPER => Caller::Keeper,
        other => panic!("unknown caller variant {other}"),
    }
}

fn vault(state: State) -> Vault {
    Vault {
        balance: Money(state.balance),
        lane_a: status(state.lane_a),
        amount_a: Money(state.amount_a),
        lane_b: status(state.lane_b),
        amount_b: Money(state.amount_b),
        pause: PauseTicks(state.pause),
        must_serve: Flag(state.must_serve),
        priority: lane(state.priority),
    }
}

/// Runs one command through the application from an admitted pre-state.
fn observe(
    authority: &Authority,
    pre: State,
    (action_id, lane_id, amount): Input,
    caller_id: u16,
    alarm: bool,
) -> Outcome {
    let project = GeneratedProject::try_new::<RustCryptoSha256>().unwrap();
    let limits = ValidationLimits::default();
    let root = project
        .admit_root::<RustCryptoSha256>(&vault(pre), limits)
        .unwrap();
    let pre_value = root.value().value().clone();
    // The typed bindings must place each field and variant at its numeric ID.
    assert_eq!(state_of(&pre_value), pre);
    let admitted_command = project
        .admit_command::<RustCryptoSha256>(
            &command(action(action_id), lane(lane_id), amount),
            limits,
        )
        .unwrap();
    let command_value = admitted_command.admitted().value().value();
    assert_eq!(variant(record_field(command_value, 130)), action_id);
    assert_eq!(variant(record_field(command_value, 131)), lane_id);
    assert_eq!(int(command_value, 132), amount);
    let admitted_context = project
        .admit_context::<RustCryptoSha256>(
            &TickContext {
                caller: caller(caller_id),
                alarm: Flag(alarm),
            },
            limits,
        )
        .unwrap();
    let context_value = admitted_context.admitted().value().value();
    assert_eq!(variant(record_field(context_value, 140)), caller_id);
    assert_eq!(flag(context_value, 141), alarm);
    let replay = format!("conformance {pre:?} {action_id} {lane_id} {amount} {caller_id} {alarm}");
    let witness = authority
        .admit_invocation(
            root,
            admitted_command.admitted().clone(),
            admitted_context.admitted().clone(),
            profile::digest("example/withdrawal-queue/principal", b"conformance").unwrap(),
            profile::digest("example/withdrawal-queue/authentication", b"conformance").unwrap(),
            profile::digest("example/withdrawal-queue/replay", replay.as_bytes()).unwrap(),
        )
        .unwrap();
    let domain = Domain::new("example/withdrawal-queue/state", 1).unwrap();
    match authority.execute(witness).unwrap() {
        Decision::Reject(reject) => Outcome {
            kind: "reject",
            reason: Some(reject.reason().rejection().reason_id().get()),
            post: pre,
            payouts: Vec::new(),
        },
        Decision::Accept(accepted) => {
            let candidate = accepted.into_candidate();
            let bundle = candidate.bundle();
            assert!(bundle.commit_plan().effects().is_empty());
            let applied = bundle
                .patch()
                .apply::<RustCryptoSha256>(&pre_value, domain)
                .unwrap();
            Outcome {
                kind: "accept",
                reason: None,
                post: state_of(applied.state()),
                payouts: bundle
                    .outbox_plan()
                    .entries()
                    .iter()
                    .map(|entry| {
                        (
                            entry.channel(),
                            variant(record_field(entry.payload(), 150)),
                            int(entry.payload(), 151),
                        )
                    })
                    .collect(),
            }
        }
        Decision::CommittedFailure(_) => panic!("this application never commits a failure"),
    }
}

/// The README's rules, restated independently of `src/program.rs`, the
/// synthesized step, and the controller tables.
fn model(pre: State, (action_id, lane_id, amount): Input, caller_id: u16, alarm: bool) -> Outcome {
    let reject = |reason| Outcome {
        kind: "reject",
        reason: Some(reason),
        post: pre,
        payouts: Vec::new(),
    };
    let accept = |post, payouts| Outcome {
        kind: "accept",
        reason: None,
        post,
        payouts,
    };
    let sender = match action_id {
        DEPOSIT => OPERATOR,
        REQUEST if lane_id == LANE_A => OWNER_A,
        REQUEST => OWNER_B,
        _ => KEEPER,
    };
    if caller_id != sender {
        return reject(200);
    }
    let mut post = pre;
    match action_id {
        DEPOSIT => {
            if pre.balance + amount > MAX_BALANCE {
                return reject(203);
            }
            post.balance += amount;
            accept(post, Vec::new())
        }
        REQUEST => {
            let occupied = if lane_id == LANE_A {
                pre.lane_a
            } else {
                pre.lane_b
            } != EMPTY;
            if occupied {
                return reject(201);
            }
            if amount > pre.balance - pre.amount_a - pre.amount_b {
                return reject(202);
            }
            if lane_id == LANE_A {
                (post.lane_a, post.amount_a) = (ARRIVED, amount);
            } else {
                (post.lane_b, post.amount_b) = (ARRIVED, amount);
            }
            accept(post, Vec::new())
        }
        _ => {
            // A lane is due when it is pending or arriving.
            let (due_a, due_b) = (pre.lane_a != EMPTY, pre.lane_b != EMPTY);
            let mut payouts = Vec::new();
            if pre.pause > 0 {
                // Paused: nothing is paid; when the pause ends with a lane
                // due, the vault must serve before honoring another alarm.
                post.pause -= 1;
                post.must_serve = post.pause == 0 && (due_a || due_b);
            } else if alarm && !pre.must_serve {
                // An honored alarm pays nothing and pauses the next ticks.
                post.pause = PAUSE_TICKS;
            } else {
                // The priority lane when both are due, else the due lane.
                let paid = if due_a && due_b {
                    Some(pre.priority)
                } else if due_a {
                    Some(LANE_A)
                } else if due_b {
                    Some(LANE_B)
                } else {
                    None
                };
                // Must-serve ends with a payout, or when nothing is due.
                post.must_serve = false;
                if let Some(paid) = paid {
                    let amount = if paid == LANE_A {
                        pre.amount_a
                    } else {
                        pre.amount_b
                    };
                    payouts.push((300, paid, amount));
                    post.balance -= amount;
                    if paid == LANE_A {
                        (post.lane_a, post.amount_a, post.priority) = (EMPTY, 0, LANE_B);
                    } else {
                        (post.lane_b, post.amount_b, post.priority) = (EMPTY, 0, LANE_A);
                    }
                }
            }
            // Every arrived lane has now been presented.
            if post.lane_a == ARRIVED {
                post.lane_a = PENDING;
            }
            if post.lane_b == ARRIVED {
                post.lane_b = PENDING;
            }
            accept(post, payouts)
        }
    }
}

/// The commands the grid explores, each with its caller and alarm.
fn grid_commands() -> Vec<(Input, u16, bool)> {
    let mut commands = Vec::new();
    for amount in 1..=MAX_AMOUNT {
        commands.push(((DEPOSIT, LANE_A, amount), OPERATOR, false));
    }
    for (lane_id, owner) in [(LANE_A, OWNER_A), (LANE_B, OWNER_B)] {
        for amount in 1..=MAX_AMOUNT {
            commands.push(((REQUEST, lane_id, amount), owner, false));
        }
    }
    for alarm in [false, true] {
        commands.push(((TICK, LANE_A, 1), KEEPER, alarm));
    }
    commands
}

/// Every state reachable from genesis under the grid's commands, with the
/// outcome of each command from each state.
type Graph = BTreeMap<State, Vec<((Input, u16, bool), Outcome)>>;

fn explored() -> &'static (Graph, usize) {
    static EXPLORED: OnceLock<(Graph, usize)> = OnceLock::new();
    EXPLORED.get_or_init(|| {
        let authority = authority().unwrap();
        let commands = grid_commands();
        let mut graph = Graph::new();
        let mut pending = vec![GENESIS];
        let mut decisions = 0;
        while let Some(state) = pending.pop() {
            if graph.contains_key(&state) {
                continue;
            }
            let mut outcomes = Vec::new();
            for &(input, caller_id, alarm) in &commands {
                let outcome = observe(&authority, state, input, caller_id, alarm);
                decisions += 1;
                assert_eq!(
                    outcome,
                    model(state, input, caller_id, alarm),
                    "state {state:?} command {input:?} caller {caller_id} alarm {alarm}"
                );
                if outcome.kind == "accept" {
                    pending.push(outcome.post);
                }
                outcomes.push(((input, caller_id, alarm), outcome));
            }
            graph.insert(state, outcomes);
        }
        (graph, decisions)
    })
}

#[test]
fn every_reachable_decision_in_the_grid_matches_the_reference_model() {
    let (graph, decisions) = explored();
    assert_eq!(graph.len(), 420);
    assert_eq!(*decisions, 3360);
    // Every reachable vault keeps the balance and the lanes' amounts within
    // law 500; the must-serve part of the law is checked below.
    for state in graph.keys() {
        assert!(state.balance <= MAX_BALANCE);
        assert!(state.balance >= state.amount_a + state.amount_b);
        assert_eq!(state.lane_a == EMPTY, state.amount_a == 0);
        assert_eq!(state.lane_b == EMPTY, state.amount_b == 0);
    }
}

#[test]
fn must_serve_is_reached_only_with_a_pending_lane_and_every_tick_from_it_pays() {
    let (graph, _) = explored();
    let mut vaults = 0;
    for (state, outcomes) in graph {
        if !state.must_serve {
            continue;
        }
        vaults += 1;
        // Only the tick that ends a pause with a lane due sets must-serve,
        // that lane is pending after the tick, and only a payout empties it.
        assert_eq!(state.pause, 0, "{state:?}");
        assert!(
            state.lane_a == PENDING || state.lane_b == PENDING,
            "{state:?}"
        );
        // So the paying branch never runs in must-serve with nothing due:
        // every tick from such a vault, with or without the alarm, pays one
        // lane and ends must-serve.
        for (((action_id, _, _), _, _), outcome) in outcomes {
            if *action_id == TICK {
                assert_eq!(outcome.payouts.len(), 1, "{state:?} {outcome:?}");
                assert!(!outcome.post.must_serve, "{state:?} {outcome:?}");
            }
        }
    }
    // One lane pending and the other empty (14 each way), both pending (16),
    // or one pending and the other arrived (16 each way), over every amount,
    // every covering balance, and both priorities.
    assert_eq!(vaults, 76);
}

#[test]
fn wrong_callers_and_ignored_fields_match_the_reference_model() {
    let (graph, _) = explored();
    let authority = authority().unwrap();
    let mut checked = 0;
    for state in graph.keys().step_by(42) {
        for input in [
            (DEPOSIT, LANE_A, 1),
            (REQUEST, LANE_B, 1),
            (TICK, LANE_A, 1),
        ] {
            for caller_id in CALLERS {
                let outcome = observe(&authority, *state, input, caller_id, true);
                assert_eq!(outcome, model(*state, input, caller_id, true));
                checked += 1;
            }
        }
        // A tick ignores the lane and amount; a deposit ignores the lane.
        for (input, caller_id) in [
            ((TICK, LANE_B, 2), KEEPER),
            ((DEPOSIT, LANE_B, 2), OPERATOR),
        ] {
            let outcome = observe(&authority, *state, input, caller_id, false);
            assert_eq!(outcome, model(*state, input, caller_id, false));
            checked += 1;
        }
    }
    assert_eq!(checked, 140);
}

/// The longest wait of a lane that is arrived or pending in `start`, in
/// ticks, over every path in the grid; `None` when some path never pays it.
fn longest_wait(graph: &Graph, start: State, lane_id: u16) -> Option<usize> {
    fn wait(
        graph: &Graph,
        state: State,
        lane_id: u16,
        memo: &mut BTreeMap<State, usize>,
        visiting: &mut Vec<State>,
    ) -> Option<usize> {
        let occupied = if lane_id == LANE_A {
            state.lane_a
        } else {
            state.lane_b
        } != EMPTY;
        if !occupied {
            return Some(0);
        }
        if let Some(&known) = memo.get(&state) {
            return Some(known);
        }
        if visiting.contains(&state) {
            return None;
        }
        visiting.push(state);
        let mut longest = 0;
        for (((action_id, _, _), _, _), outcome) in &graph[&state] {
            if outcome.kind != "accept" {
                continue;
            }
            let ticks = usize::from(*action_id == TICK);
            let paid = outcome.payouts.iter().any(|(_, paid, _)| *paid == lane_id);
            let rest = if paid {
                0
            } else {
                wait(graph, outcome.post, lane_id, memo, visiting)?
            };
            longest = longest.max(ticks + rest);
        }
        visiting.pop();
        memo.insert(state, longest);
        Some(longest)
    }
    wait(graph, start, lane_id, &mut BTreeMap::new(), &mut Vec::new())
}

#[test]
fn every_requested_withdrawal_is_paid_within_the_response_bound() {
    let (graph, _) = explored();
    let status_of = |state: &State, lane_id: u16| {
        if lane_id == LANE_A {
            state.lane_a
        } else {
            state.lane_b
        }
    };
    for lane_id in [LANE_A, LANE_B] {
        let mut longest = BTreeMap::from([(ARRIVED, 0), (PENDING, 0)]);
        for state in graph.keys() {
            if status_of(state, lane_id) == EMPTY {
                continue;
            }
            let wait = longest_wait(graph, *state, lane_id)
                .unwrap_or_else(|| panic!("lane {lane_id} can wait forever from {state:?}"));
            let worst = longest.get_mut(&status_of(state, lane_id)).unwrap();
            *worst = (*worst).max(wait);
        }
        // A recorded request is presented at the next tick and paid within
        // the bound; once presented, one tick of the bound has passed.
        assert_eq!(longest[&ARRIVED], RESPONSE_BOUND, "lane {lane_id}");
        assert_eq!(longest[&PENDING], RESPONSE_BOUND - 1, "lane {lane_id}");
    }
}

/// Parses `pre.120 .. pre.127 action lane amount caller alarm | outcome
/// reason post.120 .. post.127 | payout`, where the payout is `-` or
/// `300 lane amount`, and `must_serve` and `alarm` are 0 or 1.
fn parse_example(line: &str) -> ((State, Input, u16, bool), Outcome) {
    let parts: Vec<&str> = line.split('|').map(str::trim).collect();
    let [input, decision, payout] = parts.as_slice() else {
        panic!("three columns expected: {line}");
    };
    let numbers = |text: &str| -> Vec<i128> {
        text.split_whitespace()
            .map(|word| word.parse().unwrap())
            .collect()
    };
    let id = |raw: i128| u16::try_from(raw).unwrap();
    let state = |raw: &[i128]| State {
        balance: raw[0],
        lane_a: id(raw[1]),
        amount_a: raw[2],
        lane_b: id(raw[3]),
        amount_b: raw[4],
        pause: raw[5],
        must_serve: match raw[6] {
            0 => false,
            1 => true,
            other => panic!("must_serve must be 0 or 1, got {other}"),
        },
        priority: id(raw[7]),
    };
    let input = numbers(input);
    let [pre @ .., action_id, lane_id, amount, caller_id, alarm] = input.as_slice() else {
        panic!("thirteen inputs expected: {line}");
    };
    assert_eq!(pre.len(), 8, "eight pre-state fields expected: {line}");
    let decision: Vec<&str> = decision.split_whitespace().collect();
    let [kind, reason, post @ ..] = decision.as_slice() else {
        panic!("decision fields expected: {line}");
    };
    let kind = match *kind {
        "accept" => "accept",
        "reject" => "reject",
        other => panic!("unknown outcome {other}"),
    };
    let reason = (*reason != "-").then(|| reason.parse().unwrap());
    let post: Vec<i128> = post.iter().map(|word| word.parse().unwrap()).collect();
    assert_eq!(post.len(), 8, "eight post-state fields expected: {line}");
    let payouts = if *payout == "-" {
        Vec::new()
    } else {
        match numbers(payout).as_slice() {
            [300, paid, amount] => vec![(300, id(*paid), *amount)],
            other => panic!("unknown payout {other:?}: {line}"),
        }
    };
    (
        (
            state(pre),
            (id(*action_id), id(*lane_id), *amount),
            id(*caller_id),
            match alarm {
                0 => false,
                1 => true,
                other => panic!("alarm must be 0 or 1, got {other}"),
            },
        ),
        Outcome {
            kind,
            reason,
            post: state(&post),
            payouts,
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
        let ((pre, input, caller_id, alarm), expected) = parse_example(line);
        assert_eq!(
            observe(&authority, pre, input, caller_id, alarm),
            expected,
            "example: {line}"
        );
        covered.insert((expected.kind, expected.reason));
        count += 1;
    }
    assert_eq!(count, 26);
    assert_eq!(
        covered,
        BTreeSet::from([
            ("accept", None),
            ("reject", Some(200)),
            ("reject", Some(201)),
            ("reject", Some(202)),
            ("reject", Some(203)),
        ])
    );
}

#[test]
fn schema_admission_matches_the_finite_domain() {
    let project = GeneratedProject::try_new::<RustCryptoSha256>().unwrap();
    let limits = ValidationLimits::default();
    for balance in -1..=MAX_BALANCE + 1 {
        for pause in -1..=PAUSE_TICKS + 1 {
            let state = State {
                balance,
                pause,
                ..GENESIS
            };
            let admitted = project
                .admit_root::<RustCryptoSha256>(&vault(state), limits)
                .is_ok();
            let in_domain =
                (0..=MAX_BALANCE).contains(&balance) && (0..=PAUSE_TICKS).contains(&pause);
            assert_eq!(admitted, in_domain, "balance {balance} pause {pause}");
        }
    }
    for amount_a in [-1, MAX_BALANCE + 1] {
        let state = State {
            balance: MAX_BALANCE,
            lane_a: ARRIVED,
            amount_a,
            ..GENESIS
        };
        assert!(
            project
                .admit_root::<RustCryptoSha256>(&vault(state), limits)
                .is_err()
        );
    }
    for amount in -1..=MAX_AMOUNT + 1 {
        let admitted = project
            .admit_command::<RustCryptoSha256>(
                &command(VaultAction::Deposit, Lane::A, amount),
                limits,
            )
            .is_ok();
        assert_eq!(
            admitted,
            (1..=MAX_AMOUNT).contains(&amount),
            "amount {amount}"
        );
    }
}

#[test]
fn only_the_empty_vault_is_a_valid_genesis() {
    let authority = authority().unwrap();
    let project = GeneratedProject::try_new::<RustCryptoSha256>().unwrap();
    let limits = ValidationLimits::default();
    let candidates = [
        (GENESIS, true),
        (
            State {
                balance: 1,
                ..GENESIS
            },
            false,
        ),
        (
            State {
                priority: LANE_B,
                ..GENESIS
            },
            false,
        ),
        (
            State {
                pause: 1,
                ..GENESIS
            },
            false,
        ),
        (
            State {
                lane_a: ARRIVED,
                amount_a: 1,
                balance: 1,
                ..GENESIS
            },
            false,
        ),
    ];
    for (state, valid) in candidates {
        let root = project
            .admit_root::<RustCryptoSha256>(&vault(state), limits)
            .unwrap();
        assert_eq!(
            authority.authorize_genesis(root).is_ok(),
            valid,
            "{state:?}"
        );
    }
}
