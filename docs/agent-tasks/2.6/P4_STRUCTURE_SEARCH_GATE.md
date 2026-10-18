# Task packet P4: data-structure search, first feasibility gate

**Branch:** `agent/2.6-structure-search-gate` **Size:** extra large (the required part is large; the stretch part is
marked) **Evidence level of the result:**
- the profile checker and the gates: tested;
- the simple seed: proved (Verus);
- the balanced seed: tested and finite checked (proved only if the stretch goal lands).

## Why (the user's story)
A developer asks their LLM in one line: "find me a persistent order queue for our matching engine: cancel and
reprice by order ID, always pop the best price first, and it must never be slow under an attacker". ZenoFCIS should
answer with a data structure that is:
- **checked exactly** against what the queue must mean;
- **measured** on the workload and under adversarial traces;
- **proved** for the finalist.

Candidates come from untrusted proposers (a library of seeds, design moves, LLM proposals). So the whole loop rests
on four things:
1. a candidate language small enough to check and prove;
2. a deterministic cost meter that candidates cannot bypass;
3. an exact meaning to compare against;
4. gates that turn every failure into a concrete counterexample.

This packet builds those four for one structure, a cancel-and-reprice priority queue, and shows that the proof lane
works on the same executable source. It is the feasibility gate that decides whether the search loop goes ahead in
this form.

## The design decisions already made (do not reopen)
- **The candidate language is a checked subset of Rust,** proved with the pinned Verus, using metered primitives
  that ZenoFCIS supplies. There is no new language or interpreter. Proof annotations use the repository's
  `#[cfg_attr(verus_keep_ghost, verus_spec(...))]` style, so normal builds and tests run exactly the same body that
  Verus checks.
- **Persistence:** every version is an immutable value, built by path copying over `Arc` nodes. No `Arc::make_mut`.
  Old versions must stay usable and unchanged. A forked or branching trace (an operation applied to an old version)
  must behave exactly like the meaning.
- **Costs** are worst-case per operation. Amortized bounds are not accepted, because persistence can replay an
  expensive operation on an old version.
- **Determinism:** no randomness, no hashing with unkeyed hashes, and no clock.

## Facts about the current code (verified on the base commit)
- **`crates/zeno-fcis-collections`** is `no_std` plus alloc, with `#![forbid(unsafe_code)]` (lib.rs:12-14).
  - It has the sealed trait `PersistentMap` (lib.rs:195), and the backends `BTreeMapBackend`, `RpdsBackend` and
    `OrderedMap` (on `rpds::RedBlackTreeMapSync`).
  - Its differential-test pattern is `tests/bounded_contract.rs:86-130`: a generic `differential::<M>()` against a
    BTreeMap model and an independent byte oracle, which re-checks that old snapshots are unchanged.
  - **There is no priority queue anywhere.** A separately verified persistent red-black map is being integrated
    through another line of work. Do not write another general map.
- **The metering pattern to copy** (it is not exported, so do not reuse it directly) is
  `crates/zeno-fcis-synthesis/src/finite/execution_v2/meter.rs`: `Limits`, opaque `Usage`, `MeterFailure` and
  `charge` (:108-131), which does a checked add, refuses over the limit and leaves counters unchanged on failure.
  The resource kinds are in `crates/zeno-fcis-core/src/resource.rs:13-29`. None of them counts allocation, match,
  comparison or call.
- **The syn-based checker to follow** is `crates/zeno-fcis-cli/src/purity.rs` (with `purity_resolution.rs`): import
  and alias resolution, plus a rule table. `syn = "=2.0.119"` with `full`, `parsing` and `visit` is already in the
  lock (`crates/zeno-fcis-cli/Cargo.toml:23`).
- **The cheat scan pattern** is `tools/check_arithmetic_pilot.py:326-331`, which refuses `assume`, `admit`,
  `external_body` and `external_fn_specification`.
- **Verus** (see `verification/verus/README.md`):
  - It is pinned to `0.2026.09.27.3cf1832` (`verification/verus/toolchain.json`), and runs under Rust 1.98.1.
  - Install it with `python3 tools/check_verus.py --install`.
  - Command shape: `--crate-type=lib --edition=2024 --no-cheating --no-external-by-default -V spinoff-all
    --output-json` (`tools/check_metered_execution.py:265-266`).
  - The house idiom for an abstraction function plus an invariant is
    `crates/zeno-fcis-cli/src/neural_loop/ledger_accounting.rs:140` (`view`) and :478-499 (a `type_invariant`
    that ties `view` to the ledger).
  - There is no existing Verus proof of a collection refinement.

## What to build
Put everything in a new workspace crate, `crates/zeno-fcis-structure-search`. Its only dependencies are `syn`
(`=2.0.119`, the same features as the CLI), `proc-macro2` and `quote` at the versions already locked, and workspace
crates. Adding this workspace member to `Cargo.lock` is allowed; adding any external crate is not.

