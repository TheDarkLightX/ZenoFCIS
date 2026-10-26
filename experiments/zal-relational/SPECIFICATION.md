# Supported relational semantics

This is a bounded implementation profile, not a general satisfiability service.
The signature contains up to 128 unary/binary predicates. Domain size `n` is
positive and the domain is the labeled set `{0, ..., n-1}`. Each predicate has a
complete true extension; all other in-domain atoms are false. Unused declared
predicates still contribute to the structure space and its weights.

## Formulas

`Syntax.hs` defines immutable algebraic data types for truth constants, predicate
applications, equality, Boolean connectives, quantifiers and counts. Only variable
names `x` and `y` are supported. They may be rebound and restored under nested
quantifiers; two variable names do not limit the number of domain entities.
The public sentence must be closed, with declared predicates and correct arity.
Every branch is validated, including Boolean branches whose value is irrelevant.

Ordinary counting tests the number of distinct domain entities satisfying a body
against a nonnegative integer threshold, using equality, at-most or at-least.
It counts entities, not derivations or paths. C2mod additionally supports:

```text
count{v in D | body} mod modulus  cmp  residue
```

The modulus is positive, `0 <= residue < modulus`, and `cmp` is equality,
at-most or at-least on the remainder. At-most on the remainder is not an
ordinary at-most cardinality bound. Modulo syntax is admitted only under
`zal/c2mod/1`. Function symbols, arithmetic on entities, a third live variable,
primed state predicates and temporal operators are outside this adapter.

The JSON grammar is specified in `schemas/relational-query.schema.json` and the
closed decoder in `app/Main.hs`. The runtime additionally checks binding,
complete weights, reduced rational spelling, residue bounds and resource limits;
JSON Schema validation alone does not establish those conditions.

## Evaluation and weighted counting

Evaluation interprets the original formula on a supplied complete structure.
Quantifiers visit the finite domain, Boolean operators retain their usual truth
tables, and bound-variable shadowing restores the outer environment.

Each predicate `P` has rational weights `w_true(P)` and `w_false(P)`. A structure's
weight is the product of the appropriate weight for every ground atom in the
entire declared signature. The weighted count is the sum of these products over
structures satisfying the sentence. Arithmetic is exact. Unit weights give
ordinary labeled model counts. Signed or zero weights are permitted; a zero
weighted sum does not imply that the sentence has no models.

No isomorphism quotient or division by `n!` is performed. No normalized probability
is implied by a count. Fixed-size model counting does not decide satisfiability
over an unspecified finite size or over arbitrary infinite structures.

## Normalization and recurrence

`Normalize.hs` preserves the original sentence and introduces dependency-ordered
unary/nullary annotations for quantified subformulas. Annotation weights are one.
Quantifier-free bodies are interpreted directly instead of enumerating redundant
fresh body predicates. A general unique-extension/refinement proof for this
implementation remains open; bounded extension checks are executable evidence.

Observations retain exactly the distinctions selected by their count test:

- A justified positive exact/upper-bound obligation can reject values above its
  threshold during accumulation.
- Other exact/upper-bound uses, including negated counts, retain the above-threshold
  category using saturation at `k+1`.
- At-least uses saturation at `k`.
- Modular tests retain residues and permit wraparound; an intermediate unwanted
  residue is not enough to reject a partially constructed model.

The population engine enumerates unary/diagonal types and off-diagonal pair
possibilities. Both orientations of binary atoms and universal constraints are
checked. Weighted group convolution combines observations while retaining labeled
multiplicity. Adding the next distinguished label has a unique inverse by label
restriction; no extra label factor is inserted. Final row/global constraints are
checked after all labels have been added. Zero signed masses may cancel and are
removed from sparse maps without becoming a satisfiability claim.

The independent Python oracle evaluates the original formulas directly and
enumerates small structures. It imports neither the normalizer nor the recurrence.
Six compiled mutations test diagonal handling, reverse pairs, multiplicity,
negated thresholds, modular wraparound and an always-zero answer.

## Fixed-size comparison

Comparison takes one vocabulary/domain/context, assumptions, a left formula and
a right formula. It is unweighted. Complete bounded enumeration partitions models
satisfying the assumptions into both-true, left-only, right-only and both-false.
Equivalence requires both disagreement sets to be empty. Left-to-right implication
requires left-only to be empty. Equal cardinalities alone never establish either.

An empty admissible class returns `VacuousAt`. Refutations retain a complete model
and re-evaluate the original formulas before returning it. Positive answers remain
scoped to this size and vocabulary. The comparator's complete-enumeration limit is
separate from the population counter's practical reach.

## Resources and outcomes

The public shell caps raw bytes, nesting and formula structure. The typed core
checks positive operational limits and bounds domain, Boolean assignments,
transition work, sparse entries and rational size. Evaluation/comparison preflight
uses a conservative formula-visit bound, not a machine-instruction-cost theorem.
The Linux shell caps worker memory, CPU and wall time and uses at most 12 available
CPU IDs. A killed, refused or incomplete run has no completed semantic result.

All mutable Python construction is invocation-local. Request snapshots cross the
process boundary as owned JSON bytes. Haskell values are immutable and the core
has no persistent cache or external effect. Runtime responses retain source,
binary, request and context bindings. Those are reproducibility identities, not
authenticated user intent or proof certificates.

## Research context

Pratt-Hartmann establishes complexity results for the two-variable fragment with
counting, distinguishing finite and general satisfiability. This implementation
does not provide those general decision procedures.
[Primary paper](https://arxiv.org/abs/cs/0411031).

IncrementalWFOMC3 develops lifted counting techniques including modular counting.
This backend uses a separately scoped definitional-extension/saturation design;
the paper's theorems are not a proof of this implementation or its normalizer.
[Primary paper](https://arxiv.org/abs/2605.03391).
