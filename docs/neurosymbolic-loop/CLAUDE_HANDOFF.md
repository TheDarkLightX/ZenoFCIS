# Claude continuation contract

Status: design handoff, 2026-10-04. No feature described here is installed by
this packet. Goal: implement a small checked transformation lane, then optional
bounded neural discovery, without weakening the existing ZenoFCIS authority.

## Read before editing

Read complete files, count lines and recover any truncated output. Follow the
current checkout's AGENTS.md and `skills/handoff-completion-gate/SKILL.md`.
Then read, in order:

1. [NOTE.md](NOTE.md), [2.1 roadmap](../V2_1_FACTORY_PLAN.md),
   [existing transform design](../V2_1_TRANSFORM_DESIGN.md).
2. [Study review](../V2_1_EGRAPH_REVIEW.md),
   [Python replay](../evidence/v2_1_egraphs/replay-20261004.json) and
   [source check](../evidence/v2_1_egraphs/source-check-20261004.json).
3. [DESIGN.md](DESIGN.md), [spec index](specs/INDEX.md) and all six specifications.
4. [ADR 0003](../adr/0003-epistemic-status.md),
   [ADR 0005](../adr/0005-decision-gate.md), current scalar IR/evaluator/product
   enumeration and authority/replay source, with their exact source manifests.
5. Current `integrations/mcp/zeno_fcis_synthesis.py`, its `test_server.py`, and
   `skills/zenofcis-synthesis-first/SKILL.md` before adapter implementation.
6. [paper.md](paper.md) and [CHECKLIST.md](CHECKLIST.md) for claim boundaries and
   open obligations. For experiment reuse, read its pinned full protocol/source;
   the paper alone is not the executable specification.

Published experiment pin is
`a69ed8db594d95279a46bff0f65185ef67d51f98`; it is evidence about that historical
artifact. The design was prepared from dirty `/tmp/zenofcis-v2-simplify` at old
HEAD `1ed6f88f7b1098bed6c409bc9aa3c12c99d43c3c`. Neither is the future
implementation revision. Record branch, exact HEAD, dirty paths, active owners
and tool pins before work; preserve concurrent changes and receipts.

## First task and ownership

**First check that smaller V2 and roadmap prerequisites are complete.** If they
are not, keep this as later planning work and finish the already authorized
V2 task. Do not add neural work to V2's release gate or block its maintenance
pilot. A standing operator authorization may cover ordinary future work; do
not create needless approval dialogs for actions already within its scope.

The first track-6 task is **Boolean supplied-candidate admission and checking**:
freeze NSC-001–008 and NSF-001–003/009 against the final scalar semantics,
construct the listed distinguishing fixtures, and implement the smallest
`finite::transform` slice. Start without egg, solver, provider or MCP expansion.
Use the existing owned scalar Program and canonical representation. Propose a
codec inverse only after checking the current representation and its gates.
State a short definition of done and source-bound validation plan before edits.

Claim file ownership with the coordinating agent before editing. Intended first
scope is the synthesis finite transform module, the minimal scalar codec seam
it requires, focused acceptance fixtures and source/proof manifests. Preserve
all unrelated dirty files. Do not edit authority/publication/SQLite to make an
optimization pass. Use an isolated worktree when ownership conflicts require it;
never reset, clean or overwrite another agent's work. Do not start extra agents
without authorization. No paid model call is implied by this handoff.

Keep one heavy build/proof/test command at a time under the shared lock:

```sh
flock /tmp/zenofcis-v2-parallel/heavy-check.lock cargo test -p zeno-fcis-synthesis
```

This is an illustrative focused command; inspect current package/gate names.
Wrap each actual heavy command with the same lock, including proof tools.
The coordinator owns lock-directory creation if absent. Do not delete or bypass
the lock to gain parallelism. Lightweight independent reads can run concurrently.

## Required progression and checks

1. **Boolean checker:** canonical full-consumption decode, exact original schema,
   zero-arity singleton, complete versioned enumeration, ordered observations,
   total opcode admission, private owned result and charge-before-work.
2. **Receipt/replay:** every subject/meaning/checker/completion/cost binding,
   fresh full replay and no direct deserialization of a trusted witness.
3. **Resource/resume:** precharged attempts/calls, bounded workers/descendants,
   fixed budgets and durable reservations; stale resume returns no trusted I.
4. **Local loop:** fake/local proposer, genuine checked incumbent, original
   component guards and strict lexicographic updates; failure preserves only
   qualified state. No receipt invented for the original fallback.
5. **Adapter:** reuse the existing MCP/skill, fake-provider tests and explicit
   disclosure policy; optionally enable a real provider only after its gates.

Prove executable decoder/evaluator correspondence, enumeration bijection,
complete-comparison soundness, no incomplete-success path, immutable custody,
work charging and selection/attempt invariants. Connect abstract lemmas to
actual bodies rather than proving a disconnected model. Run independent oracle
comparisons and focused mutations: missed/duplicated row, ignored/reordered
output, changed domain/ABI, forged/stale receipt, fake model success, removed
cost guard, refund-on-failure, late response, ledger rollback and insufficient
receipt/replay budget. Use fake providers and bounded test workers first.

Checked-i64 remains a later bounded profile preserving dead/unselected overflow
and error precedence. Whole application observations include reasons, state,
ordered effects, laws, resource/refusal and identity/replay. Resource refinement,
identity upgrades and Rust/Wasm correspondence are separate work. Do not shrink
domains, ignore fields, remove law/meter checks, accept solver claims as proof,
or enlarge limits silently to obtain a passing result. Stop and report the
concrete failing obligation if a gate cannot be satisfied.

## Evidence and completion

For each bounded slice, freeze source/body/dependency manifests after edits;
run and inspect required proof, native/oracle/mutation and repository checks
on that exact state. Run `python3 tools/atdd.py run --all` immediately before
any authorized commit, per repository guidance. Do not commit, push, merge or
publish merely because this handoff exists; follow the session's authorization.

Report distinct statuses: implemented, locally checked, source-bound proved,
independently reviewed, exact-head CI passed, published/merged. Missing tools
mean unrun/blocked. A prior or nearby revision's success cannot qualify changed
source. If compute ends, persist actual files, exact revision/dirty scope,
command exits, proof assumptions, remaining gates and one next bounded task.
Do not claim completion from intent, an outline, an uninspected run or paper
proof sketches. Fresh historical Rust/Lean runs, dependency/license review and
all real neural evaluation are still open unless new receipts establish them.
