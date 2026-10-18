# Task packet P1: deterministic fault simulation for the SQLite shell (single store)

**Branch:** `agent/2.6-shell-simulation` **Size:** large **Evidence level of the result:** tested

## Why (the user's story)
An application built with ZenoFCIS keeps its state, its commit receipts and its outbox of pending deliveries in one
SQLite file (`crates/zeno-fcis-shell-sqlite`, the V2 store). Its operators need to know four things:
- a crash, a busy database or a full disk never leaves a partial commit;
- an outbox entry is never lost, and never consumed by the wrong acknowledgement;
- a retry with the original key returns the original answer, and never applies the command twice;
- a lost reply can be recovered by asking again with the original key.

Today this rests on seven injected crash points tested against an in-memory store, on the same handle. This task
adds three things:
- a deterministic simulator that explores fault schedules from a seed;
- an independent protocol model that each run is compared against, step by step;
- real child-process kill-and-reopen tests on a file database.

It is test evidence, not a proof, and the report must say so.

## Facts about the current code (verified on the base commit)
- **Store API.** `crates/zeno-fcis-shell-sqlite/src/v2.rs`:
  - `V2SqliteShell` at :559. `create` :570, `create_in_memory` :578, `open` :661 (it refuses a missing file and never
    creates one), `open_at_checkpoint` :665.
  - `commit(replay_id, publication)` :851, `commit_at(expected_version, ...)` :859 and `commit_bounded_at` :882, all
    through the private `commit_inner` :927.
  - `audit` :754, `checkpoint` :801, `snapshot` :818 (`Snapshot::pending()` :207), `publication_bundle` :1070.
  - The private `synchronize` :787 re-audits when `PRAGMA data_version` changes. Every public operation calls it
    first.
- **Replay.** There is no separate replay lookup. Committing again with the same `replay_id` and identical bytes
  returns `CommitStatus::IdempotentReplay` (v2.rs:958-980; the enum is at `crates/zeno-fcis-shell/src/lib.rs:196`).
  Different bytes under the same key return `Error::Replay`.
- **Outbox.** `src/v2/delivery.rs`:
  - `next_pending` :40, `pending_by_id` :78;
  - `Pending::deliver(&mut MemoryDestination)` :316 and `Pending::relayed(payload_sha256)` :355;
  - `Delivered::acknowledge` :522, which rechecks inside `BEGIN IMMEDIATE`.
- **Relay.** `src/v2/relay.rs`: `export_pending` :64, ordered by sequence, lane and ordinal; `acknowledge(shell, id,
  payload_sha256)` :113. Transport is at least once by design (:28-39): a relay restarted between send and
  acknowledgement sends again with the same delivery ID and payload.
- **Schema** (v2.rs:75-133, version 10): `v2_genesis`, the singleton `v2_state`, `v2_commits` (unique replay_id,
  unique chain), `v2_deliveries` (unique delivery_id, `acknowledged IN (0,1)`), `v2_checkpoints`, `v2_upgrades`.
- **The linearization point** is one IMMEDIATE transaction (v2.rs:943-945 to the commit at :1033). Inside it:
  1. the state compare-and-swap UPDATE at :1007, which returns `Error::Concurrent` unless exactly one row changes;
  2. the commit-row INSERT at :1012-1027;
  3. the outbox rows at :1029.

  Before the CAS, a cached-anchor check also returns `Error::Concurrent` (:952-954).
- **Pragmas** (`configure`, v2.rs:1952-1961): `foreign_keys=ON`, `synchronous=EXTRA`, and the default rollback
  journal (no `journal_mode` is set). No busy timeout is set in production code.
- **Crash injection.** `CrashPoint` (src/lib.rs:12-32) has seven points: BeforeTransaction, AfterValidation,
  AfterStateWrite, AfterReplayWrite, AfterOutboxWrite, BeforeCommit, AfterCommit. They are public only with the
  `crash-injection` feature. `inject` (v2.rs:2379) returns `Error::InjectedCrash` and the dropped transaction rolls
  back. The `*_with_crash*` commit variants take `Option<CrashPoint>`. The existing test is
  `every_injected_commit_interruption_is_atomic` (tests/v2.rs:171-200).
