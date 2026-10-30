# *The Algorithm Design Manual*, second edition — contract-driven synthesis intake

**Primary source:** Steven S. Skiena, *The Algorithm Design Manual*, 2nd ed. (Springer, 2008). User-supplied **739-page original PDF**, SHA-256 `fe1a100b3f30b910cb33bcc64d90f4d62892e99bd775f28cc2933d1c359640d4`. PDF page numbers below include front matter and differ from printed pagination. This is a focused technical reading and independent engineering analysis, **not** a claim to have validated every algorithm or exercise. The PDF is not uploaded to this repository; no source listings are reproduced.

## Contract and experiment lessons

| Source section (uploaded PDF page) | Lesson from the book | Application to ZenoFCIS | Evidence status |
| --- | --- | --- | --- |
| §§1.3.2–1.3.3 (pp. 24–26) | Model legal inputs and required outputs precisely; refute wrong algorithms with simple independently verifiable counterexamples. | Bind original domain and observations before proposing implementations. Retain independent concrete semantic witnesses. | Implemented in finite profile |
| §1.4 (pp. 31–33) | Map concrete tasks to established mathematical problem classes before selecting techniques. | Future strategy records classify work as bit-vector transformations, integer quotient/remainder, graph search, dynamic programming, etc.; those are proposer hints, not specification changes. | Proposed |
| §7.2 (p. 250) | Prune search as soon as a partial solution cannot lead to a success; exploit sound symmetry. | Prove necessary conditions of a closed candidate grammar **before** enumerating it. Distinguish impossibility from a budget-bounded failure. | **Implemented** with two grammar-closure laws |
| §8.1 (pp. 285–290) | Dynamic programming saves repeated work by caching subproblem results. | Experiment with memoization of **fully verified** semantic signatures, accounting for memory. Sample-only signatures are not an acceptance criterion and may lose completeness. | Proposed |
| Ch. 9 | Approximate or heuristic solutions can be useful even when exact optimization is intractable. | Keep separate `verified-candidate`, `optimal-within-grammar`, `inconclusive-budget`, and `structurally-impossible` results. | Partially implemented |
| Ch. 10 (pp. 368–372) | Choose an algorithmic strategy to solve the *right* problem, then improve implementation details. | Preserve original requirements, declare cost measurement and baseline, and assess provenance of each suggestion. | Research design |
| Part II catalog | A catalog helps recognize and reuse existing problem types and approaches. | Develop a **cited strategy index**, but do not interpret textbook examples as proof or as new algorithmic discoveries. | Proposed |

## Concrete result: bitwise-only impossibility preflight

The registered closed unary `u8-mod-word/1` grammar allows some palettes whose only operations are `AND`, `OR`, `XOR` and `NOT`. Every such operation computes output bit `i` entirely from the input bit at the **same** index `i`. Constants do not change this and inductive composition cannot create cross-bit dependence.

The **independent reference** for both `lowest-set-bit-u8` and `clear-lowest-set-bit-u8` gives output bit 1 values that differ between inputs 2 and 3, despite input bit 1 being 1 in both cases. The structural preflight therefore refutes *all* expression trees in the respective Boolean-only grammars, regardless of the search-depth limit. Their earlier bounded searches had checked 183 expressions each.

This stronger locality rule is a derived specialization of Warren's right-to-left theorem (*Hacker's Delight*, §2-1, uploaded PDF p. 25). Applying it at palette admission rather than inspecting and rejecting descendants is an instance of Skiena's §7.2 search-pruning method (uploaded PDF p. 250). It is not a new mathematical theorem, and the Python code is not a formally qualified implementation of the induction proof.

Together with the prior three low-prefix refutations, five controls now avoid **477 candidate enumerations** (366 + 111). Preflight itself consumes work, so these numbers do **not** demonstrate elapsed-time speedups, native instruction savings or improvements to Cranelift. The new results retain separately replayed finite input/output-bit witnesses.

## Two other research directions supported by this book

**Model-guided search rather than blind enumeration:** First identify whether the contract belongs to a known family. If there is a safe family-specific grammar, propose it and compare it against a general grammar under equal budgets. Record inference overhead. Never let the strategy classifier change input domains, overflow, refusal behavior or effects; matching a catalog label is neither a correctness proof nor a compiler lowering certificate.

**Verified dynamic programming:** Repeated subproblems could be cached using semantic signatures and known lower bounds. Exact signatures under a complete finite domain permit stronger deduplication than signatures based on sampled tests. For an extended arithmetic or machine model, equal sample outputs do not establish semantic equality; they must be independently checked before sound merging. Report cold and amortized cost, memory use, and missed solutions.

## Reproduction and nonclaims

From `examples/contract-guided-algorithms`:

```sh
python3 -B -m unittest -v test_research.py
python3 -B research.py replay --catalog evidence/initial_catalog.json
```

The authoring-stage local run recorded **16 focused tests passing**, six independently replayed finite 8-bit candidates, twelve palette experiments, two strict bitwise refutations, three prefix refutations, and one remaining bounded negative search. That is narrow exhaustive finite evaluation under trusted Python/reference semantics, **not** a native target proof, LLVM/Cranelift transform, or verified speedup. New source revisions and CI must be checked again before accepting changes.
