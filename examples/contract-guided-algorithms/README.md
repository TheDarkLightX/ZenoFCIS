# Contract-guided algorithm discovery: finite research pilot

**Status:** isolated ZenoFCIS 2.7 research experiment, not a released API, production verifier, Cranelift backend, or application Authority. Python 3.11+ standard library only. This experiment does not modify `zeno-fcis` execution, decision, publication, or effect behavior.

## The design in one picture

```text
reviewed closed problem + fixed u8 semantics
                       |
      human/literature/LLM palette proposals (data)
                       |
          deterministic bounded search
                       |
              sampled rejection
                       |
         INDEPENDENT full-domain oracle
                       |
            abstract-cost comparison
                       |
        content-bound catalog and replay
```

**The source contract, not the proposer, decides what counts as correct.** `corpus.json` binds six closed 8-bit functions to a fixed independent reference in `reference.py`. A palette can vary operations, constants, shifts, and search-size ceiling, but cannot change the reference, word width, inputs, semantics, or checking method. The search interpreter is separate (`research.py`). Two structural [feasibility preflights](HACKERS_DELIGHT_READING.md), derived from *Hacker's Delight* Chapter 2 and paired with Skiena's search-pruning method, now reject **five** impossible instruction palettes with independently replayed finite witnesses, before any candidate enumeration. Successful proposed expression trees are checked on **every one of the 256 possible inputs**. Probe inputs are a search filter only, not evidence of correctness.

Existing ZenoFCIS 2.6/2.7 boundaries to reuse in a later integrated iteration:

