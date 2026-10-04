# Finite Boolean signature model

`BooleanSignatures.lean` checks a small mathematical model of the experiment.
It is not a verified implementation of ZenoFCIS, egg, the lowering routine, or
the extraction solver.

The replay environment is Lean 4.30.0-rc2 (commit
`3dc1a088b6d2d8eafe25a7cd7ec7b58d731bd7cc`) and Mathlib commit
`9977002c3c9492b622fb469b0d18acc7e73aed3e`. The script uses an existing matching
Mathlib project as a dependency environment. It installs nothing and does not
copy or modify that project.

```sh
bash proof/replay.sh /path/to/matching/mathlib-project
```

## Checked statements

For `BoolValuation n = Fin n -> Bool`, the valuation space has exactly `2^n`
elements. The space of single-output Boolean functions has exactly
`2^(2^n)` elements. With `m` ordered Boolean outputs it has
`(2^m)^(2^n)` elements. These facts concern total Boolean functions with fixed
input and output arities; they do not concern all Tau syntax, unbounded types,
or temporal synthesis.

`truthSignature domain observe` is the list of complete observations obtained
by mapping `observe` over one fixed valuation list. If that list covers every
input, signature equality is equivalent to pointwise observation equality.
Duplicates do not invalidate completeness, but may waste evaluation work.
Both signatures must use the same enumeration and ordering.

The pure expression language contains variables, Boolean constants,
negation, conjunction, disjunction, and total Boolean conditionals. Pointwise
equality is a congruence for every single-hole context in that language.
Distributive factoring is proved
in this model. This congruence does not apply to potentially trapping
arithmetic nodes or to evaluation strategies with different error behavior.

`checkedCandidate` returns a candidate only when its complete signature equals
the original signature and an additional decidable acceptance condition holds;
otherwise it returns the original. The selected result is proved pointwise
equal to the original. The generator and the additional acceptance condition
are arbitrary: neither can weaken the signature-equality requirement.

The specialized observation is `Except Error (Fin m -> Bool)`: a successful
observation includes every output in ABI order, and a failed observation
includes the error identity. The generic theorem also permits other observation
types, including integer vectors and errors, provided their equality is exact.

## Implementation obligations

The mathematical proof applies to a Rust run only after these obligations hold:

- The mask enumeration covers all admitted Boolean inputs, uses the documented
  variable-to-bit mapping, and supplies the same ordered inputs to both programs.
- The observation includes the entire output tuple or the actual typed error;
  equality is exact rather than a truncated digest, SAT verdict, or unchecked
  semantic hash. Hashing may index buckets only with exact collision checks.
- The generator, signature computation, e-class merging, lowering, and gate agree
  on type, variable identity, constant values, input/output arities, and ordering.
- Local merges are restricted to the total Boolean fragment. Removing an eager
  checked-arithmetic node can remove an observable error even if its value is
  unused or belongs to an unselected conditional arm.
- The emitted `Program` passes shape, type, topological-reference, and domain
  checks. Preserving input schema makes inputs outside the admitted domain reject
  consistently; the Boolean enumeration theorem itself covers admitted inputs.
- Every proposal is checked through actual `Program.evaluate`. Lowering correctness
  and finite-loop completeness remain implementation claims tested separately.
- Cost and resource checks use the actual emitted shared instruction DAG. The
  proof establishes behavioral equality, not cost optimality, termination of
  saturation, solver correctness, or execution-time improvement.

`boolValuations` uses Mathlib's noncomputable finite-set-to-list operation to
state a complete mathematical enumeration. It is not the Rust mask enumerator;
its ordering is intentionally unspecified. The completeness theorem bridges any
complete fixed list, so the Rust enumeration is a separate refinement obligation.

## Evidence and limits

The replay prints the axiom dependencies of the principal theorem closures.
Only the usual Lean/Mathlib foundations (`propext`, `Classical.choice`, and
`Quot.sound`) are permitted. The script rejects proof placeholders, new axiom
declarations, unsafe definitions, and native-decision escapes before replay.

The replay log, source SHA-256, toolchain information, and exact theorem scope
are retained alongside this file. A compiler replay establishes that the stated
model theorems are kernel checked; it does not establish that the Rust code
implements them. Exhaustive evaluator checks supply separate implementation
evidence for the declared finite corpus and controls.
