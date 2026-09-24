//! Re-checks the shipped controller with an independent implementation of
//! OrbitSynthesis's decision procedure, and requires the Rust tables in
//! `src/controller.rs`, the synthesized step, and the shipped JSON files to
//! agree.
//!
//! The checker here is written from the contract's specification
//! (`research/prototypes/2026-09-12-strategy-checker/CONTRACT.md` in the
//! OrbitSynthesis repository), not copied from its Python: decode the
//! canonical JSON, pin the contract, enumerate the reachable product of plant
//! and memory states over every input, reject a forbidden or unsafe selected
//! transition, and for each recurrence goal reject a reachable cycle that
//! avoids it. It accepts `controller/strategy.json` and rejects both
//! strategies under `controller/rejected/` with a counterexample cycle.
//!
//! It also restates the vault's rules and the controller's choice from
//! README.md and requires the contract and strategy tables to equal those
//! restatements on every entry; requires the synthesized step that the
//! program runs to equal the certified strategy on every memory and input;
//! and computes the response bound: how many ticks a due lane can wait
//! before it is paid, over every input sequence.

use std::collections::{BTreeMap, BTreeSet};
use withdrawal_queue::{controller, synthesized};
use zeno_fcis_codec::CommitmentHasher;
use zeno_fcis_crypto::RustCryptoSha256;

const CONTRACT: &[u8] = include_bytes!("../controller/contract.json");
const PIN: &str = include_str!("../controller/contract.sha256");
const STRATEGY: &[u8] = include_bytes!("../controller/strategy.json");
const FIXED_PRIORITY: &[u8] = include_bytes!("../controller/rejected/fixed-priority.strategy.json");
const ALARM_DEFERENTIAL: &[u8] =
    include_bytes!("../controller/rejected/alarm-deferential.strategy.json");
const CONTRACT_SCHEMA: &str = "orbitsynthesis/finite-controller-contract/v1";
const STRATEGY_SCHEMA: &str = "orbitsynthesis/finite-mealy-strategy/v1";
const PAUSE_TICKS: u8 = 2;

/// A canonical JSON value: integers, ASCII strings, arrays, and objects with
/// strictly ascending keys, written without whitespace.
#[derive(Clone, Debug, PartialEq)]
enum Json {
    Int(i64),
    Str(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn peek(&self) -> u8 {
        *self.bytes.get(self.at).expect("unexpected end of JSON")
    }

    fn take(&mut self, expected: u8) {
        assert_eq!(self.peek(), expected, "byte {} of the JSON", self.at);
        self.at += 1;
    }

    fn value(&mut self) -> Json {
        match self.peek() {
            b'{' => {
                self.take(b'{');
                let mut fields: Vec<(String, Json)> = Vec::new();
                if self.peek() == b'}' {
                    self.take(b'}');
                    return Json::Object(fields);
                }
                loop {
                    let key = self.string();
                    if let Some((last, _)) = fields.last() {
                        assert!(*last < key, "object keys must be sorted and distinct");
                    }
                    self.take(b':');
                    let value = self.value();
                    fields.push((key, value));
                    if self.peek() == b',' {
                        self.take(b',');
                    } else {
                        self.take(b'}');
                        return Json::Object(fields);
                    }
                }
            }
            b'[' => {
                self.take(b'[');
                let mut items = Vec::new();
                if self.peek() == b']' {
                    self.take(b']');
                    return Json::Array(items);
                }
                loop {
                    items.push(self.value());
                    if self.peek() == b',' {
                        self.take(b',');
                    } else {
                        self.take(b']');
                        return Json::Array(items);
                    }
                }
            }
            b'"' => Json::Str(self.string()),
            b'-' | b'0'..=b'9' => {
                let start = self.at;
                if self.peek() == b'-' {
                    self.at += 1;
                }
                let first = self.peek();
                assert!(first.is_ascii_digit(), "integer expected");
                self.at += 1;
                while self.at < self.bytes.len() && self.bytes[self.at].is_ascii_digit() {
                    self.at += 1;
                }
                let text = std::str::from_utf8(&self.bytes[start..self.at]).unwrap();
                assert!(
                    first != b'0' || text.trim_start_matches('-').len() == 1,
                    "integers have no leading zeros"
                );
                Json::Int(text.parse().expect("integer"))
            }
            other => panic!("unexpected byte {other:?} at {}", self.at),
        }
    }

    fn string(&mut self) -> String {
        self.take(b'"');
        let start = self.at;
        while self.peek() != b'"' {
            let byte = self.peek();
            assert!(byte.is_ascii() && byte != b'\\', "plain ASCII strings only");
            self.at += 1;
        }
        let text = std::str::from_utf8(&self.bytes[start..self.at])
            .unwrap()
            .to_owned();
        self.take(b'"');
        text
    }
}

/// Reads exactly one canonical value: no whitespace, no trailing bytes.
fn read(bytes: &[u8]) -> Json {
    let mut reader = Reader { bytes, at: 0 };
    let value = reader.value();
    assert_eq!(
        reader.at,
        bytes.len(),
        "trailing bytes after the JSON value"
    );
    value
}

fn field<'a>(json: &'a Json, key: &str) -> &'a Json {
    let Json::Object(fields) = json else {
        panic!("object expected");
    };
    &fields
        .iter()
        .find(|(name, _)| name == key)
        .unwrap_or_else(|| panic!("field {key} missing"))
        .1
}

