# Scalar directory and mailbox composition

This LLM-generated example checks two local, channel-free cores. One directory
has two device entries; one mailbox has two message slots. Reads must use the
selected device's status and key generation. `composition.json` fixes bindings
and order. `tools/check_service_product.py --repin --binary PRODUCT_BINARY`
regenerates source and checked-program pins through the compiler.

The complete admissible product contains 3,414 states and 16 requests (54,624
rows), below the 65,536-row cap. Admissibility means both local invariants and
all global couplings hold. Unreachable admissible states are included. The
independent Python interpreter compares outcome, reason, and every successor
field, and separately checks accepted reads. Over-budget work is refused.

This check grants no store or publication authority. The directory has no
channel; a delivering directory is a different subject. There are no signature,
storage, finality, caller privacy, live credential, or collection-scale claims.
The directory's anchor operation is a modeled command, not verified finality.

## Lineage and scope

Background: C. A. R. Hoare, *Communicating Sequential Processes*, CACM, 1978,
and Lamport, *Specifying Systems*, Addison-Wesley, 2002, describe composition
and explicit state-machine reasoning. These references do not certify this
implementation. Prior-art search pending. Evidence is exhaustive only over the
declared finite domains and the specified admissibility predicates.

## Local service authority

`runtime/` adds explicit read-only validation commands so every core's actual
checked Program participates on every request. `service.json` pins the nested
composition, the schemas, request/decision laws and the service budget. Run
`tools/make_service_manifest.py --binary PRODUCT_BINARY runtime_directory` to
regenerate these pins. A schema, policy, program, binding, order or checker
change changes the service identity and requires a new store version.

`ServiceAuthority` returns a private, immutable `ServicePublication` only when
all actual per-core publications and global laws pass. Rejection discards all
provisional successors and retains actual core and wrapper accounting. Each core's
checked step bound must fit the remaining service budget before it runs; the
receipt charges actual usage. Its
preparation tables supply bindings, not decision or publication authority.
This scalar profile refuses channels, CommittedFailure and skipped cores.

## Generated SQLite audit app

Run `python3 tools/service_app_factory.py --service
examples/composition-service/runtime/service.json --out OUTPUT_DIRECTORY` with
`CARGO_TARGET_DIR` pointing at the intended build cache. The generated binary
supports `serve DATABASE PORT` (loopback HTTP), `audit DATABASE`, and
`replay SOURCE_DATABASE FRESH_DATABASE`. `POST /request` takes JSON with
`principal`, `key`, and `values`; `GET /state` returns complete internal state.
These labels scope replay and are not authentication. Keep this audit example
on a trusted local boundary; caller views, credentials and fact verification
are outside this profile.

The generated shell consumes only the nominal service publication. One head
row stores the whole canonical state; one journal row stores the request,
receipt and replay key. Both change in one SQLite transaction. Every accepted
read crosses the global version/root fence. Reopening re-executes all checked
Programs and compares every receipt byte and the complete head. Recovery
returns the committed receipt. If a duplicate was already decided locally, its
actual attempt counters are returned separately from that original receipt; a
lookup performed before deciding has no core attempt counters. No automatic
retry resets invocation usage.
A stale decision is refused. Changed service identities require a fresh store;
there is no migration or mixed-version activation API.

The profile refuses channels, so the canonical outbox is empty. It does not
claim delivery, external effects, authenticated callers, private replies, or
recovery of private mail after revocation. `http-stdio DATABASE` exercises the
same HTTP bytes/dispatch path without sockets; this alone does not qualify TCP.

SQLite's *Atomic Commit In SQLite* and *Isolation In SQLite* describe the
single-database transaction mechanism used here. These are background
references, not certification of the generated shell. Crash tests exercise
abrupt process exit at the two writes and before/after commit; they do not
model physical power loss or faulty storage hardware.
