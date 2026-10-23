# Shell effects: ports, pure programs, simulation and replay

An application's shell commits publications, recovers from a lost reply and
delivers outbox records. The aim is one shell that runs unchanged against real
storage, a deterministic faulty world or a recorded run. The shell's effects
are declared once, its decisions are pure programs, and handlers carry the
effects out. This page covers the registry, the programs, the simulation, the
record and replay layers, the explorers and the production SQLite handler.

## A commit whose reply is lost

A program asks to commit key K at version 3. The store applies the commit, but
the reply is lost, so the program sees `Unknown`. It reopens the store and reads
the audited history:

- If K is there with the exact publication, it retries without a version guard.
  V2 then answers with the original receipt, marked as a replay. Retrying
  `commit_at(3)` would be refused, because V2 checks the expected version
  before it looks up the replay key.
- If K is absent, it retries with the original guard, version 3. It never
  relaxes a guard because a record is missing.

After a crash there is no reply at all. The caller restarts the retained
request with `CommitProgram::recover`, which reopens and reads history first.
The `v2_recovery_rule` test runs both lost-reply cases against a real SQLite
store.

## The registry

`zeno_fcis_shell::effects::registry` holds `zeno-fcis/shell-effects/1`: one
signature for each of ten ports (ReadState, Commit, ReadHistory, ReadOutbox,
Send, Acknowledge, Reopen, Clock, Random and Wait). Each gives the tag, the
argument and result schemas with byte caps and integer ranges, the capability
and footprint, the IDs of its handler laws, and its default V2 calls.
- `registry::check` refuses a missing, repeated or misordered port, a malformed
  schema, a footprint beyond its capability, an unknown or unused law, and a
  store port with no V2 mapping. `registry::conforms` checks values.
- Send maps to the external transport that `relay::export_line` feeds. Clock,
  Random and Wait have no default V2 API. ReadHistory maps to
  `Lineage::audit_history_read_only` and also needs Authority: the adapter
  rebuilds each commit's publication data with `audited_publication`.
- Send answers `Sent`, `NotSent(Code)`, `Unknown(Fault)` or a policy's
  `Refused(Code)`. Wait answers a
  tick, `Interrupted` or a fault; an interrupted delivery run leaves the record
  pending. It ends `Refused(Interrupted)` before any possible send and `Unknown(Io)` afterward.

The same ten signatures are written in ZAL as
`integrations/zal/examples/shell_effects.zal` ([types and effects](zal/EFFECTS.md)).
Lowering that source gives these canonical bytes exactly; a test compares
them, and the names the bytes only number, with the registry built from the
same tree.

## Programs and the driver

A program is `start(invocation, policy)` and `resume(continuation, response)`:
total, pure functions that return `Done(outcome)` or
`Request(id, operation, continuation)`, with no async, closure, macro or I/O.
A continuation has no `Clone`, so it is resumed at most once.
- **Commit with recovery** follows the rule above with a bounded retry budget.
  Running out of retries gives an explicit `Unknown`.
- **Outbox delivery** sends each pending record's exact bytes and acknowledges
  it only when the transport's report matches its ID and payload hash. NotSent
  and Unknown leave it pending; a retry waits a backoff plus a jitter draw. An
  unordered outbox, an early wake or an out-of-range draw refuses with `Protocol`
  before any possible send. After Sent or Unknown, every subsequent failure
  ends Unknown, including acknowledgement exhaustion and later-record refusal.
  Only a complete run returns Ok; NotApplied never erases earlier send progress.
  The individual refusal remains in the transcript.

The driver checks the program's declared capabilities and holds one
outstanding request. It admits a response only if the epoch, ordinal, port,
store, handle and subject match and the result fits its schema; otherwise the
program does not advance. The registry pins the SHA-256 of `program.rs`, the
file the purity checker and the conformance test read.

`zeno_fcis_shell_sqlite::effects::bind_publication` rebuilds a publication
from `PublicationData` through Authority and refuses another identity, another
subject or inputs that do not commit. No V2 API changes.

## Simulation, stacks, record and replay

