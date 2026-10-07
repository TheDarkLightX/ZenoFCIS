# Escrow contract fixture

The escrow contract of the app-building study of 2026-10-05, in its original
design. An arbiter's split pays the buyer and the seller in two deliveries
on channel 300, with ordinals 0 and 1 and idempotency ordinals 0 and 1.

The study had to change this design twice before the generated application
ran. Generation refused the second idempotency ordinal with an opaque
`Descriptor` error. Then the study's first review showed the library refusing
every split decision, because the generated Effect limit was 1. Both are
fixed: the channel's idempotency domain covers every ordinal the rules use,
and the Effect limit is the most deliveries any case makes.

- `project.zeno` and `v2/policy.json`: the study's original files. They were
  written by an AI agent, and no person has reviewed them.
- `tests/decision-examples.txt`: the study's 23 examples. Each is on one line,
  and each payout is its own delivery (`300 payee amount`).

`tools/check_app_journey.py` builds an application from this directory in a
new temporary directory and runs exactly the commands its README lists.
