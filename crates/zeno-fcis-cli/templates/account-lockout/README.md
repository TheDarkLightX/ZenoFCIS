# Generated account lockout

This local, non-value-moving application locks one account after repeated
failed logins. It shows five patterns:
- a failed login is a *committed failure*: the login fails, but the attempt is
  recorded;
- time is an input: every request carries `now` in its context, and the
  decision never reads a clock;
- authority comes from the context: only a request marked `admin` may unlock;
- security alerts leave through the outbox: each stays pending until the
  destination acknowledges it, and the destination recognizes a repeated
  delivery by its ID;
- *declared ranges and an inductive claim*: `project.zeno` declares the range
  of each integer type, which is the schema's bounds, and claim 600 states
  that the action laws alone keep the lock invariant on every committed
  account. CVC5 attests the induction step over those ranges, and
  `tests/claims.rs` checks the rest of the argument.

The decision in `src/program.rs` is written by hand. The law checker in
`src/laws.rs` evaluates the formulas in `project.zeno` against every decision
the program makes, and refuses any decision that breaks them.

`project.zeno` owns record fields, integer ranges, command variants, reason
order, channel types, relational formulas, and the claim. `build.rs` supplies
the text bound and catalog meanings, then checks and generates the schema and
project bindings; it binds no integer, because lowering takes each range from
its declaration and refuses a binding that states other bounds. `profile.rs`
binds the exact source and the runtime-only law manifest.

## The rules

The policy is three consecutive failed logins, then a lock of 900 seconds.

The account has three fields:

| Field | ID | Meaning | Range |
| --- | --- | --- | --- |
| `failed_attempts` | 110 | consecutive failures since the last login, unlock, or lock | 0 through 2 |
| `locked_until` | 111 | logins are refused while `now` is earlier than this | 0 through 4,102,445,700 |
| `last_seen` | 112 | the time of the last committed decision | 0 through 4,102,444,800 |

Times are Unix seconds, from 0 through 4,102,444,800 (2100-01-01T00:00:00Z)
inclusive; a request's `now` has the same range, and a deadline reaches at
most one lock past it. `project.zeno` declares each range on its type
(`Attempts`, `UnixTime`, and `LockDeadline`), and the schema refuses a value
outside it before any decision. The account starts with every field at zero.
Each request is one of three commands, and carries `now` (field 130) and
`admin` (field 131) in its context. The shell checks the password; the
command reports only the verified result.

The rules apply in this order, and the first that applies decides:

1. If `now` is earlier than `last_seen`, the request is rejected with
   `clock_regressed` (200). Time never moves backwards, so a stale clock
   cannot end a lock early.
2. `LoginSucceeded` (120) or `LoginFailed` (121) while `now` is earlier than
   `locked_until` is rejected with `account_locked` (201). A request at exactly
   `locked_until` is no longer locked.
3. `AdminUnlock` (122) without `admin` is rejected with `not_admin` (202).
4. `LoginSucceeded` is accepted: `failed_attempts` becomes 0 and `last_seen`
   becomes `now`.
5. `LoginFailed` commits failure `login_failed` (203), and `last_seen` becomes
   `now`. If it is the third failure in a row, `failed_attempts` returns to 0,
   `locked_until` becomes `now` + 900, and one `Locked` alert with that deadline
   is queued. Otherwise `failed_attempts` increases by one.
6. `AdminUnlock` with `admin` is accepted: `failed_attempts` and `locked_until`
   become 0, `last_seen` becomes `now`, and one `Unlocked` alert is queued.

A field that a rule does not mention keeps its value: a login after a lock
expires, for example, leaves the expired `locked_until` in place. A rejection
changes nothing and queues nothing. Alerts go to `security-team` on channel
300, and each carries its kind (150 `Locked` or 151 `Unlocked`) and the lock
deadline after the decision: `now` + 900 for `Locked`, and 0 for `Unlocked`.

## What is checked

Every decision is checked at run time, before it can be published:
- law 500: `locked_until` is never more than 900 seconds past `last_seen`, and
  while `locked_until` is later than `last_seen`, the failure count is 0;
- law 501: a successful login clears the count and keeps the deadline, and a
  failed login is never accepted;
- law 502: an unlock requires `admin` and clears the lock and the count;
- law 503: a failed login is recorded, and the third in a row locks the
  account for 900 seconds;
