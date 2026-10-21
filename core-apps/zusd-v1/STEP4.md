# Conditional step-4 composition

Every authentication claim here means **authenticated given a verifier meeting
ReceiptVerifierV1**. The shell crate zeno-fcis-bls now authenticates receipts by
BLS (PoP quorum or G2Basic) over the canonical receipt bytes. No live owner profile
is provided.
The HMAC-SHA256 issuer exists only in tests/support/hmac.rs; it authenticates
explicit synthetic facts, not market truth, consensus or actual custody.

## ReceiptVerifierV1 contract

The port is [ReceiptVerifierV1](src/application/receipts.rs). It takes a domain,
an authority identity selected by the installed immutable policy, exact message
bytes and bounded proof bytes. It returns Ok(()) only after the domain's entire
contract holds. Invalid, unavailable and unsupported verification refuse; no
fallback, backend-supplied usage or returned economic plan is admitted.
The implementation and reachable policy must be transitively immutable and
pure: deterministic, no I/O, clock acquisition, retained scratch or shared
mutation. This is a contract assumption for implementations of the trait.

The canonical message is ASCII `zusd.ReceiptVerifierV1/1` followed by NUL,
domain u8, authority bytes32, chain bytes32, installed-policy bytes32, actual
pre-root bytes32, and a u64-BE-length-prefixed canonical request body. Proofs
are excluded from that body. The complete grammar is in
[encoding.rs](src/application/encoding.rs): mandatory fields, fixed closed tags,
unsigned BE integers, explicit 0/1 options, u64-BE byte/vector lengths, complete
consumption and canonical replay. Domains are 0 Actor, 1 Clock, 2 Oracle,
3 Custody, 4 Allocation, 5 Drip, 6 Funding, 7 ParameterPolicy, 8 ExposureRelease.
There must be every required domain exactly once, in that order, and no extra.
Genesis has its own body label, zero pre-root and Actor/Clock/Custody/Funding.

The body binds actor/nonce, full command, certified clock, backing/supply roots
and quantities, optional oracle record, allocation and drip facts. The policy
binds the whole source closure, feed/query/scheme/evaluator, custody roles,
limits, verifier authorities and every selected clock/economic/change profile.
`receipt_messages`, `genesis_messages`, encoders and source hashing are
preparation/serialization utilities, never decision, work or publication authority.

| Domain | Additional obligation of a conforming verifier |
| --- | --- |
| Actor | Actual actor owns the authorizing key/capability in the pinned registry and authorizes this complete action under this chain, policy, nonce and pre-root. |
| Clock | Backend/finality/synchronization evidence certifies the exact nondecreasing epoch and median-time watermark; a hash or service response alone is insufficient. |
| Oracle | Replay the accepted 5e13a1d oracle's exact committed/pending records and artifact graph under the pinned feed/query/scheme/profile/evaluator. The supplied canonical aggregate bytes and every extracted price, round, observation time, buffer, quarantine, status and capacity field must match. For Use, check this exact actor/action/root/epoch. PriceView itself is derived data and cannot authorize anything. |
| Custody | At this same root, every AGRS/zDEX wallet and reserved pool balance is backed once, without double-counting claims; the exact asset, denomination, health and adapter profile hold. |
| Allocation | The allocation root exclusively locks at least debt ceiling + liquidation cap + redemption cap in USD E18 to this consumer, across all feeds/consumers and the entire declared window and liability horizon. Capacity comes from the oracle's qualified independent stress/adjudication graph, never self-valued stake. |
| Drip | The named configured schedule, exact prior drip state, current certified epoch and verified zDEX supply/burn/funding inputs produce exactly this delta/next state and source event. No implicit zero schedule or calendar curve. |
| Funding | The unique source event represents an authorized transfer of existing AGRS/zDEX into this exact custody domain/recipient, with correct supply/funding roots; genesis authorizes the 35M zDEX reclassification from the funder's existing wallet. It cannot authorize zUSD mint. |
| ParameterPolicy | A separately approved, pinned admissible policy authorizes this exact change/actor/activation, not merely a signature from an arbitrary key. Installation of that policy is a separate adoption boundary. |
| ExposureRelease | Every named retained event has ended all linked exposure, dispute, finality and withdrawal liabilities. Elapsed local epochs alone do not prove those conditions. |

These graph/finality/custody obligations remain explicit issuer-profile premises.
The BLS adapter authenticates the complete attestation; it does not independently
replay the oracle graph or establish finality/custody truth. Qualified registry
adoption must establish these obligations, including exact accepted-oracle
extraction. See ../../crates/zeno-fcis-bls/CONTRACT.md for the concrete scheme and
registry contract. A signature alone does not discharge these semantic premises.

## Core and ownership

[initialize and decide](src/application/transition.rs) take immutable policy,
state, request and verifier references. They return an owned state or a sealed,
non-Clone Prepared witness. Public getters expose shared references only.
Policy owns Config; State owns all nested vectors; Prepared owns the cloned
request, successor, ordered movements, trace, measured usage and canonical bundle.
Caller mutation of a raw request cannot affect a retained witness.

