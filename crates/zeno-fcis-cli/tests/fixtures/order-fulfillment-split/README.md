# Order fulfillment with a split state and payment payload

This fixture restates the shipped `templates/order-fulfillment` contract. State
field 120 is consumed by Boolean field 122 `paid` and integer field 123
`progress` (0..3): Placed=(false,0), AwaitingPayment=(false,1),
Cancelled=(false,2), Paid=(true,0), Shipped=(true,1), Delivered=(true,2), Settled=(true,3).
Field 121 keeps the payment-attempt count. Payment channel 300 keeps field
140 and replaces action 141 by integer request kind 142 (Capture=0, Void=1, Refund=2, Release=3).
Shipping channel 301 keeps its existing payload.

Rules, reasons, branch order and all project laws preserve the original contract's
observations, including CommittedFailure with reason 204 on payment decline.
The migration uses the existing closed expression programs. Historical delivery
bytes and IDs remain under their original version; only future publications use
the new representation.

The decision examples map every retained shipped label through these explicit
state and payload mappings. The source file's provenance and owner-review
statement concern the original example labels. Its SHA-256 is retained in the
derived file. No new owner approval is claimed for the transformed rows.

Regenerate with `tools/refresh_order_split_fixture.py`; it transforms the
current template inputs and invokes the contract generator. The checks use
forward simulation, as described in Baier and Katoen,
[*Principles of Model Checking*](https://mitpress.mit.edu/9780262026499/principles-of-model-checking/).

Forward-simulation counts (28 states, 23 admitted, one genesis, 22,356 compared
tuples) are assertions for native qualification, not evidence of a completed run.
