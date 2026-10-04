Independent checks passed for 100 frozen cases.

Fixed node-count criterion: **passed**.

Observed search: 3 iterations, 1000 nodes as a threshold checked between rewrite batches, and 30.0 seconds as the LP solving budget.

This search configuration was amended after the initial run. Meeting the fixed cost threshold in this rerun does not establish success under the original registered search protocol.

| Family | Cases | Raw | Local | Rewrite egg | Semantic egg | Semantic wins vs local |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| small_gate | 8 | 34 | 18 | 18 | 18 | 0 |
| distributed_factoring | 18 | 217 | 169 | 170 | 170 | 0 |
| absorption_complement | 18 | 159 | 48 | 42 | 42 | 1 |
| mixed_ite_demorgan | 18 | 157 | 120 | 124 | 120 | 0 |
| shared_guards | 18 | 357 | 246 | 243 | 243 | 6 |
| balanced_general | 20 | 582 | 294 | 257 | 223 | 11 |

All comparisons use actual shared FCIS-basis node counts. Negative deltas mean fewer nodes.

- semantic_vs_local: 816 vs 895 nodes; 18 wins, 78 ties, 4 regressions.
- rewrite_vs_local: 854 vs 895 nodes; 13 wins, 80 ties, 7 regressions.
- semantic_vs_rewrite: 816 vs 854 nodes; 12 wins, 88 ties, 0 regressions.
- local_vs_raw: 895 vs 1506 nodes; 93 wins, 7 ties, 0 regressions.

Independently checked protected selection uses a semantic candidate only when it strictly reduces actual nodes; otherwise it retains the checked local baseline. Its aggregate is 812 nodes over 100 cases, with selected variants {'local_baseline': 82, 'semantic_egg': 18}.

Descriptive Rust timing observations (milliseconds):

| Variant | Component | Samples | Sum | Median | Maximum |
| --- | --- | ---: | ---: | ---: | ---: |
| local_baseline | evaluation | 100 | 13.107 | 0.112 | 0.346 |
| local_baseline | total | 100 | 13.115 | 0.112 | 0.346 |
| raw | evaluation | 100 | 14.044 | 0.122 | 0.352 |
| raw | total | 100 | 14.054 | 0.122 | 0.352 |
| rewrite_egg | saturation | 100 | 214.378 | 0.130 | 200.159 |
| rewrite_egg | extraction | 100 | 10316.296 | 1.481 | 10012.563 |
| rewrite_egg | evaluation | 100 | 15.238 | 0.123 | 1.141 |
| rewrite_egg | total | 100 | 10546.208 | 1.910 | 10013.850 |
| semantic_egg | saturation | 100 | 16.985 | 0.129 | 0.862 |
| semantic_egg | extraction | 100 | 744.153 | 1.400 | 119.574 |
| semantic_egg | evaluation | 100 | 14.939 | 0.123 | 0.926 |
| semantic_egg | total | 100 | 776.372 | 1.743 | 120.479 |

Timing components:

- saturation: E-graph construction, bounded rewrite search, and final semantic unions where enabled.
- extraction: Extractor construction, solving, and expression reconstruction; the LP budget limits solving only.
- evaluation: Candidate parsing/lowering, admission, complete finite checks, and receipt serialization.
- total: This variant's complete Rust proposal/check interval; excludes the separately precomputed Python baseline search.

Separately measured Python 3.9.6 baseline search: 3 repetitions per case, 59.834 ms sum of case medians and 0.146 ms median case. This cross-language observation does not establish a fair algorithm-speed ratio.

Limits:

- No arithmetic/error-producing expression, authoritative state, effects, shell behavior, temporal or weak-omega equivalence is established by this corpus.
- This is a deterministic synthetic sample, not an application distribution or an optimum-extraction theorem.
- Recorded solver optimality applies to the extraction surrogate, not necessarily actual shared FCIS-basis node count.
- Rust timings are component observations; baseline outputs were precomputed in Python, so lowering time alone omits local optimization overhead.
- Fewer modeled DAG instructions need not imply faster generated code or lower protocol-visible logical budgets.
