# API quota

One client sends requests to a metered API. Each request carries a key and a
payload, both 0..2. The service admits at most three distinct new requests per
epoch. Retrying the most recent request is answered from memory, in any epoch,
and is never charged twice. An admitted new request emits exactly one service
intent on channel 300 to `service`, with payload (epoch, key, payload). A
request with the remembered key and a different payload refuses.

## Epoch quota from the existing rate limiter

The quota is the catalogue's fixed-window rate limiter with `W=1, N=3`; no new
family is added. Its window is one time unit, so the supplied `epoch` acts as
an epoch number and `window_start` is the epoch of the current window:

- the first request in a later epoch resets the window to that epoch with
  `used = 1`;
- a request in the current epoch charges one more unit while `used < 3`;
- a request from an earlier epoch refuses (`stale_epoch`).

The memo is the idempotency slot `C=2`. It remembers only the most recent key
and payload. `composition.json` pins both instances, their views, field owners
(quota: 110, 111; memo: 112, 113, 114) and the two occurrences: `retry` records
the memo only; `admit` records the memo, then the quota Acquires. Each
occurrence's listed step order is descriptive: every step reads the same
pre-state.

## Difference from the earlier design note

The V2.3 research note built this application from an epoch quota, an
idempotent request slot and a versioned register. This profile drops the
versioned register: no decision here reads or writes a version. The memo
recognizes an exact retry by its key and payload, and the quota counts the
charges, so a register would own no field that any rule uses. Document release
and equipment booking still use the register family.

## Contract and evidence

The complete input domain has 63,504 tuples, including invalid coupled states.
The rules and coupled laws in `project.zeno` and `v2/policy.json` were written
from the requirements brief and compile into the ordinary library-owned
Authority contract. The independent Python reference used for the decision
examples was written separately from the same brief; the complete actual
Authority table is compared with it on every input. Neither is an owner
adoption or an independent review.

## Limits

- Only the most recent request is remembered. Key 1, then key 2, then key 1
  again with the same payload is a new request and is charged again.
- `epoch` and `authorized` are supplied data. Deriving the epoch from a clock
  and authenticating the client are shell obligations outside this proof.
- A service intent records a decision; it does not prove the service ran.
- Closed finite scope only: three requests per epoch, epochs 0..6, keys and
  payloads 0..2. No family-wide theorem.

`tools/check_core_app.py --app api-quota --static` checks the complete
reference corpus, and that the contract's rules, read through the names the
composition's views use, reproduce every written decision. On every accepted
input, each stepping component's input and successor, derived from the
composition's views, lie in its instance's domains, and its local reference
accepts that input and yields that successor; components that take no step keep
their state, the owners partition the state, and the declared effect is the
whole outbox. Every accepted input fires at least one declared occurrence, and
every declared occurrence fires on at least one accepted input. Separately, it
checks the composition against its reviewed pin and the component sources
against the catalogue. Its execution mode also checks the actual Authority, a
freshly scaffolded app, the durable journeys and law-violating candidates.
