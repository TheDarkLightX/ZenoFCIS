# Equipment booking

One capacity-two pool books one unit. Reserve creates version1. Confirm before
logical threshold4 or Timeout at4 consumes that version once; the other operation
refuses a stale version. Confirmation retains the held unit and emits one booking
intent. Timeout releases exactly one unit. Winning-command retries preserve all
fields and emit nothing. Authorization and logical time are supplied assumptions.

The pool, versioned register and deadline have distinct state owners. The
register is the catalogue's `C=3` instance: it holds the booking phase as its
value, and Timeout writes phase 3, which the `C=2` instance cannot hold. The
bounded counter owns no state: its view is the register's version, so its step
only checks that each step adds one to the version and changes nothing.
In `composition.json` the winning-command retries are the `confirm-retry` and
`timeout-retry` occurrences, which take no step. Each occurrence's listed step
order is descriptive: every step reads the same pre-state.
The complete decision is declarative data evaluated by the library Authority.
No application Rust decision callback is introduced.

This profile has 97,200 raw tuples, four legal states, 15 positive input tuples
and four first-confirmation intent tuples. The complete reference passed an
independent comparison; actual Authority replay, generated native tests,
the two durable/CAS journeys and runtime fault controls remain pending.
Local metadata and Identified component references do not prove the composition.

There is one booking and one fixed absolute threshold, with no rebooking,
release after confirmation, authenticated clock or real equipment fulfillment.
Concurrent commit safety requires the SQLite store checks, not the pure model.
