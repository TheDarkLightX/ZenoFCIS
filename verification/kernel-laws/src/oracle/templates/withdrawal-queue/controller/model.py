#!/usr/bin/env python3
"""The withdrawal queue's finite model: the vault's tick rules, the fair
controller, and the files that ZenoFCIS and OrbitSynthesis check.

Run from this directory with Python 3.12 or later; only the standard library
is used. The pipeline has two phases, with `zeno-fcis synth run` between
them; README.md gives the exact commands.

    python3 -B model.py problem

writes the synthesis problem `../synthesis.json` (the tick rules as a
per-step contract, and the fair policy as a sketch with holes), the
OrbitSynthesis contract `contract.json` with its pin `contract.sha256`, and
the two rejected strategies under `rejected/`.

    python3 -B model.py tables

reads the vectors that `zeno-fcis synth run` checked,
`../synthesized/vectors.json`, and writes the strategy they define,
`strategy.json`, pinned to the contract, and the Rust tables
`../src/controller.rs`. The strategy is what OrbitSynthesis's checker
certifies; the synthesized step is what the application runs;
`../tests/controller.rs` requires the two to agree on every input.
"""

from hashlib import sha256
import json
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
CONTRACT_SCHEMA = "orbitsynthesis/finite-controller-contract/v1"
STRATEGY_SCHEMA = "orbitsynthesis/finite-mealy-strategy/v1"
PROBLEM_SCHEMA = "zeno-fcis/synthesis-problem/1"
PROBLEM_PROFILE = "zeno-fcis/finite-i64/1"

# Ticks that stay paused after an honored alarm's own tick.
PAUSE_TICKS = 2
PAUSE_VALUES = PAUSE_TICKS + 1
# Plant state: lane A pending, lane B pending, pause, must-serve.
PLANT_STATES = 2 * 2 * PAUSE_VALUES * 2
# Input: arrivals (bit 0 lane A, bit 1 lane B) plus 4 when the alarm is raised.
INPUTS = 8
# Output: wait, pay lane A, pay lane B.
WAIT, PAY_A, PAY_B = 0, 1, 2
OUTPUTS = 3
# Controller memory: the plant state it tracks, and which lane has priority.
MEMORY_STATES = PLANT_STATES * 2


def plant_index(pending_a, pending_b, pause, must_serve):
    return ((int(pending_a) * 2 + int(pending_b)) * PAUSE_VALUES + pause) * 2 + int(must_serve)


def plant_fields(index):
    must_serve = index % 2
    index //= 2
    pause = index % PAUSE_VALUES
    index //= PAUSE_VALUES
    return index // 2, index % 2, pause, must_serve


def plant_step(pending_a, pending_b, pause, must_serve, arrive_a, arrive_b, alarm, output):
    """The vault's response to one tick, or None when the output is forbidden."""
    due_a = pending_a or arrive_a
    due_b = pending_b or arrive_b
    if pause > 0:
        # Paused: nothing is paid, and the pause counts down. When it ends
        # with a lane due, the vault must serve before honoring another alarm.
        if output != WAIT:
            return None
        return (due_a, due_b, pause - 1, pause == 1 and (due_a or due_b))
    if alarm and not must_serve:
        # An honored alarm pays nothing and starts the pause.
        if output != WAIT:
            return None
        return (due_a, due_b, PAUSE_TICKS, False)
    # Not paused, and either no alarm or the alarm is ignored: the vault may
    # pay one due lane.
    if output == PAY_A and not due_a:
        return None
    if output == PAY_B and not due_b:
        return None
    return (
        due_a and output != PAY_A,
        due_b and output != PAY_B,
        0,
        must_serve and output == WAIT and (due_a or due_b),
    )


def contract():
    """The vault's rules in OrbitSynthesis's contract format."""
    transitions = []
    for plant in range(PLANT_STATES):
        pending_a, pending_b, pause, must_serve = plant_fields(plant)
        for symbol in range(INPUTS):
            arrive_a, arrive_b, alarm = symbol & 1, (symbol >> 1) & 1, symbol >> 2
            for output in range(OUTPUTS):
                successor = plant_step(pending_a, pending_b, pause, must_serve,
                                       arrive_a, arrive_b, alarm, output)
                transitions.append(-1 if successor is None else plant_index(*successor))
    # Each lane is not pending infinitely often.
    goals = [[plant for plant in range(PLANT_STATES) if not plant_fields(plant)[lane]]
             for lane in (0, 1)]
    return {"schema": CONTRACT_SCHEMA, "states": PLANT_STATES, "inputs": INPUTS,
            "outputs": OUTPUTS, "initial": plant_index(False, False, 0, False),
            "safe_states": list(range(PLANT_STATES)), "transitions": transitions,
            "recurrence": goals}