`effects::simulation` is a terminal handler over an explicit world: durable
store, each handle's volatile state, transport queue, receiver with durable
deduplication, and an epoch clock. Every fault point is an explicit choice:
answer, busy, an ambiguous reply, a lost reply after the write, a crash before
or after it, drop, duplicate, a wake time or interruption, and a jitter draw. A
lost reply may still arrive; it is offered late at every later point and must be
refused. Only the simulation chooses.

A stack is layers in front of one terminal. `handler::bind` refuses a terminal
that does not cover a port the program can request. The record layer appends
every request, answer, crash and outcome to a transcript. Each link holds the
SHA-256 of its predecessor, starting from a manifest that binds the registry,
the program source, each qualified handler implementation, stack order, the
schedule, configuration, explicit seed, initial world and start.
This is integrity evidence, not a signature. `record::replay` re-runs the
program from a transcript with no handler: each request must equal the
recorded one, and anything missing, extra, reordered or changed refuses.

## Exploration and laws

`effects::explore` runs every schedule of a profile depth-first or
breadth-first. Each node re-runs from the start, because a continuation is
never resumed twice; states at every choice are fingerprinted and expanded
once. A run is `complete` only with no frontier left; a budget stop is
`incomplete` and a malformed profile or choice is `refused`. An incomplete
report owns an opaque checkpoint with the popped and queued prefixes, visited
states, final states and accumulated evidence. Pass it to `explore::resume`
with an additional run budget. Source or profile substitution refuses. This
is an in-memory continuation, without a persisted checkpoint format. Report
statistics are advisory; editing them cannot change the retained progress.

The frozen profile has two handles, two keys and two publication bodies, two
deliveries per commit, a queue of two, ticks 0 to 3, jitter below 2, and retry
budgets 0 to 2. One crashed run per actor may restart. Two scenarios are
explored: two commit programs on two handles, and delivery to a deduplicating
receiver.
- Every handled request is checked against the nine registry laws. Probes from
  every reached store cover requests no program makes, such as changed bytes
  under a key or an unknown delivery ID.
- Each stack is qualified separately: the bare simulation, record, and
  identity forwarder with record. Every final transcript must replay exactly.
- Every final state must hold the protocol invariants: the store is the fold of
  its history by an independent specification that does not call Simulation,
  including every delivery field, each key applies once, receipts belong to the
  original key, no effect repeats past deduplication, nothing is sent before it
  is committed,
  and only a record that was sent is acknowledged.

## Qualified response alphabet

`simulation::FAULT_ALPHABET` names `shell-faults/2`. Successful read/commit/ack
responses retain their exact schemas and existing V2 refusal codes. The choices
below occur in the listed order; each injected failure is explicit data.

| Port | Choices |
|---|---|
| ReadState, ReadHistory, ReadOutbox, Reopen; refused or replayed Commit; refused Acknowledge | Exact answer; NotApplied(Busy); crash; NotApplied(Full); NotApplied(Io) |
| Applied Commit/Acknowledge | Exact answer; NotApplied(Busy); Unknown(Io) without applying; apply with lost reply Unknown(Io); crash before; crash after; NotApplied(Full); NotApplied(Io) before applying |
| Send, with queue room | Sent; NotSent(Delivery); Unknown(Io) with drop; carry with lost report Unknown(Io); crash before; crash after carrying; duplicate if two slots remain |
| Send, queue full | NotSent(Delivery); crash |
| Clock | Ticks from current through 3, ascending; Err(Busy); Err(Full); Err(Io) |
| Random | Draws from zero below the bound, ascending; Err(Busy); Err(Full); Err(Io) |
| Wait | Wakes from max(deadline,current) through 3, ascending; Interrupted; Failed(Busy); Failed(Full); Failed(Io) |

A Wait with an out-of-profile tick or another epoch has no successful wake.
The forwarding Fault handler uses the same fault classes. Its successful time
choice forwards once to Time rather than enumerating ticks or draws. Scheduler
choices separately select an actor, receiver copy, restart, statement-boundary
interruption on SQLite, or late reply. Policies can additionally refuse Send.
These are finite environment choices; Send ambiguity uses Io, and this profile
does not qualify every arbitrary Transport/Time implementation. Transcript
histories are projected out at scheduling fingerprints. The production explorer
includes retained adapters, pending injection, connection/cache state, actors
and the complete store and receiver snapshots; its test adapters expose all
future-relevant state. Native SQLite/OS internals remain environment premises.

