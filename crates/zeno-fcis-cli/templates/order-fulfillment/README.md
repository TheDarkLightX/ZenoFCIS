# Order fulfillment

One order coordinates a customer, payment provider, and carrier. The checked
V2 Authority evaluates `v2/policy.json` and the independent `project.zeno` laws
before publishing a state change and its requests together. The state records
status and payment attempt, not a money balance or a physical parcel.

## Decisions and deadlines

The context supplies an authenticated caller and an absolute day in 0..=8.
This bounded example uses a fixed calendar: payment before day 2, dispatch
before day 4, delivery before day 6, and dispute before day 8. A deployment
must supply a trustworthy, nondecreasing clock and service recovery requests
at the final admitted day; this example does not roll its calendar forward.
`invoke` preserves the original day-zero demonstration; `invoke_at` supplies
an explicit day.

| Action | Caller | Transition | Outbox |
| --- | --- | --- | --- |
| Checkout | Customer | Placed to AwaitingPayment; increment attempt | Capture |
| PaymentCaptured | Provider | AwaitingPayment to Paid before day 2 | Shipping request |
| PaymentDeclined | Provider | AwaitingPayment to Placed; committed failure 204 | None |
| ParcelDispatched | Carrier | Paid to Shipped before day 4 | None |
| ParcelDelivered | Carrier | Shipped to Delivered before day 6 | None |
| CancelOrder | Customer | Placed or AwaitingPayment to Cancelled | Void if awaiting |
| TimeoutOrder | Customer | AwaitingPayment/Paid/Shipped to Cancelled at day 2/4/6 | Void/Refund/Refund |
| DisputeDelivery | Customer | Delivered to Cancelled before day 8 | Refund |
| ReleaseDelivered | Any authenticated role | Delivered to Settled at or after day 8 | Release |

Provider callbacks must name the current attempt. There are at most three
attempts. Checkout at a late day is allowed, but its customer can immediately
time out an awaiting payment. A timely dispute refunds without adjudication.
A carrier report counts as delivery confirmation; the customer has at least
two days to dispute before release. Cancelled and Settled are final.

Reject precedence is wrong caller (200), wrong status (201), stale payment
callback (202), exhausted attempts (203), missed deadline (205), then early
recovery (206). Rejection changes nothing and queues nothing. An eligible
request cannot be replaced with an arbitrary rejection. The original day-zero
checkout, decline, retry, payment, shipping, and delivery sequence is retained.

## What the laws bind

Original laws 500–506 retain state shape, callers, transitions, and decline
behavior. Added laws 507–508 bind recovery, dispute, release, and settled-state
shape. Laws 510–511 bind the first rejection guard and callback deadlines.
Laws 512–515 bind the exact request count, attempt, action, channel, and
recipient; law 516 binds the committed-failure reason. Capture, Void, Refund,
and Release go to `payment-provider`; shipping goes to `carrier`. No unrelated
request may accompany another transition. The generated structural law 909 checks unchanged state and no effects on
rejection alongside project law 510; conformance law 991 remains mandatory.

The customer has a local recovery path from every pending stage even when
the carrier and provider send no callbacks. After confirmed delivery, any
role can request release at day 8. These are paths to checked requests and
terminal local states, not a proof that an external recipient transfers money.
The reachable run checks in `v2/run-properties.json` also establish at most one
terminal payment request, at most one shipping request, and no later payment or
shipping request after a Void or Refund, over the declared finite domain.

## External responsibilities

Capture must reserve the attempt's payment until Release. Void must close an
attempt and return any payment already captured, even if a callback arrives
late; Refund must return its held payment. Release must pay the merchant once.
The receiver must persist delivery IDs and closed attempt IDs, and safely
handle delayed or reordered Capture/Void/Refund/Release requests. These are
integration obligations: the template neither authenticates remote callers
nor implements a payment provider or parcel custody. Physical delivery racing
with timeout needs an external return policy.

The shell persists requests and retries them with the same delivery identity.
The demonstration's recipient ledger is in memory. Publication, receipt, and
external settlement are distinct. A changed schema identity requires a new
database or a separately reviewed migration.

## Regeneration and checks

Use `zeno-fcis generate contract <directory>` and its `--check` mode. Contract
scenario files check concrete sequences; `tools/test_template_rules.py` checks
that counterpolicies are refused by the independent laws. Native trace games
and the finite table liveness checker assess only their declared actors,
commands, clock values, and scheduling assumptions. Earlier synthesis and
native-model checks are historical comparisons for the original day-zero
behavior, not coverage of the added recovery actions.

Generate an isolated application with `zeno-fcis new --template
order-fulfillment <directory>` and run `cargo +1.97.1 test --offline` there.
The `sqlite` feature supplies `create`, `invoke`, `invoke_at`, and `journey`;
without it the declarations and checked decision API remain available.

## Lineage

Guarded state transitions and independent predicates follow the established
approach of E. W. Dijkstra, “Guarded Commands, Nondeterminacy and Formal
Derivation of Programs,” Communications of the ACM 18(8), 1975,
[doi:10.1145/360933.360975](https://doi.org/10.1145/360933.360975).
Deadline refunds and delayed release are conventional application choices;
prior-art search pending. No novelty claim is made.

The server clock holds its final admitted day after the finite horizon. Late
recovery remains available in that terminal phase; no operator clock reset is
needed. This does not automatically submit a refund or release.
The typed `invoke_at` helper uses the same terminal phase, records the bounded
logical day, and continues to refuse negative times.