def fair(pause, must_serve, alarm, due_a, due_b, priority_b):
    """Pays a due lane whenever paying is allowed; the priority lane when both are due."""
    if pause > 0 or (alarm and not must_serve):
        return WAIT
    if due_a and due_b:
        return PAY_B if priority_b else PAY_A
    return PAY_A if due_a else PAY_B if due_b else WAIT


def fixed_priority(pause, must_serve, alarm, due_a, due_b, priority_b):
    """Always favors lane A: a lane-B request can wait forever."""
    return fair(pause, must_serve, alarm, due_a, due_b, False)


def alarm_deferential(pause, must_serve, alarm, due_a, due_b, priority_b):
    """Waits on every alarm, even in must-serve: a sustained alarm freezes payouts."""
    if alarm:
        return WAIT
    return fair(pause, must_serve, alarm, due_a, due_b, priority_b)


def strategy_rows(choose, flip_priority):
    """The Mealy table of a policy that tracks the plant in its memory."""
    rows = []
    for memory in range(MEMORY_STATES):
        plant, priority_b = memory // 2, memory % 2
        pending_a, pending_b, pause, must_serve = plant_fields(plant)
        for symbol in range(INPUTS):
            arrive_a, arrive_b, alarm = symbol & 1, (symbol >> 1) & 1, symbol >> 2
            due_a, due_b = pending_a or arrive_a, pending_b or arrive_b
            output = choose(pause, must_serve, alarm, due_a, due_b, priority_b)
            successor = plant_step(pending_a, pending_b, pause, must_serve,
                                   arrive_a, arrive_b, alarm, output)
            if successor is None:
                raise ValueError(f"memory {memory} input {symbol}: forbidden output")
            if flip_priority and output == PAY_A:
                priority_next = 1
            elif flip_priority and output == PAY_B:
                priority_next = 0
            else:
                priority_next = priority_b
            rows.append([output, plant_index(*successor) * 2 + priority_next])
    return rows


def strategy(pin, rows):
    return {"schema": STRATEGY_SCHEMA, "contract_sha256": pin,
            "memory_states": MEMORY_STATES, "initial_memory": 0, "rows": rows}


def canonical_bytes(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"),
                      ensure_ascii=True, allow_nan=False).encode("ascii")


class Graph:
    """A `zeno-fcis/finite-i64/1` expression graph, built node by node."""

    def __init__(self):
        self.nodes = []

    def add(self, node):
        self.nodes.append(node)
        return len(self.nodes) - 1

    def input(self, position):
        return self.add(["input", position])

    def int(self, value):
        return self.add(["int", value])

    def bool(self, value):
        return self.add(["bool", value])

    def not_(self, a):
        return self.add(["not", a])

    def and_(self, a, b, *more):
        node = self.add(["and", a, b])
        for other in more:
            node = self.add(["and", node, other])
        return node

    def or_(self, a, b):
        return self.not_(self.and_(self.not_(a), self.not_(b)))

    def implies(self, a, b):
        return self.not_(self.and_(a, self.not_(b)))

    def eq(self, a, b):
        return self.add(["eq", a, b])

    def lt(self, a, b):
        return self.add(["lt", a, b])

    def sub(self, a, b):
        return self.add(["sub", a, b])

    def select(self, condition, a, b):
        return self.add(["select", condition, a, b])

    def hole(self, number, alternatives):
        return self.add({"hole": number, "alternatives": alternatives})


# The synthesized step's fields, in wire order. The controller's memory is the
# plant state it tracks plus the priority lane; the tick brings the arrivals
# and the alarm.
STEP_INPUTS = ["memory.pending_a", "memory.pending_b", "memory.pause", "memory.must_serve",
               "memory.priority_b", "tick.arriving_a", "tick.arriving_b", "tick.alarm"]
STEP_OUTPUTS = ["tick.output_wait_pay_a_pay_b", "next.pending_a", "next.pending_b",
                "next.pause", "next.must_serve", "next.priority_b"]
BOOL = {"kind": "bool"}
PAUSE_TYPE = {"kind": "int", "min": 0, "max": PAUSE_TICKS}
OUTPUT_TYPE = {"kind": "int", "min": WAIT, "max": PAY_B}