## The production SQLite handler

`zeno_fcis_shell_sqlite::effects::production::Production` is the terminal for
one handle on one store. Each port calls the V2 API the registry names, so no
commit SQL is added or reordered. Path rebinds additionally compare the
audited genesis, Authority and interpreter with the initially retained store
identity, including lazy reopen and history reads. Send goes to a caller's
`Transport`; Clock, Random and Wait go to an opt-in `Time`, and stay uncovered
without one. V2 refusals
keep their codes. SQLite busy or full is `NotApplied`, because the transaction
rolled back. Any other SQLite failure is `Unknown` for a write and `NotApplied`
for a read.

A store's identity is the SHA-256 of its original genesis envelope, its
Authority's identity and the delivery interpreter's identity; a handle adds
its number. Two stores created from the same genesis share an identity. A
request for another store or handle is refused untouched.

`handler::Fault` sits in front of any terminal. It makes the simulation's
fault choices for real: busy, full, I/O, an ambiguous answer, a lost answer after
forwarding, a crash before or after, and a duplicate Send. It never
forwards a commit or an acknowledgement twice. The
`effects_production` tests check the production handler against the
independent protocol model from the SQLite fault simulation. After every
step they compare a complete snapshot of every `v2_*` table and the
receiver. They cover:
- every fault schedule of two commit programs on two handles at retry
  budgets 0 and 1, and of delivery to a deduplicating receiver at budgets
  0 to 2 (commit at budget 2 is checked only in the simulation);
- an interruption at each of the eight statement boundaries of a commit,
  followed by recovery;
- a real busy store;
- the ten real process aborts, each followed by recovery and redelivery.

Disk-full and other I/O failures are qualified only in the simulation, by
the `NotApplied` and `Unknown` branches they map to. No deterministic SQLite
fixture produces them.

## Authenticated local HTTP ingress

`zeno-fcis-ingress` is an opt-in shell crate. It uses a hand-built current-thread
runtime and HTTP/1, outside every purity-checked semantic crate. TLS terminates
at a trusted proxy in front of the ingress. `Server::serve_one` requires a
loopback listener and peer. `serve_connection` accepts a caller-protected local
byte stream; its caller is responsible for transport and peer validation.
Neither API supplies application decision authority.

Configure one random 32-byte credential, its principal, the SHA-256 of the full
Authority identity, one exact browser origin, and a nonzero deadline up to 60
seconds. The credential header is exactly `Authorization: Bearer ` followed by
64 lowercase hexadecimal digits. A full-width comparison checks the decoded
credential. The source rule checks that comparison's form; it does not prove
compiled-code or hardware timing independence. No credential enters a trace.

The committing route is `POST /command`. The optional authenticated
`GET /state` and `POST /preview` delegate to application reporting and
preparation methods; their default implementations refuse. A preview cannot
commit. `Server::with_assets` installs an owned, closed list of public GET
routes, with at most 128 assets and 32 MiB total, and no asset above 16 MiB.
Those routes perform no filesystem lookup. GET routes refuse framed bodies;
all routes retain header, origin and deadline checks. Application views and
previews require the configured credential. `POST /preview` uses the same
request envelope and body cap; its tail is application preparation data.

The command route's binary body is version byte 1, contract
hash (32 bytes), caller-retained retry key (32 bytes), expected revision (8 bytes,
big endian), then the exact canonical application command. `encode_command`
constructs it. There is no actor field. Headers are bounded to 4096 bytes and 24
entries; all repeated headers and non-ASCII values refuse. A bounded prefix
guard rejects duplicate Content-Length and length-plus-transfer-encoding before
hyper can normalize them. Body framing remains entirely with hyper. Bodies, including
chunked bodies, are bounded to 65536 bytes while streaming. A present Origin
must match exactly; cross-site Fetch Metadata also refuses. Missing Origin is
allowed for native clients. The same total deadline covers header and body
acquisition. Once entered, a synchronous core or SQLite call runs to completion;
the deadline cannot roll back a committed transaction.