1. **The meter** (`src/meter.rs`, `no_std` plus alloc).
   - `Meter { limits, used }` counts `calls`, `matches`, `compares`, `arith`, `allocs`, `shares` and `max_depth`.
   - `Exhausted { resource, limit, attempted }`. Each charge is a checked add; going over the limit refuses and
     leaves the counters unchanged.
   - **The API candidates must use:**
     - `m.enter()?` (a call, and depth plus one) and `m.exit()` (depth minus one);
     - `m.scrut(x)?`, which charges a match and returns `x`;
     - `m.cmp(&a, &b)?`, which returns `Ordering`;
     - `m.eq(&a, &b)?`;
     - `m.add(a, b)?` and `m.sub(a, b)?`, which are checked and refuse on overflow;
     - `m.alloc(v)?`, which returns `Arc<T>`;
     - `m.share(&arc)?`, which returns a clone of the `Arc`.
   - After `Exhausted`, the operation is refused and the meter state is reported as is.
2. **The profile checker** (`src/profile.rs`, profile ID `zeno-fcis/structure-profile/1`).
   - **Input and output:** it takes one candidate source file, parses it with syn, and returns accepted or a list of
     violations. Each violation has a rule ID, a line and a column.
   - **Allowed:**
     - the items `use zeno_fcis_structure_search::prelude::*;`, `struct`, `enum`, `fn` and inherent `impl`;
     - `#[derive(Clone, Debug, PartialEq, Eq)]`, `#[cfg_attr(verus_keep_ghost, verus_spec(...))]` and `proof!{...}`
       blocks.
   - **Refused, each with its own rule ID:**
     - `unsafe`, `static`, `extern`, trait impls (including `Drop`) and macros other than `proof!`;
     - loops (`loop`, `while`, `for`) and closures;
     - `?` on anything but meter calls and calls to functions declared in the file;
     - `panic!`, `unwrap`, `expect` and `unreachable!`;
     - indexing, `as` casts and division or remainder;
     - the operators `+ - * < <= > >= == !=` on values (use the meter);
     - `Arc::new` and `.clone()` on `Arc` (use `alloc` and `share`);
     - field types other than the allowed ones: `i64`, `u64`, `u32`, `bool`, `Option<_>`, tuples, `Arc<T>` of
       types declared in the file, and types declared in the file;
     - any of `Cell`, `RefCell`, `UnsafeCell`, `Mutex`, `RwLock`, `OnceCell`, `OnceLock`, `LazyCell`, `LazyLock`,
       atomics, `Rc`, `Box<dyn _>`, references in fields, function pointers and `dyn`;
     - any `cfg` other than the Verus spec attribute, and any `external_body`, `external`, `assume`, `admit` or
       `external_fn_specification`, even inside `proof!`.
   - **Shape rules:**
     - Every function takes `m: &mut Meter` and has the body shape `{ m.enter()?; let r = ...; m.exit(); Ok(r) }`,
       with refusal results as declared below.
     - Every `match` scrutinee is `m.scrut(...)?`.
     - Every function in a call-graph cycle has a `decreases` clause in its `verus_spec`.
   - Paths are resolved against the prelude only. An unresolved name is a violation, not a pass.
3. **The meaning** (`src/queue_model.rs`): a plain, obviously correct reference.
   - **State:** `BTreeMap<OrderId(u64), (Price(i64), Seq(u64))>` plus `next_seq: u64`.
   - **Operations, with refusals** (the precedence is the listed order, and refusals leave the state unchanged):
     - `insert(id, price)`: `Duplicate` if the ID is present; `SeqExhausted` if `next_seq == u64::MAX`. Otherwise it
       stores (price, next_seq) and increments next_seq.
     - `cancel(id)`: `Missing` if the ID is absent; otherwise it removes the ID.
     - `reprice(id, price)`: `Missing` if the ID is absent, then `SeqExhausted` as above. Otherwise it stores the new
       price with a **new** sequence number, so a price change loses time priority, as under price-time priority
       matching.
     - `peek_min()`: the entry with the least (price, seq), or `None`.
     - `pop_min()`: removes and returns that entry, or `Empty`.
     - `len()`.
   - Write this semantics, with the precedence and a worked example, in `docs/STRUCTURE_SEARCH_GATE.md`.
4. **An independent small-scope reference** (`tools/structure_queue_reference.py`). It imports nothing from the
   crate. It writes every model state reachable from empty, with up to k = 4 entries over IDs {0,1,2,3,4}, prices
   {-1,0,1} and `next_seq` below 12, crossed with every operation and argument in those domains. The output is a
   JSON table with its SHA-256. Rust tests load it.