def tick_facts(g):
    """The facts both graphs derive from the eight inputs."""
    pending_a, pending_b, pause, must_serve, priority_b, arriving_a, arriving_b, alarm = (
        g.input(position) for position in range(8))
    due_a = g.or_(pending_a, arriving_a)
    due_b = g.or_(pending_b, arriving_b)
    return {"pause": pause, "must_serve": must_serve, "priority_b": priority_b,
            "alarm": alarm, "due_a": due_a, "due_b": due_b, "any_due": g.or_(due_a, due_b),
            "paused": g.lt(g.int(0), pause), "ending": g.eq(pause, g.int(1)),
            "counted": g.sub(pause, g.int(1))}


def step_contract():
    """The tick rules as a relation over the step's inputs and outputs: what
    the output may be, and what the next memory must be."""
    g = Graph()
    f = tick_facts(g)
    output, next_a, next_b, next_pause, next_serve, next_priority = (
        g.input(position) for position in range(8, 14))
    honored = g.and_(g.not_(f["paused"]), g.and_(f["alarm"], g.not_(f["must_serve"])))
    may_pay = g.and_(g.not_(f["paused"]), g.not_(honored))
    waits, pays_a, pays_b = (g.eq(output, g.int(code)) for code in (WAIT, PAY_A, PAY_B))
    priority_lane = g.select(f["priority_b"], g.int(PAY_B), g.int(PAY_A))
    rules = [
        # Nothing is paid while paused or on an honored alarm.
        g.implies(g.or_(f["paused"], honored), waits),
        # Only a due lane is paid.
        g.implies(pays_a, f["due_a"]),
        g.implies(pays_b, f["due_b"]),
        # When paying is allowed: both due pays the priority lane, one due
        # pays that lane, none due waits.
        g.implies(g.and_(may_pay, g.and_(f["due_a"], f["due_b"])), g.eq(output, priority_lane)),
        g.implies(g.and_(may_pay, g.and_(f["due_a"], g.not_(f["due_b"]))), pays_a),
        g.implies(g.and_(may_pay, g.and_(f["due_b"], g.not_(f["due_a"]))), pays_b),
        g.implies(g.and_(may_pay, g.not_(f["any_due"])), waits),
        # A due lane stays pending unless it is paid.
        g.eq(next_a, g.and_(f["due_a"], g.not_(pays_a))),
        g.eq(next_b, g.and_(f["due_b"], g.not_(pays_b))),
        # The pause counts down, and ending with a lane due starts must-serve.
        g.implies(f["paused"], g.and_(g.eq(next_pause, f["counted"]),
                                      g.eq(next_serve, g.and_(f["ending"], f["any_due"])))),
        # An honored alarm starts the pause.
        g.implies(honored, g.and_(g.eq(next_pause, g.int(PAUSE_TICKS)),
                                  g.eq(next_serve, g.bool(False)))),
        # Otherwise there is no pause, and must-serve ends with a payout or
        # when nothing is due.
        g.implies(may_pay, g.and_(g.eq(next_pause, g.int(0)),
                                  g.eq(next_serve, g.and_(f["must_serve"],
                                                          g.and_(waits, f["any_due"]))))),
        # Priority moves to the other lane after a payout.
        g.implies(pays_a, next_priority),
        g.implies(pays_b, g.not_(next_priority)),
        g.implies(waits, g.eq(next_priority, f["priority_b"])),
    ]
    return {"nodes": g.nodes, "roots": [g.and_(*rules)]}


