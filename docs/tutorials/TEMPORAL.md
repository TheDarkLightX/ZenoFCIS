# Tutorial: run finite time claims

Run the executable walkthrough:

```bash
cargo +1.97.1 run --quiet -p zeno-fcis-spec \
  --example temporal_walkthrough --locked
```

It prints:

```text
two inert claims; runtime admission and publication have not run
```

The example parses and elaborates a two-claim project and checks each claim's
mode. Claim 2, `eventually true`, is finite with a horizon of three events.
Claim 3, `always true`, is unbounded. Elaboration evaluates neither claim.
`zeno-fcis prove` sends finite claims to CVC5 or Z3 (see the
[formal-tools tutorial](FORMAL_TOOLS.md)), and an unbounded claim becomes a
proof obligation.

Each step is one logical event. The number does not represent seconds,
processor cycles, or wall-clock time.

The [Goodstein example](../../examples/goodstein/README.md) shows why a finite
zero witness, an unfinished prefix, and a general termination theorem are
different claims. It includes a standalone Lean progress theorem and contrasts
logical steps with Zeno-style timing. The public Lean exporter is currently
blocked, as explained in the [formal-tools tutorial](FORMAL_TOOLS.md); parsing
an unbounded claim does not supply its proof.

## How finite claims are judged

A finite claim covers every nonempty trace of at most its horizon of events.

`always p` requires `p` at every remaining event.

`eventually p` requires one remaining event where `p` holds, so
`eventually false` fails on every trace.

`next p` needs another event. On a one-event trace it fails at step 0; on a
two-event trace it holds at step 0 when `p` holds at step 1.

An unbounded claim has no built-in evaluator. It creates a request for a
proof, and a finite run is never turned into a claim about every future event.

## See both modes in `.zeno`

The Mini Determinator authoring example contains:

```zeno
claim 500 finite_state_reflexivity all finite 4 =
  always atom(pre.100 == pre.100);
claim 501 unbounded_state_reflexivity lean unbounded =
  always atom(pre.100 == pre.100);
```

The equality in these two claims is reflexive, which makes them small,
easy-to-read examples of the finite SMT translation and the unbounded Lean
translation. The Mini Determinator's schedule-independence result comes from
the executable Rust model, where opposite worker completion orders produce
byte-identical runs. Run
`cargo +1.97.1 test -p zeno-fcis-spec mini_determinator --locked` to check that
behavior.

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
and exceeded limits return `Indeterminate`, and a release or deployment stays
blocked.

The [temporal reference](../TEMPORAL_LOGIC_V1.md) defines `next`, `always`,
`eventually`, and `until`, including every final-step rule and limit. The
[formal-tools tutorial](FORMAL_TOOLS.md) shows how finite claims reach CVC5 or
Z3 and how unbounded claims reach Lean.
