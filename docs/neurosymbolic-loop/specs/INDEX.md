# Implementation specifications

Status: proposed contracts, 2026-10-04; no implementation qualification implied.
Read [DESIGN.md](../DESIGN.md) and [CLAUDE_HANDOFF.md](../CLAUDE_HANDOFF.md) first.
`MUST` and `MUST NOT` below are acceptance requirements for an enabled feature.
Each gate applies to one frozen source revision plus its complete dependency
closure, not to a nearby commit or a historical experiment receipt.

| Order | Contract | First delivery |
| --- | --- | --- |
| 00 | [Request and observations](00-contract.md) | Frozen Boolean contract and canonical admission |
| 01 | [Bounded adaptive loop](01-loop.md) | Checked incumbent state machine without a real model |
| 02 | [Checker, feedback and receipts](02-checker-feedback.md) | Supplied-candidate checker and replay; implement before 01's adapter |
| 03 | [Resources, accounting and resume](03-resources-resume.md) | Enforced shell caps and fail-closed resume |
| 04 | [Synthesis integration](04-integration.md) | Existing MCP/skill extension, optional provider adapter |
| 05 | [Qualification and evaluation](05-qualification-evaluation.md) | Exact-source feature gates, later preregistered experiment |

Implementation dependency order is 00 → Boolean 02 → receipt 02 → 03 → 01
with a deterministic local proposer → 04 with a fake provider → optional real
provider. Specification numbers group concerns; they do not override this order.
05 defines the gate for every stage. Finish V2 and preserve roadmap prerequisites
before any runtime work. Checked-i64/application/target modes remain disabled.

Identifiers are stable within this draft: NSC (contract), NSL (loop), NSF
(checker/feedback), NSR (resources/resume), NSM (integration), NSE (qualification).
A later change to a MUST, domain, observation, checker or budget is a reviewed
contract/version change, never a silent repair to make a failed case pass.
