# Bounded benchmark review

Date: 2026-10-04. Author: gpt-6-astra, high reasoning effort.
Reviewer: a separate gpt-6-astra, max reasoning effort, followed by one focused
review of fixes. This is a second agent pass, not independent-model-family evidence.
Root independently checked the fixtures with a separately written interpreter.
The review covered the design and development data, not a production implementation.

## Findings and disposition

1. **Diagnostic attribution overstated; resolved.** I07 expects Equivalent when
   both graphs return MAX at MAX-1 and Arithmetic at MAX. Skipping error rows still
   produces its expected relation, so it cannot alone detect success-only comparison.
   Root reproduced that mutant: it wrongly accepts I08 and I14. Erasing scalar-error
   distinctions wrongly accepts I14. DESIGN section 8 and PROTOCOL Q18 now assign
   skipped-error detection to I08, erased-tag detection to I14, and legitimate
   equal-error acceptance to I07. No additional fixture was needed.
2. **Input-renaming split policy ambiguous; resolved.** B02 computes x0 and B04 x1
   over the same two-Boolean product. Root checked all four tuples after swapping
   inputs. The split rule now includes type/domain-preserving input permutations,
   with output positions fixed; both fixtures and the generator share their group.
   Candidate ABI requirements remain exact. The 28-behavior count uses fixed input
   positions, not equivalence modulo renaming.

Root also clarified that full-width i64 output intervals are allowed; the complete
i64 input interval exceeds this benchmark's enumeration ceiling.
The review follow-up confirmed both corrections and found no concrete new issue.
The reviewed fixed files are identified by [reviewed-file hashes](review-source-hashes.json).

## Replayed evidence and remaining scope

Both reference interpreters reproduced 32 pairs, 355 input tuples, 21 Equivalent
and 11 Different relations, including every first witness. Exact fixture regeneration
also passed after the metadata correction. The node/domain/program/witness data
were unchanged by the split-group correction.
Final cases SHA-256: `65ad66a61de3bbd8d0133cdf30c451f6344a3077c5eef989f39682890c6a1f55`.

Production decoding, Rust correspondence, proof/body coverage, canonical-byte costs,
metering, receipts, process limits, actual hidden-set splitting, optimizer performance
and diagnostic usefulness remain unperformed or unqualified. Review agreement and
these host checks do not establish Herbie-comparable effectiveness or application authority.
