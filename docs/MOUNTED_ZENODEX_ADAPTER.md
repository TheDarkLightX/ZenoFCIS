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

The generic adapter does not certify arbitrary ZenoDEX binaries or Python
modules. Its concrete single-vault zUSD runner mounts the exact external
revision documented in `MOUNTED_ZENODEX_ZUSD_V1.md`, enforces bounded process
and transport controls, persists generated replay fixtures, and retains one
bounded parity report. That report is mounted runtime-refinement evidence only
for its declared corpus and revisions. Passing generic adapter unit tests is
not mounted evidence. Neither result grants production authority.
