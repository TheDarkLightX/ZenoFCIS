# Order fulfillment with a split state and payment payload

This fixture restates the shipped `templates/order-fulfillment` contract. State
field 120 is consumed by Boolean field 122 `paid` and integer field 123
`progress` (0..2): Placed=(false,0), AwaitingPayment=(false,1),
Cancelled=(false,2), Paid=(true,0), Shipped=(true,1), Delivered=(true,2).
Field 121 keeps the payment-attempt count. Payment channel 300 keeps field
140 and replaces action 141 by Boolean capture 142 (Capture=true, Void=false).
Shipping channel 301 keeps its existing payload.

Rules, reasons, branch order and laws 500-506 preserve the original contract's
observations, including CommittedFailure with reason 204 on payment decline.
The migration uses the existing closed expression programs. Historical delivery
bytes and IDs remain under their original version; only future publications use
the new representation.

The decision examples map every retained shipped label through these explicit
state and payload mappings. The source file's provenance and owner-review
statement concern the original example labels. Its SHA-256 is retained in the
derived file. No new owner approval is claimed for the transformed rows.

Forward-simulation counts (24 states, 20 admitted, one genesis, 1,440 compared
tuples) are assertions for native qualification, not evidence of a completed run.
