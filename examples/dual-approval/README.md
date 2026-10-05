# Dual approval

A payment request is released only after two different officers approve it.
This directory is a contract: `project.zeno` declares the types, reasons,
channel and laws, `v2/policy.json` holds the decision rules and the genesis
state, and `tests/decision-examples.txt` lists twelve decisions. Nothing else
is needed to build the application:

```
zeno-fcis new dual-approval --contract examples/dual-approval
cd dual-approval
cargo test
cargo run -- approvals.sqlite
```

## Rules

The first rule that applies decides; a request starts pending with no
approvals.

1. A request that is approved or cancelled cannot change: reject, reason 200.
2. A caller who is not an officer (Nobody) cannot act: reject, reason 201.
3. Any officer can cancel a pending request: a committed failure, reason
   202, that sets the status to Cancelled and keeps the approvals.
4. The officer who gave the first approval cannot approve again: reject,
   reason 203.
5. The first approval is recorded and the request stays pending.
6. A second approval, by a different officer, approves the request and sends
   a release to the treasury desk naming both officers.

## Laws

The library Authority checks these on every decision, from `project.zeno`:

- 500, on every committed decision and on genesis: an approved request names
  two different officers.
- 501, on every accept: the request was pending, is not cancelled, and
  records the calling officer as an approval.
- 502, on every committed failure: it is a cancel, the request is cancelled,
  and both approvals are unchanged.

Generation adds law 509, that a reject changes nothing and delivers nothing,
law 990, that genesis is the stated genesis, and law 991, that every decision
is the one the rules select.

## What the checks cover

`cargo test` checks each of the twelve examples against the Authority's
decision, checks that genesis accepts only the stated genesis state, and runs
the examples as one SQLite session from genesis: two committed decisions, each
replayed exactly, and the release delivered across a reopen of the database.
The examples were written by an AI agent together with the rules, so they
are not an independent oracle, and no person has reviewed them yet.
