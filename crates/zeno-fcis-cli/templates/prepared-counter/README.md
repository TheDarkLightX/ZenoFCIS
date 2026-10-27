# Prepared counter

The original count domain is 0..3 and each of the three scalar deltas is -1..1.
Normal preparation uses the checked library-owned ordered cursor. It reserves
Read6/Write3/Candidate3/Byte120 with complete input98/output22 tuple framing.
A separate aggregate Step64 budget charges actual instruction attempts across
chunks, failed chunks and retries. Successful three-item execution consumes9
Steps. Dropping preparation cancels; restart admits and recomputes the original
ordered inputs. The cursor exposes no partial accumulator or publication power.

`cargo run -- NEW_DATABASE_PATH` prepares [1,1,1] in chunks one and two, commits
3, retries the exact original publication, and interrupts notification delivery
before acknowledgement. It reopens the database, retries that exact delivery
without duplicating the destination effect, then publishes [-1,-1,-1] and drains
to0. Actual final version/bundles/replays/pending/deliveries are2/2/2/0/2.
The memory destination retains its idempotency ledger only in this process;
a real remote destination must durably retain its own independent ledger.

`PreparedBatch::start` admits the actual persisted full history binding H through
the actual Authority before reserving preparation. The owned original state,
command and context envelope bytes, root/version, principal/authentication labels
and replay commitment bind its immutable operation. A sealed shell SHA-256
association supplies the cursor invocation context; this is not a proof of hash
injectivity. Finish derives context again from a fresh transactionally checked
Snapshot, then also compares complete H/state/context bytes and root/version.
Recurrent roots at another version and competing database handles refuse stale
work. The commit transaction repeats expected-version/root/chain CAS.

Publication independently reevaluates the complete original command/context
through Authority.admit_history(actual H). The cursor result remains untrusted
and must match the complete genuine successor, empty effects and exact ordered
notification, including roots, channel, ordinal and marker. Every original
named/framework law and scope remains required by the reviewed declaration.
Preparation and its result never substitute for the distinct HistoryPublication
capability consumed by SQLite.

The returned descriptive `Publication` retains its original full H alongside
R and the original invocation. Replay admits that source H before comparing it
with the destination's stored binding and recomputing R. A different admitted
policy refuses; exact replay under the original binding remains idempotent.

The original16,384-byte publication cap remains bare canonical command + bare
canonical successor Value + complete scoped authorization R + complete durable
bundle B. B includes duplicate R/successor, all state/commit row fields, sequence,
replay/pre/post roots, previous/current chain, receipt and every ordered durable
effect/outbox field, locator, marker, ID, acknowledgement and count. Receipt and
outbox sizes are reported as included B subcomponents. The actual measured full
aggregate is checked before persistence and again inside the committing
transaction, with min(requested,16,384); no caller-supplied lengths are accepted.

Schema8 stores actual full H once with scoped genesis and re-admits that stored H
on reopen, import, recovery and checked access. Canonical(H,R) and the actual
transaction certificate bind schema8 delivery identity, so different policies
and fresh ABA transactions cannot silently share a delivery ID. Schema7 standalone
bytes/APIs remain separate and unchanged. Mixed or old formats refuse; there is
no implicit durable conversion. Missing, truncated or changed source H cannot
fall back to a destination authority's current metadata.

The complete H copy/clone, admission equality, context hashing, pair construction
and SQLite I/O add O(len H) work outside the old invocation meter and aggregate
cap. Snapshot commit/replay counts query actual durable rows and may scan history.
This cap is not a limit on OS memory, elapsed time, physical storage or allocation.
SQLite/OS/observed acknowledgements and the sealed SHA-256 collision assumptions
remain the trusted shell boundary. No privacy or exclusive-handle claim is made.

Normal tests retain the original216 decision inputs and independently run all216
through scoped history/preparation, all68 successful commits and each one-under
aggregate refusal, four successful partitions, original seven crash boundaries
through the store's dev-only `crash-injection` feature,
retry/meter exhaustion/cancellation/incomplete, stale/ABA/second-handle checks,
exact replay/reopen/import and interrupted delivery. Checked finite-model exit
plans retain the original domains and also execute their chosen exits through
the real publication path. Model evidence does not authorize partial results or
promise fair scheduling/external delivery. The complete original native bodies
and ten regression assertions remain independently executable in the private
nonpublished template oracle.

SQLite is the optional `sqlite` feature, on by default. Without it the library
retains generated declarations/schema admission, the complete V2 contract,
profile metadata, delivery adapter and checked authority construction. `create`,
`journey`, `prepare` and the demonstration binary require SQLite. Strict native,
Clippy and no-default evidence is source-pinned separately. Independent review,
whole-workspace/full-V2 acceptance and fresh Wasm qualification remain separate.

`PreparedBatch::publish` takes the shell, authority, current context and byte
limit. Normal builds expose no interruption parameter or store crash API;
`cargo test` enables the store's feature through the dev dependency.

## Delivery obligations

Every accepted batch queues exactly one notification on channel 300 to
`local-observer`. Field 112, `notified_count`, equals the final committed count.
This includes a zero-delta batch and a batch whose deltas cancel. Law 510 checks
the actual candidate's count and payload; law 511 independently checks its
destination. Each bounds the complete outbox by one. Rejections emit no message,
and the original law 502 forbids committed failures. The original four law lines,
ordered-prefix rules and all 216 decision inputs remain unchanged.

These are local request obligations. They do not establish destination
availability, acknowledgement or an external effect. Publication still compares
the complete ordered request and independently reevaluates the original batch.
Preparation handles supply no decision or publication authority.

## Policy for the owner

The existing policy notifies once per accepted complete batch, including a
no-op, and sends no intermediate-prefix notifications. A denied batch takes
precedence over a capacity error. Its observer is the fixed local destination.
These choices remain as shipped; the delivery laws do not select a different
notification frequency, destination or rejection policy.

## Background and scope

The independent transition predicates follow E. W. Dijkstra, “Guarded commands,
nondeterminacy and formal derivation of programs”, CACM 18(8), 1975,
DOI 10.1145/360933.360975. The delivery revision applies those predicates to the
candidate's local notification request. Prior-art search pending because this
revision was offline; no novelty claim is made. Whole-domain delivery-gap
determination and guarantees about a remote receiver remain outside this check.
