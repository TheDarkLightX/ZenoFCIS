# Spend-approval contract fixture

The spend-approval contract from the app-building study of 2026-10-05, copied
unchanged. A payment request moves through an approval matrix by amount tier:
tier 0 needs no approval, tier 1 the CFO, tier 2 the CFO and the CEO, and
tier 3 also a board resolution. Only the clerk executes. An execution over the
daily limit is a committed failure that blocks the request and alerts the risk
desk.

- `project.zeno`, `v2/policy.json` and `tests/decision-examples.txt` are the
  study's files. An AI agent wrote them as a first-time user of the 2.1 tools,
  and nobody has reviewed them as an owner would.

Two kinds of tests use it:

- **App journey.** `tools/check_app_journey.py` exports the decision program,
  optimizes it, adopts the candidate as version 2, and upgrades version 1
  stores that already have committed history. The CLI's
  `tests/contract_export.rs` exports and optimizes it too.
- **Contract review.**
  - Unchanged, the contract reviews with no refusal and no finding.
  - The agent planted a bug: the CFO check in "execute: tier 1 and above need
    the CFO" moved from `tier >= 1` to `tier >= 2`. A tier 1 request without
    the CFO's approval then executes, and state law 500 refuses the successor.
    Review must report those 16 refusals as a finding, because their
    pre-states satisfy every state law.
