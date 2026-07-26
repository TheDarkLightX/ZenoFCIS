# Deterministic synthesis integration

## Boundary

`zeno-fcis-synthesis` is a bounded search kernel for reviewed ZenoFCIS schemas,
contracts, holes, and grammars. It enumerates candidate assignments in
canonical ZCVE-byte order and gives every assignment to an external
`CandidateChecker`. The checker owns reference/SMT refinement, composition, and
counterexample authority.

An LLM or heuristic may propose values before problem construction. It cannot
select the schema, grammar, dependency set, search bound, checker result,
certificate, or release status.

## Inputs and outputs

- input: nonzero schema, contract, grammar, algorithm, and checker hashes;
- holes: unique stable IDs and nonempty closed candidate domains;
- budget: an exact maximum assignment count that must cover the full Cartesian
  search space;
- output: either a verified assignment plus a content-addressed certificate, a
  complete no-solution certificate, or a typed incomplete/indeterminate error.

## Laws and negative cases

- reordering hole declarations or candidate insertion history does not change
  the trace, selected candidate, blockers, or certificate;
- duplicate hole IDs and duplicate candidate encodings fail closed;
- a budget below the exact search cardinality is `IncompleteSearch`, never
  evidence of no solution;
- every rejection retains a normalized content-bound counterexample;
- an indeterminate checker result blocks certification immediately;
- acceptance requires nonzero independent reference and composition claims;
- the certificate binds schema, contract, grammar, algorithm, checker, bounds,
  complete trace hash, selected assignment, counterexamples, and nonclaims.

## Resource bounds

The initial kernel admits at most 64 holes, 1,024 values per hole, and
1,000,000 complete assignments. Cardinality uses checked multiplication before
search begins. Every generated assignment is immutable and bounded by the
existing ZenoFCIS value encoder.

## Nonclaims

This crate does not embed ESSO, an SMT solver, a compiler, or an LLM. It is the
deterministic authority boundary into which a mounted ESSO proposer and
independent checkers plug. Unit tests establish bounded enumeration and binding
laws; they do not prove a synthesis grammar complete, a candidate economically
correct, or a generated runtime refined.
