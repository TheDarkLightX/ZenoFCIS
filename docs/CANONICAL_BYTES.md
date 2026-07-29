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