fn keys(json: &Json) -> Vec<&str> {
    let Json::Object(fields) = json else {
        panic!("object expected");
    };
    fields.iter().map(|(name, _)| name.as_str()).collect()
}

fn int(json: &Json) -> i64 {
    match json {
        Json::Int(value) => *value,
        other => panic!("integer expected, got {other:?}"),
    }
}

fn text(json: &Json) -> &str {
    match json {
        Json::Str(value) => value,
        other => panic!("string expected, got {other:?}"),
    }
}

fn items(json: &Json) -> &[Json] {
    match json {
        Json::Array(items) => items,
        other => panic!("array expected, got {other:?}"),
    }
}

#[derive(Clone, Debug)]
struct Contract {
    states: i64,
    inputs: i64,
    outputs: i64,
    initial: i64,
    safe: BTreeSet<i64>,
    transitions: Vec<i64>,
    recurrence: Vec<BTreeSet<i64>>,
}

#[derive(Clone, Debug)]
struct Strategy {
    contract_sha256: String,
    memory_states: i64,
    initial_memory: i64,
    rows: Vec<(i64, i64)>,
}

fn state_set(json: &Json, states: i64) -> BTreeSet<i64> {
    let listed: Vec<i64> = items(json).iter().map(int).collect();
    let set: BTreeSet<i64> = listed.iter().copied().collect();
    assert_eq!(
        listed,
        set.iter().copied().collect::<Vec<_>>(),
        "state sets are sorted and duplicate-free"
    );
    assert!(set.iter().all(|state| (0..states).contains(state)));
    set
}

fn contract(bytes: &[u8]) -> Contract {
    let json = read(bytes);
    assert_eq!(
        keys(&json),
        [
            "initial",
            "inputs",
            "outputs",
            "recurrence",
            "safe_states",
            "schema",
            "states",
            "transitions"
        ]
    );
    assert_eq!(text(field(&json, "schema")), CONTRACT_SCHEMA);
    let states = int(field(&json, "states"));
    let inputs = int(field(&json, "inputs"));
    let outputs = int(field(&json, "outputs"));
    assert!(
        (1..=64).contains(&states) && (1..=16).contains(&inputs) && (1..=16).contains(&outputs)
    );
    let initial = int(field(&json, "initial"));
    assert!((0..states).contains(&initial));
    let transitions: Vec<i64> = items(field(&json, "transitions")).iter().map(int).collect();
    assert_eq!(transitions.len() as i64, states * inputs * outputs);
    assert!(transitions.iter().all(|next| (-1..states).contains(next)));
    let recurrence: Vec<BTreeSet<i64>> = items(field(&json, "recurrence"))
        .iter()
        .map(|goal| state_set(goal, states))
        .collect();
    assert!(recurrence.len() <= 16);
    Contract {
        states,
        inputs,
        outputs,
        initial,
        safe: state_set(field(&json, "safe_states"), states),
        transitions,
        recurrence,
    }
}