- [`docs/DETERMINISTIC_SYNTHESIS.md`](../../docs/DETERMINISTIC_SYNTHESIS.md): proposals are not checker authority; finite budgets and incomplete results remain distinct.
- [`docs/LANGUAGE_NEUTRAL_SYNTHESIS.md`](../../docs/LANGUAGE_NEUTRAL_SYNTHESIS.md): contracts, grammars, candidate programs, and emitted-target conformance are different subjects.
- [`docs/zal/REFINEMENT.md`](../../docs/zal/REFINEMENT.md): a declared refinement preserves the exact listed observations.
- [`docs/ABSTRACT_MACHINE_DISCOVERY_27_PROPOSAL.md`](https://github.com/TheDarkLightX/ZenoFCIS/blob/draft/2.7-abstract-machine-discovery/docs/ABSTRACT_MACHINE_DISCOVERY_27_PROPOSAL.md) on the experimental 2.7 branch: reusable, versioned macros require independent checking and explicit budgets.
- [`docs/MACHINE_CODE_SYNTHESIS_27.md`](https://github.com/TheDarkLightX/ZenoFCIS/blob/wip/2.7-machine-code-20261028/docs/MACHINE_CODE_SYNTHESIS_27.md) on the 2.7 machine-code WIP branch: native bytes, ABI and SMT checks need a **separate** target profile.

These last two documents live on development branches; the links are conceptual references, not assertions that they are already available on `main`.

## Exact pilot behavior

| Function | Reference meaning for `x` in 0..255 | Knowledge source / hypothesis |
|---|---|---|
| `lowest-set-bit-u8` | smallest set bit as a power of two, or 0 | rightmost-bit arithmetic |
| `clear-lowest-set-bit-u8` | clear the least-significant set bit | rightmost-bit arithmetic |
| `divide-by-eight-u8` | floor(x / 8) | division by power of two |
| `modulo-eight-u8` | x mod 8 | power-of-two masking |
| `gray-code-u8` | adjacent-bit XOR (Gray encoding) | Gray-code bit identities |
| `rotate-left-one-u8` | cyclic left shift of eight bits | bit rearrangement |

The candidate language is a bounded **pure expression tree**, not executable Python. The profile is `u8-mod-word/1`: all additions, subtractions, bitwise complements and left shifts are reduced modulo 256; right shifts are logical; shift counts are literals 0..7. No signed integers, branches, variable shifts, flags, traps, memory effects, calls, nondeterminism, poison, ABI, or timing model are admitted. Unsupported terms refuse.

`corpus.json` includes the expected-vocabulary experiments and negative-control palettes deliberately missing operations needed by the known strategies. The checker is **not** supplied by the palette. The reference uses independent simple algorithms: scanning bit positions and Python integer division/modulo rather than executing the proposed instruction expression.

`research.py` enumerates candidates in increasing abstract tree-node count, with deterministic operation/operand ordering. It does **not** globally prove minimum costs. It records whether each palette found a verified result, exhausted its stated finite grammar without one, or stopped inconclusively due to candidate or grammar-size limits. The stopping rule is never interpreted as proof of impossibility beyond the declared grammar.

## Reproduce and inspect the evidence

From this directory:

```bash
python3 -m unittest -v test_research.py
python3 research.py replay --catalog evidence/initial_catalog.json

# Separate mutable work directory, fresh artifact path:
mkdir -p /tmp/zeno-algorithm-research
python3 research.py run --corpus corpus.json \
  --max-candidates 10000 --out /tmp/zeno-algorithm-research/catalog.json
python3 research.py replay --corpus corpus.json \
  --catalog /tmp/zeno-algorithm-research/catalog.json
```

`run` refuses an existing output instead of overwriting it. A successful verification response means **exhaustive finite comparison of one 8-bit reference** using the pinned local Python reference/interpreter. It does not mean a proof for 64-bit inputs, a formal proof of the Python source, proof of the physical ISA, or a faster Cranelift rewrite.

The catalog retains:

- A structural palette-level refutation when a contract needs downward bit information but the admitted operations preserve low-bit prefixes. This has a finite witness and replay; see [reading notes](HACKERS_DELIGHT_READING.md).

- SHA-256 identities of the exact corpus, candidate expressions, palettes, and the two checker/interpreter source files.
- Reproducible palette-by-palette search results, including failed searches and exact candidate/check budgets.
- A deduplicated entry per distinct `(contract identity, candidate identity)` and its full-domain evidence.
- **Abstract expression-tree size** of the candidate and independently verified reference baseline. This is **not** emitted instruction bytes, latency, micro-ops, throughput, Cranelift IR size, or gas.

`replay` rechecks the full finite input domain, cost records, source identities, search outcomes, completeness of the experiment list, and exactly the candidates that would be selected by replayed searches. Rehashing a maliciously changed candidate is insufficient to make it pass the semantic check. The root filesystem, Python interpreter, and reference implementation remain named trusted premises; hashes are integrity bindings, not authenticated provenance.

## What would qualify the next milestone?

1. **Explicit semantic profiles:** Lift from `u8-mod-word/1` to separately qualified `i64/u64`, signed saturation, checked/wide arithmetic, flag, error and ABI profiles. Never translate one into another by narrowing inputs.
2. **True machine cost:** Hand candidate/contract packets to the 2.7 machine-code route, emit actual bytes, and compare against an exactly matched GCC/Cranelift baseline. Native code byte counts and abstract node counts are different objectives.
3. **Independent native checks:** Include the target decoder/SMT check, emitted-object reproducibility and execution against a reference. All relevant calling-convention effects, scratch conditions, flags, poison/undefined behavior and hardening constraints must be explicitly modeled or remain unsupported.
4. **Knowledge-driven search:** Allow separately versioned strategy templates (e.g., reciprocal multiplication, overflow detection, multiword arithmetic, linear-space dynamic programming, search-frontier organization), each with preconditions, counterexamples, and independently checked expansion. Models propose these **as data**, not as executable checker code.
5. **Cross-target catalog:** Keep separate target identities, cost evidence and checker receipts, and deduplicate reusable candidates without erasing competing baselines or failure evidence.
6. **Experimental discipline:** Compare manually curated palettes, model-proposed palettes and literature-informed palettes under equal budgets, predeclare train/test separation, and count all failed and inconclusive runs. A prompt withholding a textbook example is not proof that an LLM has never seen it during training.

See [`SOURCES.md`](SOURCES.md) for the eight-book inventory and rights caveats, and the [*Hacker's Delight* chapter research map](HACKERS_DELIGHT_READING.md) for the structural feasibility rule, explicit mathematical limits, and remaining source-access caveats. No copyrighted PDF or copied implementation is included here.