- in Rust, because formulas cannot see them: the rejection reason, the exact
  alert, the absence of effects, and a zero genesis.

Law 500 is also claimed by induction, for `zeno-fcis prove`:

```text
claim 600 lock_state_stays_consistent all inductive accept [501, 502] failure [503] = pre.100.111 <= pre.100.112 + 900 && (pre.100.111 > pre.100.112 -> pre.100.110 == 0);
```

Claim 600 states law 500 over the account before a decision, and assumes
laws 501 and 502 on accepts and law 503 on committed failures, which is
where the law manifest enforces them. An induction step asks whether any
transition the assumed laws admit can take an account that satisfies the
invariant to one that does not. The step assumes what admission guarantees:
the command is one of its three variants, `admin` is 0 or 1, and each
integer lies in the range `project.zeno` declares for its type. CVC5
answers `unsat`: no such transition exists. That answer is attested, not
independently checked, and it says nothing about this application until
`tests/claims.rs` checks that:
- claim 600 restates law 500 exactly, over the account before a decision;
- the law manifest enforces laws 501 and 502 on accepts and law 503 on
  committed failures, as the claim assumes, and refuses a law assumed
  outside its scope;
- the law checker's own observer, `laws::state_observations`, reads every
  field the invariant reads, and on each of 129 admitted accounts, a grid
  across every boundary the invariant compares, the invariant has a definite
  value, the one the words above give it;
- the invariant holds on the exact genesis account that a new database
  stores;
- the declared ranges are the bounds the generated schema enforces, every
  field the invariant reads has one, and they agree with the program: the
  count locks at the range's end plus one, and a deadline reaches at most
  one lock past the latest time.

`tests/conformance.rs` also evaluates the invariant before and after each of
the 295 decisions in its grid that commit, and each of the 12 committed
examples, through the same observer.

The declared ranges are what makes the step hold. Without them, the step
assumes only that each integer fits in 128 bits, and CVC5 finds a
transition the schema never admits: a login 899 seconds below the top of
that range, after which `last_seen + 900` overflows and the invariant has
no value. Z3 finds another, a failed login from `failed_attempts` 3 at a
negative time, from which law 503 constrains only `last_seen`. With
`Attempts` widened to `0..=3`, a failed login from 3 to a deadline of 901
refutes the step, and two tests fail: the ranges no longer agree with the
program, and the schema admits a count of 3.

A rejection publishes nothing by construction: a rejected decision carries no
patch, effect, or outbox entry, and the law framework records law 509 as
satisfied for every rejection. That law has no formula, so `profile.rs`
registers it instead of `project.zeno` carrying an always-true placeholder.
As a result, this passes:

```sh
zeno-fcis check project.zeno --require-substantive --require-resolved-paths
```

The tests check the running application:
- `tests/conformance.rs` runs every decision through admission, the
  authority, the program, the law checker, the committed patch, and the
  outbox. It compares 20 examples in `tests/decision-examples.txt` and 606
  grid inputs (24 reachable states, times on both sides of every deadline,
  every command, both `admin` values) against the expected outcome, and
  evaluates claim 600's invariant before and after every decision that
  commits. It also checks the schema bounds and that only the zero account
  is a valid genesis.
- `tests/claims.rs` connects the induction step to the application, as
  described above.
- `tests/laws.rs` gives the law checker decisions a faulty program could
  make, such as a lock of the wrong length, a missing alert, or the wrong
  reason, and requires it to refuse each one. It also gives decisions that
  only a formula refuses, such as a login that moves the deadline, so a
  weakened formula fails a test.
- `tests/lifecycle.rs` runs the demonstration and shows that a lock survives
  a database reopen.
- `tests/determinism.rs` decides 120 requests eight times each through
  `execute_probed`, then again in three child processes with a changed
  environment, and requires every decision digest to match.

The grid's expected outcomes come from a reference model in the test, written
from these rules by the program's author. It catches binding and adapter
errors, not a misreading shared by both. The examples file is the check on
that: its header records who wrote the examples and whether the project's
owner has reviewed them.

The static check covers what the probe runs might not exercise, such as a
clock read:

```sh
zeno-fcis purity src/program.rs src/laws.rs
```