fn strategy(bytes: &[u8], contract: &Contract) -> Strategy {
    let json = read(bytes);
    assert_eq!(
        keys(&json),
        [
            "contract_sha256",
            "initial_memory",
            "memory_states",
            "rows",
            "schema"
        ]
    );
    assert_eq!(text(field(&json, "schema")), STRATEGY_SCHEMA);
    let memory_states = int(field(&json, "memory_states"));
    assert!((1..=64).contains(&memory_states));
    let initial_memory = int(field(&json, "initial_memory"));
    assert!((0..memory_states).contains(&initial_memory));
    let rows: Vec<(i64, i64)> = items(field(&json, "rows"))
        .iter()
        .map(|row| {
            let [output, next] = items(row) else {
                panic!("a row is [output, next_memory]");
            };
            let (output, next) = (int(output), int(next));
            assert!((0..contract.outputs).contains(&output));
            assert!((0..memory_states).contains(&next));
            (output, next)
        })
        .collect();
    assert_eq!(rows.len() as i64, memory_states * contract.inputs);
    Strategy {
        contract_sha256: text(field(&json, "contract_sha256")).to_owned(),
        memory_states,
        initial_memory,
        rows,
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    RustCryptoSha256::hash(bytes)
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// One product edge: a tick from a plant and memory state on one input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Step {
    plant: i64,
    memory: i64,
    input: i64,
    output: i64,
    next_plant: i64,
    next_memory: i64,
}

type Vertex = (i64, i64);
type Graph = BTreeMap<Vertex, Vec<Step>>;

#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    Accepted { states: usize, edges: usize },
    ContractPinMismatch,
    StrategyPinMismatch,
    Unsafe(Step),
    Starved { goal: usize, cycle: Vec<Step> },
}

/// Every reachable plant and memory pair with its edge for each input, or the
/// first selected transition the contract forbids.
fn reachable(contract: &Contract, strategy: &Strategy) -> Result<Graph, Step> {
    let root = (contract.initial, strategy.initial_memory);
    let mut graph = Graph::new();
    let mut pending = vec![root];
    while let Some(vertex) = pending.pop() {
        if graph.contains_key(&vertex) {
            continue;
        }
        let (plant, memory) = vertex;
        let mut edges = Vec::new();
        for input in 0..contract.inputs {
            let (output, next_memory) = strategy.rows[(memory * contract.inputs + input) as usize];
            let index = (plant * contract.inputs + input) * contract.outputs + output;
            let next_plant = contract.transitions[index as usize];
            let step = Step {
                plant,
                memory,
                input,
                output,
                next_plant,
                next_memory,
            };
            if next_plant < 0 || !contract.safe.contains(&next_plant) {
                return Err(step);
            }
            edges.push(step);
            pending.push((next_plant, next_memory));
        }
        graph.insert(vertex, edges);
    }
    Ok(graph)
}

/// A cycle in the reachable graph that never enters `goal`, if one exists.
fn cycle_avoiding(graph: &Graph, goal: &BTreeSet<i64>) -> Option<Vec<Step>> {
    // 1 marks a vertex on the current path, 2 a finished vertex.
    let mut color: BTreeMap<Vertex, u8> = BTreeMap::new();
    for &root in graph.keys() {
        if goal.contains(&root.0) || color.contains_key(&root) {
            continue;
        }
        color.insert(root, 1);
        // The path's vertices with the next edge to try, and the edges between them.
        let mut stack = vec![(root, 0usize)];
        let mut path: Vec<Step> = Vec::new();
        while let Some(&(vertex, next)) = stack.last() {
            let edges = &graph[&vertex];
            if next == edges.len() {
                color.insert(vertex, 2);
                stack.pop();
                if !stack.is_empty() {
                    path.pop();
                }
                continue;
            }
            stack.last_mut().unwrap().1 += 1;
            let step = edges[next];
            let target = (step.next_plant, step.next_memory);
            if goal.contains(&target.0) {
                continue;
            }
            match color.get(&target) {
                Some(1) => {
                    let start = stack.iter().position(|(v, _)| *v == target).unwrap();
                    let mut cycle = path[start..].to_vec();
                    cycle.push(step);
                    return Some(cycle);
                }
                Some(_) => {}
                None => {
                    color.insert(target, 1);
                    path.push(step);
                    stack.push((target, 0));
                }
            }
        }
    }
    None
}