def step_sketch():
    """The fair policy, with holes for the choices an author could get wrong.
    `zeno-fcis synth run` keeps the one assignment that satisfies the contract
    on every input."""
    g = Graph()
    f = tick_facts(g)
    # Hole 1: is an alarm honored during must-serve? Deferring to it there
    # freezes payouts under a sustained alarm.
    honoring = g.hole(1, [["input", 7], ["and", f["alarm"], g.not_(f["must_serve"])]])
    honored = g.and_(g.not_(f["paused"]), honoring)
    may_pay = g.and_(g.not_(f["paused"]), g.not_(honored))
    # Hole 2: the lane paid when both are due. A fixed lane starves the other.
    both = g.hole(2, [["int", PAY_A], ["int", PAY_B],
                      ["select", f["priority_b"], g.int(PAY_B), g.int(PAY_A)]])
    one = g.select(f["due_a"], g.int(PAY_A), g.select(f["due_b"], g.int(PAY_B), g.int(WAIT)))
    output = g.select(may_pay, g.select(g.and_(f["due_a"], f["due_b"]), both, one), g.int(WAIT))
    waits, pays_a, pays_b = (g.eq(output, g.int(code)) for code in (WAIT, PAY_A, PAY_B))
    next_a = g.and_(f["due_a"], g.not_(pays_a))
    next_b = g.and_(f["due_b"], g.not_(pays_b))
    # Hole 3: how long an honored alarm pauses payouts.
    length = g.hole(3, [["int", 1], ["int", 2], ["int", 3]])
    next_pause = g.select(f["paused"], f["counted"], g.select(honored, length, g.int(0)))
    next_serve = g.select(f["paused"], g.and_(f["ending"], f["any_due"]),
                          g.select(honored, g.bool(False),
                                   g.and_(f["must_serve"], g.and_(waits, f["any_due"]))))
    # Holes 4 and 5: where priority goes after paying lane A, and after lane B.
    after_a = g.hole(4, [["bool", False], ["bool", True]])
    after_b = g.hole(5, [["bool", True], ["bool", False]])
    next_priority = g.select(pays_a, after_a, g.select(pays_b, after_b, f["priority_b"]))
    return {"nodes": g.nodes, "roots": [output, next_a, next_b, next_pause, next_serve,
                                        next_priority]}


def problem():
    return {"schema": PROBLEM_SCHEMA, "profile": PROBLEM_PROFILE,
            "inputs": [{"name": name, "type": PAUSE_TYPE if name == "memory.pause" else BOOL}
                       for name in STEP_INPUTS],
            "outputs": [{"name": name, "type": OUTPUT_TYPE if name.startswith("tick.")
                         else PAUSE_TYPE if name == "next.pause" else BOOL}
                        for name in STEP_OUTPUTS],
            "contract": step_contract(), "sketch": step_sketch()}


def problem_text(value):
    """JSON with one graph node per line, so the graphs can be read."""
    def block(items, indent):
        pad = " " * indent
        return ("[\n" + ",\n".join(pad + json.dumps(item) for item in items)
                + "\n" + " " * (indent - 2) + "]")

    graphs = ",\n".join(
        f'  "{name}": {{\n    "nodes": {block(value[name]["nodes"], 6)},\n'
        f'    "roots": {json.dumps(value[name]["roots"])}\n  }}'
        for name in ("contract", "sketch"))
    return (f'{{\n  "schema": {json.dumps(value["schema"])},\n'
            f'  "profile": {json.dumps(value["profile"])},\n'
            f'  "inputs": {block(value["inputs"], 4)},\n'
            f'  "outputs": {block(value["outputs"], 4)},\n{graphs}\n}}\n')


def strategy_from_vectors(vectors):
    """The Mealy table that the synthesized step's complete vectors define."""
    rows = [None] * (MEMORY_STATES * INPUTS)
    for case in vectors["cases"]:
        (pending_a, pending_b, pause, must_serve, priority_b,
         arriving_a, arriving_b, alarm) = case["input"]
        output, next_a, next_b, next_pause, next_serve, next_priority = case["output"]
        memory = plant_index(pending_a, pending_b, pause, must_serve) * 2 + priority_b
        symbol = arriving_a + 2 * arriving_b + 4 * alarm
        if rows[memory * INPUTS + symbol] is not None:
            raise ValueError("duplicate vector")
        successor = plant_index(next_a, next_b, next_pause, next_serve)
        rows[memory * INPUTS + symbol] = [output, successor * 2 + next_priority]
    if any(row is None for row in rows):
        raise ValueError("the vectors do not cover every memory and input")
    return rows


def product(model, candidate):
    """Reachable (plant, memory) graph under the candidate; None on a forbidden step."""
    root = (model["initial"], candidate["initial_memory"])
    graph = {}
    pending = [root]
    while pending:
        vertex = pending.pop()
        if vertex in graph:
            continue
        plant, memory = vertex
        edges = []
        for symbol in range(INPUTS):
            output, memory_next = candidate["rows"][memory * INPUTS + symbol]
            plant_next = model["transitions"][(plant * INPUTS + symbol) * OUTPUTS + output]
            if plant_next < 0:
                return None
            edges.append((symbol, output, (plant_next, memory_next)))
            pending.append((plant_next, memory_next))
        graph[vertex] = edges
    return graph


