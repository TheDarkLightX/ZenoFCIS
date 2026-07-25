# Mounted ZenoDEX full-decision adapter

## Boundary

`zeno-fcis-adapter-zenodex` admits output from a callable runtime or an exact
JSON-line exchange and normalizes it into `zeno-fcis-refine::NormalizedDecision`.
The adapter compares decision kind, stable reason, all six input/profile
bindings, pre/post roots, candidate identity, patch, commit plan, outbox plan,
receipt, and complete bundle.

JSON is not consensus encoding. It is a strict shell transport whose sole
accepted representation has fixed field order, lowercase hex, no whitespace,
no unknown or duplicate fields, and exactly one trailing LF. ZCVE component
bytes remain authoritative.

## Inputs and outputs

- input: one caller-defined canonical request plus one complete runtime result;
- output: a normalized decision or a typed fail-closed adapter error;
- comparison output: an exact report and a content-addressed replay fixture for
  every mismatch.

## Authority and trust

The runtime proposes a complete decision. `NormalizedDecision::try_new` checks
the three-way decision shape and `compare_exact` decides refinement. Serde and
Serde JSON are trusted only to parse this non-consensus shell format. They do
not encode protocol values, roots, receipts, or bundles.

## Bounds and negative cases

Line and individual artifact sizes are explicit. The adapter rejects missing,
duplicate, unknown, reordered, whitespace-aliased, multi-line, uppercase-hex,
odd-hex, oversized, wrong-kind, incomplete-candidate, changed-on-reject, crash,
and runtime-reported failures. Timeout and tool-disagreement classifications
are supplied by the mounted `JsonLineRuntime` implementation as runtime errors.

## Nonclaims

This crate does not ship or certify a particular ZenoDEX binary or Python
module. A promotion run must mount the exact external revisions, enforce its
timeout and process sandbox, persist generated replay fixtures, and submit the
result through the evidence and promotion gates. Passing adapter unit tests is
not mounted runtime-refinement evidence and grants no production authority.
