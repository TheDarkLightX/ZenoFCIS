# Original typed output encoding

This bounded unit adds `canonical_v2::output` source over the existing
`execution_v2::composition` reexports of the actual `decision::Atom` and
`Field`. It does not construct a candidate or grant authority. Root owns module
registration, source identity, use from private evaluations, schema binding,
and combined qualification. Base: `1ed6f88f7b1098bed6c409bc9aa3c12c99d43c3c`
plus the 124 individually checked copied sources in the starting manifest.

## Mathematical object and API

Three functions return owned `Result<Vec<u8>, Failure>`:

- `encode_atom(Atom, max_bytes: usize)` encodes one complete original value.
- `encode_record(&[Field], max_bytes: usize)` encodes every original field in
  strictly increasing ID order, without sorting, omission or renumbering.
- `encode_envelope(root: u32, schema: &[u8; 32], payload: &[u8], max_bytes: usize)`
  preserves the payload verbatim under the exact original frame.

The normative sequences use ZCVE/1 tags: false 01, true 02, U128 03, I128 04,
Bytes 05, ASCII Text 06, Enum 07, Record 09, payload-free Sum 0a. Integers occupy
16 big-endian bytes with two's-complement signed interpretation. Blob lengths,
record counts and type IDs occupy four big-endian bytes; field and variant IDs
occupy two. Sum appends a zero payload-present flag. Records retain all fields,
including ID zero and empty, 32-field and 100-field cases. There is no nested
record, tuple, vector, map, Unit, or payload-bearing Sum extension.

The envelope is `ZFCISV1\0 ++ BE32(root) ++ schema32 ++ BE32(payload.len) ++
payload`, with exactly 48 header bytes. Root zero, all root/schema bits, and
empty payloads are valid framing. The function neither checks schema meaning
nor recomputes its hash; the integrating authority must obtain these inputs
from its immutable binding and supply bytes of its actual private candidate.

Every application-owned executable helper has a total exact postcondition and
zero executable requires. The unsigned word specifications are connected to
the existing canonical integer reader specification by checked arithmetic and
bit-vector lemmas. The original readers and framing source, rather than copied
decoder implementations, are part of the whole harness. All claimed proof
results require a passing final receipt; source existence is not qualification.

## Refusal order and resource scope

Atom refusal is: u32 blob length, ASCII validity for Text, checked native total
length, then the complete byte cap. Non-ASCII text refuses even with a zero
atom cap. Other scalar cases have fixed sizes (1, 17, 7 or 8 bytes).

Record refusal is: u32 field count, five-byte header cap, then each field in
original order. Each field checks ascending uniqueness before its two-byte ID
cap, then applies the atom's validation and remaining cap. An earlier cap
failure takes precedence over a malformed later field. Duplicate/unordered
fields are refused, not normalized. A later error discards all private bytes.

Envelope refusal is: u32 payload length, checked native addition of 48, then
the complete cap. Private append checks native-length addition before mutation.
The `Length`, `Text`, `Order`, and `Limit` variants are pure serialization
failures, not protocol decisions. Inputs are immutable borrows; only fresh
local vectors are mutated, and no partial output escapes any public error.

Encoding/construction is outside the current invocation logical meter, like
existing authority sealing. The cap bounds the returned sequence and every
record prefix, not CPU, elapsed time, allocator capacity, or peak heap. The
record builder temporarily holds one separately encoded atom. Allocation
failure and machine resource exhaustion are outside the mathematical model;
standard-library allocation, compiler, platform and verifier assumptions remain
explicit. No caller supplies a verdict, callback, substitute contract or usage
report to these APIs.

## Design choice and preservation

The alternative was a second value language or application-specific conversion
of each candidate into legacy Values. That requires another set of seven atom
shapes, a record representation, conversion ownership, and repeated application
conversion/error paths. This design retains the existing seven Atom variants,
uses three direct entry points and one error enum, and keeps only a local byte
builder plus a current atom buffer. There is no cache, registry or stateful
encoder abstraction. No existing validation or independent acceptance check is
removed. Adding source/coverage identities is an intentional root integration
change; the original wire format and candidate ownership are preserved.

## Verification and independent comparison

`verification/verus/original_output.rs` directly imports the actual canonical,
scalar evaluation, and complete execution modules. Its finite namespace consists
only of reexports so production imports resolve unchanged; it contains no
replacement executable adapter. Root has registered the output module. The harness reexports that actual
registered module once; the earlier unregistered harness is historical. Native tests import the actual
legacy `zeno-fcis-value` and `zeno-fcis-codec` crates from pinned Cargo artifacts.
No legacy encoder formal proof is inferred from those comparisons.

The independent corpus covers every supported kind, signed/unsigned extremes
and bit positions, all ASCII byte values, non-ASCII refusal, arbitrary type and
variant IDs, zero/max/gapped field IDs, empty/large records, full headers and
all cap boundaries in the corpus. It also exercises the actual signed/unsigned
readers, actual original envelope framing, and actual protected flat-input
projection with permuted variant IDs/codes. The general formal bridge to the
flat-input decoder remains open; these comparisons are sample evidence.

The gate retains complete source, tool pins, commands, stdout/stderr, exits,
whole-harness reports, translated function inventory, executable contract/body
and spec-body fingerprints, and each mutation specimen. Mutations challenge
wire tags, widths, sign conversion, IDs/order, payload flags, blob/record counts,
ASCII, caps, original header identity/length and payload custody. Genuine
verification of weakened/omitted unused contracts and equivalent spec changes
must be rejected by the separate coverage guard. Compiler, VIR, timeout and
resource failures never count as semantic kills. The coverage guard is a
reviewed development check, not an additional proof checker.

Replay, offline and serialized against other workers:

```sh
flock /tmp/zenofcis-v2-parallel/heavy-check.lock python3 -B tools/check_original_output_v2.py --out /tmp/zenofcis-v2-parallel/original-output/replay.json
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tools -p test_check_original_output_v2.py
```

The gate uses Verus 0.2026.09.27.3cf1832 / verifier Rust 1.98.1 with
`--no-cheating --no-external-by-default --num-threads 2 -V spinoff-all`.
The last option gives each function a fresh Z3 instance. All functions and
default resource limits remain intact; the original shared prover context hit
an inherited authority metadata resource limit. Native oracle builds
use Cargo +1.97.1, `--locked --offline`, jobs 1 and the required shared target.
Native tests run with one thread. The registered public Cargo integration test
and actual synthesis crate no_std compile are retained separately. The gate sets
`CARGO_INCREMENTAL=0`: the default shared-target build failed to resolve the
registered module, while the nonincremental public test passed. Both diagnostic
runs are retained; no semantic cfg, target directory or cache deletion changes. Strict
Cargo Clippy is run without a waiver; inherited diagnostics remain a reported
root-owned blocker, separate from the encoder proof/native qualification. The receipt separates development
`positive_only` from full `passed` qualification.

## Remaining root obligations

Root registered `pub mod output;` in this worktree. Carry that registration
into integration, bind the actual source and specification into evaluator identity, and requalify the final combined module
layout. Expose encoded output only from the actual private evaluation with
immutable bound root/schema and real complete candidate fields. Encode complete
effect payloads in existing declared delivery order; these utilities do not
choose business meaning or delivery metadata. Check total output policy and
preserve Reject/CommittedFailure semantics at that integration boundary.

Root retains combined native/Miri/Clippy/no_std, mandatory authority, real
templates, ATDD, exact-source review and CI obligations. This unit does not
complete V2, the breaking-change ledger, original-schema hash recomputation,
mandatory shell authority, legacy codec verification, or external delivery.
