# Algorithm source intake — research scope and provenance

**Important distinction:** This is a *targeted source-intake and experiment-design record*, not a claim that all eight PDFs were read cover to cover. The source collection was inspected through its GitHub directory metadata, public chapter listings, and accessible excerpts. The original GitHub binary PDFs were not available for complete in-session extraction. One independently hosted chapter from Gusfield (ch. 12) and an accessible excerpt of the Chinese *Hacker's Delight* translation were examined as PDF text; an illustrative first page of Gusfield's chapter was also visually inspected. Other subject mappings below are from publisher/book contents, book metadata, and established mathematical knowledge. This limitation **must not** be silently upgraded to "full PDF review" or "reproduced textbook code".

The eight file paths below are those in [`lancetw/ebook-1/02_algorithm`](https://github.com/lancetw/ebook-1/tree/master/02_algorithm); Git blobs are included so a later researcher can identify exactly which archive files were offered. This archive grants no implied right to redistribute the books. The pilot contains independently authored small mathematical test problems, not copied book text, examples, tables, illustrations or program listings.

| Archive file / source | Git blob SHA | Intake and target use |
|---|---|---|
| *Hacker's Delight*, 2nd ed., Henry S. Warren, Jr. | `8019c4eb732eb59431b7a9bab0da5d03dad04147` | Publisher contents reviewed: rightmost-bit transforms (ch. 2), bounds/overflow (chs. 2–4), counting bits (ch. 5), bit rearrangement (ch. 7), wide multiplication/division (chs. 8–9), division by constants (ch. 10). **Priority 1** for native instruction synthesis. |
| *Algorithms on Strings, Trees, and Sequences*, Dan Gusfield | `a2a9666842bfe8f5a1554795c1aa9e34ddc8a60c` | Accessible chapter 12 read in part: linear-space string-alignment DP, including Hirschberg-style recomputation. Future independent contract for alignment optimum, tie behavior, witness construction, and O(min(n,m)) workspace. |
| *Data Structures and Algorithm Analysis in C*, Mark Allen Weiss | `3566a6c95ca87131d1ce6b67d4b3d690b60a3714` | Available catalog metadata reviewed: amortized analysis, priority queues, hashing, trees and graph algorithms. Study frontier data structures and hash canonicalization **only after profiling** existing candidate search. |
| *The Algorithm Design Manual*, Steven S. Skiena, 2nd ed. | `59b2aaad1966cdb2e44a5ee5c8c111441af0d911` | Publisher chapter list reviewed: combinatorial search/heuristics, dynamic programming, algorithm-selection catalog. Use as strategy-taxonomy and experimental-baseline guidance, **not** as a source of program correctness. |
| Chinese edition of Weiss, *Data Structures and Algorithm Analysis in C* | `49a5e38dae04eeed6d3d61103d533b8d3365c20e` | Translation/edition of the same general work; don't count as an independent algorithm discovery or statistically independent source. Full PDF not examined. |
| Chinese *Introduction to Algorithms*, 2nd ed., Cormen et al. | `61af4dce59ed540fd6def780f435af70f3322433` | Established algorithm-analysis themes: invariants, dynamic programming, divide-and-conquer, graph search, amortized bounds. Full PDF not examined; seek chapter-specific references before implementation. |
| Chinese *Introduction to Algorithms* solutions | `f2c48f14196519e062b97266bbccf33e7b901079` | Treat as potential **training contamination**, not independent correctness evidence; do not use its answers as both candidate source and oracle. Full PDF not examined. |
| Chinese *Hacker's Delight*, 2nd ed. | `08175fb7cdc589dff28dba430dff2856ccd8703e` | Translation of Warren; accessible PDF text excerpt reviewed for chapter layout and emphasis on integer/bit operations. Avoid double counting as independent source. |

## Research hypotheses selected from the sources

| Problem family | Contract invariants that must survive | Initial comparison / falsifying control |
|---|---|---|
| Rightmost set bit | Full word domain, zero input, fixed-width modular two's complement | `x & (0-x)` and `x & (x-1)` forms; omit subtraction from the palette as a negative control. This pilot checks original u8 formulations only. |
| Divide or remainder by power of two | Unsignedness, divisor >0, rounding direction, exact width | Logical shift/mask versus mathematical `//` and `%`; omit shifts or the mask constant as controls. |
| Signed saturation / overflow | Mathematical sum and clamp boundaries, no intermediate overflow, flag/ABI policy | Compare branch/bit-mask alternatives, including both positive/negative extremes; **not covered by u8 pilot**. |
| Constant division by non-power-of-two | Exact quotient and remainder for every input, signed truncation or floor, multiplication widening and shift rules | Correct magic-multiplier candidates with independent high-word multiplication and correction oracle. No claims until a new checker profile exists. |
| Wide multiplication and division | Full-width intermediate limbs, carry/borrow, division quotient/remainder invariants | Arithmetic oracle over unbounded Python integers plus a distinct native implementation checker; this is a future experiment, not today's u8 proof. |
| Gray-code and rotate operations | Exact bit widths, logical versus arithmetic shift, rotation wraparound | Bit-neighbor-parity and one-position rotate experiments with missing-shift negative controls; available in the pilot. |
| Bit counting and scans | Zero handling, endianness, word width, popcount/CLZ/CTZ domain | Compare table, tree reduction and target ISA primitives under actual backend costs. |
| Dynamic programming / alignment | Equality of optimal value, witness validity, tie rules, workspace bound and total termination | Full-matrix reference vs linear-space solution; measure peak workspace separately from runtime. |
| Search architecture | Search order, admissible bounds, completeness scope, deterministic replay, timeout status | Uniform search versus informed frontiers and DP memoization under sealed workloads. |
| Candidate catalog indexing | Identity normalization and no loss of distinct semantic/proof subjects | Baseline exact hash lookup vs more complex string/tree index; add the index only if profiling justifies it. |

## Bibliographic and access aids

- Warren: [publisher table of contents](https://www.pearson.com/en-us/subject-catalog/p/hacker-s-delight/P200000000672/9780133085013), including constant division and overflow sections.
- Skiena: [publisher 2nd-edition table of contents](https://link.springer.com/book/10.1007/978-1-84800-070-4), including search, dynamic programming and the problem catalog.
- Gusfield: [an accessible copy of chapter 12](https://dmice.ohsu.edu/bedricks/courses/cs655/pdf/readings/gusfield_chap_12.pdf), which discusses Hirschberg's linear-space alignment method. This excerpt is not the complete book.
- Weiss: [book metadata and contents](https://books.google.com/books/about/Data_Structures_and_Algorithm_Analysis_i.html?id=JngZAQAAIAAJ).

## Knowledge extraction gate for later chapters

Before importing a new strategy, record: (1) exact public bibliographic chapter/section, (2) the mathematical claim rewritten independently, (3) domain and refusal semantics, (4) relevant target/memory/cost assumptions, (5) an independent oracle or proof obligation, (6) known counterexamples, (7) training-versus-evaluation separation, and (8) authorship/copyright provenance. A claimed novel algorithm must additionally have a substantive prior-art review; rediscovering an established technique is not mathematical novelty.