5. **The gates** (`src/gates.rs`, with a test binary per gate). A candidate stops at its first failing gate, and
   every failure is a structured counterexample (JSON).
   - **G0:** the profile check, then the build. A build failure counts as a G0 failure, not as a caught defect of a
     later gate.
   - **G1:** a random differential against the model over 10,000 deterministic seeded traces of length up to 64.
     At least 30% of the steps fork: they apply an operation to a retained older version. After every step, compare
     the result and the abstract view, and re-check that every retained version's view is unchanged.
   - **G2:** bounded exhaustive refinement. Do a breadth-first search over representations reachable from `r0`
     inside the reference's domain. For each one, check the following against the reference table:
     - **O1:** `abs(r0)` is the empty model, and `inv(r0)` holds.
     - **O2:** for every operation and argument, the result and `abs(r')` match the reference, and `inv(r')` holds.

     Results are compared as model values, never as representations. A counterexample gives the representation,
     the operation, the arguments, and the expected and actual values. It also says whether the representation is
     reachable, and gives the path from `r0`.
   - **G3:** cost.
     - Run the metered workload at sizes 2^4 to 2^12.
     - Run an adversarial search for slow traces: deterministic hill-climbing over operation sequences that
       maximizes the steps of one operation.
     - Check both against the candidate's declared per-operation bound, a formula `a·log2(n+1) + b` or `a·n + b`
       with stated constants.
     - Report the worst trace found. A bound counts as **measured** up to the stated size, never as proved.
6. **The candidates** (in `candidates/`; each is a single file in the profile):
   - **C1 `sorted_list`:** a persistent singly linked list sorted by (price, seq). Lookup by ID is a linear scan.
     The declared bound is linear.
   - **C2 `indexed_avl`:** two persistent AVL trees, one keyed by (price, seq, id) and one by id to (price, seq).
     The declared bound is logarithmic.
   - **Six planted wrong candidates** (in `candidates/planted/`), each failing a **named gate at a stated
     coordinate**:
     1. interior mutability in a node (G0);
     2. a direct `<` comparison that skips the meter (G0);
     3. reprice that keeps the old sequence number (G1 or G2: give the trace);
     4. ties popped by the larger sequence number (G2);
     5. cancel of a missing ID that silently succeeds (G2, a refusal mismatch);
     6. an insertion without rebalancing, so the cost is linear (G3).
   - **The comparison-disabled controls:** for each gate, a test run with that gate's comparison disabled must let
     its planted candidate through. This shows that the gate, and not something else, is what catches it.
7. **The proof lane for C1** (required).
   - The Verus harness is `verification/verus/structure_queue_sorted_list.rs`. It `#[path]`-includes the C1 file
     unchanged.
   - Prove these with nothing assumed:
     - O1 and O2 against a `spec fn` model over `Map<u64, (i64, u64)>`;
     - O3, termination, by `decreases` on the list length;
     - the meter totality: each operation's meter counts equal a stated spec cost function of the position and
       the length.
   - Persistence: the inputs are taken by shared reference, and the old view is unchanged; prove it as an ensures
     clause on each operation.
   - The gate script is `tools/check_structure_queue.py` (and its test), following
     `tools/check_metered_execution.py`. Record the counts in a profile JSON.
   - A proof control: a C1 mutation in which reprice keeps the old sequence number must fail O2 in Verus, at a
     named obligation.
8. **Stretch: the proof lane for C2.** The same obligations, plus the AVL height bound. If you do not finish it,
   report how far you got and leave C2 at tested and finite checked. Do not weaken anything to make it pass.

## Done when
- These pass:
  - `cargo +1.97.1 test -p zeno-fcis-structure-search --locked`
  - `cargo +1.97.1 clippy -p zeno-fcis-structure-search --all-targets --locked -- -D warnings`
- C1 and C2 pass G0-G3.
- All six planted candidates fail their named gate at the stated coordinate.
- The disabled-comparison controls let their plants through.
- The C1 Verus gate passes with `--no-cheating` and nothing assumed, and the proof control fails.
- `docs/STRUCTURE_SEARCH_GATE.md` (new; at most 150 lines) gives, in plain words:
  - the user story;
  - the profile and why each rule exists;
  - the meter;
  - the meaning;
  - the gates and what evidence each adds;
  - the results table, with each claim at its level;
  - the trust basis: the Rust compiler, the pinned Verus, vstd and Z3, and erasure of the `verus_keep_ghost`
    attributes;
  - what this gate decided.

## Out of scope
- The search loop itself: proposers, the archive, LLM packets;
- the ZAL collection profile;
- other structures;
- a general map;
- emitting into `zeno-fcis-collections`;
- performance tuning beyond G3's measurements.

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

