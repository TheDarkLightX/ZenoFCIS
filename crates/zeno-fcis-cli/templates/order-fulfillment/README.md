# Generated order fulfillment

This local application follows one order from checkout to delivery. Payment
and shipping happen in other systems: the order sends them requests through
the outbox, and their answers come back as commands. It shows three patterns:
- a synthesized finite decision core, which selects a complete decision
  branch from the order, command, and caller;
- idempotent external requests: each request is queued once, by the decision
  that moves the order, and carries a number the receiver can use to recognize
  a retry;
- callbacks that arrive twice or late are rejected and change nothing: the
  order has moved on, or the callback names an older payment attempt.

`synthesis.json` defines the closed finite decision relation. `zeno-fcis
synth` selected `synthesized/transition.rs` after checking all 1,728 input
tuples. `src/program.rs` is a reviewed adapter from the selected branch to
typed state updates, rejection reasons, and outbox requests. Its executed
decisions are checked against an independent model for all 1,440 inputs over
lawful pre-states. The law checker in `src/laws.rs` also evaluates the formulas
in `project.zeno` against each decision before publication.

`project.zeno` owns record fields, command variants, reason order, channel
types, and relational formulas. `build.rs` supplies explicit scalar bounds and
catalog meanings, then checks and generates the schema and project bindings.
`profile.rs` binds the exact source and the runtime-only law manifest.

## The rules

The order has two fields: `status` (field 120) and `payment_attempts` (field
121, from 0 to 3). It starts `Placed` with no payment attempt. Every command
has an `action` (field 125) and a `callback_attempt` (field 126, from 0 to 3),
and carries its `caller` (field 130) in the context. A payment provider's
callback uses `callback_attempt` to name the attempt it answers; every other
action ignores it.

| Action | ID | Sent by | From status | To status | Request queued |
| --- | --- | --- | --- | --- | --- |
| `Checkout` | 150 | `Customer` | `Placed` | `AwaitingPayment` | capture, numbered with the new attempt |
| `PaymentCaptured` | 151 | `PaymentProvider` | `AwaitingPayment` | `Paid` | shipping, numbered with the paid attempt |
| `PaymentDeclined` | 152 | `PaymentProvider` | `AwaitingPayment` | `Placed` | none |
| `ParcelDispatched` | 153 | `Carrier` | `Paid` | `Shipped` | none |
| `ParcelDelivered` | 154 | `Carrier` | `Shipped` | `Delivered` | none |
| `CancelOrder` | 155 | `Customer` | `Placed` or `AwaitingPayment` | `Cancelled` | from `AwaitingPayment`, a void of the pending attempt |

Statuses are 160 `Placed`, 161 `AwaitingPayment`, 162 `Paid`, 163 `Shipped`,
164 `Delivered`, and 165 `Cancelled`. `Delivered` and `Cancelled` are final.

The rules apply in this order, and the first that applies decides:

1. An action from any other caller is rejected with `wrong_caller` (200).
2. An action from any other status is rejected with `invalid_transition`
   (201). This covers a repeated or late callback: a second
   `PaymentCaptured` finds the order already `Paid`.
3. A `PaymentCaptured` or `PaymentDeclined` whose `callback_attempt` is not
   the current `payment_attempts` is rejected with `stale_callback` (202). A
   late answer about an earlier attempt cannot settle the current one.
4. `Checkout` when three payment attempts have been made is rejected with
   `attempts_exhausted` (203).
5. `Checkout` accepts, adds one to `payment_attempts`, and queues a capture
   request numbered with the new count.
6. `PaymentDeclined` commits failure `payment_declined` (204): the order
   returns to `Placed` and keeps its attempt count, so the customer can check
   out again with a new attempt number.
7. Every other action in the table accepts as shown, and changes no field
   the table does not name.

A rejection changes nothing and queues nothing. Payment requests go to
`payment-provider` on channel 300 with the attempt number and the action (175
`Capture` or 176 `Void`); shipping requests go to `carrier` on channel 301 with
the paid attempt number.

## How requests stay idempotent

- A request is queued by the same decision that changes the status, so a
  status change and its request are published together or not at all.
- A queued request stays pending until the destination acknowledges it, and
  every delivery attempt carries the same delivery ID. The destination must
  record delivery IDs, so a delivery repeated after a crash is recognized, not
  acted on twice.
- The attempt number in each payment request lets the provider recognize a
  retried capture. A new checkout after a decline uses a new number, so the
  provider can tell a new attempt from a retry.
- The provider's answers name the attempt too, and an answer about an older
  attempt is rejected. A decline of attempt 1 that arrives late, after the
  customer checked out again, cannot decline attempt 2.
- Incoming duplicates meet two layers. Publishing the same authorized decision
  again is an idempotent replay in the shell; the demonstration checks this
  for every committed decision. A callback that arrives again, under the same
  message ID or a new one, is decided against the order's current status: once
  the order has moved on, the state machine rejects it.