def response_bound(graph):
    """Longest run of ticks a due lane can wait, over every input sequence.

    A tick presents a lane when it is pending or arriving. The wait counts
    that tick and every later tick up to and including the one that pays.
    Returns None when some cycle never pays a due lane.
    """
    if graph is None:
        return None
    bounds = []
    for lane, pay in ((0, PAY_A), (1, PAY_B)):
        memo = {}

        def wait(vertex, symbol, visiting=()):
            key = (vertex, symbol)
            if key in memo:
                return memo[key]
            if key in visiting:
                return None
            plant = vertex[0]
            due = plant_fields(plant)[lane] or (symbol >> lane) & 1
            if not due:
                memo[key] = 0
                return 0
            output, target = next((o, t) for s, o, t in graph[vertex] if s == symbol)
            if output == pay:
                memo[key] = 1
                return 1
            longest = 0
            for later in range(INPUTS):
                rest = wait(target, later, visiting + (key,))
                if rest is None:
                    return None
                longest = max(longest, rest)
            memo[key] = 1 + longest
            return memo[key]

        for vertex in graph:
            for symbol in range(INPUTS):
                result = wait(vertex, symbol)
                if result is None:
                    return None
                bounds.append(result)
    return max(bounds)


def rust_tables(model, accepted, pin):
    # Laid out as rustfmt formats nested arrays: an array of arrays goes
    # vertical, one element per line; a short array of integers stays on one
    # line, and a long one takes a block.
    def nested(rows, width):
        blocks = []
        for start in range(0, len(rows), width):
            lines = ["        [" + ", ".join(str(value) for value in row) + "],"
                     for row in rows[start:start + width]]
            blocks.append("    [\n" + "\n".join(lines) + "\n    ],")
        return "\n".join(blocks)

    transitions = nested([model["transitions"][start:start + OUTPUTS]
                          for start in range(0, len(model["transitions"]), OUTPUTS)], INPUTS)
    table = nested(accepted["rows"], INPUTS)
    goals = ",\n".join(
        "    [\n        " + ", ".join(str(int(plant in goal)) for plant in range(PLANT_STATES))
        + ",\n    ]"
        for goal in (set(goal) for goal in model["recurrence"]))
    return f'''//! The checked controller's tables. Generated by `controller/model.py` from
//! `controller/contract.json`, the vault's rules, and `controller/strategy.json`,
//! the table that the synthesized step defines. `tests/controller.rs` requires
//! these tables to equal those files, the step to equal the strategy on every
//! input, and the strategy to pass the contract's checks.
//!
//! Plant state `((a * 2 + b) * 3 + pause) * 2 + must_serve`, where `a` and
//! `b` are 1 while lane A or lane B is pending. Input `arrivals + 4 * alarm`,
//! where arrivals has bit 0 for a lane-A arrival and bit 1 for lane B. Output
//! 0 waits, 1 pays lane A, and 2 pays lane B. Controller memory
//! `plant * 2 + priority`, where priority 0 favors lane A and 1 favors lane B.

/// SHA-256 of the canonical contract bytes: the pin in `controller/contract.sha256`.
pub const CONTRACT_SHA256: &str =
    "{pin}";
/// Ticks that stay paused after an honored alarm's own tick.
pub const PAUSE_TICKS: u8 = {PAUSE_TICKS};
pub const PLANT_STATES: usize = {PLANT_STATES};
pub const INPUTS: usize = {INPUTS};
pub const OUTPUTS: usize = {OUTPUTS};
pub const MEMORY_STATES: usize = {MEMORY_STATES};
pub const INITIAL_PLANT: u8 = {model["initial"]};
pub const INITIAL_MEMORY: u8 = {accepted["initial_memory"]};
pub const WAIT: u8 = {WAIT};
pub const PAY_A: u8 = {PAY_A};
pub const PAY_B: u8 = {PAY_B};

/// The plant state after `[plant][input][output]`, or -1 when the contract
/// forbids that output there. Flattened in that order, it is the contract's
/// `transitions` list.
pub const TRANSITIONS: [[[i8; OUTPUTS]; INPUTS]; PLANT_STATES] = [
{transitions}
];

/// `[output, next_memory]` for `[memory][input]`. Flattened in that order, it
/// is the strategy's `rows` list.
pub const ROWS: [[[u8; 2]; INPUTS]; MEMORY_STATES] = [
{table}
];

/// Plant states each recurrence goal accepts: lane A not pending, then lane B
/// not pending. The controller must visit each set infinitely often.
pub const RECURRENCE: [[u8; PLANT_STATES]; 2] = [
{goals},
];

/// The plant state index of the given fields.
#[must_use]
pub fn plant_index(pending_a: bool, pending_b: bool, pause: u8, must_serve: bool) -> u8 {{
    ((u8::from(pending_a) * 2 + u8::from(pending_b)) * (PAUSE_TICKS + 1) + pause) * 2
        + u8::from(must_serve)
}}

/// The fields of a plant state index: lane A pending, lane B pending, pause,
/// must-serve.
#[must_use]
pub fn plant_fields(plant: u8) -> (bool, bool, u8, bool) {{
    let must_serve = plant % 2 == 1;
    let plant = plant / 2;
    let pause = plant % (PAUSE_TICKS + 1);
    let plant = plant / (PAUSE_TICKS + 1);
    (plant / 2 == 1, plant % 2 == 1, pause, must_serve)
}}

/// The controller memory index of a tracked plant state and priority.
#[must_use]
pub fn memory_index(plant: u8, priority_b: bool) -> u8 {{
    plant * 2 + u8::from(priority_b)
}}

/// The input symbol of one tick.
#[must_use]
pub fn input_index(arriving_a: bool, arriving_b: bool, alarm: bool) -> u8 {{
    u8::from(arriving_a) + 2 * u8::from(arriving_b) + 4 * u8::from(alarm)
}}

/// The strategy's output and next memory, or `None` outside the tables.
#[must_use]
pub fn row(memory: u8, input: u8) -> Option<[u8; 2]> {{
    ROWS.get(usize::from(memory))?
        .get(usize::from(input))
        .copied()
}}

/// The contract's successor plant state, or `None` when the output is
/// forbidden there or an index is outside the tables.
#[must_use]
pub fn successor(plant: u8, input: u8, output: u8) -> Option<u8> {{
    let next = TRANSITIONS
        .get(usize::from(plant))?
        .get(usize::from(input))?
        .get(usize::from(output))?;
    u8::try_from(*next).ok()
}}
'''