/// OrbitSynthesis's decision: `G safe AND (AND_j GF goal_j)` for every input
/// sequence, with the contract pinned separately from the strategy.
fn verify(contract_bytes: &[u8], strategy_bytes: &[u8], pin: &str) -> Verdict {
    if sha256_hex(contract_bytes) != pin {
        return Verdict::ContractPinMismatch;
    }
    let contract = contract(contract_bytes);
    let strategy = strategy(strategy_bytes, &contract);
    if strategy.contract_sha256 != pin {
        return Verdict::StrategyPinMismatch;
    }
    assert!(
        contract.safe.contains(&contract.initial),
        "unsafe initial state"
    );
    let graph = match reachable(&contract, &strategy) {
        Ok(graph) => graph,
        Err(step) => return Verdict::Unsafe(step),
    };
    for (goal, states) in contract.recurrence.iter().enumerate() {
        if let Some(cycle) = cycle_avoiding(&graph, states) {
            return Verdict::Starved { goal, cycle };
        }
    }
    Verdict::Accepted {
        states: graph.len(),
        edges: graph.values().map(Vec::len).sum(),
    }
}

/// The longest wait of a due lane, in ticks, over every input sequence: the
/// tick that presents the lane counts, and so does the tick that pays it.
/// `None` when some cycle never pays a due lane.
fn response_bound(graph: &Graph) -> Option<usize> {
    fn wait(
        graph: &Graph,
        lane: usize,
        vertex: Vertex,
        input: i64,
        memo: &mut BTreeMap<(Vertex, i64), usize>,
        visiting: &mut Vec<(Vertex, i64)>,
    ) -> Option<usize> {
        if let Some(&known) = memo.get(&(vertex, input)) {
            return Some(known);
        }
        if visiting.contains(&(vertex, input)) {
            return None;
        }
        let fields = controller::plant_fields(u8::try_from(vertex.0).unwrap());
        let pending = if lane == 0 { fields.0 } else { fields.1 };
        let due = pending || (input >> lane) & 1 == 1;
        if !due {
            memo.insert((vertex, input), 0);
            return Some(0);
        }
        let step = graph[&vertex][input as usize];
        let pays = step.output
            == i64::from(if lane == 0 {
                controller::PAY_A
            } else {
                controller::PAY_B
            });
        let ticks = if pays {
            1
        } else {
            visiting.push((vertex, input));
            let target = (step.next_plant, step.next_memory);
            let mut longest = 0;
            for later in 0..graph[&target].len() as i64 {
                longest = longest.max(wait(graph, lane, target, later, memo, visiting)?);
            }
            visiting.pop();
            1 + longest
        };
        memo.insert((vertex, input), ticks);
        Some(ticks)
    }
    let mut bound = 0;
    for lane in 0..2 {
        let mut memo = BTreeMap::new();
        for &vertex in graph.keys() {
            for input in 0..graph[&vertex].len() as i64 {
                bound = bound.max(wait(
                    graph,
                    lane,
                    vertex,
                    input,
                    &mut memo,
                    &mut Vec::new(),
                )?);
            }
        }
    }
    Some(bound)
}

/// The vault's response to one tick, restated from the README: the next
/// pending flags, pause, and must-serve, or `None` when the output is not
/// allowed there.
#[allow(clippy::too_many_arguments)]
fn vault_rule(
    pending_a: bool,
    pending_b: bool,
    pause: u8,
    must_serve: bool,
    arriving_a: bool,
    arriving_b: bool,
    alarm: bool,
    output: u8,
) -> Option<(bool, bool, u8, bool)> {
    let (due_a, due_b) = (pending_a || arriving_a, pending_b || arriving_b);
    if pause > 0 {
        // Paused: nothing is paid; the pause counts down, and ending with a
        // lane due starts must-serve.
        return (output == controller::WAIT).then_some((
            due_a,
            due_b,
            pause - 1,
            pause == 1 && (due_a || due_b),
        ));
    }
    if alarm && !must_serve {
        // An honored alarm pays nothing and starts the pause.
        return (output == controller::WAIT).then_some((due_a, due_b, PAUSE_TICKS, false));
    }
    if (output == controller::PAY_A && !due_a) || (output == controller::PAY_B && !due_b) {
        return None;
    }
    Some((
        due_a && output != controller::PAY_A,
        due_b && output != controller::PAY_B,
        0,
        must_serve && output == controller::WAIT && (due_a || due_b),
    ))
}

