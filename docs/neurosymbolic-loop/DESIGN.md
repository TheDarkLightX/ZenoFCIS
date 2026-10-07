# Bounded neurosymbolic transformation for ZenoFCIS

Design draft, 2026-10-04. Prepared with gpt-6-astra, maximum reasoning effort.
This extends track 6 of the [2.1 roadmap](../V2_1_FACTORY_PLAN.md) and the
[transform design](../V2_1_TRANSFORM_DESIGN.md); it installs no feature.
ZenoFCIS is being built as a **neurosymbolic software factory for high assurance
FCIS applications**: neural systems assist discovery and authoring, while
qualified symbolic checks admit artifacts under explicit contracts. The
integrated loop below is planned. The published experiment was symbolic.

## Delivery boundary

Finish smaller V2 first, including its mandatory Authority → Publication →
SQLite route. Preserve the compound/U128 prerequisite and tracks 1–5. Implement
track 6's supplied-candidate Boolean `transform`, receipt replay and bounded
local optimizer before adding a neural adapter. The maintenance pilot does not
wait for this loop. Later checked-i64, target compilation and application
adoption require their own contracts; none is enabled by this document.

The [review](../V2_1_EGRAPH_REVIEW.md) and
[replay receipt](../evidence/v2_1_egraphs/replay-20261004.json) support reuse of
the Boolean corpus, independent Python oracle, signature construction and
guarded-selection idea. Decoder qualification, executable checker soundness,
strict receipts, resource enforcement and dependency/license review remain
work. Do not copy the historical interpreter into a second production IR.

## Frozen request and small state

A reviewed request R contains the original canonical artifact P0; its complete
ordered domain D and ABI; execution, observation and error semantics; artifact
codec and enumeration versions; qualified checker identity; cost objective;
deterministic work limits; shell resource and provider-disclosure policy.
Retain their exact canonical bytes. RequestId is a domain-separated commitment
to those bytes, not permission to skip exact binding checks. Cryptographic
binding claims assume the chosen digest's collision resistance.

`FunctionalBoolV1` is the first profile: 0–6 Boolean inputs, 1–16 ordered
Boolean outputs, 1–256 eager acyclic Input/Bool/And/Not/Select nodes, at most
64 input tuples. Scan every node, including dead nodes and unselected arms.
Zero inputs have the singleton domain containing the empty tuple. Empty or
shrunk domains, changed ABI ordering, arithmetic and unsupported codecs are
refused. Duplicate output positions and unused inputs remain observable ABI.
The existing last-field-fastest product enumerator needs a versioned bridge to
the experiment's variable-bit signature ordering; equal row counts alone do
not prove coverage. Unexpected Boolean evaluation errors are inconclusive.

For admitted P, let O_R(P,x) be its full ordered observation and C(P)=(N(P),B(P))
its actual stored instruction count and canonical artifact byte length. The
checker compares the candidate Q to P0 on every x in D, never merely to a
model's answer, a signature supplied by the proposer, or the latest incumbent.

The session state is:

```text
S = (R, incumbent, witnesses, bounded_history, ledger, next_attempt, status)
incumbent = OriginalAdmitted(P0)
          | CheckedReplacement(Q, genuine_bound_equivalence_receipt)
```

The original is mathematically equivalent to itself. `OriginalAdmitted` does
not manufacture a transform receipt or imply receipt replay occurred. A
replacement has owned immutable bytes and a private checker-constructed
witness. Receipt bytes cannot be deserialized directly into that witness.
Fresh exclusive scratch may mutate locally; request, policy and retained
results remain transitively immutable across functional-core calls.

## One bounded loop

The shell may use an arbitrary adaptive proposer. It can emit a complete
canonical candidate or, in a separately enabled mode, a strategy in a closed
DSL that selects existing rule IDs, bounded rounds and a supported extractor.
No model-provided host code, imports, callbacks, paths or dynamic rule code is
executed. Start with whole candidates; keep strategy mode unavailable until
its parser, interpreter, bounds and fixed rule set are qualified.

```text
admit and freeze R; incumbent := OriginalAdmitted(P0)
while an attempt can be reserved within R's limits:
    durably reserve attempt and any model-call/token/money allowance
    proposal := bounded proposer(R's allowed view, verified feedback)
    record failure/timeout or decode bounded proposal
    if a strategy: run bounded symbolic worker and obtain actual DAG bytes
    report := transform(P0.bytes, candidate.bytes, R.check_policy)
    record report and all consumed/reserved work, including every failed exit
    if report is Different: replay witness before using it as factual feedback
    if report is Equivalent and candidate meets the original cost guards
       and C(candidate) is strictly lexicographically less than C(incumbent):
        incumbent := report.owned_checked_replacement
    stop on exhaustion, unavailable enforcement or terminal request failure
return incumbent, exact receipt status, stop reason and accounting
```