All of these are detectors: agreement shows that these runs matched and that
no error-level rule fired, not that the application is deterministic or
correct for every input. The induction step covers every value in the
declared ranges, but rests on the solver's unchecked `unsat`.

## How ZenoFCIS built and checked this

Every step below was run with `zeno-fcis` 1.1.0 built from this checkout
with Rust 1.97.1, and the pinned solvers CVC5 1.3.3 and Z3 4.16.0. Lean
4.30.0 is pinned in the same tools manifest, but there is no Lean export for
inductive claims, so it was not run. Commands run from this application's
directory.

1. `zeno-fcis check project.zeno --require-substantive
   --require-resolved-paths` checked the project: 4 laws and 1 claim, every
   formula able to constrain a transition, every path declared.

   ```text
   $ zeno-fcis check project.zeno --require-substantive --require-resolved-paths
   checked project.zeno: project=1 components=1 claims=1 unresolved_obligations=2 semantic_program_hash=2921c4fda435c50d718a3bfb019693da5a17ca104a1fea31d7a56004e90261d4
   ```

2. `cargo +1.97.1 build` ran `build.rs`, which lowered the schema from the
   declared ranges and the text bound, and generated the typed bindings in
   `OUT_DIR`. Nothing in `src/` restates what generation provides.

3. `zeno-fcis purity src/program.rs src/laws.rs` reported
   `purity: clean (0 errors, 0 warnings) in 2 files`.

4. Proofs. The solvers are not part of the default gate. They need a tools
   manifest naming the installed binaries by path, version, and SHA-256; the
   library's `docs/FORMAL_TOOLS_RC3.md` gives its schema. `prove` keeps every
   run's exact input and output under `.zeno-fcis/evidence/` next to the
   project, so these runs were made on a copy of this directory.

   ```text
   $ zeno-fcis doctor --tools zeno-fcis.tools.json
   cvc5 1.3.3 e8d7870d57ab55e81619d2373b043da05ea1d37ca393931bdb5d8b9788cd64c4
   z3 4.16.0 e583c4186a45e72411fa2cb2048401eed03f0f8e5f24694676a8f6271a50b765
   lean 4.30.0 3e0d0d3d801675359f2d4cf9815bfdb417b20b92fdd9d48b3b14c95bbae28bbf
   ```

   CVC5 answered `unsat` for the induction step. ZenoFCIS classifies that
   as a proposal and exits with code 2, because the proof output is not
   checked independently: the step is attested, not kernel-checked. The
   scope line says what else the application must check.

   ```text
   $ zeno-fcis prove project.zeno --claim 600 --backend cvc5 --tools zeno-fcis.tools.json
   cvc5 claim 600: UNSAT proposal retained; proof output was not independently checked
   cvc5 claim 600 assumes laws [] on every commit, [501, 502] on accepts, [503] on committed failures
   cvc5 claim 600 scope: every transition that satisfies the assumed laws preserves the invariant; it holds on every committed state only when the application also observes every value the invariant reads, checks the invariant on its exact genesis state, and enforces each assumed law on the decisions the claim assumes it on
   (exit 2)
   ```

   Z3 also answered `unsat`. RC3 has no Z3 proof checker, so the run is
   recorded as blocked evidence, with exit code 2. There is no Lean export
   for inductive claims, so `--backend lean` selects nothing.

   ```text
   $ zeno-fcis prove project.zeno --claim 600 --backend z3 --tools zeno-fcis.tools.json
   z3 claim 600 blocked: UnsupportedEvidence
   z3 claim 600 assumes laws [] on every commit, [501, 502] on accepts, [503] on committed failures
   (exit 2)
   $ zeno-fcis prove project.zeno --claim 600 --backend lean --tools zeno-fcis.tools.json
   claim 600 does not select compatible lean
   no compatible claim/backend pair was selected
   (exit 2)
   ```

   As a control, the same command on a copy whose three integer types
   declare no range printed, for CVC5:

   ```text
   cvc5 claim 600: replayed counterexample retained; the claim has no value there (overflow)
   cvc5 claim 600 assumes laws [] on every commit, [501, 502] on accepts, [503] on committed failures
   cvc5 claim 600 scope: a transition that satisfies the assumed laws breaks the invariant; its starting state may be unreachable, so strengthen the invariant or assume more laws
   (exit 1)
   ```

   and, for Z3, `z3 claim 600: replayed counterexample retained` with the
   same scope line and exit code. Each counterexample is retained as
   `counterexample.json`: CVC5's is a login at time 2^127 − 900 from an
   account last seen at 2^127 − 901, after which `last_seen + 900` overflows;
   Z3's is a failed login from `failed_attempts` 3 at time −902. The schema
   admits neither.

