# Assurance boundary

This is a working experimental backend. The local validation snapshot reports
actual execution on the published source tree; CI must qualify its own exact
head. Finite examples, mathematical lemmas and source-level proofs are distinct.

| Obligation | Implemented evidence | Open work |
|---|---|---|
| Admission and CNL lowering | Closed typed inputs; whole-tree checks; duplicate/arity/binding controls; CNL round trips | General parser/adapter refinement theorem |
| Original semantics | Independently written Python/Haskell model evaluations | Source-level evaluator correctness |
| Normalization | Ranked definitions and bounded unique-extension checks | General unique-extension, inlining and implementation proofs |
| Weighted counting | Independent small-model oracles; diagonal/reverse/multiplicity controls; six compiled wrong counters | General partition and recurrence induction tied to source |
| Count observations | Six Lean arithmetic lemmas and 132 Haskell/FCIS example transitions | Haskell-to-Lean refinement |
| Formula comparison | Complete bounded enumeration, independent partitions/witnesses and vacuity controls | Source-level enumeration/partition theorem; all-size reasoning |
| Reporting and resources | Process caps, conservative work budgets, forged-response and interruption controls | Complete shell/refusal and machine-cost proofs |
| Discussion replay | Exact formal-request/result replay and altered-record controls | Authenticity, human approval and unrestricted prose fidelity |

The six Lean results cover saturation, addition of residues, exact/at-most/
at-least observations and a strict bounded-composition fact over natural numbers.
The proof script imports `Std.Tactic`, checks for placeholders and reports axiom
closures. Its deliberately wrong cutoff must fail for an arithmetic counterexample;
a compiler error or timeout does not satisfy that control. These theorems do not
prove the Haskell compiler, decoder, normalizer, counter or shell.

The FCIS fixtures are finite components, including nine observation instances.
`check_fcis_bridge.py` compiles the actual Haskell observation bodies and compares
all 132 fixture transitions. The standard qualification does not invoke native
FCIS or turn the fixture oracle into a proof of the entire counter. The separate
histogram fixture retains an inconsistent-pre-state defense excluded by its state
invariant; a native review may report that excluded-state case. Do not weaken the
invariant merely to remove the diagnostic.

The reference implementation uses ordinary immutable Haskell values and local
maps; Python owns bounded process/transport handling and an independent oracle.
There is no persistent authoritative store, execution grant or external effect.
The current Python shell can be replaced by another adapter only if its input,
resource, binding and refusal contracts are preserved and checked again.

The existing CNL parser is reused, not replaced or proved by this contribution.
The visual notation library is not wired into this backend. No new checked
translation from this relational model into a native FCIS specification or
generated executable is included. These are explicit integration obligations,
not consequences of passing a model comparison.

The proof and validation claims cover the declared assumptions and profiles.
Neither successful replay nor a content digest is a human acknowledgement,
production authorization, general satisfiability result or proof of deployed code.