`Durable` binds the principal to a defensively copied canonical application
context. The application must express that principal's rights in its checked
contract. Audited history recovers the original inputs, not a capability.
Authority rebuilds every committing publication. Commit and delivery use the
existing checked programs and production terminal, retaining the transaction's
expected-version check and atomic state, receipt, replay and outbox publication.
No SQL or schema order changes. A retry key is scoped by principal and contract;
changed command bytes, context or expected revision under that key refuse.
After uncertainty retain the exact body and original key. Never generate a new
key just because a reply was lost.

A successful response body (`ZFIP1`) contains request and scoped replay hashes,
the original publication subject and poststate (each with an 8-byte big-endian
length), then original commit version (8 bytes) and chain hash (32 bytes).
Fresh commit and replay return identical packet bytes; the transient replay
status and delivery progress are excluded. HTTP failure statuses distinguish
malformed request (400), credential (401), origin/binding (403), size (413),
replay/revision conflict (409), core refusal (422) and uncertainty (503). A 503
may follow a successful commit. Delivery retries are bounded to two with the
program's backoff and supplied time adapter. Exhaustion preserves the pending
outbox. Send and acknowledgement still rely on the terminal's transport report,
which is no proof of authenticated receipt or exactly-once external effects.

`TraceSink` receives only a fixed event and optional request, replay, decision,
commit and delivery hashes. Early parser/credential failures have no admitted
request ID. Crashes may leave only a prefix. Sinks must preserve that prefix;
they must not invent a commit event. No request bytes, context, payload, header,
credential or arbitrary error text is passed to the sink.

Ownership follows one serial path: configuration and admitted command own their
bytes; Authority and its bound descriptor remain immutably borrowed; publication
views cannot outlive their borrowed inputs. The server exclusively owns its
terminal and sink. Its short RefCell borrow begins after body acquisition and
ends before the next request. Program continuations and output packet buffers
are invocation-local owned values, discarded on refusal or moved to the reply.
Terminal storage state is effectful shell state, not a core cache. Per-handle
abort controls exist only with `crash-injection`, preserve the original mode's
revision/cap guards, and are consumed on the next commit.

The ingress tests use real loopback by default. `INGRESS_TEST_MEMORY=1` explicitly
selects a socket-free HTTP byte-stream adapter. It exercises the same parser,
credential checks, programs and production SQLite calls, including actual child
process aborts, but supplies no TCP or loopback evidence. The fault corpus covers
each applicable stage-2 Fault choice individually, eight SQLite statement
boundaries, a real busy transaction, duplicate/reordered deliveries and a lost
reply. It is a targeted corpus, not the exhaustive stage-2 state space. Disk-full
and other I/O faults remain simulation-only. There is no power-loss qualification
or delivery-liveness claim.

Run the ingress tests with the pinned locked toolchain. Run
`python3 tools/check_ingress_controls.py --out <receipt.json>` for seven compiled
negative controls and restored baselines; `--memory` selects only the explicit
socket-free evidence lane. `tools/check_ingress.py` checks the source inventory,
credential rule and dependency feature pins. `--base-lock <Cargo.lock>` also
requires precisely the reviewed add-only dependency closure. Source-bound
receipts must be refreshed after final edits. These checks are development
evidence, not release authority.

The root Cargo manifests belong to the evaluator's approved source closure.
Workspace or dependency changes therefore require the sanctioned
`tools/check_authority_v2.py --generate-sources` command with the approved
`verification/verus/authority_v2_sources.json`, followed by `--check-sources`.
This intentionally changes evaluator and Authority identity bytes. Requalify
source-bound artifacts before adoption; opening an existing store under a
changed identity requires the explicit library upgrade path.

## Checks

`tools/check_shell_effects_controls.py` builds twenty-five mutants. Each must fail
its named check while the unmodified tree passes:

