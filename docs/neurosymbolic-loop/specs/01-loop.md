# 01 — Bounded adaptive proposal and selection

Parent: [design](../DESIGN.md). Depends on 00, qualified 02 and resource 03.

## Inputs and outputs

Input: admitted immutable R; bounded proposer interface; optionally a session
that passed resume admission. Output: admitted original without manufactured
receipt, or genuine best checked replacement, plus typed stop and full ledger.
The first implementation uses a deterministic fake/local proposer.

## Requirements

- **NSL-001:** State MUST hold R, a tagged original-or-checked incumbent,
  replayable witnesses, bounded history, consumed/reserved limits and attempt
  index. `OriginalAdmitted` MUST NOT serialize a fabricated equivalence receipt.
- **NSL-002:** The shell MUST charge/reserve an attempt before proposal work.
  Malformed, empty, repeated, refused, timed-out and failed attempts MUST consume
  that attempt. There MUST be at most eight attempts and eight candidate checks;
  no hidden retry or retry with increased bounds is permitted.
- **NSL-003:** Proposals MUST be bounded complete candidates or an explicitly
  enabled closed strategy DSL over fixed rule IDs, bounded rounds and fixed
  extractors. Whole candidates are the initial mode. Bounded strategy data may
  drive an already-qualified fixed interpreter; model-supplied host code,
  interpreter implementations, imports, arbitrary rules or callbacks MUST NOT
  execute. No strategy may alter R.
- **NSL-004:** Every candidate considered for selection MUST be checked against
  P0, not only the incumbent. Only a private qualified-checker equivalence result
  may enter selection. An optimizer/model/receipt file MUST NOT assert that type.
- **NSL-005:** Selection MUST require N(Q)≤N(P0), B(Q)≤B(P0), at least one strict,
  and C(Q)<lex C(I). Equal costs MUST keep the incumbent. A deterministic ordered
  stream resolves candidates; no byte-only tie replacement is allowed. The
  documented claim MUST be original component bounds and incumbent lexicographic
  descent, not componentwise descent between incumbents.
- **NSL-006:** `EquivalentWithoutImprovement` MUST distinguish original-cost
  guard failure, tie and failure to improve the incumbent. `Different`,
  `Refused` and `Inconclusive` MUST remain distinct. None may replace I.
- **NSL-007:** Death, timeout, unavailable supervision or malformed search output
  MUST preserve only an already admitted/checked incumbent in the live session.
  Untrusted persisted bytes MUST first pass 03's resume rules before reuse.
- **NSL-008:** Return MUST state `NoCheckedImprovement` or `BestCheckedSoFar`,
  actual stop reason, receipt status, all attempted costs and incomplete
  accounting where relevant. It MUST NOT imply global optimality, success
  probability, model learning, application authority or wall-time speedup.

## Transition precedence

Terminal invalid request stops all work. Otherwise reserve attempt and stage
allowances → bounded proposal → decode/lower → 02 checker → feedback → selection.
A timeout before completed acceptance prevents a new witness; a completed,
validated result delivered before the supervisor closes that stage may be
selected. Log the decision boundary explicitly; late messages cannot update a
closed session. Stop on request deadline, limit exhaustion or unavailable
enforcement; diagnostic collection MUST remain bounded.

Required sequences: eight identical invalid proposals; model attempts to change
R; valid worse candidate; valid tie; original (12,300), then (10,200), then
(9,250); regression after improvement; timeout after improvement; invalid
original; unknown-optimality complete candidate checked independently; truncated
solver output; stale late provider response. Fake proposer tests MUST show
returned receipt identity matches returned artifact on every branch.

## Source-bound acceptance

Prove/refine the selection invariant for arbitrary proposer outputs, accounting
monotonicity and bounded attempt termination. Review the abstract induction
against each executable return/exception path. Kill mutants that accept model
status, compare only to incumbent with changed R, omit a cost guard, replace on
ties, refund failed attempts or process a response after closure. Do not require
optimizer correctness/global extraction optimality to prove checked equivalence.
