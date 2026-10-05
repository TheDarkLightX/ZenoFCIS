# 03 — Enforced resources, accounting and session resume

Parent: [design](../DESIGN.md). Shell controls are distinct from pure semantics.

## Inputs and outputs

Input: fixed request limits, admitted work estimates, operator authorization and
provider policy, optional persisted session. Output: enforceable reservations,
bounded stage outcomes and monotonic ledger, or typed unavailable/refused resume.
Do not dispatch work whose worst-case permitted charge cannot be reserved.

## Requirements

- **NSR-001:** Compiled ceilings MUST be no weaker than DESIGN's initial limits:
  64 KiB/artifact; eight attempts/checks; four calls; 4,096 input/2,048 output
  tokens per call and 24,576 reserved total; 1,000,000 checker units/check and
  8,000,000/session including replays; 20-second whole session deadline.
  Lower operator limits MUST win. Tokenization failure MUST prevent dispatch.
- **NSR-002:** Each search worker MUST have an external five-second deadline,
  512 MiB memory cap including descendants, one solver thread and bounded I/O.
  A checker worker MUST have a two-second deadline and 64 MiB cap. Enforce via
  owned cgroup `memory.max`, no swap and descendant termination, or a qualified
  equivalent. Failure to install caps MUST make that worker unavailable.
- **NSR-003:** Parse/solver/extraction/subprocess work MUST all lie inside those
  limits. Between-batch e-graph node thresholds and solver solve-time settings
  MUST NOT be called hard memory or whole-worker limits. Cleanup latency MUST
  be measured separately; no hard-real-time guarantee may be inferred.
- **NSR-004:** Transcript retention MUST be ≤512 KiB; per-session retained
  canonical request/artifact/receipt/witness storage MUST be ≤2 MiB, subject to
  each object's own cap. Overflow MUST stop/refuse or explicitly omit advisory
  history; it MUST NOT omit required bindings or truncate an accepted artifact.
- **NSR-005:** Attempts, model calls and worst-case tokens/money MUST be reserved
  before dispatch. Failures/refusals/duplicates and uncertain remote completion
  MUST retain charges/reservations. Billing reconciliation MUST follow a pinned
  approved policy; no invented refund or unknown-equals-zero rule is allowed.
  Default hosted allowance is zero and hosted mode disabled. If the provider
  cannot enforce the approved hard spend ceiling, the mode MUST be unavailable.
- **NSR-006:** A standing operator configuration MAY authorize ordinary new
  requests within finite aggregate budgets and disclosure scope; a fresh human
  dialog is not required when authorization already exists. Automatic session
  resets/retries MUST NOT bypass per-request or aggregate limits. Model text
  MUST NOT authorize spending, disclosure or a new contract.
- **NSR-007:** Persist immutable request, actual artifacts and receipts, plus a
  shell-owned append-only reservation ledger. Resume MUST verify request/source
  identity, ledger continuity and remaining aggregate authorization, re-admit P0,
  and replay any incumbent before returning or using it as checked. Stale or
  tampered stored artifacts MUST NOT be returned as retained checked results.
- **NSR-008:** Witnesses reused as facts MUST pass fresh witness replay. Resume
  replay MUST consume the remaining checker/session allowance. Truncated,
  rollback-suspected or unverifiable ledgers MUST fail closed; crash-pending
  reservations remain charged unless the approved provider policy reconciles
  them. Plain content hashing MUST NOT be claimed to prevent ledger rollback.
- **NSR-009:** If resume fails, the outcome MUST be `ResumeRefused` or
  `ResumeInconclusive` with no trusted incumbent. A separately authorized fresh
  admission of the original may create `OriginalAdmitted`, with no inherited
  receipt. The host-integrity/ledger durability assumption MUST be explicit.
- **NSR-010:** Clocks, remote calls, credentials, storage and cancellation MUST
  stay in the shell. Core work charging MUST remain deterministic and happen
  before work. Credentials/private data MUST NOT leak into provenance or MCP;
  hosted source disclosure MUST follow the explicit operator policy.

## Precedence and scenarios

Validate request/ledger/authorization → reserve allowance → install limits →
dispatch. On deadline close the stage, terminate descendants and ignore late
responses. Record last reliable usage and unresolved reservations. On resume,
binding/ledger failure precedes any reuse; replay failure prevents witness reuse.

Required tests: model never returns; oversized output; repeated refusals; worker
forks descendants; memory cap; parser expansion; solver stalls outside solve;
unsupported cgroup host; death between reservation and reply; lost usage report;
late reply; changed request; stale checker; edited receipt; ledger rollback;
insufficient remaining replay allowance; aggregate budget exhausted across two
otherwise valid sessions; secrets omitted from success and failure transcripts.

## Source-bound acceptance

Use fake providers and bounded hostile workers, without paid calls. Prove core
meter behavior; test actual host enforcement, ledger recovery and process-tree
cleanup on each supported deployment. Record platform and latency limitations.
Resource and resume qualification are required before enabling that adapter;
they do not prove hosted server behavior or application resource refinement.