The checker is a pure function with a deterministic work policy. The shell
owns clocks, model calls, files, process limits and nondeterministic discovery.
No path from model text, extraction status, stored receipt text or reviewer
opinion constructs `CheckedReplacement`. Panic, death, allocation failure or
partial enumeration leaves the incumbent unchanged.

The checker returns `Equivalent`, `Different(witness)`, `Refused(reason)` or
`Inconclusive(stop)`, plus measured work on every exit. Selection reports either
`CheckedImprovement` or `EquivalentWithoutImprovement(reason)`. The latter
includes exceeding either original cost component or failing to beat the
incumbent. A request can end with `NoCheckedImprovement`, preserving the
admitted original, or `BestCheckedSoFar`, preserving a genuine checked result.
An invalid original ends in refusal; it is not a safe fallback.

## Feedback and acceptance are different objects

A replayable counterexample binds R, P0 and Q, the tuple ordinal and tuple,
and both typed observations. Replay recomputes the tuple and observations
through the qualified semantics before the shell calls it a verified witness.
`Inconclusive` and `Refused` carry diagnostic reasons, not fabricated semantic
counterexamples. A model may ignore or misread feedback; that affects search
quality, not the acceptance rule. Prompt feedback is not weight training.

Bounded feedback can contain the first verified difference, exact admission
reason, checker stop, or actual cost and selection result. Older witnesses may
be omitted from the prompt when the transcript cap is reached; they remain
versioned artifacts where retention is allowed. There is no CEGIS completeness
claim: the model need not satisfy prior witnesses and may repeat failures.

Interpret "independent checker" as an acceptance path separate from search,
executing the actual admitted artifact with qualified source and enumeration.
Use a separately written semantics oracle and meaningful mutations during
qualification. Shared evaluator bugs remain part of the trusted base until
their correspondence obligations are discharged. A second model's review is
not a substitute. Hashes bind artifacts; full observations establish equality.

## Conditional claims and their limits

Assume (A1) artifact admission/decoding and observation semantics are correct;
(A2) the enumerator covers each member of D exactly once; (A3) `Equivalent`
requires complete exact comparison and only it constructs a replacement;
(A4) request/artifact binding, custody and receipt replay hold; and (A5) actual
costs and the fixed selection rule are computed correctly.

**Invariant.** Every reachable incumbent I has O_R(I,x)=O_R(P0,x) for all x in D.
Every replacement has a genuine checker witness under the unchanged R, and
N(I)≤N(P0), B(I)≤B(P0). The initial case follows from reflexivity and admission,
without issuing a receipt. For a step, failures and nonselection preserve the
state. A selected candidate satisfies equivalence by A1–A4 and the original
cost guards by A5. Induction holds for any adaptive proposer; no probability
or intelligence assumption about the model is needed.

**Selection.** A replacement must satisfy N(Q)≤N(P0), B(Q)≤B(P0), with at least
one strict inequality, and C(Q)<lex C(I). Thus incumbent costs descend strictly
lexicographically on updates, and are componentwise bounded by the original.
They do **not** descend componentwise between updates: from original (12,300),
incumbents (10,200) then (9,250) are legal. Ties retain the incumbent; proposals
do not cause byte-only identity churn. A finite integer cost box implies
finitely many accepted updates, bounded above by (N(P0)+1)(B(P0)+1)−1.

**Termination.** Finite cost descent does not bound failed or duplicate
proposals. A precharged attempt counter, finite stage limits and an external
supervisor bound attempted work independently of success. With functioning
deadline enforcement and cleanup, the session eventually returns its incumbent
or an explicit failure report. Scheduling and remote cancellation are stated
assumptions, not hard-real-time theorems. No convergence, global minimum,
optimization success, runtime speedup or correctness of natural-language
intent follows. These are paper proof sketches, not new Lean/Verus results.

## Caps, charging and restart

These conservative ceilings extend the transform design and require measurement
and enforcement qualification before use. The effective limit is the smaller
of the compiled ceiling and the reviewed request policy.

| Item | Initial ceiling |
| --- | --- |
| Artifact | 64 KiB each; Boolean structural and 64-row bounds above |
| Session | 8 attempts, including malformed, empty, duplicate and failed proposals; at most 8 complete candidate checks |
| Model | 4 calls; each at most 4,096 input and 2,048 output tokens; 24,576 total reserved tokens |
| Checker | 1,000,000 deterministic work units per check, 8,000,000 per session including replay work; standalone worker 2 seconds and 64 MiB |
| Search worker | 5-second external deadline; 512 MiB including descendants; one solver thread; bounded input/output parsing |
| Whole session | 20-second external search/call/check deadline; no stage starts after expiry; bounded cleanup reported separately |
| Retained storage | 512 KiB transcript plus at most 2 MiB request/artifact/receipt/witness storage; advisory truncation is labeled |
| Hosted model spending | Disabled by default, monetary allowance zero; explicit approved provider/disclosure/billing policy required |

