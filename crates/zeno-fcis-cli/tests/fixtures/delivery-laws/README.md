# Delivery accounting overlay

`escrow-law.zeno` appends to the retained escrow `project.zeno`;
`escrow-law.json` supplies its `delivery_laws` object. Add law kind
`510: AssetConservation`. The native tests construct this overlay in memory
and use the existing generator, checked Authority and library law evaluator.

State fields 113 and 114 accumulate seller and buyer payouts; 111 is held
and 112 is funded. Existing state conservation relates these four amounts.
The overlay additionally equates the actual sum of channel 300 field 141 to
the increase in cumulative payouts. It counts that channel and admits up to
two total deliveries. Funding needs zero payments; splitting releases 5,000
held and queues 2,000 plus 3,000. The planted wrong split queues 5,000 twice,
while the counters still increase by only 5,000. The two-entry bound admits
its length; the accounting equality must refuse it with law 510.

The retained escrow examples were written by the same AI author as its
rules and are not an independent human oracle. Tests replay all 23 original
decisions through both Authorities and compare complete decision observations.
Direct library frames additionally probe missing/wrong-type payloads, overflow,
multiple channels and metering; they do not represent publishable Authorities.
No recipient execution, settlement, owner approval or new proof is claimed.

This fixture and its native checks are source-stage additions. Qualification
must run the `delivery-accounting-laws` acceptance scenario and the existing
contract checks; source inspection alone is not a test pass.