No answer ever comes back inside a decision: the provider's and the carrier's
answers are new commands.

## What is checked

Regenerate and check the decision core with:

```sh
python3 decision_to_synthesis.py --check
zeno-fcis synth run synthesis.json --out synthesized --check
zeno-fcis synth verify synthesis.json --out synthesized
```

The synthesis claim covers the pure branch-selection function and its declared
finite inputs. The app-level conformance test additionally checks the adapter's
post-state, reasons, and request plan on every lawful finite input. The
external payment and carrier systems are outside that claim.

Every decision is checked at run time, before it can be published:
- law 500: an order awaiting payment, paid, shipped, or delivered has made at
  least one payment attempt (a cancelled order may have made none);
- laws 501 to 505: each accepted action came from its caller and started from
  its status, a capture named the current attempt, the order moved as the
  table says, and a declined payment is never accepted;
- law 506: a decline of the current attempt returns the order to `Placed`
  and keeps the count;
- in Rust, because formulas cannot see them: the rejection reason, the exact
  request, the absence of effects, and the genesis.

A rejection publishes nothing by construction, and the law framework records
law 509 as satisfied for every rejection. That law has no formula, so
`profile.rs` registers it instead of `project.zeno` carrying an always-true
placeholder. As a result, this passes:

```sh
zeno-fcis check project.zeno --require-substantive --require-resolved-paths
```

Each law with a formula also declares the decisions it is enforced on: `on
commit, genesis` for 500, `on accept` for 501 to 505, and `on failure` for
506. `authority()` checks the law manifest against those declarations before
it builds the authority, and `tests/laws.rs` shows a manifest that binds law
501 to every commit, or to the genesis, reported as a mismatch.

The tests check the running application:
- `tests/conformance.rs` explores the application from genesis and finds 20
  reachable states: exactly the admitted states that satisfy law 500. It runs
  1,440 commands from those states, every action with every attempt a
  callback could name, from every caller, through admission, the authority,
  the program, the law checker, the committed patch, and the outbox. It
  compares each outcome with the expected one, checks that every reachable
  order can still reach `Delivered` or `Cancelled`, and compares 23 examples
  in `tests/decision-examples.txt`.
- `tests/laws.rs` gives the law checker decisions a faulty program could
  make, such as a capture from the customer, a capture or decline of an older
  attempt, a second shipping request, or a cancellation without a void, and
  requires it to refuse each one.
- `tests/lifecycle.rs` runs the demonstration, a late decline of an older
  attempt, a repeated capture callback, and a cancellation while payment is
  pending.
- `tests/determinism.rs` decides 400 inputs (every reachable state with every
  action and caller, plus each payment callback naming a stale attempt) eight
  times each through `execute_probed`, then again in three child processes
  with a changed environment, and requires every decision digest to match.

The expected outcomes come from a reference model in the test, written from
these rules by the program's author. It catches binding and adapter errors,
not a misreading shared by both. The examples file is the check on that: its
header records who wrote the examples and whether the project's owner has
reviewed them.

```sh
zeno-fcis purity src/program.rs src/laws.rs
```

All of these are detectors: agreement shows that these runs matched and that
no error-level rule fired, not that the application is correct for every
possible deployment.

## Run this development candidate

This template depends on the current ZenoFCIS checkout. From that checkout,
run:

```sh
python3 tools/check_generated_application.py
```

That gate creates a fresh application using the CLI, patches its dependencies
to the exact checkout, checks dependency versions against the workspace lock,
then compiles and runs its tests and demonstration as an isolated package.
With these development dependencies available in a standalone checkout:

```sh
cargo +1.97.1 test --locked
cargo +1.97.1 run --locked -- new-order.sqlite
```

The SQLite shell is the `sqlite` feature, on by default. Without it,
`cargo +1.97.1 build --no-default-features` builds the core alone: the
generated bindings, the program, the law checker, the profile, the delivery
adapter, and `authority()`, with no database; the gate checks that it also
compiles for `wasm32-unknown-unknown`. `create`, `invoke`, `journey`, and the demonstration
binary need the feature.

The demonstration requires a new database path. It declines one payment and
refuses a late capture of it. After a second checkout, it refuses a late
decline of the first attempt, a capture from the wrong caller, a repeated
capture, and a late cancellation, then pays, ships, and delivers the order.
It then interrupts request delivery, reopens the database, and finishes
delivery. It prints a JSON summary.

The destination keeps an in-memory idempotency ledger, which survives the
database reopen only within one process. A real destination must persist its
delivery IDs and entry hashes. The context's `caller` and the principal are
trusted tutorial inputs, not remote authentication: a deployment must
authenticate each caller before admission. Source hashes identify reviewed
example policy, not certified binaries or release evidence. Changing the source
changes policy identity and requires a new database unless a separately
reviewed migration is implemented.