5. `zeno-fcis graph project.zeno --format mermaid` prints one node,
   `c400[account]`: the project has a single component, so no diagram is
   included here.

What the claim says, and what it does not. Claim 600 says: take any account
within the declared ranges that satisfies law 500's invariant, any command
and context whose values are declared, and any decision that satisfies laws
501 and 502 if it accepts or law 503 if it commits a failure; then the
account after the decision satisfies the invariant too. Together with the
checks in `tests/claims.rs` and the authority's refusal of any decision that
breaks an enforced law, every account this application commits keeps its
lock within 900 seconds of its last decision and counts no failures while
locked.

It does not say:
- anything about an input the authority did not check: `now` and `admin`
  are trusted inputs, and a deployment must supply them itself;
- that the laws are the rules: a law that under-describes the program gives
  counterexamples, and one that over-describes it refuses decisions at run
  time. `tests/conformance.rs` is the check that the laws and the program
  agree, on every input of its grid;
- that the step is proved: CVC5's `unsat` is attested; `prove` exits 2, and
  the proof text is not checked. Z3's `unsat` is recorded as unsupported
  evidence, and there is no Lean export for inductive claims;
- anything outside the declared ranges: the step assumes them. That no
  committed account leaves them is the schema's doing, which the authority
  checks at admission, at genesis, and on every transition.

Policy identity. Declaring the ranges and the claim changed the canonical
bytes of `project.zeno`, so `check` prints a new semantic program hash: the
previous version of this template printed
`dc9109009626bd19beace495618ceadad10aeb2c5c5de5ecbf609dd01f629ff8`, and this
one prints the hash in step 1. The source hash in `profile.rs`, and the
program, checker, and policy hashes derived from it, changed with it: as the
note below says, a database created by the previous version cannot be
carried over without a separately reviewed migration. The program, the
reference model, the examples, and the schema's bounds are unchanged, and the
demonstration prints the same summary. Putting the SQLite shell behind the
`sqlite` feature then changed `Cargo.toml`, `src/lib.rs`, and `src/delivery.rs`,
which the source hash also covers, so the source, program, checker, and policy
hashes changed once more; `project.zeno` did not, so the semantic program hash in step 1
stands.

## Run this development candidate

This template depends on the current ZenoFCIS checkout. From that checkout,
run:

```sh
python3 tools/check_generated_application.py
```

That gate creates a fresh application using the CLI, patches its dependencies
to the exact checkout, checks dependency versions against the workspace lock,
then compiles and runs its tests and demonstration as an isolated package.
The solvers are not part of the gate; step 4 above lists their commands.
With these development dependencies available in a standalone checkout:

```sh
cargo +1.97.1 test --locked
cargo +1.97.1 run --locked -- new-account.sqlite
```

The SQLite shell is the `sqlite` feature, on by default. Without it,
`cargo +1.97.1 build --no-default-features` builds the core alone: the
generated bindings, the program, the law checker, the profile, the delivery
adapter, and `authority()`, with no database; the gate checks that it also
compiles for `wasm32-unknown-unknown`. `create`, `invoke`, `journey`, and the demonstration
binary need the feature.

The demonstration requires a new database path. It locks the account, refuses
logins during the lock, a stale clock, and an unlock without `admin`, lets a
login through when the lock expires, unlocks as an administrator, then
interrupts alert delivery, reopens the database, and finishes delivery. It
prints a JSON summary.

The destination keeps an in-memory idempotency ledger, which survives the
database reopen only within one process. A real destination must persist its
delivery IDs and entry hashes. The context's `now` and `admin`, and the
principal, are trusted tutorial inputs, not remote authentication: a
deployment must authenticate them before admission. Source hashes identify
reviewed example policy, not certified binaries or release evidence. Changing
the source changes policy identity and requires a new database unless a
separately reviewed migration is implemented.
