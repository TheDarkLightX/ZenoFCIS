# Goodstein: what “eventually” promises

This example separates a concrete termination witness, a finite observation
window, and a theorem about every starting value. It also explains why logical
steps and elapsed time need separate specifications.

## Run the example

From the repository root:

```bash
cargo +1.97.1 run --locked -p zeno-fcis-spec --example goodstein_eventually
cargo +1.97.1 test --locked -p zeno-fcis-spec --example goodstein_eventually
```

The example computes these genuine Goodstein prefixes:

```text
start 3: [3, 3, 3, 2, 1, 0]; zero witnessed at position 5
short prefix: [3, 3, 3, 2]; unbounded eventuality still pending
start 4: [4, 26, 41, 60, 83, 109]; horizon exhausted, no nontermination claim
```

Each transition writes the current integer in hereditary base notation,
increases the base by one everywhere, including inside exponents, and subtracts
one. For example, 4 is 2^2, so the first base change produces 3^3, then
subtracting one gives 26. Ordinary base conversion would give the wrong result.

The Rust computation uses checked `u128` arithmetic and at most 256 observed
values. Overflow stops the computation with an arithmetic limit. It does not
produce a mathematical successor or imply that the sequence never terminates.
The example prints fixed cases; it is not a general-purpose Goodstein runner.

## State the eventuality

At logical position i, `eventually P` means that P holds at some finite position
j with j >= i. The current position counts. There is no deadline in the formula.
For Goodstein, the general mathematical statement is:

```text
For every starting natural number n, there exists a finite k with G(n, k) = 0.
```

The witness k may depend on n. This does not give a fixed deadline for every n.

The paired claims in [project.zeno](project.zeno) use the same formula:

```zeno
claim 500 finite_zero all finite 6 = eventually atom(post.100 == 0);
claim 501 eventual_zero lean unbounded = eventually atom(post.100 == 0);
```

These are authoring declarations only. They do not define the Goodstein
transition relation, bind observations to the computed runs, or quantify over
every starting natural number. Parsing them proves neither claim. Checking a
formula over arbitrary observation assignments also does not prove anything
specific about Goodstein sequences.

The complete six-position run starting at 3 contains a zero witness. Its
four-position prefix does not. If that prefix is the entire finite trace,
`eventually zero` is false on that trace. If it is only an observation of an
ongoing run, the unbounded eventuality is still pending. A long prefix without
zero alone is not a counterexample to eventual termination.

## Check a progress theorem with Lean

With the example's pinned Lean 4.32.0 installed:

```bash
cd examples/goodstein
lean --version
lean Descent.lean
```

[Descent.lean](Descent.lean) imports only Lean's `Init`. It checks:

- `eventually_of_descent`: if every unfinished step strictly decreases a rank
  in a well-founded relation, some finite step is finished.
- `countdown_eventually_zero`: ordinary natural-number countdown reaches zero,
  with the explicit witness k = n.

The first theorem uses Lean's standard `propext`, `Classical.choice`, and
`Quot.sound`; the countdown theorem reports no axioms. The file prints both
dependency lists. It introduces no new axioms or unfinished proof terms.

The general progress theorem has explicit hypotheses. To apply it to all
Goodstein sequences, a formalization must define the mathematical sequence,
construct its ordinal rank below epsilon_0, prove strict descent whenever the
value is nonzero, and establish well-foundedness. Those Goodstein-specific
obligations are not supplied here. The Lean theorem also does not verify the
Rust implementation.

A program can check the full Goodstein proof in suitable foundations. The
general theorem is not provable in Peano arithmetic; this is a restriction on
that proof system, not an ambiguity in “eventually.” See
[Rathjen's account](https://arxiv.org/html/1405.4484v1). A separate
[Lean formalization](https://github.com/WilliamAngus/Goodstein) publishes the
ordinal argument; it has not been rebuilt or admitted by this example.

The current public ZenoFCIS Lean exporter returns `UnsupportedMode`. The
standalone Lean check above does not enable `prove --backend lean`, produce a
ZenoFCIS certificate, or qualify an application. See the
[formal-tools tutorial](../../docs/tutorials/FORMAL_TOOLS.md).

## Compare with Zeno's paradox

Goodstein termination requires finitely many transitions. Zeno-style timing
instead allows infinitely many events in a finite duration. For example, event
times t_k = 1 - 2^(-k) approach 1, but no finite event index reaches time 1.
On that infinite event trace, `eventually time >= 1` is false even though the
times have limit 1.

Timed systems therefore need explicit time-progress conditions where relevant.
Positive delays alone are insufficient: their infinite sum may be finite.
The existing temporal operators count logical positions; this example adds
neither clocks nor a timed-automata checker. See
[research on Zeno runs](https://arxiv.org/abs/1106.1850) and the
[temporal reference](../../docs/TEMPORAL_LOGIC_V1.md).