- **No clock, RNG or transport trait exists** in the shell crates. The only destination trait is
  `IdempotentDestination` (`crates/zeno-fcis-shell/src/delivery.rs:16-28`, with `MemoryDestination` :34).
  `Pending::deliver` takes the concrete `MemoryDestination`. A custom receiver must use `relayed` and
  `acknowledge`.
- **No child-process kill test exists** for the store. Temporary files in tests use `std::env::temp_dir()` and a Drop
  guard (tests/v2.rs:121-135).
- **Dependencies.** The crate's dev-dependencies are rusqlite `=0.40.1` (with `hooks`) and sibling workspace crates.
  There is no proptest, quickcheck or tempfile, and you must not add them.
- **The existing pure reference model** is in `crates/zeno-fcis-shell/src/lib.rs` (`apply_reference_bundle` :208,
  `acknowledge` :326). It is product code, so it does not count as the independent model below.

## What to build
Everything goes under `crates/zeno-fcis-shell-sqlite/tests/` as new files: a `simulation.rs` integration test, a
`sim/` support module and `sim/fixtures/`. You may add one test-only `[[test]]` entry with
`required-features = ["crash-injection"]` to the crate's Cargo.toml. Change no production code, except one optional
addition: a busy-timeout setter behind the existing `crash-injection` feature, if the busy fault needs it. Justify it
in the report.

1. **A deterministic world.**
   - A seeded PRNG written in the test code (SplitMix64 or xoshiro256**, about 20 lines, with a published test
     vector).
   - A logical clock (a counter).
   - A scheduler that, from one `u64` seed, picks each step's action and fault.
   - The same seed always gives the identical schedule, events and final bytes. Test that by running each seed
     twice.
