# Agent treasury guard

A treasury starts with 6 quote and 1 base. An authenticated Agent may propose a
swap; a Dex role reports settlement or failure. The checked V2 Authority evaluates
`v2/policy.json` and the independent `project.zeno` laws before publication. The
scripted demonstration calls no model and uses no network.

## Rules

The state holds quote and base balances, today's spending, the last committed
tick, and one pending swap's direction, amount, and minimum output. Balances are
bounded at 20. Quote must remain at least 2. A proposal sells 1–3 units, spends at
most 3 quote of oracle value per trade and 4 per day, and requests at least three
quarters of the oracle value. A day is four ticks. Integer division follows the
explicit floor/ceiling formulas in laws 502 and 507.

A proposal requires the approved `TreasuryAgentV2` model, a price of 1 or 2 observed
no more than one tick ago, and no pending swap. It debits the sold asset and records
it as held. Every commit advances the clock. A new day resets spending; failure
or refund within the same day does not restore the spent budget. The Agent cannot
report a settlement or Dex failure. Callbacks name the pending intent, and a
settlement must meet its minimum output. Accepted settlement credits exactly the
reported bought amount; committed Dex failure refunds exactly the sold amount.

The clock domain is 0..=11. Proposals after tick 9 reject because their two-tick
expiry would be unreachable. `RefundExpired` lets the Agent recover an outstanding
intent at or after its expiry, without a Dex callback, approved model, or fresh
price. It returns the held sold asset, clears the pending swap, and retains the
ordinary day accounting. The last safe proposal at tick 9 can be refunded at 11.
An early, stale, or duplicate recovery rejects. Settlement that commits first
closes the intent, so recovery then rejects. A deployment must supply a trusted,
nondecreasing clock and service recovery at the final admitted tick; this bounded
example does not roll the calendar forward.

Original laws 500–508 retain balances, clock, budget, caller, proposal, settlement,
and failure constraints. Added laws 510–514 reserve recovery time, bind the exact
refund, prevent rejection of eligible recovery, require the numeric request fields,
and ensure a refund fits its balance bound. Laws 515–516 bind the sold/bought asset
variants and exact destination. The proposal queues exactly one request on channel
300 to the service named `zenodex`, carrying the intent, both assets, sold amount,
minimum output, and two-tick expiry. Other committed actions queue nothing.

## Authority and external effects

The context's caller, model identity, tick, and oracle price are trusted inputs
supplied by the host. The example does not authenticate a remote caller, validate
an oracle feed, or establish that a reported settlement occurred. Price limits
bound the local proposal against the supplied quote; they do not bound market loss.

An outbox entry is a request, not a transfer. The receiver must deduplicate intents,
enforce the expiry, and prevent a refunded intent from settling afterward. The
local timeout must be backed by control of the held asset; an integration that
irrevocably transfers it on proposal cannot use this refund rule unchanged. The
shell's retry identity does not itself prove external payment or exactly-once
execution. The demonstration receiver keeps its ledger only in memory.

Liveness checks cover an honest Agent acting within the declared finite game and
fair opportunities to issue its recovery command. They do not guarantee scheduler
availability. Original claims 600–601 cover the stated treasury limits and budget;
they are not external settlement claims. New schema identities require a new
database or a separately reviewed migration.

## Regeneration and checks

Use `zeno-fcis generate contract <directory>` and `--check` to reproduce the V2
artifacts. `contract scenario` checks the shipped examples and concrete recovery
paths; `tools/test_template_rules.py` checks independently refused counterpolicies.
Native trace-game certificates and the finite table liveness checker assess their
explicit actor and clock domains. Earlier native synthesis/oracle tests are
historical comparisons for the original actions, not coverage of the new timeout.

Generate an isolated application with `zeno-fcis new --template
agent-treasury-guard <directory>`, then run `cargo +1.97.1 test --offline`. The
`sqlite` feature supplies `create`, `invoke`, and the scripted `journey`. The pure
checked declarations also build without that feature. `refund_expired(intent)`
constructs the new recovery command. The original journey and its expected
balances remain unchanged.

## Lineage

Guarded transitions and independent predicates follow E. W. Dijkstra, “Guarded
Commands, Nondeterminacy and Formal Derivation of Programs,” Communications of
the ACM 18(8), 1975,
[doi:10.1145/360933.360975](https://doi.org/10.1145/360933.360975).
Deadline refunds are a conventional application choice; prior-art search pending.
No novelty claim is made.