def write_problem():
    model = contract()
    contract_bytes = canonical_bytes(model)
    pin = sha256(contract_bytes).hexdigest()
    (HERE / "contract.json").write_bytes(contract_bytes)
    (HERE / "contract.sha256").write_text(pin + "\n", encoding="ascii")
    (HERE / "rejected").mkdir(exist_ok=True)
    for name, choose, flip in (("fixed-priority", fixed_priority, False),
                               ("alarm-deferential", alarm_deferential, True)):
        candidate = strategy(pin, strategy_rows(choose, flip))
        (HERE / "rejected" / f"{name}.strategy.json").write_bytes(canonical_bytes(candidate))
        print(f"{name}: response bound {response_bound(product(model, candidate))}")
    (HERE.parent / "synthesis.json").write_text(problem_text(problem()), encoding="ascii")
    print(f"contract sha256 {pin}")
    print(f"plant states {PLANT_STATES}, inputs {INPUTS}, outputs {OUTPUTS}, "
          f"memory states {MEMORY_STATES}")
    print("wrote synthesis.json; next: zeno-fcis synth run synthesis.json --out synthesized")


def write_tables():
    model = contract()
    contract_bytes = canonical_bytes(model)
    pin = sha256(contract_bytes).hexdigest()
    if (HERE / "contract.json").read_bytes() != contract_bytes:
        raise SystemExit("contract.json is stale; run `python3 -B model.py problem` first")
    vectors = json.loads((HERE.parent / "synthesized" / "vectors.json").read_text())
    rows = strategy_from_vectors(vectors)
    if rows != strategy_rows(fair, True):
        raise SystemExit("the synthesized step differs from the fair policy")
    accepted = strategy(pin, rows)
    (HERE / "strategy.json").write_bytes(canonical_bytes(accepted))
    (HERE.parent / "src" / "controller.rs").write_text(rust_tables(model, accepted, pin),
                                                       encoding="ascii")
    graph = product(model, accepted)
    edges = sum(len(edges) for edges in graph.values())
    print(f"strategy from {len(vectors['cases'])} synthesized vectors equals the fair policy")
    print(f"accepted strategy: {len(graph)} reachable product states, {edges} edges, "
          f"response bound {response_bound(graph)} ticks")


def main(arguments):
    if arguments == ["problem"]:
        write_problem()
    elif arguments == ["tables"]:
        write_tables()
    else:
        raise SystemExit("usage: model.py problem|tables")


if __name__ == "__main__":
    main(sys.argv[1:])