2. **Actions:**
   - commit a fresh command (from the store's test fixtures, as tests/v2.rs builds them);
   - retry the same key with the same bytes;
   - retry the same key with changed bytes;
   - commit at a stale expected version from a second handle;
   - export pending;
   - deliver to a receiver;
   - acknowledge (right, wrong-payload, duplicate, unknown ID);
   - reopen the store;
   - audit.
3. **Storage faults:**
   - each of the seven crash points;
   - a busy database: hold a write lock on a second connection;
   - an I/O error or disk full, if it can be made deterministic. For example, use `PRAGMA max_page_count` on a
     second connection to force SQLITE_FULL. If a fault cannot be made deterministic, list it as not covered.
4. **Transport faults** between the relay and a simulated receiver:
   - drop;
   - duplicate;
   - reorder;
   - a lost acknowledgement reply, where the receiver applied the delivery but the ack never reached the store.

   The receiver keeps a durable deduplication table keyed by delivery ID, as a separate SQLite file. It applies an
   effect and records the ID in one transaction.
5. **An independent protocol model:** pure Rust, in the test tree. It must not call any store or reference-model
   logic; it may import plain data types and hash helpers. Its state is:
   - durable: sequence, state bytes, root and chain head, replay records (key to command, context and receipt),
     pending and acknowledged deliveries;
   - volatile: an open transaction, which a crash discards.

   Each action has a model transition giving the expected outcome: committed, idempotent replay, or a named
   refusal.
6. **The comparator.**
   - After every step, read a complete consistent snapshot of the real database: all v2_* tables in one read
     transaction.
   - Project it to the model's abstract state, and compare. Also compare the outcome and the event-trace entry:
     request, replay key, decision, commit sequence and delivery IDs.
   - Any difference fails with the seed, the step index, both states and a minimised schedule.
7. **The invariants**, checked on the model and on the projected real state after every step:
   - **I1.** The state equals the fold of the committed history from genesis.
   - **I2.** A replay key maps to exactly one applied command. A replay never applies twice.
   - **I3.** Every committed command's required outbox entries exist, exactly, with no extras.
   - **I4.** An acknowledgement consumes only the matching (ID, payload hash) record. Acknowledged is monotone.
   - **I5.** Publication is atomic: after any fault, either all of state, commit row and outbox rows exist, or none.
   - **I6.** Under the durable deduplicating receiver, each delivery ID has at most one effect.

   Without deduplication, show the duplicate effect as the stated limit of at-least-once transport, as a passing
   test that asserts the duplicate is possible.
8. **The unknown outcome.**
   - A commit whose result is lost (AfterCommit, or a kill just after COMMIT) must be recoverable by retrying with
     the original key, which returns `IdempotentReplay` and the original receipt.
   - Test that a retry under a new key can apply twice, as the documented limit.
   - Test that absence in a stale read is not treated as proof of non-commit.
9. **Schedules.**
   - A default corpus of 256 seeds × 64 steps must finish in under 90 s in a debug build.
   - A long run (4,096 seeds × 128 steps) sits behind `#[ignore]`.
   - Failing schedules are minimised by delta debugging over the step list and saved under `sim/fixtures/` as
     regression cases, replayed by a normal test. Seeds and schedules are plain JSON.
10. **Kill and reopen.**
    - The test binary re-executes itself (`std::env::current_exe()` with an environment variable naming the role).
    - The child commits on a file database and stops itself at a chosen point with `std::process::abort()`. The
      points are: before BEGIN, after each statement inside the transaction (use the crash points), right after
      COMMIT returns, and between delivery and acknowledgement.
    - The parent waits, reopens with `open`, audits, compares against the model, and checks recovery by the
      original key.
    - Keep these separate from injected-error rollback, and name them separately in the report. Neither is
      power-failure qualification.
11. **The freshness note.** Disabling `synchronize()` alone may still be caught by the in-transaction anchor check.
    A double-apply control must reach the CAS or the binding predicate. Otherwise, classify it as an availability
    control and say so.

## Controls (planted defects)
Write `tools/check_shell_simulation_mutations.py`, following `tools/check_bounded_mutations.py`. That tool copies the
tree, applies one textual mutation, builds, and runs the named test; a build failure is not a kill.

| Mutation | Must fail |
|---|---|
| Skip the outbox INSERT for the second entry | I3 or I5 |
| Insert one extra outbox row | I3 |
| Remove the `sequence=?5` term from the CAS WHERE clause | the stale-writer test |
| Ignore the payload hash in `acknowledge` | I4 |
| Treat changed bytes under the same replay key as an idempotent replay | I2 |
| Mark acknowledged before the transaction commits | I4 or I5 |
| A comparator that always returns equal (patch the test code) | the comparator self-test |
| A receiver that records its dedup ID outside the effect's transaction | I6 under the lost-reply schedule |

The baseline must pass first. Record each mutation's result in the report.

## Done when
- The simulation, the kill-and-reopen tests and the controls all pass. The unmodified tree passes:
  - `cargo +1.97.1 test -p zeno-fcis-shell-sqlite --locked --features crash-injection`
  - `cargo +1.97.1 clippy -p zeno-fcis-shell-sqlite --all-targets --all-features --locked -- -D warnings`
- Every control in the table is killed by its named check.
- The default corpus meets its time limit. Record the time.
- `docs/SHELL_SIMULATION.md` (new; at most 120 lines) states, in plain words:
  - the user story;
  - what is simulated;
  - the fault model;
  - the invariants;
  - what is tested;
  - the premises: SQLite atomicity, locking and durability; the OS and file system; the hash implementations;
  - the non-claims: no power-failure qualification; transport is at least once; an ack is not proof of external
    delivery; rollback to an earlier consistent database is undetectable without an external anchor; one store
    only.

## Out of scope
- New server crates or HTTP ingress;
- multiple stores or distributed simulation;
- upgrade and lineage paths (list them as later work);
- performance work;
- any production behaviour change.

## Common rules (part of every packet)

**Base.** Start from `main` of github.com/TheDarkLightX/ZenoFCIS at commit `c951cd957a831326373c710efe6165ec0f6bb8a3`
(release 2.4.0 plus its status notes). Work on one new branch named in the packet. A later release line (2.5) will
be merged under this work, so keep changes additive:
- prefer new files and new modules;
- keep edits to existing files small and local;
- do not reformat files you did not change (run `rustfmt` on changed files only, never `cargo fmt --all`);
- avoid these paths unless the packet names them: `crates/zeno-fcis-cli/src/`, `crates/zeno-fcis-synthesis/src/finite/execution_v2/`,
  `crates/zeno-fcis-contract/src/`, `crates/zeno-fcis-loop-doors/`, `core-apps/`, `verification/kernel-laws/`, `tools/atdd.py`.

**Where this packet lives.** The packets are on the branch `tasks/2.6-agent-packets` under `docs/agent-tasks/2.6/`.
Start your work from `main` (or from that branch, which adds only the packets). Do not edit the packet files.

**Before editing.** Read `AGENTS.md` and `skills/handoff-completion-gate/SKILL.md` and follow them. State the
definition of done and the checks you will run, at the top of your final report.

**Toolchain.** Rust 1.97.1, as pinned in `rust-toolchain.toml`. Build and test with `cargo +1.97.1 ... --locked`. Use
`CARGO_PROFILE_DEV_DEBUG=0` and at most `-j 2` if the container is small.
- Add no new crates to `Cargo.lock` unless the packet allows it. Never run `cargo update`.
- External tools (Lean, Verus, solvers) use only the exact versions and hashes the repository pins. Verify the
  hashes; never substitute a newer release.

**Forbidden commands.** `git reset --hard`, `git clean -f`, `git checkout --`, `cargo update`, `cargo publish`,
`git tag`, `gh pr merge` and `gh release create`. Do not edit release files, version numbers, `CHANGELOG.md`,
the top-level `README.md` or `docs/RELEASE_NOTES.md` (template READMEs named in a packet are fine). Do not run the full acceptance suite (`python3 tools/atdd.py run --all`);
it is the release gate and runs later on the maintainer's host.

**Proof hygiene.**
- Verus: no `assume`, `admit`, `#[verifier::external_body]`, `#[verifier::external]` or trusted specifications on
  the proved path. Run with `--no-cheating` where the repository does. Do not raise an rlimit beyond the
  repository's recorded convention.
- Lean: no `sorry`, `admit`, `axiom` or `unsafe`. The axiom report may list only the repository's allowed axioms.
- No `unsafe` Rust in new code.

**Controls.** Every new check ships with at least one planted defect: a small, realistic mutation that must make a
named test fail. Show that the mutation builds and that the named test fails for the stated reason. Then show that
the test passes on the real code. A build failure is not a caught defect.

**Evidence words.** State each claim at its true strength, with one of these levels:
- **tested**;
- **finite checked** (every case in a stated finite domain);
- **solver attested**;
- **kernel checked** (Lean kernel);
- **proved** (Verus, nothing assumed).

Name the scope, premises and trust basis next to each claim. A timeout, an incomplete run or a skipped case is never
a pass. Say "exhaustively checked over the stated domain", never "correct by construction".

**Public wording.** This repository is public. In code comments and documents:
- write plain English;
- describe generated code as "LLM-generated" or "synthesized", never "hand-written";
- never use the word "authoring";
- name no AI model, agent or vendor;
- include no local machine paths and no private project names.

**Deliverable.**
- One branch with focused commits. Each commit message ends with a line saying which checks were run.
- The pull request description is the final report (at most 60 lines):
  - definition of done;
  - what was built;
  - each claim with its evidence level;
  - every command run, with its exit code;
  - the controls table (mutation, named test, result);
  - what is not done, and why.
- List every new public document in `release/public-documents.txt` and link it from `docs/INDEX.md`; then
  `python3 tools/check_public_docs.py` and `python3 tools/check_library_docs.py` must pass.
- Open the pull request as a draft. Do not merge it.