| Control | Check |
|---|---|
| Acknowledge signature removed | `port_registry` |
| Subject comparison removed from binding | `publication_binding` |
| Store comparison removed from the driver | `wrong_store_response` |
| Ordinal binding removed from the driver | `duplicate_response` |
| Recovery always retries unversioned | `step_conformance` |
| A step function reads the clock | `zeno-fcis purity crates/zeno-fcis-shell` |
| A commit writes one of its two outbox rows | `simulation_laws` |
| Reopen resets the replay keys | `simulation_laws` |
| An acknowledgement sets another record's bit | `simulation_laws` |
| A wait wakes before its deadline | `simulation_laws` |
| A jitter draw reaches its bound | `simulation_laws` |
| The driver admits older ordinals | `late_replies_refused` |
| Replay continues live when the transcript ends | `replay_is_exact` |
| Chain verification skips its link checks | `transcript_chain` |
| Binding skips the coverage check | `stack_binding` |
| The store identity ignores the Authority | `store_identity_binds_genesis_authority_and_interpreter` |
| Production commits `At(v)` without its version | `production_agrees_with_model` |
| Production reports SQLite busy as `Unknown` | `production_real_busy` |
| Production fingerprints omit retained time | `production_fingerprint_retains_clock` |
| A committed delivery names the wrong recipient | `simulation_laws` |
| A later failure erases send uncertainty | `lost_send_remains_unknown_after_clock_failure` |
| Reopen accepts a substituted genesis | `production_rebind_rejects_other_genesis` |
| The fault alphabet omits Full | `frozen_alphabet_contains_required_failures` |
| A checkpoint drops its frontier | `resumed_search_matches_uninterrupted_closure` |
| A manifest omits handler sources | `manifest_sources_configuration_and_seed_are_bound` |

Control tools separate Cargo workspace artifacts by source-copy path using a
pass-through compiler wrapper. `tools/test_cargo_source_scope.py` checks
backdated copies, a building mutant and restoration against one shared target.
This protects the named behavioral checks from stale artifact reuse.

The default tests explore retry budgets 0 and 1 in the simulation, and fewer
on SQLite. The ignored test `explore_frozen_profile_all_budgets` covers every
budget in the simulation. On SQLite, the ignored tests
`production_agrees_with_model_commit_budget_one` and
`production_agrees_with_model_delivery_budget_two` close commit at budget 1
and delivery at budget 2. Commit at budget 2 is checked only in the
simulation: each budget multiplied the SQLite runs about seventy-fold.

## Opt in from an application

“I want my app's shell to run against real storage or a recorded run, so I
can reproduce a lost reply and check recovery before deploying.”

The generated `withdrawal-queue` template offers this as an optional library
API. Enable its `shell-effects` Cargo feature; its existing entry points keep
their default behavior even with that feature enabled. In a newly generated
app, run `cargo +1.97.1 test --offline --features shell-effects` once to prune
the copied workspace lock, then add `--locked` to subsequent commands.

Open `shell_effects::Effects` with an existing store, its checked single-version
`Lineage`, and your `Transport` and `Time` adapters. Retain a `CommitInvocation`
containing the original key, guard and `publication_data` from a genuine
publication. Call `commit(&request)`; after an uncertain result or a restart,
call `recover(&request)` with those same inputs. Call `deliver(epoch)` to send
pending payouts. These calls use the production handler, which rebuilds each
publication through Authority. Each run allows two retries and at most 4,096
requests; exhaustion can leave an unresolved outcome and pending payouts.
Delivery waits ten adapter ticks before retrying, with zero jitter.

For an unmonitored `Effects::open`, recording starts off.
`record_private(true)` enables the Record handler;
`transcript()` borrows the last run's private in-memory evidence. The next run
replaces it, and disabling recording discards it. Capture never writes a file,
prints a transcript, changes relay export or supplies authority to a decision.
Recording requires both adapters to supply `AdapterProvenance`: their actual
source digest, configuration including retained state, and seed (zero without
randomness). Missing provenance refuses capture before executing the run. The
production manifest also binds the SQLite handler source and connection
configuration. The template adds its own source, retry limit and request cap.
These declarations are provenance, not authentication or proof
of adapter conformance. Keep any copy private. Its manifest identifies the
store, not a complete initial database snapshot; retain that snapshot separately for investigation.

