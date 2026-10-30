# *Hacker's Delight*, second edition — contract-guided research intake

**Intake status:** The linked 12.8 MB GitHub PDF could not be fetched by the available text-only GitHub connector. I reviewed the major technical sections of all 18 chapters in an [accessible HTML rendering](https://studylib.net/doc/28542894/hacker-s-delight-2nd-edition) and cross-checked the edition and chapter contents with the [publisher's PDF sample](https://ptgmedia.pearsoncmg.com/images/9780321842688/samplepages/0321842685.pdf). HTML extraction garbles some equations and figures. This is **not** a claim of exact review of every formula, table, figure, or exercise in the binary PDF. Unclear formulas and machine-specific details require direct verification against legible authorized references.

**Attribution and prior art:** Henry S. Warren, Jr., *Hacker's Delight*, second edition (Addison-Wesley, 2013). The mathematical techniques are established prior art; automated rediscovery is not new mathematics. These are independent research notes, not reproduced chapters, tables, or code listings. No book PDF or program listing is copied into this repository.

## Chapter-by-chapter engineering extraction

| Book chapter | Contract or experiment we can derive | Required care | Priority |
| --- | --- | --- | --- |
| 1. Introduction | Fix machine width, word interpretation and logical/arithmetic shifts before search. | The book's simplified instruction-cost model is **not** Cranelift machine bytes or measured target throughput. | P0 |
| 2. Basics | Rightmost-bit identities, checked-overflow predicates, flag- and carry-sensitive comparisons; **right-to-left computability preflight**. | Input/output low-bit dependencies, signedness, flags and carry must be explicit. | **P0 (first gate implemented)** |
| 3. Power-of-two boundaries | Rounding to multiples and next representable powers. | Define zero and out-of-range behavior; distinguish mathematical overflow from modulo wrap. | P1 |
| 4. Arithmetic bounds | Range propagation to establish rewrite preconditions. | Prove the original source domain implies the optimization precondition; do not silently narrow it. | P1 |
| 5. Counting bits | Popcount, parity, CLZ/CTZ; tree, de Bruijn, table and ISA strategies. | Fix zero input conventions, lookup-memory cost and multiply/shift semantics. | P1 |
| 6. Searching words | First zero byte and positions of longest/shortest runs. | Define endianness, indexing convention and tie/no-match outcomes. | P2 |
| 7. Rearranging bits/bytes | Compress/expand, interleaving, permutations, mask-specialized precomputation. | Specify mask, output arrangement, target feature (e.g. BMI2) and cold vs amortized setup. | P1 |
| 8. Multiplication | Multiword products, high-half signed/unsigned conversions, constant-multiply chains. | Preserve wide limbs, carries, sign corrections and register pressure. | **P0 U256** |
| 9. Integer division | Full-width quotient/remainder contracts, short/long division alternatives. | Define signed truncation vs Euclidean/floor, division by zero and MIN/-1. | **P0 U256** |
| 10. Division by constants | Multiply-high magic reciprocals, correction terms, signed shift bias; domain-bound specialized multipliers. | Exact quotient rounding, admissible input bound and correction step are obligations. | **P0 Cranelift** |
| 11. Elementary functions | Integer sqrt/cuberoot, addition-chain exponentiation, log2. | Prove result interval, termination and exact zero/overflow behavior. | P1 |
| 12. Unusual bases | Negative-base encodings as representation-refinement tests. | Preserve interpretation of bit sequences, not just identical stored bits. | P3 |
| 13. Gray code | Bidirectional Gray conversion and one-bit changes for incrementing. | Exact word width, wrap-around, and inverses. | P2 |
| 14. CRC | GF(2) polynomial arithmetic; compare bitwise/table/ISA implementation families. | Polynomial, reflected input/output, bit order, initial register and final XOR are all part of the contract. | P1 |
| 15. Error-correcting codes | Hamming/SEC-DED syndrome generation and correction. | Specify error budget, syndrome layouts, uncorrectable outcomes and correction effects. | P2 |
| 16. Hilbert's curve | Coordinate↔rank conversions and bit-parallel vs iterative search. | Establish inverse law and orientation for exact finite grid order. | P2 |
| 17. Floating point | IEEE conversions, NaN/infinity/signed zero and rounding. | Integer bit-equivalence is insufficient; flags, NaN payloads and rounding mode can be observable. | P2 |
| 18. Prime formulas | Bounded sums/products and number-theoretic constructions. | Mathematical definability does not establish executable efficiency. | P3 |
| Appendices | Four-bit arithmetic truth tables and Newton method for oracles. | Small finite exhaustive evidence is not a universal machine-code proof. | P1 |

## Implemented use: a proof-oriented grammar feasibility check

Chapter 2 discusses **right-to-left computability**. For an 8-bit unary function `F`, write `a ≡_k b` when the lowest `k` bits of inputs `a` and `b` agree. A function respects low-bit dependency iff:

```text
For every k ∈ {1,...,8} and all a,b in 0..255:
    a ≡_k b   implies   F(a) ≡_k F(b)
```

The atoms `input`, constants, bitwise NOT/AND/OR/XOR, modulo-256 ADD/SUB and fixed left shifts all respect that dependency, **and composition preserves it**. Positive logical right shifts can move higher bits into lower positions, so this particular closure argument cannot reject a grammar containing one. A missing right shift does not imply impossibility for arbitrary ISAs or for any grammar with another rightward-information primitive.

The new preflight checks the *independent reference*, never a candidate interpreter. It obtains a concrete witness of violated low-bit dependency; the admitted instruction set is checked for closure. That establishes that no tree of this **closed grammar** can implement the reference, regardless of search depth. The check does not claim that the Python interpreter has been machine-verified.

| Problem / inadmissible palette | Witness inputs with equal bit 0 | Different required output bit 0 |
| --- | --- | --- |
| unsigned `x // 8`, no right shifts | 0 and 8 | 0 versus 1 |
| binary-to-Gray encoding, no right shifts | 0 and 2 | 0 versus 1 |
| 8-bit rotate left 1, no right shift | 0 and 128 | 0 versus 1 |

Old bounded searches inspected 18, 9 and 84 candidate trees respectively (111 total). New searches report **`infeasible-right-to-left-palette` with zero candidates enumerated**. The preflight itself costs work, so these counts do not prove a wall-clock speedup.

Every experiment now persists `preflight` (null when absent) with a bound witness and receives `zeno-fcis/algorithm-research-catalog/2`. Replay recomputes the reference dependency and checks the full expected witness; forged evidence is refused. The original six successful, exhaustively checked u8 examples and the independent reference remain unchanged.

This is more informative than `none-within-grammar` (a bounded negative result), but narrower than a machine-checked SMT theorem for arbitrary word widths.

## Contracts worth implementing next, not delivered by this PR

1. **Constant signed division (ch. 10):** Specify `q = trunc(n/d)` and `r = n-qd`, word width, exceptional `d=0` or `MIN/-1`, high-half multiply/correction. Check the complete domain against an independent mathematical oracle. Pin target machine lowering and cost.
2. **Wide unsigned division (chs. 8–9):** Require `q d + r = n`, `0 <= r < d`, nonzero divisor, full intermediate widths and failure ordering. Verify source/target representation, not merely random quotient samples.
3. **Mask-specialized compress/expand (ch. 7):** Check `expand(compress(x,m),m) = x & m`, retain a proof that each output-position convention matches the contract; account for precomputed masks in cold and amortized cost.
4. **Population count and zero scans (ch. 5):** Compare table, tree, de Bruijn and target ISA realizations. Fix `clz(0)` and `ctz(0)` rather than inheriting undefined or arbitrary target behavior.
5. **CRC32 (ch. 14):** Parameterize the polynomial, reflection, initial state, update bit order, final XOR and expected residue before optimizing bitwise/table/hardware variants.
6. **Integer sqrt (ch. 11):** Establish `r*r <= n < (r+1)*(r+1)` in a sufficiently wide mathematical domain and prove termination; compare Newton and restoring methods.

## Extension/verification gates

- Apply structural impossibility rules to wider words only with checked source semantics, and preferably an SMT formulation over **two symbolic inputs**. Solver unknown/timeout means inconclusive.
- Every new instruction must have a proved compatibility rule. Rotates, right shifts, high-half multiplication, flags, memory operations, and variable shifts can invalidate naive low-bit locality.
- Keep algorithmic technique labels as untrusted proposal metadata. The contract, proof/checker, artifact source identity, and target cost model remain independent.
- Use actual emitted bytes/instruction throughput for the Cranelift launch. The current expression-tree node metric is not native execution cost.
- Keep training and evaluation independent; literature examples used to construct palettes cannot also be presented as novel blind evaluation cases.
- Benchmark **preflight overhead**, not just excluded candidates; retain negative and inconclusive results as first-class evidence.

## Reproduction

From `examples/contract-guided-algorithms`:

```bash
python3 -B -m unittest -v test_research.py
python3 -B research.py replay --catalog evidence/initial_catalog.json
```

The first gate's observed local result was **14 tests passing** and an independently replayed catalog with six verified finite entries, twelve experiments and three structural impossibility witnesses. CI and external native execution remain separate gates. This research component does not grant ZenoFCIS application Authority or touch effect handlers.
