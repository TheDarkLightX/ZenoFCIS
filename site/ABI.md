# Browser API, version 2

The browser is an untrusted caller. It may call the exports in any order and
pass any 32-bit words. It does not receive the module's linear memory, a
pointer, or an allocation to release. This intentionally replaces version 1's
pointer/length and allocate/free interface; the loader and modules must be
deployed together.

The module owns one request buffer (4,096 bytes), one response buffer (131,072
bytes), and one demo. It permits 64 complete UTF-8 requests per reset,
including requests the application's parser refuses. This bounds retained
demo history. Reset drops that history and returns to genesis. Beginning a
request, stepping, reading state, or resetting replaces the previous reply;
reading does not allocate or change state.

| Export | Contract |
| --- | --- |
| `demo_abi_version()` | Returns 2. |
| `demo_begin(length)` | Discards the pending request and reply. Returns 1 for a length from 1 to 4,096, otherwise 0. |
| `demo_write(word)` | Appends up to four bytes, least significant byte first. The final word's unused bytes are ignored. Returns 1 while filling the declared request; otherwise invalidates the request and returns 0. |
| `demo_step()` | Consumes a complete request exactly once and returns the reply's byte length. Missing/incomplete input, invalid UTF-8, an exhausted session, or a missing demo gives an input refusal before application execution. |
| `demo_state()` | Discards a pending request and returns the current state's JSON byte length. |
| `demo_reset()` | Discards the old session and pending request, builds the exact genesis, and returns its JSON byte length. |
| `demo_read(offset)` | Returns up to four response bytes, least significant byte first and zero padded. An offset outside the latest reply returns 0. |

A reply is UTF-8 JSON, with no length prefix. Input refusals retain the demo's
state. A zero result length means the boundary is unavailable (busy or
poisoned); the caller must discard the instance. If a report exceeds the
response capacity, the session is discarded and a transport error explicitly
says that a decision may have executed. No success or rollback is invented.

## Design and assurance

Previously, callers had to preserve pointer validity, allocation lengths,
ownership and exactly-once frees. Checking those would require an allocation
registry while still exposing writable Rust memory. The replacement has one
pending request, one latest reply, checked slices and scalar calls. It removes
all raw pointer operations from the interface. The build removes memory and
address exports from the compiled artifact: `wasm.py` rewrites only its export
section and copies all other sections and function entries byte for byte.
LLD exports memory even when no export flag is supplied. The loader and
compiled-module tests independently reject memory exports or imports.

Application decisions, admission, scoped laws, canonical authorizations,
commit checks and replay checks remain in the existing library path. This
change adds transport and session limits; it does not remove any authority
check or give the page external effects. The authority also validates the
pre-state against its own schema before constructing an invocation witness,
along with the command and context, under its own validation limits.

Evidence consists of ordinary boundary and capacity tests, compiled export
checks, existing decision-example replays, host tests and the served page's
browser checks. These are regression evidence, not a proof of the whole
application. The safety argument for this interface is Rust-owned bounded
buffers, checked access and private memory. Browser termination, resource
exhaustion outside these bounds, and compiler or library defects remain
outside this argument. Decisions that are LLM-generated rather than
synthesized keep their documented determinism contract.

## Factory module

The factory module uses these same seven scalar exports and capacities. Reset
installs an owned stateless request handler through `reset_instance`; it does
not authorize a decision. Each factory request builds fresh local preparation
data and invokes immutable library checker APIs. Reports own their JSON data,
and no authority, bound contract, frame or candidate reference escapes.
Synthesis candidate requests and the final synthesis call are separate bounded
messages so the worker can send real progress. The worker resets transport
before each act; the same 64-request limit includes parsing refusals.
Inventory progress uses `inventory_rows` with a checked start and count
(1–48 rows, wholly inside the 864-input domain), followed by the existing
complete review. `inventory_detail` accepts one checked ordinal and evaluates
both authorities afresh. Preparation is exclusively owned within each call;
neither request retains a model or checked-result alias. These requests add
no exports and preserve all byte, session and refusal limits.
The factory creates no publication, durable state or receipt.
