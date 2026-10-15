# Reliable jobs: one job, three attempts

This declarative application combines a retry budget, idempotency slot and
logical deadline. One ordinary ZenoFCIS Authority checks the complete decision;
the generated SQLite application commits it atomically. Application business
logic is declaration data, with no decision or law callback.

Start charges one attempt and emits one execution intent. An exact Start retry
is harmless. Fail keeps the charge and enables the fixed logical threshold four.
Starting again before four refuses; at four the next attempt can start. A stale
completion refuses. Matching success closes the job; its exact retry is harmless.
The final third attempt works, and a subsequent fresh start refuses exhaustion.

The coupled state binds phase, consumed attempts, active attempt and the deadline.
There is no second attempt counter. The memo owns no state: its view reads the
attempts the retry budget has consumed, so its Record step only checks the
attempt number and changes nothing. Only the retry budget and the deadline hold
component state; the application rules own phase and clock. `composition.json`
names the component versions, source pins, owners, selected transitions,
inactive frames and coordinator. Each occurrence's listed step order is
descriptive: every step reads the same pre-state. Metadata cannot authorize a
publication. This is a closed application, rather than a general composer.

There are 96,000 declared raw input tuples, including unlawful prestates, and two
persistent journeys containing 17 steps. The reference was independently compared
with the bounded requirements; actual Authority, native, fault controls and review
remain required until their fresh qualification receipt passes.

Authorization and logical time are supplied assumptions. The threshold is
absolute and enabled by Waiting; it is not a relative or exponential timer.
An intent is not actual job execution, and a success event is not authenticated
completion. No reset, multiple jobs, liveness, unbounded family theorem or release
claim is made. Stored component proof references retain their original status.
