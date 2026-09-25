# Generated inventory reservation

This local application keeps the stock of one product. Units are either
available to sell or reserved for an order; shipping sends reserved units out,
and restocking brings units in. It shows three patterns:
- a *synthesized* decision core: `synthesis.json` states the rules as a
  relation over every input, and `zeno-fcis synth` selects a program that
  satisfies it on all 432 of them;
- a conservation law: every accepted decision keeps the count of units
  exactly right;
- commands with parameters: each command carries an action and a quantity.

`src/program.rs` checks the operator and then calls the synthesized step in
`synthesized/transition.rs`, mapping its output into typed staging. The law
checker in `src/laws.rs` evaluates the formulas in `project.zeno` against every
decision, and refuses any decision that breaks them.

`project.zeno` owns record fields, command variants, reason order, channel
types, and relational formulas. `build.rs` supplies explicit scalar bounds and
catalog meanings, then checks and generates the schema and project bindings.
`profile.rs` binds the exact source, including the synthesis files, and the
runtime-only law manifest.

## The rules

The stock has two fields, `available` (field 110) and `reserved` (field 111),
each from 0 to 5; every combination is a valid state. It starts with both at
zero. Each command has an `action` (field 120) and a `quantity` from 1 to 3
(field 121), and its context says whether the operator is `authorized` (field
130).

| Action | ID | Effect when accepted |
| --- | --- | --- |
| `Reserve` | 150 | moves `quantity` units from `available` to `reserved` |
| `Release` | 151 | moves `quantity` units from `reserved` back to `available` |
| `Ship` | 152 | removes `quantity` units from `reserved` and queues one shipment request for them |
| `Restock` | 153 | adds `quantity` units to `available` |

The rules apply in this order, and the first that applies decides:

1. A command without `authorized` is rejected with `not_authorized` (200).
2. `Reserve` of more units than are available is rejected with
   `insufficient_available` (201).
3. `Release` or `Ship` of more units than are reserved is rejected with
   `insufficient_reserved` (202).
4. A movement that would leave either field above 5 is rejected with
   `over_capacity` (203): `Reserve` onto the reserved units, and `Release`
   or `Restock` onto the available ones.
5. Otherwise the command is accepted as the table says.

A rejection changes nothing and queues nothing. Shipment requests go to
`warehouse` on channel 300 with the shipped units. The shipped units leave this
state in the same decision that queues the request; the request tells the
warehouse to send them.

## The synthesized step

`synthesis.json` has two parts:
- a contract, which states the rules above as a relation over the stock, the
  action code, the quantity, and the outputs;
- a sketch: a decision program with two holes, the stock capacity and the
  decision code for releasing or shipping more than is reserved.

`zeno-fcis synth run` evaluates hole assignments over all 432 inputs and
selects the first that satisfies the contract on every one: capacity 5 and
code 1, which the adapter maps to `insufficient_reserved`. The selected
program, its complete input and output vectors, and the emitted Rust are
checked in under `synthesized/`. The operator check sits in `src/program.rs`,
in front of the step, so the synthesis stays small.

That selection is exhaustive verification against the contract, for the
program as the library's interpreter runs it. The emitted Rust carries the same
result once `synth verify` has replayed it:

```sh
zeno-fcis synth run synthesis.json --out synthesized --check
zeno-fcis synth verify synthesis.json --out synthesized --receipt rust-conformance.json
zeno-fcis synth run synthesis.json --target python --out python-step
zeno-fcis synth verify synthesis.json --target python --out python-step --receipt python-conformance.json
```

It is only as right as the contract: `tools/check_synthesis.py` in the library
compares the vectors with a separate restatement of the rules, and the examples
in `tests/decision-examples.txt` check the running application against this
README.

## What is checked

Every decision is checked at run time, before it can be published:
- law 500: both fields stay within their bounds. Every combination of the two
  fields is valid, so this law only restates the schema bounds;
- law 501: units are conserved. Reserving and releasing move units without
  changing the total, shipping removes exactly the quantity, and restocking
  adds exactly the quantity;
- law 502: only an authorized operator moves stock, and each action moves
  exactly its quantity between the fields it names;
- in Rust, because formulas cannot see them: the rejection reason, the exact
  shipment request, and the absence of effects.

Two more laws have no formula, so `profile.rs` registers them instead of
`project.zeno` carrying always-true or always-false placeholders:
- 508: the application never commits a failure, and the law checker refuses
  one outright;
- 509: a rejection publishes nothing, by construction. The law framework
  records it as satisfied for every rejection.

As a result, this passes:

```sh
zeno-fcis check project.zeno --require-substantive --require-resolved-paths
```

The tests check the running application:
- `tests/conformance.rs` runs all 864 admitted inputs through admission, the
  authority, the adapter, the law checker, the committed patch, and the
  outbox. It compares each outcome with the synthesized step read through the
  adapter's output table and checks that units are conserved. It also checks
  that schema admission matches the finite domain in both directions, that
  genesis is exactly empty, that all 36 states are reachable from it, and that
  20 examples in `tests/decision-examples.txt` match.
- `tests/laws.rs` gives the law checker decisions a faulty program could
  make, such as a unit moved the wrong way, a shipment of the wrong size, or an
  over-capacity restock, and requires it to refuse each one.
- `tests/lifecycle.rs` runs the demonstration and shows that rejections leave
  the stock and the outbox unchanged.
- `tests/determinism.rs` decides 216 inputs (each field at 0, 2, and 5,
  with every action, quantity, and operator flag) eight times each
  through `execute_probed`, then again in three child processes with a changed
  environment, and requires every decision digest to match.

```sh
zeno-fcis purity src/program.rs src/laws.rs synthesized/transition.rs
```

All of these are detectors: agreement shows that these runs matched and that
no error-level rule fired. The conformance test covers every admitted input,
so for this finite domain it checks every decision the application can make.

## Run this development candidate

This template depends on the current ZenoFCIS checkout. From that checkout,
run:

```sh
python3 tools/check_generated_application.py
```

That gate creates a fresh application using the CLI, checks its synthesis in
Rust, Python, and JavaScript, patches its dependencies to the exact checkout,
checks dependency versions against the workspace lock, then compiles and runs
its tests and demonstration as an isolated package. With these development
dependencies available in a standalone checkout:

```sh
cargo +1.97.1 test --locked
cargo +1.97.1 run --locked -- new-stock.sqlite
```

The SQLite shell is the `sqlite` feature, on by default. Without it,
`cargo +1.97.1 build --no-default-features` builds the core alone: the
generated bindings, the program, the law checker, the profile, the delivery
adapter, and `authority()`, with no database; the gate checks that it also
compiles for `wasm32-unknown-unknown`. `create`, `invoke`, `journey`, and the demonstration
binary need the feature.

The demonstration requires a new database path. It restocks, reserves,
releases, and ships, meeting every rejection reason along the way. It then
interrupts shipment delivery, reopens the database, finishes delivery, and
checks that the units in stock equal the units restocked minus the units
shipped. It prints a JSON summary.

The destination keeps an in-memory idempotency ledger, which survives the
database reopen only within one process. A real destination must persist its
delivery IDs and entry hashes. The context's `authorized` flag and the
principal are trusted tutorial inputs, not remote authentication. Source hashes
identify reviewed example policy, not certified binaries or release evidence.
Changing the source, including the synthesis files, changes policy identity and
requires a new database unless a separately reviewed migration is implemented.