/// The controller's choice, restated from the README: pay a due lane whenever
/// paying is allowed, the priority lane when both are due, and give priority
/// to the other lane after a payout.
#[allow(clippy::too_many_arguments)]
fn controller_rule(
    pending_a: bool,
    pending_b: bool,
    pause: u8,
    must_serve: bool,
    priority_b: bool,
    arriving_a: bool,
    arriving_b: bool,
    alarm: bool,
) -> (u8, bool) {
    let (due_a, due_b) = (pending_a || arriving_a, pending_b || arriving_b);
    let output = if pause > 0 || (alarm && !must_serve) {
        controller::WAIT
    } else if due_a && due_b {
        if priority_b {
            controller::PAY_B
        } else {
            controller::PAY_A
        }
    } else if due_a {
        controller::PAY_A
    } else if due_b {
        controller::PAY_B
    } else {
        controller::WAIT
    };
    let priority = match output {
        controller::PAY_A => true,
        controller::PAY_B => false,
        _ => priority_b,
    };
    (output, priority)
}

fn every_input() -> impl Iterator<Item = (u8, bool, bool, bool)> {
    (0..controller::INPUTS as u8)
        .map(|symbol| (symbol, symbol & 1 == 1, symbol & 2 == 2, symbol & 4 == 4))
}

#[test]
fn the_shipped_files_are_canonical_and_pinned() {
    assert_eq!(PIN.len(), 65);
    assert!(PIN.ends_with('\n'));
    let pin = PIN.trim_end();
    assert!(
        pin.bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    );
    assert_eq!(sha256_hex(CONTRACT), pin);
    assert_eq!(controller::CONTRACT_SHA256, pin);
    let model = contract(CONTRACT);
    for bytes in [STRATEGY, FIXED_PRIORITY, ALARM_DEFERENTIAL] {
        assert_eq!(strategy(bytes, &model).contract_sha256, pin);
    }
}

#[test]
fn the_rust_tables_equal_the_shipped_files() {
    let model = contract(CONTRACT);
    assert_eq!(model.states as usize, controller::PLANT_STATES);
    assert_eq!(model.inputs as usize, controller::INPUTS);
    assert_eq!(model.outputs as usize, controller::OUTPUTS);
    assert_eq!(model.initial, i64::from(controller::INITIAL_PLANT));
    assert_eq!(model.safe, (0..model.states).collect());
    assert_eq!(
        model.transitions,
        controller::TRANSITIONS
            .iter()
            .flatten()
            .flatten()
            .map(|next| i64::from(*next))
            .collect::<Vec<_>>()
    );
    assert_eq!(model.recurrence.len(), controller::RECURRENCE.len());
    for (goal, accepted) in model.recurrence.iter().zip(controller::RECURRENCE) {
        let expected: BTreeSet<i64> = (0..model.states)
            .filter(|plant| accepted[*plant as usize] == 1)
            .collect();
        assert_eq!(*goal, expected);
    }
    let accepted = strategy(STRATEGY, &model);
    assert_eq!(accepted.memory_states as usize, controller::MEMORY_STATES);
    assert_eq!(
        accepted.initial_memory,
        i64::from(controller::INITIAL_MEMORY)
    );
    assert_eq!(
        accepted.rows,
        controller::ROWS
            .iter()
            .flatten()
            .map(|[output, next]| (i64::from(*output), i64::from(*next)))
            .collect::<Vec<_>>()
    );
    for memory in 0..controller::MEMORY_STATES as u8 {
        for input in 0..controller::INPUTS as u8 {
            assert_eq!(
                controller::row(memory, input),
                Some(controller::ROWS[usize::from(memory)][usize::from(input)])
            );
        }
    }
    assert_eq!(controller::row(0, 8), None);
    assert_eq!(controller::row(48, 0), None);
    assert_eq!(controller::successor(0, 0, 3), None);
    assert_eq!(controller::successor(24, 0, 0), None);
}

