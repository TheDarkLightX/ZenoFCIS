# BLS receipt authentication

This shell crate implements ReceiptVerifierV1. Receipts are authenticated by BLS
(PoP quorum or G2Basic) over the complete canonical receipt bytes. Acceptance is
conditional on the installed registry and qualified issuers meeting the domain
obligations in core-apps/zusd-v1/STEP4.md. A signature authenticates an attestation;
it does not independently replay the accepted oracle graph, establish market
truth, synchronize a clock, prove custody or approve a parameter policy.
Those remain explicit registry, issuer-profile, clock and finality premises.
No live registry, owner policy or activation is supplied.

## Dependency and API boundary

blst is pinned to =0.3.17, default features disabled, portable and no-threads.
Only its safe min_pk API is used: compressed 48-byte G1 keys and 96-byte G2
signatures. The wrapper forbids unsafe code. It exports verification, validated
PoP admission and immutable receipt-registry construction; no secret key,
key-generation or signing interface. Tests alone import SecretKey.
The semantic crate never depends on this crate, even through dev dependencies.
This crate depends inward on the semantic trait. No existing economic relation,
receipt grammar, failure order, resource tariff or publication path is changed.

G2Basic uses BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_NUL_. PoP signatures use
BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_; proofs over the exact compressed key
use BLS_POP_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_. There is no caller-selected DST.
Every member passes key_validate, including aggregate members; every verify and
aggregate_verify sets sig_groupcheck=true and pk_validate=true. The fast path
aggregates validated PoP keys and calls verify with both flags true, avoiding
blst's fast_aggregate_verify internal pk_validate=false. Basic aggregates require
pairwise distinct complete messages. PoP allows repeated messages. The primitive
PoP aggregate permits repeated keys for reference compatibility; receipt registries
and vote indexes independently reject duplicate voters.

PopKey owns its bytes and can be constructed only by verifying possession under
its own key and the proof DST. Authorities own keys, threshold and domain/id;
BlsReceiptVerifier owns the complete immutable registry and a message-byte bound.
Registries contain 1..256 distinct domain/id entries; quorums have 1..64 distinct
members and a nonzero attainable threshold. Adopting these entries as qualified
issuers and source-binding their exact contents to installed authority identities
is the host's explicit adoption boundary. Registry rotation requires a new owned
verifier and approved policy; no mutable lookup, global registry or fallback exists.

## Receipt envelope and refusals

The message is exactly the existing ReceiptVerifierV1 canonical bytes (including
chain, installed policy and actual pre-root). The adapter additionally checks its
prefix, domain, selected authority, complete u64-BE body length and size bound.
It verifies the entire string, without hashing conventions or normalization.
Proof = version u8 (1), suite u8 (0 Basic, 1 PoP), count u16-BE,
count strictly increasing signer indexes u16-BE, signature bytes96. No trailing
bytes. Basic requires exactly index 0. PoP requires threshold distinct in-range
indexes. Its aggregate signs the identical complete message for all reporters.
Max proof size is 228 bytes (64 members). Helpers do not confer authorization.

Refusal preserves stage (public key, signature decoding, verification, PoP) and
all seven blst error meanings: bad encoding, off curve, wrong subgroup, aggregate
type mismatch, failed pairing, infinity key and bad scalar. Additional refusals
name lengths, empty/oversized aggregates, unequal lengths, duplicate Basic messages,
invalid registry, unknown authority, message bound/grammar, proof encoding/suite,
insufficient quorum and signer order. blst itself sometimes reports VERIFY_FAIL
for multiple causes; no more precise cause is invented. The trait maps unknown
authority to Unsupported and every other error to Invalid; both refuse in zUSD.

## Ownership and work review

The authoritative verify boundary takes shared references and returns only an
owned Result<(), error>. Authority, PopKey and verifier fields are private and
owned; clones copy keys/vectors. No mutable aliases or interior mutability escape.
Parsing and group operations use exclusively invocation-owned scratch; blst's
safe internal FFI operates on owned/local points. no-threads avoids its global
pool. Scratch and partially parsed proofs are discarded on refusal, with no
retained cache, I/O, random input, clock read or Usage authority. The core charges
its existing conservative verification tariff before each invocation, including
refusals and replay. This logical tariff is not a crypto CPU/RSS bound; the owner
must qualify configured limits against the selected registry and host budget.

The new design adds one crypto wrapper, one registry and one receipt adapter.
It keeps one core and one atomic ledger implementation. Shared test journey
assertions run with both the old test-only MAC issuer and production BLS verifier;
only test issuance differs. Test signatures attest synthetic facts, including
pending price changes, custody and clocks; live external correctness is not claimed.
The source-bound core identity intentionally changes with the updated contract
and workspace manifest. BLS source/lock hashes are retained with external evidence;
registry-to-policy adoption and exact blst build provenance remain host obligations.

## Reference and negative evidence

The unchanged 82-case TSV and generator come from the approved supply review,
using py_ecc 8.0.0, Tau Testnet's independent G2Basic reference. Computing/signing
parity and raw rogue-key aggregate math run only in tests. Production admission
rejects both forged rogue-key proofs; raw math acceptance is not registry admission.
The pk_validate flag mutant is tested directly at the private parsed-input helper
with the order-3 torsion vector: public key_validate is an independent defence and
would otherwise mask that mutant. The order-13 signature is rejected even with the flag off in the reviewed raw
backend, so it cannot kill that mutation behaviorally. A compiled contract
assertion independently requires the shared signature-group-check flag to be true;
the torsion rejection check remains. This is a configuration kill, not a
claimed accepting torsion-signature exploit.
Other mutants remove PoP admission, Basic uniqueness or use the wrong Basic DST.
Each has an assertion kill, an unrelated positive control and a disabled-comparison
control, compiled in private copies. No production flag exposes these defects.

The supply review's old/partial audit and incomplete functional verification are
unchanged. Portable's non-ADX runtime path and other CPUs/toolchains are untested
here. Build qualification must exclude prebuilt libblst.a and overriding CFLAGS
(e.g. __ADX__), retain the compiler/assembly provenance and recheck upstream
advisories manually when changing the pin. No network or release is part of this work.

## Native oracle verification

OracleVerifier implements the semantic oracle's installed immutable BlsVerifier
port. Every key reaches the same key_validate helper; signature shapes also
reject infinity and wrong subgroups. PoP admission uses PopKey::admit. The raw
PoP-domain signature check authenticates registration/rotation before their
separate mandatory possession check, preserving ordered refusal reasons.
It cannot retain or admit a key. There is no oracle aggregation path; each of
the three distinct reporters signs its own complete report. Basic aggregate
message uniqueness and both group checks retain their existing enforcement.
The semantic crates have no BLS dependency, including in tests. Core lifecycle
tests use an explicit test double; independent unchanged py_ecc vectors and
public lifecycle/composition journeys exercise this shell implementation.