Tests compare default bytes, original-key recovery, matching acknowledgements,
retry exhaustion and private capture. The seeded performance check runs the
unchanged 256 × 64 SQLite corpus twice and, in its opt-in lane, also mounts this adapter once
per seed against the independent model. These are tests; the finite simulation
checks above apply only to their declared profile. The premises include retained
original inputs, a stable identity, surviving SQLite storage, one trusted writer
and relay, the compiler, OS, codecs and hashes, and the receiver's contract.
External effects may repeat; power failures and guaranteed progress are not
qualified.

## Scope and limits

The evidence is finite exploration of the stated profile, tests and a static
rule check; it is not a proof. The laws are checked against the simulation and
its stacks. SQLite is checked by agreement with the independent model, and
the transport is only a test receiver. The premises are one store,
one trusted writer and relay, and two handles; history absence is trusted
only under that single-writer premise. Creation, upgrades, multiple stores,
multiple processes, exactly-once external effects and power-loss durability are
not covered.

## Opt-in effect policies

A policy is data and a pure step:
`policy::step(&Policy, &State, &Operation, Option<&Response>) -> (State, Verdict)`.
`Verdict` is only `Allow` or `Reject(Code)`. It cannot contain an operation.
`None` admits a request; `Some(response)` observes its result. The shipped
policies read no response payload fields and never release a reservation.
They are defense in depth: the core still decides and grants publication
through Authority. A policy cannot authorize a new action.

Bind `Layer::Policy(Monitor::new(policy))` before the terminal in the opt-in
`Stack`. Every request, including retries, passes all policies before one
terminal call with the original bytes. A rejection returns `Sent::Refused`
with `PolicyDuplicate`, `PolicyRecipient`, `PolicyFields` or `PolicyCapacity`;
the delivery program stops immediately, retaining the pending record. Its
outcome is Refused before any possible send, or Unknown after earlier send
uncertainty or progress. The record layer captures that response and the final
outcome. The transcript also carries the initial monitor snapshots and an
ordered admission record for every request. Handler-free replay evaluates the
same immutable policy configuration and state, compares every verdict and
proposed successor, and enforces the corresponding refusal before resuming.
Changing or omitting a policy record fails even if its hash chain is rebuilt.
Success, refusal, Unknown, NotApplied and crash are explicit transcript events;
the latter three classify as incomplete actions. Exploration closure and
individual action completion are separate results.

| Policy | Fields read besides the Send tag | Rule |
|---|---|---|
| `AtMostOne { capacity }` | `Store.identity`, `Delivery.id` | At most one forwarded Send attempt per pair, including identical sends |
| `Policy::allowlist(recipients)` | `Delivery.destination` | Exact destination bytes must occur in the bound list |

Send carries no publication replay key. Its effect key is the store identity
and delivery ID, shared across handles. It is not a payload hash or a subset
comparison. Every actual attempt consumes its reservation, including NotSent,
Unknown and crash; capacity exhaustion refuses. A suspension releases a
reservation only with explicit evidence that no effect invocation occurred.
`Terminal::handle_with_invocation` returns an owned outcome and conservative
invocation report; its default is `MayHaveInvoked`, including on Suspend.
The direct simulators establish `NotInvoked` at their unselected branches.
`Fault` combines every inner invocation report: once any call may have
forwarded, a later Suspend keeps the reservation. Transport duplication can
still produce multiple copies from that single admitted send.

The versioned policy identity binds the canonical allowlist, capacity, field
declarations, and source of `policy.rs`, `handler.rs`, `policy_record.rs`,
`record.rs` and `codec.rs`. Projection exposes
only declared fields to the decision body; an absent field access refuses.
Stack order, policy identities and initial reservation states are bound into
the explorer's manifest start bytes. Policy state and the independent send
multiset enter each explorer fingerprint and survive modelled restarts.
`explore_with_policies` returns complete, incomplete or refused, with identities,
source, action counts and premises; violations include a reproducing schedule.
Use `next_choice_with_policies` and `transcripts_with_policies` to reproduce
that schedule under the same configuration.
The frozen policy check uses both policies, retry budgets 0-2, every modelled
response, and both an admitted and a denied recipient configuration.

