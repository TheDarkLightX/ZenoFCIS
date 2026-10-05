# 02 — Independent checker, factual feedback and replay

Parent: [design](../DESIGN.md). Depends on 00; implement before neural search.

## Inputs and outputs

`transform(P0_bytes,Q_bytes,policy)` returns measured work plus
`Equivalent(owned_private_result)`, `Different(bound_witness)`,
`Refused(admission_reason)` or `Inconclusive(stop)`.
`replay_transform(P0_bytes,Q_bytes,receipt_bytes,policy)` reconstructs a result
only after fresh admission and complete checking. Receipt files are untrusted.

## Requirements

- **NSF-001:** The checker MUST execute both actual decoded artifacts across the
  complete original domain in a qualified order. It MUST compare exact typed
  observations, not only hashes, success counts, summaries or proposer masks.
  Only complete exhaustion and terminal/count checks may construct equivalence.
- **NSF-002:** Enumeration MUST be a bijection from bounded ordinals to D,
  with the existing last-ABI-field-fastest order explicitly versioned.
  Historical bit-index signatures MUST be converted or labeled with their old
  order. Merely seeing the expected number of iterations is insufficient.
- **NSF-003:** Admission, parsing/allocation, node attempts, comparisons and
  receipt creation MUST be charged before work. Every exit MUST retain measured
  and attempted refusal work. Checker exhaustion MUST NOT be encoded as a
  program error. Unexpected panic/process death MUST be outer inconclusive,
  with accounting completeness honestly marked.
- **NSF-004:** A difference witness MUST bind R and both artifact identities,
  ordinal, tuple and both full typed observations. Before model feedback labels
  it factual, witness replay MUST recompute tuple and observations from current
  immutable artifacts. A spoofed, stale or nonreproducing witness MUST fail.
- **NSF-005:** Feedback MUST be typed: verified semantic difference, admission
  refusal, incomplete check, or equivalent with actual cost/selection reason.
  The model's explanation MUST be labeled advisory. Incomplete checking MUST
  NOT yield a counterexample; candidate failure MUST NOT weaken the contract.
- **NSF-006:** Receipt MUST bind exact request identity; both canonical artifact
  digests, lengths and codec; complete ordered domains/ABI; semantic/error/
  observation/enumeration versions; checker source closure/build/dependencies;
  expected/visited rows, terminal status and trace commitment; actual costs;
  deterministic work policy and usage. Unknown/trailing fields MUST fail.
- **NSF-007:** Replay MUST re-admit both supplied artifacts, rerun all rows and
  compare every deterministic receipt field. Replay overhead MUST be recorded
  separately from reproduced check usage and included in total session usage.
  A stale checker identity MUST prevent reuse, not silently migrate evidence.
- **NSF-008:** Search status, model/provider/prompt/response metadata, timing and
  provider/proposer-reported token, billing and quality estimates MUST remain
  separate untrusted provenance. Checker-derived actual node/byte costs and
  deterministic usage MUST remain checked receipt fields. Hashes MUST derive
  from actual bytes, not copied labels. Raw receipts MUST NOT construct private
  witnesses or Authority/Publication capabilities.
- **NSF-009:** Qualification MUST use a separately written semantics oracle and
  source/body correspondence checks. Shared runtime code is permitted only
  with its trusted-base dependency stated; another model review alone MUST NOT
  count as checker soundness. Release claims MUST list undischarged assumptions.

## Precedence and scenarios

Apply 00 admission, preflight complete work and reserve it conservatively, then
evaluate in ordinal order. On first exact mismatch return `Different`; otherwise
finish all rows and receipt work. Resource stops at any point are inconclusive.
Cost selection happens after equivalence; equivalent expensive artifacts remain
equivalent without becoming improvements. Replay first rejects malformed or
mismatched bindings, then runs fresh comparison; it cannot accept by digest only.

Test zero arity/all 64 rows, repeated outputs, unequal output permutation,
constant/dead nodes, dropped/duplicated rows, changed original/candidate/ABI,
altered trace/count/cost/work/version/build, unknown/trailing receipt fields,
stale witness tuple, forged observations, equal hashes simulated with unequal
observations, final-receipt budget exhaustion and evaluator failure. Require
every relevant receipt-field mutant to fail its intended gate.

## Source-bound acceptance

Discharge decoder/evaluator/observation correspondence, enumerator coverage,
successful-loop invariant, charge-before-work and private-result custody against
the implementation revision. Publish native-oracle comparisons and mutation
receipts with exact source manifests. Stored study proofs establish an abstract
boundary only; fresh Rust/Lean reproduction remains an explicitly separate task.
