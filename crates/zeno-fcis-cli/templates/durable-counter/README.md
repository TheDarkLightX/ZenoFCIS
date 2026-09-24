# Generated durable counter

This local, non-value-moving application connects authored shapes, generated
Rust types, a synthesized pure step with a reviewed adapter, runtime law checks, nominal authorization,
SQLite publication, and an idempotent demonstration destination.

`project.zeno` owns record fields, command variants, reason order, channel
types, and relational formulas. `build.rs` supplies explicit scalar bounds and
catalog meanings, then checks and generates the schema and project bindings.
`profile.rs` binds the exact source and runtime-only law manifest. Generated
artifacts and their manifests remain inspectable under Cargo's build output.

Both counters start at zero and are bounded by three. `Increment` increases
only `count` and accepts. `RecordFailure` increases only `failures` and returns
committed failure 202. A false context rejects with reason 200; a full selected
counter rejects with reason 201. Denial takes precedence over capacity. Each
committing result queues exactly one notification of the new state.

The law checker evaluates the authored formulas against the exact pre-state,
post-state, numeric command tag (120 or 121), and Boolean context (0 or 1).
It separately checks the complete notification, empty commit effects, failure
reason, rejection conditions, and zero genesis. Missing predicates and
indeterminate evaluation fail closed. External proof submissions receive no
authority.

## Run this development candidate

This template depends on the schema-lowering and invocation-binding APIs in
the current ZenoFCIS checkout. The RC3 package version has not been released
again. From that checkout, run:

```sh
python3 tools/check_generated_application.py
```

That gate creates a fresh application using the CLI, patches its dependencies
to the exact checkout, checks dependency versions against the workspace lock,
then compiles and runs its tests and demonstration as an isolated package.
With these development dependencies available in a standalone checkout:

```sh
zeno-fcis check project.zeno
cargo +1.97.1 test --locked
cargo +1.97.1 run --locked -- new-counter.sqlite
```

The demonstration requires a new database path. It checks acceptance,
rejection without publication, committed failure, exact duplicate replay,
database reopen, and retry after delivery but before acknowledgement.

The destination retains an in-memory idempotency ledger across the database
reopen in one process. It demonstrates the required remote contract; it does
not provide durable remote delivery across process loss. A real destination
must persist its delivery IDs and entry hashes. The local Boolean context and
principal are trusted tutorial inputs, not remote authentication. Source hashes
identify reviewed example policy, not certified binaries or release evidence.
Changing the source changes policy identity and requires a new database unless
a separately reviewed migration is implemented. No Lean installation is used.

## Check the application against its model

`tests/conformance.rs` runs every admitted input (16 states, two commands, and
two contexts) through the application: admission, the authority, the adapter
in `src/program.rs`, the law checker, the committed patch, and the outbox. It
compares each decision, reason, new state, and notification with:

- the synthesized function, read through the output table documented in the
  test;
- `tests/decision-examples.txt`, examples keyed by the numeric IDs in
  `project.zeno` and checked against this README by a reviewer other than
  their author. The file's header records how they were written and
  reviewed.

It also checks that schema admission and the finite input domain agree in both
directions, that genesis is exactly zero, and that every admitted state is
reachable from genesis. Swapping the count and failure bindings in the adapter
fails these tests. The project's owner accepted the examples on 2026-09-23.

## Check by induction that the counters never go negative

Claim 600 in `project.zeno` states an invariant, and names the laws its
induction step may assume:

```text
claim 600 counters_never_negative all inductive accept [501] failure [502] = pre.100.110 >= 0 && pre.100.111 >= 0;
```

Law 501 describes an accepted increment, and law 502 a committed failure. The
authority refuses any decision that breaks the law enforced on its kind, so
these two laws bound every transition the counter can commit.
`zeno-fcis prove` checks the step: every transition that satisfies them, and
starts with both counters nonnegative, ends with both nonnegative. It checks
this over the full integer range, not only the 16 admitted states. With the
pinned CVC5:

```sh
zeno-fcis prove project.zeno --claim 600 --backend cvc5 --tools zeno-fcis.tools.json
```

It prints `UNSAT proposal retained` and exits 2, as it does for every solver
result: a solver's `unsat` is attested, not independently checked. Run it on a
copy, because `prove` keeps its records in `.zeno-fcis/evidence` next to the
project file.

The step says something about this application only with more checks,
which `tests/induction.rs` runs:
- the law checker observes the state before and after every decision through
  one function, `state_observations`, which reads every field the invariant
  reads. The invariant has a definite value on every admitted state;
- the invariant holds on the exact genesis state that a new shell stores, as
  that observer sees it;
- the law manifest checks law 501 on accepts and law 502 on committed
  failures, as the claim assumes.

The same step fails for `count <= 3`: from 3, an accepted increment reaches 4.
The cap comes from the bound law 500 and the program's capacity check, not
from the action laws. See the library's guide to inductive claims.

## Check determinism

`tests/determinism.rs` decides every admitted input eight times through
`execute_probed`, which withholds a decision whose runs disagree. It then
decides the corpus again in three child processes: one with a cleared
environment, one with another time zone, locale, and allocator fill, and one
in another working directory. Every decision digest must match. The static
check covers sources that these runs might not exercise:

```sh
zeno-fcis purity src/program.rs src/laws.rs synthesized/transition.rs
```

Both are detectors. Agreement shows that these runs matched and that no
error-level rule fired; it does not prove the application deterministic. See
the library's determinism guide.

## Inspect and replay synthesis

`synthesis.json` independently specifies all 64 combinations of current state,
command, and context. The grammar selects a field, guard limit, and update step.
The checked-in `synthesized/transition.rs` is the actual function used by
`src/program.rs`. The adapter maps its complete decision and notification plan
through typed staging; the existing law checker still validates every actual
decision. Source bindings include the specification and emitted artifacts.

```sh
zeno-fcis synth run synthesis.json --out synthesized --check
zeno-fcis synth verify synthesis.json --out synthesized --receipt rust-conformance.json
zeno-fcis synth run synthesis.json --target python --out python-step
zeno-fcis synth verify synthesis.json --target python --out python-step --receipt python-conformance.json
zeno-fcis synth run synthesis.json --target javascript --out javascript-step
zeno-fcis synth verify synthesis.json --target javascript --out javascript-step --receipt javascript-conformance.json
```

Emission and runtime conformance have separate results. Keep receipts outside
artifact directories. Rust, Python and JavaScript satisfy the same finite
relation; these checks do not establish completeness of arbitrary requirements
or properties outside the declared bounds. Automatic replay uses Linux and the
existing Rust 1.97.1, Python 3 or Node.js 22 installation. The JavaScript module
accepts and returns primitive strings of canonical decimal integers separated
by single spaces, using exact `BigInt` arithmetic internally. It returns `null`
on invalid input or an arithmetic trap. Discovery describes each target's ABI.