Work and candidate vectors are exclusively invocation-owned scratch. They are
not retained across calls; only checked counters and authoritative successor
data escape. Preparation, canonical copies, validation, arithmetic, verifier
calls and output are charged before work with source-bound conservative tariffs.
Verification charges are fixed by policy, independent of backend reports. Every
refusal drops the entire candidate and retains completed usage; retries start
from identical immutable inputs. These are logical tariffs, not CPU/RSS proofs.
The exact ownership/source review is retained with the external check artifacts.

The composition delegates economic calculations to the existing owner-vault,
liquidation, redemption, SP, staking and coupled gain-to-vault relations. It alone
derives the ledger movements. Independent successor invariants check the complete
three-asset balance sheet, supply=raw active+default debt, every pool role,
recorded gas reserves, SP/staking claims and custody complements. External funding
is an authenticated exact non-zUSD ingress; other AGRS/zDEX legs conserve.

Only the oracle ingestion route can change stored oracle references. Pending
prices restrict risk additions at both full risk views; they never price payout
or authorize seizure. A pending report cannot clear a committed dispute, and a
status refresh cannot erase an admitted pending restriction. Commitment checks
buffer/record continuity without consulting MCR/TCR. Age derives from certified
observation epoch/time, never commit time. Fee/bootstrap minutes are derived from
epoch differences under an explicit immutable ClockRule; no mapping is selected.

Exposure uses fixed exclusive lane allocation plus local debt/rolling-throughput
checks. Each liquidation charges upward-rounded entire collateral value plus
keeper reserve; each redemption charges gross collateral plus surplus. This is
a conservative risk envelope, not an economic payout change. Events are retained
through price changes, restarts and epoch advances until authenticated release
also passes the local window bound. No source policy or available capacity is
invented when an owner dependency is unbound.

The supported parameter-change family changes only minimum net debt and the
reserve for future openings. It requires the separately approved policy receipt,
consecutive version, positive representable values, room for retained versions,
and every live raw net debt at least the new minimum. It leaves the complete
book, balances, claims and old vault versions/reserves untouched. Every existing
reserve is later discharged from its recorded value exactly once. Other families
and migrations refuse; there is no installed live change policy or generic mint.

## One publication path

[FileLedger](src/application/publication.rs), enabled only with `std`, is the
stateful shell. It owns a directory, not decision authority. A stable OS file lock
serializes publishers, including distinct handles/processes. Under that lock it
replays every retained request through ReceiptVerifierV1 and the core, compares
the complete canonical bundle, then checks expected root/version. A stale witness
conflicts; an identical retained bundle is a duplicate without another write.

One file contains the canonical genesis bytes and anchor, followed by complete
ordered commit bundles. Each
bundle contains the exact state, movements/economic outbox, nonce/nullifiers,
exposure, trace, receipt binding and core-derived usage. The shell writes a separate
temporary file, synchronizes it, atomically renames it over the ledger, then
synchronizes the directory. No field, outbox or replay counter publishes separately.
Cold load decodes only raw requests; successor bytes are rederived and compared,
never imported as an unchecked State. Complete history and frame bounds apply.

The small design retains one world, one input, one sealed witness, one ledger file
and one stable lock. Full bounded replay avoids a second mutable state cache or
incremental publication protocol. Its cost grows with retained history; this is
an explicit bounded research adapter, not an unrestricted-history service.
Filesystem access/encoding and FileLedger are shell/preparation interfaces, not
pure decision APIs. The host must designate one authoritative store per chain;
filesystem permissions, atomic rename/locking and durable storage are assumptions.
Outbox records are internal ledger data; no exactly-once physical token delivery,
external E8 rail, real custody, global consensus or live activation is claimed.

## Evidence boundary

The tests retain a state-threaded, durably published multi-user journey: opens,
pending rises/drops, committed-only liquidation, offset/redistribution, redemption,
surplus, host/staking claims, dispute/halt/freeze, price-independent recovery,
insolvent commitment, buffered recovery, stale receipts and exposure release.
The liquidation and redemption steps themselves include partial-write rollback,
competing CAS refusal and duplicate retry without another outbox record.
Separate checks cover approved/unapproved changes, funding/replay/asset bounds,
retained inputs/refusal usage, canonical request boundaries, concurrent CAS,
cold replay, corruption and subprocess termination at six publication points (including a partial write).
Compiled mutants cover bad tags, PriceView authority, non-atomic publication and
unapproved parameter changes, with positive/unrelated and comparison-disabled controls.

The MAC and BLS test issuers and all clock/drip/security numbers are test-only fixtures. Real
artifact replay, independent clock/finality, actual custody/funding/rail,
shared allocation proofs and approved policies remain backend/owner qualification.
The original schema dispatcher/application-wide A63 gate, full finite atlas,
V1/EVM/C1 differential replay, 100% money-branch coverage and conservation/No free
value/all-history proofs are separate qualification, not supplied by this step.
No new V1 economic departure is introduced; the inherited D1-D4 obligations remain.
