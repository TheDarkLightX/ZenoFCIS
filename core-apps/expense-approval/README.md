# Expense approval

One fixed expense joins the approval queue K=2, consumable budget C=7 and
idempotency slot C=7. Two distinct supplied principals approve amount 3, key 1.
Execute atomically records approval completion, spends exactly 3 and emits one
payment intent. An exact retry preserves the budget and emits nothing. Changed
subjects, duplicate votes, insufficient quorum and unauthorized requests refuse.

The complete input domain has 55,296 tuples, including invalid coupled states.
The test reference was written separately from the explicit requirements,
corrected once at a boundary, and checked independently on every input. This
reference is test evidence, not a business decision callback or formal proof.
The rules and laws in `project.zeno` and `v2/policy.json` are compiled into the
ordinary library-owned Authority contract.

`composition.json` binds exact component instances and their projections. The
queue owns phase and votes; the budget owns remaining units. The memo owns no
state: its view reads the queue's phase and the fixed subject (key 1, amount 3),
so its Record step only checks the request and changes nothing. Only the queue
and the budget hold state. Each occurrence's listed step order is descriptive:
every step reads the same pre-state. One physical component publishes the
complete coupled transition.

Authorization and principal identity are supplied assumptions. A payment intent
is not bank settlement. This profile has no reset, second expense, general
multi-key memory or unbounded quorum. Evidence is pending actual complete
Authority replay, native durable journey, controls and final review.