Ownership review: step takes transitively immutable shared inputs and returns
owned state. Its cloned reservation vector is exclusive scratch, never a
mutable alias of the input. `Monitor` and `Stack` are retained shell handles,
not decision, metering or publication authority. Only their exclusive owner
installs a successor after all policies admit, before calling the terminal,
including if that call unwinds. Another policy's rejection discards all
proposals. A suspension rolls back only when the terminal establishes
non-invocation across the complete composed call. Invocation evidence is owned
per-call scratch, not a retained last-call flag; unqualified terminals keep
reservations conservatively. Suspended prefixes remain incomplete and do not
constitute completed replay evidence. The monitor's
private bound configuration permits caching its identity without invalidation.
The source digest above pins this review to the implementation being checked.
A closed two-case policy enum and projected fields avoid a callback runtime,
extra operation language and response-dependent rollback bookkeeping.
The shared `policy_record::admit` evaluates immutable snapshots and returns
owned candidate monitors and evidence. Both the stack and replay use it;
independent payment acceptance uses actual commit occurrences, never this
helper's verdict as evidence of authorization.
Existing port tags, failure codes, request and response encodings stay unchanged.
The policy transcript event appends tag 4; monitored manifests append a
versioned policy-snapshot section. Unmonitored event and manifest field encodings
retain their shape. Source-bound policy and record identities intentionally
refresh, changing dependent transcript digests. The program source, SQL order
and schema stay unchanged. Existing independent regression checks remain.

Provider idempotency is an environment assumption. A timeout can hide success.
The claim is "no second send of the same key by this protocol", never
"executed exactly once externally". One trusted relay must retain policy
state across retries and restarts; real process loss requires durable state
restoration before sending. That persistence adapter is outside this finite
simulation check. Receiver deduplication remains a separate environment law.
Every policy exploration result carries these premises and nonclaims.

`tools/check_effect_policy_controls.py --out <file>` builds six source mutants:
recipient rewrite, duplicate send admission, missing allowlist, silent record
drop, undeclared field access, and skipping policies on retry. Each must fail
its named behavioral check and pass again after restoration. The ignored
`policy_frozen_profile` test runs the complete frozen policy product.

### Generated payment sender and independent product

The generated `withdrawal-queue` shell offers `Effects::open_with_policies`.
It binds a policy lifetime at open and retains the stack on the handle before
calling a transport, including if that transport unwinds. Every run shares
those reservations. A monitored handle requires private capture and adapter
provenance. It has no live policy-reset operation. Opening another handle
starts another lifetime; durable reservation storage and restoration remain
an application obligation before sending after real process loss.

The [payment sender journey](../examples/effect-policies/payment-sender/README.md)
shows the hidden-success case and its exact finite domain. `policy_product`
runs the real delivery program, driver and policy stack against a separate
finite payment environment. The Python reference independently implements the
controller, monitor, environment and canonical encoding. It compares every
prefix through a declared request bound K, keeping exact traces. Counterexamples
include a policy bypass followed by two payments of one key, and duplicate
transport copies without the declared provider-idempotency assumption.
Request-bound leaves classify as incomplete actions, even when the finite
prefix tree has been explored completely.

### Background, contribution and scope

Background: Matija Pretnar, *An Introduction to Algebraic Effects and Handlers.
Invited Tutorial Paper*, Electronic Notes in Theoretical Computer Science,
2015, describes effect requests and handlers. Fred B. Schneider, *Enforceable
Security Policies*, ACM Transactions on Information and System Security,
2000, studies execution monitors. Edmund M. Clarke, Orna Grumberg and Doron A.
Peled, *Model Checking*, MIT Press, 1999, describes transition-system products
and counterexamples.

What is new here: these mechanisms are integrated with the shell's source-bound
transcripts and bounded product evidence. Comparative prior-art search pending.
No novelty claim is made.

Scope: finite checks cover the declared domains and environment assumptions.
Runtime and SQLite tests establish the stated tested behavior; they supply no
decision authority, authenticated delivery, guaranteed progress or exactly-once
external-execution claim.
