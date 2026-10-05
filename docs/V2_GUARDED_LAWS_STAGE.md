# V2 conditional law observation

Base: `1ed6f88f7b1098bed6c409bc9aa3c12c99d43c3c`, plus the 129-file
hash-checked development seed. This bounded unit adds a library instruction;
it grants no transition, publication, merge or release authority.

`Op::ObserveWhen(guard_node, observation, default)` uses a strictly earlier
Bool node. Its default is immutable program data of the exact declared Atom
variant. The existing eager graph traversal, pure instructions, observation
selectors, scope table, mandatory families and first-failure semantics remain.

## Mathematical and operational contract

The total node transition takes the actual borrowed original frame, prior
values, law/node IDs, limits, arbitrary initial eight-counter usage and an
existing read prefix. It returns an exact typed result, usage and prefix:

1. Attempt Step first; failure preserves usage and reads and returns Budget.
2. Reject self/forward, missing and non-Bool guards as Undefined after Step.
3. False returns the exact default. It neither charges Read nor observes a
   frame slot, even when that observation is unavailable or Read is exhausted.
4. True follows the same Read block as ordinary Observe: attempt Read, append
   the exact permitted/denied ReadAttempt, then return Budget on denial or read
   the actual frame. Missing/unavailable observations return Undefined.
5. No active observation error becomes a default. Every later eager node
   retains its original semantics; the final predicate must still be Bool.

Metadata precedes frame checks and all protected law work. Shape admission
requires a prior guard and checks every guarded default, even in an unused node
or skipped law. Text defaults must be ASCII, exactly the complete policy's
existing Literal standard. All other Atom variants are exact full-width typed
scalars or opaque bytes, including zero/max type IDs and empty bytes. Existing
lower-level Literal behavior and its separate complete-policy validation are
unchanged. The guarded check lives in owned predicate source; it does not
require a modification to another worker's composition admission.

The minimal failure witness is an accepted inventory reserve with no outbox:
an eager Observe inside an implication/Select still fails. The new opcode
skips only its explicitly guarded observation. Treating every Undefined as a
fallback would also hide missing payloads on actual shipment, so it is refused.
Bool/integer coercion and fallback type erasure are also explicitly excluded.

## Encoding and source boundary

Existing law opcode tags are 0 through 11. New tag 12 encodes, in order:
Word(12), Word(full usize guard), complete existing observation tokens, then
complete existing typed Atom tokens for the default. Law count/order, kind,
scope, genesis, program length and root remain in the complete policy encoding.
Every preexisting opcode keeps its exact tokens.

The sole production changes are laws/types.rs, predicate.rs, spec.rs and
existing tests.rs; authority/metadata.rs, metadata_spec.rs and the generated
library_sources.rs. Tests inside metadata.rs use independent standard-library
big-endian canonical bytes. The generated getter uses the unchanged root
approved 79-path/3-pin list (SHA-256
`168b54005ab3fdad7d9a69a086933666eb1036c25944d8e19934a354e8d5b7a8`).
Source-byte lengths intentionally refresh; root owns the later complete source
list expansion and final identity. No approval hash or shared registration is
changed here.

No evaluator cache, alternate meter, supplied verdict, callback or mutable
program state is introduced. The existing single dispatcher now chooses a
protected observation or returns the exact inactive default; both observing
variants share one Read/trace/frame block. One default-admission helper and one
pure read specification name the added obligations. No acceptance check is
removed. Legacy regressions and all 26 legacy semantic/coverage controls are
retained alongside 15 guarded controls.
The 41 stable controls comprise 35 intended proof refusals, five genuinely
verifying raw-coverage refusals and one independent native ordinal refusal.
The ordinal control retains the exact legacy runtime mutation and requires its
unchanged named regression to pass on the baseline and fail specifically with
Err(Frame) versus Ok(()) on the mutant. It is a declared control kind, never a
fallback from a solver error. Earlier ordinal resource-limit probes are kept
as inconclusive evidence and are not counted as semantic proof failures.

## Evidence and replay

`verification/verus/guarded_laws.rs` includes the actual complete registered
execution/canonical/scalar modules, including metadata, the producer bridge,
authority constructor and source getter. It proves all reachable executable
helpers with zero executable requires under Verus 0.2026.09.27.3cf1832,
verifier Rust 1.98.1, no-cheating, no-external-by-default and two threads. The
separate reviewed profile retains every raw translated function signature,
contract and Exec/Spec body. No read-ID normalization or coverage waiver exists.

The native portfolio contains a wider-arithmetic counter/attempt oracle over
arbitrary-start samples and overflow; full-width typed default/actual values;
false/no-Read, active missing/denied, malformed guard/default, scope/genesis and
first-refusal controls; independent complete law token vectors; and all 864
original inventory inputs. The public test binds the original graph (checked
byte-for-byte against program.zcve), decodes original raw schema fields, runs the
actual producer and evaluates a guarded shipment implication on its actual
frame. It independently checks decisions, payload/destination/channel, original
bytes, exact law reads and the producer-to-law shared usage delta. It is a
bounded law/bridge corpus, not a migrated inventory application.

The gate classifies SMT semantic refusal only on a diagnostic in the intended
actual function, with compile/VIR/resource failures excluded. Coverage controls
must genuinely verify before their intended raw function contract/spec/body or
inventory mismatch counts. Embedded source-array length adjustments do not
constitute a semantic kill or the intended coverage refusal. Every specimen,
stdout, stderr, command and exit status is retained.

Replay (all commands are offline and heavy suites keep one shared lock):

```
python3 tools/check_authority_v2.py --generate-sources verification/verus/authority_v2_sources.json
flock /tmp/zenofcis-v2-parallel/heavy-check.lock python3 tools/check_guarded_laws_v2.py --out /tmp/guarded-laws-replay
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tools -p test_check_guarded_laws_v2.py
```

Use a new evidence directory. `--refresh-coverage` is an explicit development
manifest generation step after reviewing final code, never a routine replay
option. `--positive-only` and `--controls` produce explicitly partial receipts.
Cargo validation uses only the shared target, incremental=0, one build/test
thread, Rust +1.97.1, --locked and --offline; required commands and inspected
results are in the final worker evidence manifest.

The acceptance receipt is separate from these procedures. An unrun or failed
gate leaves this unit incomplete. Root owns template migration, final combined
source identity/registration, Miri, global acceptance, exact-head review and CI.
Verus/vstd/Z3, Rust translation/erasure, compiler, allocator and platform remain
named trusted components. These results do not establish physical resource
costs, external authentication, atomic shell commit or successful delivery;
they do not become ADR-0003 independently checked solver proof objects.
