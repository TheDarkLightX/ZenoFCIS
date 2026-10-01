# V2 protected flat-record proof unit

Parent: `f88f1a3edcb519e6c6dce6b53974a981b6ca20d4`, with clean-head byte,
scalar, meter and arithmetic gates passed after 44/44 acceptance. Work proceeds
on the isolated branch `agent/v2-protected-input-20261001`.
This bounded unit does not replace the V2 completion plan.

## Mathematical subject

A record descriptor is an ordered sequence of (field ID, closed leaf schema).
Leaf schemas are inclusive i64 integer intervals, Boolean, closed Enum, and
closed payload-free Sum. A variant map contains distinct u16 variant IDs and distinct i64 codes within
its explicit inclusive i64 min/max interval, whose mathematical cardinality
is exactly the map length. Preserve arbitrary intervals, negative and boundary
codes, and arbitrary variant-map order; do not assume codes start at zero.
Type IDs are u32.
Metadata requires min <= max, strict record field IDs, distinct variant IDs
and codes, and interval cardinality in int/i128 arithmetic.
Generic ZCVE permits ID zero; do not confuse the finite catalog's further ID
restrictions with the wire encoding. Empty records are legal. No arbitrary
16-field limit may exclude a legal 32-input finite program. Malformed metadata
is refused before raw input access. Unknown shapes are outside this descriptor
language and cannot be smuggled in through an unchecked external adapter.

For fixed descriptor S, bytes B and limits L, define a total transition returning
(result, eight counters, descriptor-based Read-attempt observations). Start all
counters at zero in the public wrapper. The private helper is total for arbitrary
initial counters and preserves an existing attempt prefix, appending this
record's observations. It clears its scalar output buffer on every failure,
including invalid metadata; it never clears earlier charges or attempts.
Descriptor admission is metadata work outside this logical
cost profile; it is still checked executable code with an exact specification.
A successful record returns the complete ordered projected scalar sequence.
A failure returns no projected scalars and retains the previous logical charges
and observations. Concrete refusals are Schema, Header (tag/count/truncation), Field with the
expected descriptor ID (all ID or leaf-decode failures), Trailing, and Budget
with the exact meter failure. Intra-leaf errors deliberately share the same
Field result; no unspecified diagnostic precedence changes the observation.
This is not a CPU, allocation, hashing or physical-I/O model.

## Order and refusal rules

1. Refuse malformed schema with no charges or observations.
2. Charge Byte for the full raw slice length before reading tag or count.
   Charge overflow or limit refusal preserves all counters and prevents parsing.
3. Require TAG_RECORD 0x09 and a big-endian u32 count equal to schema length.
   Truncation, tag mismatch or count mismatch refuses without any Read attempt.
4. For each descriptor field in order, append its expected field ID as a Read
   attempt and charge one Read before inspecting raw field ID or leaf payload.
   Record whether the charge permitted access. A denied Read retains prior
   charges and the denied descriptor-based attempt; no field bytes are read.
5. Require the raw big-endian u16 field ID equal the descriptor ID. The descriptor
   IDs must be strictly increasing, so duplicate or unordered wire fields refuse.
6. Decode exactly: I128 uses 0x04 plus 16 signed big-endian bytes and must lie in
   its admitted i64 interval; Bool is 0x01/0x02; Enum is 0x07 plus u32 type/u16
   variant; Sum is 0x0a plus those IDs plus exactly zero payload flag. Require the
   closed type ID and a declared variant; use its declared, possibly permuted code.
7. Require complete consumption after the last field; trailing data refuses.
   Each failed field decode retains its successful Read charge and attempt.

Read-attempt observations are metadata about the requested descriptor field and
logical permission, not physical access tracing or evidence that a payload was
present. Reviewed body coverage must separately keep charge/refusal before byte
inspection; final result/counter theorems alone do not establish operational
order. A cached parse before charge can preserve those final observations.

## Architecture and source obligations

Use a private execution_v2::input_view module so it can reuse the existing Meter
and later compose with private evaluate_into; do not widen public meter custody.
Provide a private project_into accepting that same Meter, and an opaque public
one-record outcome whose wrapper constructs the zero meter. A later full-decision
wrapper must call project_into with one shared meter, never reset it through
public execute. Directly reuse the checked integer-reader source, with no opaque
external_body, assume, executable requires or hand-translated trusted parser.
Make only the ghost canonical reader specification visible within the finite
module, so the record theorem reuses that exact model instead of duplicating it.
Every new application-owned executable helper needs a total exact contract.
The shared Verus harness must include the complete new source dependency closure
and updated translated-function/spec/body coverage rather than omit new modules.

## Definition of done

Pinned Verus correspondence over all schema/byte/limit inputs; exact source and
function coverage; independent native wire-format oracle including legal ID/code
permutations, all four shapes, empty/full records and malformed/truncated data;
mutations for order, width/sign/IDs/count/closed mapping/trailing data/refusal
cleanup/retained charges and coverage; actual Miri, no-std and Clippy; full ATDD
immediately before commit; clean-head replay, independent source review and
source-bound template identity regeneration; a draft development PR.

Still open: multi-record state/command/context composition; catalog descriptor
extraction and binding permutation; original-envelope framing/hash admission;
complete decisions/successor state/effects; law/genesis checks; mandatory V2
byte-based authority and replay; compiler/erasure/platform and shell assumptions.
This unit does not authorize an application transition or finish V2.
