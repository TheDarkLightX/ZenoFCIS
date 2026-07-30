# Tutorial: run finite time claims

Run the executable walkthrough:

```bash
cargo +1.97.1 run --quiet -p zeno-fcis-spec \
  --example temporal_walkthrough --locked
```

It prints:

```text
finite always true, 3 events: satisfied
finite eventually false, 3 events: counterexample at step 0
finite next true, 1 event: counterexample at step 0
finite next true, 2 events: satisfied
unbounded always true: proof obligation
```

Each step is one logical event. The number does not represent seconds,
processor cycles, or wall-clock time.

## Read the five results

`always true` holds at all three events.

`eventually false` has no event where its inner statement becomes true. The
evaluator returns the first step from which the claim fails.

`next true` needs another event. A one-event trace ends at step 0, so the claim
fails there. A two-event trace has step 1, where `true` holds.

The last line selects the unbounded mode. The built-in evaluator returns a
request for a proof. It never turns a finite run into a claim about every
future event.

## See both modes in `.zeno`

The Mini Determinator authoring example contains:

```zeno
claim 500 finite_state_reflexivity all finite 4 =
  always atom(pre.100 == pre.100);
claim 501 unbounded_state_reflexivity lean unbounded =
  always atom(pre.100 == pre.100);
```

The equality in these two claims is reflexive. It gives a small, easy-to-read
example of the finite SMT translation and the unbounded Lean translation. The
Mini Determinator's schedule-independence result comes from the executable Rust
model, where opposite worker completion orders produce byte-identical runs.
Run `cargo +1.97.1 test -p zeno-fcis-spec mini_determinator --locked` to check
that behavior.

Check both declarations:

```bash
cargo +1.97.1 run --quiet -p zeno-fcis-cli --locked -- \
  check examples/mini-determinator/project.zeno
```

The first claim may be evaluated on a nonempty trace of at most four events.
The second creates a Lean proof request.

## Where these claims help

Finite claims are useful for bounded workflows such as:

- an accepted command eventually emits an outbox item within four events;
- a rejected command leaves state unchanged at the next event;
- a lock remains held until a release event;
- every event in a retained replay keeps a balance nonnegative.

Overflow, missing observed data, missing named calculations, an empty trace,
and exceeded limits return `Indeterminate`. A release or deployment remains blocked.

The [temporal reference](../TEMPORAL_LOGIC_V1.md) defines `next`, `always`,
`eventually`, and `until`, including every final-step rule and limit. The
[formal-tools tutorial](FORMAL_TOOLS.md) shows how finite claims reach CVC5 or
Z3 and how unbounded claims reach Lean.