#[test]
fn the_contract_states_the_readme_rules() {
    let model = contract(CONTRACT);
    assert_eq!(
        model.initial,
        i64::from(controller::plant_index(false, false, 0, false))
    );
    for plant in 0..controller::PLANT_STATES as u8 {
        let (pending_a, pending_b, pause, must_serve) = controller::plant_fields(plant);
        assert_eq!(
            controller::plant_index(pending_a, pending_b, pause, must_serve),
            plant
        );
        for (symbol, arriving_a, arriving_b, alarm) in every_input() {
            for output in 0..controller::OUTPUTS as u8 {
                let expected = vault_rule(
                    pending_a, pending_b, pause, must_serve, arriving_a, arriving_b, alarm, output,
                )
                .map(|(a, b, pause, must_serve)| controller::plant_index(a, b, pause, must_serve));
                assert_eq!(
                    controller::successor(plant, symbol, output),
                    expected,
                    "plant {plant} input {symbol} output {output}"
                );
                let index = (usize::from(plant) * controller::INPUTS + usize::from(symbol))
                    * controller::OUTPUTS
                    + usize::from(output);
                assert_eq!(
                    model.transitions[index],
                    expected.map_or(-1, i64::from),
                    "contract entry {index}"
                );
            }
        }
        // Goal 0 accepts every state where lane A is not pending; goal 1, lane B.
        assert_eq!(model.recurrence[0].contains(&i64::from(plant)), !pending_a);
        assert_eq!(model.recurrence[1].contains(&i64::from(plant)), !pending_b);
    }
}

#[test]
fn the_strategy_states_the_readme_controller() {
    let model = contract(CONTRACT);
    let accepted = strategy(STRATEGY, &model);
    assert_eq!(
        accepted.initial_memory,
        i64::from(controller::memory_index(controller::INITIAL_PLANT, false))
    );
    for memory in 0..controller::MEMORY_STATES as u8 {
        let (plant, priority_b) = (memory / 2, memory % 2 == 1);
        assert_eq!(controller::memory_index(plant, priority_b), memory);
        let (pending_a, pending_b, pause, must_serve) = controller::plant_fields(plant);
        for (symbol, arriving_a, arriving_b, alarm) in every_input() {
            assert_eq!(
                controller::input_index(arriving_a, arriving_b, alarm),
                symbol
            );
            let (output, priority) = controller_rule(
                pending_a, pending_b, pause, must_serve, priority_b, arriving_a, arriving_b, alarm,
            );
            let next_plant = vault_rule(
                pending_a, pending_b, pause, must_serve, arriving_a, arriving_b, alarm, output,
            )
            .map(|(a, b, pause, must_serve)| controller::plant_index(a, b, pause, must_serve))
            .expect("the controller never chooses a forbidden output");
            assert_eq!(
                accepted.rows[usize::from(memory) * controller::INPUTS + usize::from(symbol)],
                (
                    i64::from(output),
                    i64::from(controller::memory_index(next_plant, priority))
                ),
                "memory {memory} input {symbol}"
            );
        }
    }
}

