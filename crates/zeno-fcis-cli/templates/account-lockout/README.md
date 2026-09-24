# Generated account lockout

This local, non-value-moving application locks one account after repeated
failed logins. It shows four patterns:
- a failed login is a *committed failure*: the login fails, but the attempt is
  recorded;
- time is an input: every request carries `now` in its context, and the
  decision never reads a clock;
- authority comes from the context: only a request marked `admin` may unlock;
- security alerts leave through the outbox: each stays pending until the
  destination acknowledges it, and the destination recognizes a repeated
  delivery by its ID.

The decision in `src/program.rs` is written by hand. The law checker in
`src/laws.rs` evaluates the formulas in `project.zeno` against every decision
the program makes, and refuses any decision that breaks them.

`project.zeno` owns record fields, command variants, reason order, channel
types, and relational formulas. `build.rs` supplies explicit scalar bounds and
catalog meanings, then checks and generates the schema and project bindings.
`profile.rs` binds the exact source and the runtime-only law manifest.

## The rules

The policy is three consecutive failed logins, then a lock of 900 seconds.

The account has three fields:

| Field | ID | Meaning | Range |
| --- | --- | --- | --- |
| `failed_attempts` | 110 | consecutive failures since the last login, unlock, or lock | 0 through 2 |
| `locked_until` | 111 | logins are refused while `now` is earlier than this | 0 through 4,102,445,700 |
| `last_seen` | 112 | the time of the last committed decision | 0 through 4,102,444,800 |

Times are Unix seconds, from 0 through 4,102,444,800 (2100-01-01T00:00:00Z)
inclusive; a request's `now` has the same range. The account starts with
every field at zero. Each request is one of three commands, and carries `now`
(field 130) and `admin` (field 131) in its context. The shell checks the
password; the command reports only the verified result.

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
  every command, both `admin` values) against the expected outcome. It also
  checks the schema bounds and that only the zero account is a valid genesis.
- `tests/laws.rs` gives the law checker decisions a faulty program could
  make, such as a lock of the wrong length, a missing alert, or the wrong
  reason, and requires it to refuse each one.
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
correct for every input.

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
cargo +1.97.1 run --locked -- new-account.sqlite
```

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
