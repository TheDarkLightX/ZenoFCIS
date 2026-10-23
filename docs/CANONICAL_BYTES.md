# Canonical bytes and admission

Canonical bytes give one semantic value one deterministic byte identity.
ZenoFCIS uses ZCVE/1 instead of Rust memory layout, Serde, JSON, database
encoding, or a collection's internal representation as protocol meaning.

## Why one representation matters

Several byte strings can describe the same apparent value in a permissive
format. Object fields may be reordered, integers may have multiple textual
forms, optional fields may be omitted or made explicit, and parsers may ignore
trailing input. Hashing those representations would give one value several
identities.

ZCVE/1 instead fixes:

- type tags;
- big-endian fixed-width integer encodings;
- length prefixes;
- record-field and map-key ordering;
- tuple, vector, record, map, sum, and optional structure;
- ASCII text admission in the closed value model;
- complete input consumption.

The intended byte-level laws are:

```text
equal admitted values -> equal canonical bytes
unequal admitted values -> unequal canonical bytes
accepted bytes -> encode(decode(bytes)) == bytes
```

The last law rejects encoding aliases. It is the reason an input that decodes
to a plausible value may still be rejected as noncanonical.

## Concrete wire examples

Bytes below are hexadecimal. Lengths and counts occupy four bytes, most
significant byte first. Each example is also an independent literal in the
codec limit tests and the decoder specification checks.

| Value | Complete canonical bytes |
| --- | --- |
| Unit | `00` |
| False | `01` |
| True | `02` |
| Bytes `[9, 10]` | `05 00 00 00 02 09 0a` |
| Vector containing Unit | `0b 00 00 00 01 00` |
| Map from Unit to True | `0c 00 00 00 01 00 00 00 01 00 00 00 00 01 02` |

Map keys and values have separate length-prefixed frames. A map's payload
budget includes both frames' encoded contents and every nested byte/text
payload. The last example has three nodes, depth one, and two payload bytes.
For a map whose key and value are each two-byte byte strings, both encoded
contents occupy seven bytes: its payload charge is `7 + 7 + 2 + 2 = 18`.

## Verified value decoder

The [value decoder gate](../verification/verus/README.md#value-decoder-refinement)
checks the actual production `zeno_fcis_codec::decode_value` and the value
constructors, readers, validation and encoding functions it calls. Its harness
includes the production files directly. The qualification profile pins the
toolchain, required functions and exact verification count.

For every finite input slice and every `DecodeLimits`, the verified source
path has no unchecked arithmetic overflow, out-of-bounds access or explicit
panic, and its recursion and loops terminate. If it returns `Ok(value)`, then:

- the value satisfies the constructor invariants and supplied depth, node,
  collection and payload limits;
- depth is at most `min(limits.value.max_depth, 1024)`, with the root at depth
  zero;
- the entire input equals the specification's canonical encoding of that
  value, so trailing input and alternative accepted encodings are excluded;
- the input length is at most `limits.max_input_bytes`.

The proof first establishes these acceptance properties for a total
specification decoder, then proves correspondence of the production functions
using named cursor and resource-state invariants. Error variants are mapped
to rejection in the correspondence theorem; exact error text and refusal
order are checked by differential execution. Concrete encoding and admission
checks test the specification against independent codec vectors.

This qualification covers Linux x86-64 and the value-decoder configuration in
the pinned harness. The trusted base includes Verus and its bundled standard
library specifications, Z3, rustc, and five additional standard-library
contracts for allocation conversions, byte-slice cloning, ASCII checking and
UTF-8 conversion. Those additional contracts have native sample tests.
Execution assumes sufficient heap and stack; stack byte consumption is tested
and is not proved. The result does not establish machine-code correspondence,
allocator availability, encoder agreement for every mathematically admitted
value, or decoder completeness. Envelope and commit-bundle decoding remain
outside this proof's scope.

The method follows the layered refinement, explicit assumptions, specification
testing and configuration boundaries described in the
[seL4 proof overview](https://sel4.systems/Verification/proofs.html). Background:

- [seL4: Formal Verification of an OS Kernel](https://doi.org/10.1145/1629575.1629596),
  SOSP 2009.
- [Comprehensive Formal Verification of an OS Microkernel](https://doi.org/10.1145/2560537),
  ACM TOCS 32(1), 2014.
- [Verus: Verifying Rust Programs using Linear Ghost Types](https://arxiv.org/abs/2303.05491),
  OOPSLA 2023, extended version.

## Enforcement pipeline

The untrusted-input path is:

```text
untrusted bytes
    -> input-size bound
    -> bounded structural decoding
    -> ordering, uniqueness, tag, and shape checks
    -> rejection of trailing bytes
    -> canonical re-encoding
    -> exact comparison with the original bytes
    -> immutable admitted value
```

`zeno_fcis_codec::decode_value` and `decode_envelope` perform the strict
decode/re-encode check. `zeno_fcis_value::AdmittedValue` owns a value only after
bounded validation. `zeno_fcis_codec::AdmittedEnvelope` privately retains its
admitted value and canonical payload length so callers cannot pair invented
metrics with different data.

Generated project APIs add schema admission. They should be preferred at an
application boundary because an admitted ZCVE value proves canonical shape,
while a schema-bound envelope also proves that the value has the reviewed
project type.

## Commitments

A commitment hashes a versioned, domain-separated preimage containing
canonical bytes. Different roles use different domains so identical payload
bytes used as a command, context, state, receipt, or evidence artifact do not
silently receive the same protocol identity.

ZenoFCIS derives values such as:

```text
state roots
command and context commitments
candidate IDs
receipt and bundle identities
replay bindings
schema, profile, catalog, and evidence commitments
```

from canonical representations. Do not hash debug output, JSON, Serde bytes,
database rows, host paths, collection internals, or Rust object memory when
constructing protocol identity.

## What byte-level enforcement proves

Canonical admission provides:

- deterministic identity across conforming implementations;
- rejection of malformed, ambiguous, reordered, duplicate, or trailing input;
- stable inputs for hashing, replay comparison, signatures, fixtures, and
  cross-language differential tests;
- independence from Rust layout and storage-backend representation.

It does not provide:

- confidentiality or encryption;
- authenticity without an approved provider and authority binding;
- schema correctness unless schema admission also succeeds;
- authorization, conservation, invariant preservation, or correct external
  effect interpretation;
- collision resistance beyond the selected commitment provider's evidence.

The production relationship is:

```text
canonical byte admission
    + schema and catalog admission
    + exact invocation binding
    + project-law verification
    + nominal catalog authority
    + atomic shell publication
```

Each layer answers a different question. Canonical bytes answer, "What exact
value is this?" The later layers answer, "What does it mean, is it valid for
this invocation, and may it change authoritative state?"

## Integration checklist

- Decode all external protocol bytes with explicit limits.
- Require complete input consumption and canonical re-encoding equality.
- Prefer generated schema-bound envelopes over raw `Value`.
- Keep admitted values transitively owned and immutable.
- Derive protocol hashes only from versioned, domain-separated canonical data.
- Retain canonical bytes or their exact commitments in replay and evidence
  artifacts.
- Test noncanonical field order, map order, duplicates, malformed lengths,
  invalid tags, truncation, trailing bytes, and limit boundaries.
- Treat byte admission as one prerequisite for authority, never as authority
  by itself.