#[test]
fn the_synthesized_step_equals_the_certified_strategy_on_every_input() {
    let mut checked = 0;
    for memory in 0..controller::MEMORY_STATES as u8 {
        let (plant, priority_b) = (memory / 2, memory % 2 == 1);
        let (pending_a, pending_b, pause, must_serve) = controller::plant_fields(plant);
        for (symbol, arriving_a, arriving_b, alarm) in every_input() {
            // The step's inputs and outputs, in the order `synthesis.json`
            // declares them.
            let flag = i64::from;
            let step = synthesized::transition(&[
                flag(pending_a),
                flag(pending_b),
                i64::from(pause),
                flag(must_serve),
                flag(priority_b),
                flag(arriving_a),
                flag(arriving_b),
                flag(alarm),
            ])
            .unwrap_or_else(|| panic!("memory {memory} input {symbol}: the step trapped"));
            let [
                output,
                next_a,
                next_b,
                next_pause,
                next_serve,
                next_priority,
            ] = step;
            let next_plant = controller::plant_index(
                next_a == 1,
                next_b == 1,
                u8::try_from(next_pause).unwrap(),
                next_serve == 1,
            );
            let next_memory = controller::memory_index(next_plant, next_priority == 1);
            assert_eq!(
                controller::row(memory, symbol),
                Some([u8::try_from(output).unwrap(), next_memory]),
                "memory {memory} input {symbol}"
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 384);
    // Inputs outside the checked domain are refused, never guessed.
    assert_eq!(synthesized::transition(&[0, 0, 3, 0, 0, 0, 0, 0]), None);
    assert_eq!(synthesized::transition(&[2, 0, 0, 0, 0, 0, 0, 0]), None);
    assert_eq!(synthesized::transition(&[0, 0, 0, 0, 0, 0, 0]), None);
}

#[test]
fn the_shipped_strategy_is_accepted_for_every_input_sequence() {
    assert_eq!(
        verify(CONTRACT, STRATEGY, PIN.trim_end()),
        Verdict::Accepted {
            states: 26,
            edges: 208
        }
    );
}

/// Checks that a counterexample is a reachable cycle that avoids the goal.
fn valid_cycle(cycle: &[Step], goal: &BTreeSet<i64>, graph: &Graph) {
    assert!(!cycle.is_empty());
    let first = cycle[0];
    let last = cycle[cycle.len() - 1];
    assert_eq!(
        (last.next_plant, last.next_memory),
        (first.plant, first.memory)
    );
    for pair in cycle.windows(2) {
        assert_eq!(
            (pair[0].next_plant, pair[0].next_memory),
            (pair[1].plant, pair[1].memory)
        );
    }
    for step in cycle {
        assert!(
            !goal.contains(&step.plant),
            "cycle enters the goal at {step:?}"
        );
        assert!(graph[&(step.plant, step.memory)].contains(step));
    }
}

#[test]
fn a_fixed_priority_starves_lane_b() {
    let model = contract(CONTRACT);
    let graph = reachable(&model, &strategy(FIXED_PRIORITY, &model)).unwrap();
    match verify(CONTRACT, FIXED_PRIORITY, PIN.trim_end()) {
        Verdict::Starved { goal: 1, cycle } => {
            valid_cycle(&cycle, &model.recurrence[1], &graph);
            // Lane A arrives on every tick of the cycle and is paid every time.
            assert!(
                cycle
                    .iter()
                    .all(|step| step.input & 1 == 1 && step.output == i64::from(controller::PAY_A))
            );
        }
        other => panic!("expected lane B starved, got {other:?}"),
    }
    assert_eq!(response_bound(&graph), None);
}

#[test]
fn deferring_to_alarms_in_must_serve_freezes_payouts() {
    let model = contract(CONTRACT);
    let graph = reachable(&model, &strategy(ALARM_DEFERENTIAL, &model)).unwrap();
    match verify(CONTRACT, ALARM_DEFERENTIAL, PIN.trim_end()) {
        Verdict::Starved { goal: 0, cycle } => {
            valid_cycle(&cycle, &model.recurrence[0], &graph);
            // The alarm stays raised and the controller waits in must-serve.
            assert!(cycle.iter().all(|step| step.input & 4 == 4
                && step.output == i64::from(controller::WAIT)
                && controller::plant_fields(u8::try_from(step.plant).unwrap()).3));
        }
        other => panic!("expected lane A starved, got {other:?}"),
    }
    assert_eq!(response_bound(&graph), None);
}

#[test]
fn a_wrong_pin_or_a_forbidden_row_is_rejected() {
    assert_eq!(
        verify(CONTRACT, STRATEGY, &"0".repeat(64)),
        Verdict::ContractPinMismatch
    );
    let other = String::from_utf8_lossy(STRATEGY).replace(PIN.trim_end(), &"0".repeat(64));
    assert_eq!(
        verify(CONTRACT, other.as_bytes(), PIN.trim_end()),
        Verdict::StrategyPinMismatch
    );
    // Paying lane A at the initial state, where nothing is due, is forbidden.
    let model = contract(CONTRACT);
    let mut candidate = strategy(STRATEGY, &model);
    candidate.rows[0] = (i64::from(controller::PAY_A), 0);
    assert_eq!(
        reachable(&model, &candidate).unwrap_err(),
        Step {
            plant: 0,
            memory: 0,
            input: 0,
            output: 1,
            next_plant: -1,
            next_memory: 0
        }
    );
}

#[test]
fn every_due_lane_is_paid_within_eight_ticks() {
    let model = contract(CONTRACT);
    let graph = reachable(&model, &strategy(STRATEGY, &model)).unwrap();
    assert_eq!(response_bound(&graph), Some(8));
}
