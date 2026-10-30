# Experiment contract, scope and acceptance tests

This record fixes the *meaning* of a successful finite experiment independently of its candidate palette.

- **Immutable problem subject:** `corpus.json`, exactly six named `u8-mod-word/1` unary operations with fixed reference IDs and source/baseline expressions.
- **Candidate language:** bounded, pure, acyclic JSON trees. Operations `and`, `or`, `xor`, `add`, `sub`, `not`, `shl`, `lshr` have fixed modulo-256/unsigned meanings. Operand reads are pure and no state is written. Only `x` may be read.
- **Proposer power:** choose a registered list of operations/constants/shift counts and maximum expression-node count. The proposer cannot choose the oracle, expand admitted input widths, claim successful solver status, write a verification receipt, or change cost units.
- **Validity:** a candidate passes only if `execute(candidate,x) == reference.result(function,x)` for every `x ∈ {0,...,255}`. The reference and evaluator are separate source files. A mismatch produces `x`, actual and expected values. Input `x=0` and all boundary values are checked by construction.
- **Search completeness:** `none-within-grammar` only after enumerating the declared grammar within its node limit. `inconclusive-budget` and `inconclusive-grammar-size` are never proof of no solution. The pilot's generated grammar count is conservatively capped before creating intermediate levels.
- **Cost:** one abstract syntax-tree node per input, constant, and operation. The baseline is independently checked. These are *not* machine instructions, CLIF operations, emitted bytes, latency, throughput, or a deployable optimization claim.
- **Replay:** hashes bind exact source files, corpus, candidate and palette; re-execution checks semantic agreement, candidate selection, costs, experiment list and all retained winners. A hash is an integrity commitment only, not a correctness proof or trusted authorization capability.
- **Evidence:** finite exhaustive Python agreement for each candidate, not an SMT theorem, Lean/Verus proof, global minimization or processor conformance result. Interpreter, oracle, Python runtime and host are trusted assumptions.
- **Non-vacuity:** all 256 source inputs must be evaluated; a deny-all candidate has no representation in this grammar. Negative tests include incorrect candidate semantics, altered hash, forged cost, missing experiment, unknown instruction/profile and exhausted budget.
- **Safety/authority:** no IO, host execution, target machine code, runtime publishing, memory effects or external capabilities in the candidate representation. This experiment adds no ZenoFCIS release gate or application Authority capability.

## Required focused checks

```bash
cd examples/contract-guided-algorithms
python3 -m unittest -v test_research.py
python3 research.py replay --catalog evidence/initial_catalog.json
```

A real next-stage Cranelift comparison requires a **separate** target contract with signedness, flags, poison/undefined assumptions, ABI and exact native cost. Translation from this pilot's u8 AST to ISLE/CLIF is not yet implemented and must not be implied.