Each prompt must fit the token and transcript caps; otherwise skip/refuse the
call, without silently changing the semantic request. Reserve the worst-case
call charge before dispatch under a pinned tariff and enforceable provider
contract. If a hard monetary ceiling cannot be enforced, that provider mode is
unavailable. Credentials never enter prompts, logs, receipts or MCP output.
No private source goes to a hosted provider without explicit shell disclosure
configuration. Local client/worker limits do not bound a hosted server's RAM.

Use an owned cgroup with `memory.max`, no swap and descendant cleanup, or a
qualified equivalent. Include solver, extraction, parsing and subprocesses.
egg's between-batch node threshold is a heuristic; the study exceeded 1,000
nodes. A solver timeout is not the whole-worker deadline. Unsupported enforcement
means unavailable search, not an unbounded fallback. Charge deterministic core
work before work and retain attempted refusal charges. Shell reservations are
distinct from exact core usage; unknown remote completion retains its reserved
charge and incomplete-accounting status. No automatic retry enlarges a budget.

Persist immutable R, actual artifacts, deterministic receipts and bounded
provenance with a shell-owned append-only reservation ledger. Resume must match
R exactly, re-admit P0, replay a retained replacement before use, revalidate any
witness used as fact, and retain all prior consumed/reserved allowances. Replay
itself spends the remaining checker allowance. Rolled-back, truncated or
unauthenticated accounting cannot resume as the same session; fail closed.
Crash-pending reservations remain spent unless reconciled by the approved
provider policy. A new session requires explicit authorization and a new ID;
it must not be an automatic budget-reset retry. Standing operator configuration
may supply this authorization within a finite aggregate budget and disclosure
policy; ordinary authorized requests need no new approval dialog. Failed resume
returns no trusted incumbent: stale/tampered saved bytes cannot be reused until
fresh admission/replay succeeds. Document the ledger's host
integrity assumption; a plain self-hash does not prevent rollback.

## Integration and receipts

Reuse the existing synthesis MCP adapter
`integrations/mcp/zeno_fcis_synthesis.py`, its `test_server.py`, and
`skills/zenofcis-synthesis-first/SKILL.md`. Review those current files before
implementation. A proposed narrow flow is `transform_request` →
`transform_candidate` → typed feedback → `transform_replay`; these names are
design suggestions, not installed tools. Keep model/provider adapters in the
shell and use existing canonical synthesis types rather than a new framework.

Extend the transform receipt's subject, meaning, checker, completion and cost
bindings with RequestId and exact request identity. Bind both actual artifact
digests/lengths, complete ordered domains/ABI, semantic/error/observation and
enumeration versions, checker source closure/build, count/terminal state/trace,
actual costs and deterministic usage. Keep model/provider/version, prompt and
response digests, decoding parameters, seeds, search/solver status, tokens,
money and timing in separately labeled untrusted provenance. Provider-reported
versions and usage must not be presented as independently verified.

Replay re-decodes actual bytes and reruns the complete checker before building
private results; stale source closures invalidate dependent evidence. Receipt
replay cannot create Authority, Evaluation or Publication. A checked DAG does
not prove equivalence of separately printed Rust/Wasm. Do not activate or
reinterpret an application, pending effect or historical journal via this loop.

Checked-i64 later requires a complete bounded domain, stable exact errors and
eager evaluation of unused/unselected arithmetic, including first-error order.
Whole applications additionally observe reasons, decision class, successor,
patch, ordered effects/outbox, law/genesis frames, resource/trace/refusal and
identity/replay binding. Resource refinement and old/new identity mappings need
separate reviewed contracts through roadmap tracks 2 and 4. Equal outputs do
not erase those observations or establish application or intent correctness.

## Bounded implementation and evaluation

The [six specifications](specs/INDEX.md) define executable milestones. First
freeze the Boolean decoder/schema/observation and supplied-candidate checker,
prove coverage and successful-loop invariants against executable bodies, and
run independent oracle/mutation checks. Then qualify receipts and resume,
bounded local search, and only then an optional model adapter with fake-provider
tests. Release each enabled slice only under its exact-source gates and
[ADR 0003](../adr/0003-epistemic-status.md)/
[ADR 0005](../adr/0005-decision-gate.md). Unimplemented modes stay unavailable.

Future neural evaluation must preregister independently accepted held-out
contracts, task count, repeats, seeds/provider versions, budgets, metrics and
stopping rules before tuning. Compare local-only, fixed rewrite e-graph,
semantic e-graph, model-only whole proposals plus checker, hybrid without
feedback and hybrid with feedback. Hold all semantic/resource contracts fixed;
include all failures and costs. Historical Boolean tasks are calibration only.
Measure checked actual cost, improvement rate, calls/tokens/money, censored time
to first pass, refusals/timeouts, human review, attempted specification weakening
and planted-checker-bug false acceptance. Report paired uncertainty and all
repeats, not just best runs. A later whole-vs-strategy ablation is optional.
No neural benefit has been measured by this packet.
