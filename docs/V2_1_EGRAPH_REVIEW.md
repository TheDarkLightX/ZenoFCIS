# Semantic e-graph study: bounded review for ZenoFCIS 2.1

Reviewed 2026-10-04 by **gpt-6-astra, reasoning effort max**, in a separate
reviewer invocation. Status: research review and design input; no production
qualification or V2 completion condition.

The published study supports a useful Boolean-only optimization prototype.
This review found **no issue that invalidates the reported 816 versus 895
instruction comparison or the checked Boolean semantics of the stored successful
programs**. Its acceptance harness needs additional work before serving as a
2.1 artifact acceptance boundary. The [proposed design](V2_1_TRANSFORM_DESIGN.md)
keeps those obligations separate from smaller V2 completion.

## Scope and evidence

The exact published subject is
[`a69ed8db594d95279a46bff0f65185ef67d51f98`][study], including the seven-page
[paper][paper], source, frozen corpus, raw runs, amendments, analysis and Lean
packet. It adds 55 experiment files in two commits above the recorded production
baseline `cb1366cddc5c813f9977b377f541e64bcf94035c`; the coordinator's comparison
found no production-file changes. This is an experiment review, not an audit of
the whole ZenoFCIS implementation or the separately pinned egg fork.

The complete local governing inputs were read: `AGENTS.md` (54 lines),
`V2_1_FACTORY_PLAN.md` (469), the repository handoff skill (22), and ADRs 0003
(190) and 0005 (144). Functional-core safety guidance was also applied.
Selected scalar and authority APIs were inspected in the dirty V2 workspace
at HEAD `1ed6f88f7b1098bed6c409bc9aa3c12c99d43c3c`; that HEAD alone does not
identify its working tree or establish current qualification. No live source
was edited, built, committed, pushed or merged by this reviewer.

| Evidence | What was established in this review |
| --- | --- |
| Stored bundle | All 54 manifest-listed file digests match; the manifest excludes itself. Five proof-receipt file digests also match. |
| Fresh coordinator Python replay | Both successful raw receipts pass the current published analyzer; all non-timing analysis fields match. The analyzer self-test rejects its 13 planted mutations; both earlier failed runs are refused by the success analyzer. See the [replay receipt](evidence/v2_1_egraphs/replay-20261004.json). |
| Fresh reviewer file/record checks | Across the two successful runs, all 800 variant records have matching recomputed program/truth-signature digests: 1,600 checks, zero mismatches. Programs, ordered observations, outputs, fallback choices, configuration, controls and selected search metadata agree between runs. |
| Stored execution evidence | Rust build/tests, original optimization runs and the Lean kernel replay are retained reports. No fresh optimizer build/run, Rust tests, Lean or Verus execution occurred in this review. |

The [reviewer's supporting record](evidence/v2_1_egraphs/source-check-20261004.json)
was produced by `astra/review_checks.py` in the review handoff directory. It also
records hashes of the inspected live V2 files. Python 3.12.3 replay differs from the stored
3.9.6 analysis only in floating-point timing sums, by at most about
`3.64e-12` milliseconds; this is retained in the coordinator receipt. Matching
digests establish consistency with this pinned bundle, not independent
authentication of historical executions.

## Findings and their effect

Severities below rank work needed for integration. They are not findings that
the study exceeded its explicitly limited claims.

| ID / priority | Finding | Effect on the study and 2.1 |
| --- | --- | --- |
| R1 / medium | The analyzer recomputes semantics, coverage and cost, but does not validate each `checksums` object, the serialized program `profile`, or executable/source provenance. Negative controls are accepted from their IDs and `passed` flags; their arithmetic programs are not independently executed by Python. [Analyzer:37–73,119–164][analyzer-check], [harness:138–150,292–302][harness-bind]. | The actual stored digests were checked separately and match. This does not overturn the numerical/Boolean result. The analyzer is not a general receipt admission/replay API; 2.1 must bind canonical artifacts, semantics, domain, checker and actual runtime independently. |
| R2 / medium | Search has between-batch node/iteration/time checks and a CBC solving budget, without an enforced whole-process memory cap or deadline. The final stored graph reaches 1,351 nodes under the 1,000 threshold. [Optimizer:235–312][optimizer-limits]. | The paper and README disclose this correctly. The evidence supports bounded configurations on the recorded host, not a hard memory bound. Integration needs externally enforced caps, bounded decoding and refusal/fallback on termination. |
| R3 / medium | The formal theorem does not establish correspondence of the Rust enumerator, checker, serialization, lowering or runtime. Error observations use Rust `Debug` spelling. The experiment also omits authoritative decisions, laws and metering. [Evaluation:7–11][evaluation], [Lean:146–176][lean-gate]. | These are explicit assurance boundaries, not counterexamples. Reusing the theorem or truth tables cannot qualify the new V2 execution route. A stable error codec and source-bound executable checker are needed. |
| R4 / low | The harness records FCIS/egg commit strings and the LP source/lock digests, but does not derive the commit identities from a verified source closure. `vendor/egg` is absent from the published subtree; its path dependency has no source checksum in Cargo.lock. [Harness:19–20,292–302][harness-bind], [Cargo files][cargo]. | The README supplies an exact fetch/check-out recipe, so this is not evidence that reproduction is impossible. The fork's source, patch, native solver build and licenses remain unverified here. Pinning and license review precede dependency adoption. |

## Assurance chain checked against source

**Domain, masks and complete enumeration.** `parse_outputs` admits 0–6 declared
inputs, 1–16 outputs, bounded expression text and nodes, and canonical `vi`
names. The study corpus uses 3–6 inputs. Every variant is lowered using the same
full Boolean input schema, including unused inputs. The signature has one bit
for every row: `rows = 2^n`, with `u64::MAX` at 64 rows to avoid a shift by 64;
other masks are `(1 << rows) - 1`. Variable `vi` selects bit `i` of each row;
negation and the false conditional arm are masked. The guarded calls make
all shifts safe at the admitted arities, including the zero-input singleton.
E-class merges compare the exact 64-bit vector, not a digest.
[Boolean:18–56][bool-input], [optimizer:14–77][truth].

Rust comparison enumerates ascending masks `0..2^n`, reconstructs every input
tuple, and compares source and candidate `Program.evaluate` observations. It
also compares direct-expression observations and frozen corpus signatures.
Python independently rebuilds the full expected ordered trace and interprets
every serialized DAG node, checking references, types, roots and domain shape.
A reported tuple count alone cannot pass: the actual trace must match, row for
row. Each successful run has 12,544 admitted candidate/input comparisons across
400 variants and 8,176 separate invalid-input probes.
[Evaluation:14–99][evaluation], [analyzer:37–92,146–160][analyzer-check].

**Ordered multi-output behavior.** Rust lowering uses one interning builder
across the ordered output roots; repeated roots remain separate output
positions. Extraction reconstructs each returned root in order. Rust compares
entire output vectors, while Python compares ordered signatures and roots.
The corpus deliberately includes repeated roots and shared guards. Swapping
unequal outputs changes the checked observation. No set comparison discards
order or multiplicity. [Boolean:88–148,196–218][bool-lowering],
[corpus:289–355][corpus-cases].

**Actual lowered artifacts.** The Rust check executes the constructed `Program`,
not just an e-class or extracted expression. The independent Python interpreter
checks its serialized instruction graph as well as the extracted expressions.
This closes a useful experimental AST-to-DAG check for every stored Boolean
case. It does not establish a compiled Rust/Wasm backend, canonical wire
decoder, artifact activation or authority route. The serialized JSON is an
experiment format, not the production `Program.value()` canonical artifact.
[Harness:118–151][harness-bind], [Boolean:170–194][bool-lowering],
[analyzer:146–163][analyzer-check].

**Eager errors and admission.** The original evaluator executes every stored
instruction in order and uses checked addition/subtraction before projecting
roots. Unused and unselected arithmetic can therefore fail. The optimizer's
Boolean admission scans *every* node, excluding arithmetic and comparisons
even when unreachable from outputs. Controls cover changed truth, non-Boolean
schema, dead addition overflow, unselected-arm overflow, wrong input arity and
invalid Boolean values. They are meaningful negative examples; they are not a
positive checked-i64 optimization result. Invalid-input probes are a finite
sample; all-invalid-input equivalence additionally relies on equal schemas and
the common admission semantics. [Original evaluator:218–272][fcis-eval],
[Boolean:150–167][bool-lowering], [controls:117–184][controls].

**Corpus and shared-DAG cost.** The frozen seed is `20261003`; all 100 cases and
six family counts are retained. The local baseline selects strictly decreasing
tuples of actual shared lowered node count, expression length and ordered
spelling, with a 1,024-step failure bound. It does not consult signatures when
proposing a rewrite. All variants share structurally identical nodes across
all roots. Python recomputes actual lowered count and checks equality with both
`fcis_nodes` and serialized instruction count. [Corpus:97–123,245–287][corpus-cost],
[analyzer:158–163][analyzer-check].

The 33 ordinary egg rules and search controls are shared; only semantic egg
adds signature-union sweeps. Both variants still compute signatures as a
rewrite-consistency check. The OR weight of four is a surrogate because
generated NOTs may share after lowering. CBC's reported optimality concerns
the constructed finite, class-consistent surrogate model, subject to solver
behavior/tolerances. It is not global minimality of the emitted graph. The
patched extractor's internal SCC/rank implementation is outside this source
review because the fork was not fetched. [Optimizer:80–145,214–312][optimizer].

## Outcomes, failures and repeat

| Configuration | Retained outcome |
| --- | --- |
| 6 iterations / 5,000 threshold / 2-second solve | Original registered run failed: 12 extractions (2 rewrite-only, 10 semantic); nine decoding failures and three behaviorally valid but non-optimal incumbents. |
| 6 / 5,000 / 30 seconds | Resource amendment failed: one semantic extraction; its valid 22-node incumbent remains a failed result. |
| 3 / 1,000 / 30 seconds | Exploratory search amendment and repeat pass all stored variant/control/tool gates. |

The initial failure **must remain a failure** in all integration notes and
future comparisons. No missing candidate is assigned a favorable cost.
The strict analyzer refuses failed runs. The separate failure audit preserves
accepted cases and decodable rejected incumbents without claiming a successful
aggregate experiment. [Resource amendment][resource], [search amendment][search],
[failure audit:57–106][failure-audit].

The successful raw totals are 1,506 original, 895 local, 854 rewrite-only and
816 semantic instructions. Semantic versus local is 18 wins, 78 ties and four
regressions across three winning families; semantic versus rewrite-only is
12 wins and 88 ties. The distinct protected-selection simulation totals 812
by keeping the local baseline in 82 cases. These values reproduce from stored
artifacts. [Published analysis:9–25][analysis].

Source-freeze hashes match the current optimization/generation sources; only
`analyze.py` differs among its twelve entries. The README explicitly explains
later analyzer revisions, and the current manifest binds them. The repeat
establishes observed stability of these outputs on these runs. It proves
neither universal determinism nor speedup; optimizer timing order reverses
between runs, and no application runtime benchmark was performed.
[README:91–96,194–206][readme], [repeat analysis][repeat].

## Lean and dependency boundary

The Lean development proves valuation/function cardinalities, complete fixed
signature equality iff pointwise equality, congruence for total Boolean
contexts, factoring, and guarded candidate selection. Its observation can
include an exact error or all ordered outputs. Its enumeration is a
noncomputable mathematical list, distinct from the Rust mask loop; completeness
and exact observation are premises of the generic acceptance theorem.
[Lean:18–78,99–176][lean], [proof boundary:50–89][proof-boundary].

The stored seven theorem closures report only `propext`, `Classical.choice`
and `Quot.sound`. The replay wrapper checks tool/Mathlib pins and rejects named
trust escapes. Source/log digests match, but this review did not rerun Lean or
inspect the actual historical dependency binaries. No theorem proves the
Rust checker, CBC, extraction optimality, application refinement or Tau
semantics. [Replay script:13–66][proof-replay].

The experiment pins Rust 1.97.1, egg fork commit
`12997cd15a123156abd726155363701f13358040`, and native CBC 2.10.13. Cargo.lock
records `good_lp 1.15.3`, `coin_cbc 0.1.9` and `coin_cbc_sys 0.1.3`; it does
not capture the native library build or vendored path bytes. ZenoFCIS declares
`MIT OR Apache-2.0` and includes those notices. This review did not verify the
fork/solver/transitive licenses or distribution compatibility. A separate
proposer executable can avoid a solver dependency in the core, but does not
remove distribution obligations. [Setup:14–50][readme], [lock][lock],
[workspace declaration][workspace-license].

[study]: https://github.com/TheDarkLightX/ZenoFCIS/tree/a69ed8db594d95279a46bff0f65185ef67d51f98/experiments/semantic-egraphs
[paper]: https://github.com/TheDarkLightX/ZenoFCIS/blob/a69ed8db594d95279a46bff0f65185ef67d51f98/experiments/semantic-egraphs/paper.pdf
[readme]: https://github.com/TheDarkLightX/ZenoFCIS/blob/a69ed8db594d95279a46bff0f65185ef67d51f98/experiments/semantic-egraphs/README.md
[bool-input]: https://github.com/TheDarkLightX/ZenoFCIS/blob/a69ed8db594d95279a46bff0f65185ef67d51f98/experiments/semantic-egraphs/src/boolean.rs#L18-L56
[bool-lowering]: https://github.com/TheDarkLightX/ZenoFCIS/blob/a69ed8db594d95279a46bff0f65185ef67d51f98/experiments/semantic-egraphs/src/boolean.rs#L88-L218
[truth]: https://github.com/TheDarkLightX/ZenoFCIS/blob/a69ed8db594d95279a46bff0f65185ef67d51f98/experiments/semantic-egraphs/src/optimizer.rs#L14-L77
[optimizer]: https://github.com/TheDarkLightX/ZenoFCIS/blob/a69ed8db594d95279a46bff0f65185ef67d51f98/experiments/semantic-egraphs/src/optimizer.rs#L80-L312
[optimizer-limits]: https://github.com/TheDarkLightX/ZenoFCIS/blob/a69ed8db594d95279a46bff0f65185ef67d51f98/experiments/semantic-egraphs/src/optimizer.rs#L235-L312
[evaluation]: https://github.com/TheDarkLightX/ZenoFCIS/blob/a69ed8db594d95279a46bff0f65185ef67d51f98/experiments/semantic-egraphs/src/evaluation.rs#L7-L99
[controls]: https://github.com/TheDarkLightX/ZenoFCIS/blob/a69ed8db594d95279a46bff0f65185ef67d51f98/experiments/semantic-egraphs/src/evaluation.rs#L117-L184
[harness-bind]: https://github.com/TheDarkLightX/ZenoFCIS/blob/a69ed8db594d95279a46bff0f65185ef67d51f98/experiments/semantic-egraphs/src/main.rs
[analyzer-check]: https://github.com/TheDarkLightX/ZenoFCIS/blob/a69ed8db594d95279a46bff0f65185ef67d51f98/experiments/semantic-egraphs/analyze.py#L37-L164
[corpus-cases]: https://github.com/TheDarkLightX/ZenoFCIS/blob/a69ed8db594d95279a46bff0f65185ef67d51f98/experiments/semantic-egraphs/corpus.py#L289-L355
[corpus-cost]: https://github.com/TheDarkLightX/ZenoFCIS/blob/a69ed8db594d95279a46bff0f65185ef67d51f98/experiments/semantic-egraphs/corpus.py
[fcis-eval]: https://github.com/TheDarkLightX/ZenoFCIS/blob/a69ed8db594d95279a46bff0f65185ef67d51f98/crates/zeno-fcis-synthesis/src/finite/ir.rs#L218-L272
[resource]: https://github.com/TheDarkLightX/ZenoFCIS/blob/a69ed8db594d95279a46bff0f65185ef67d51f98/experiments/semantic-egraphs/RESOURCE_AMENDMENT.md
[search]: https://github.com/TheDarkLightX/ZenoFCIS/blob/a69ed8db594d95279a46bff0f65185ef67d51f98/experiments/semantic-egraphs/SEARCH_AMENDMENT.md
[failure-audit]: https://github.com/TheDarkLightX/ZenoFCIS/blob/a69ed8db594d95279a46bff0f65185ef67d51f98/experiments/semantic-egraphs/audit_failed_run.py#L57-L106
[analysis]: https://github.com/TheDarkLightX/ZenoFCIS/blob/a69ed8db594d95279a46bff0f65185ef67d51f98/experiments/semantic-egraphs/results/independent-bounded-analysis.md#L9-L25
[repeat]: https://github.com/TheDarkLightX/ZenoFCIS/blob/a69ed8db594d95279a46bff0f65185ef67d51f98/experiments/semantic-egraphs/results/independent-bounded-repeat-analysis.md
[lean]: https://github.com/TheDarkLightX/ZenoFCIS/blob/a69ed8db594d95279a46bff0f65185ef67d51f98/experiments/semantic-egraphs/proof/BooleanSignatures.lean
[lean-gate]: https://github.com/TheDarkLightX/ZenoFCIS/blob/a69ed8db594d95279a46bff0f65185ef67d51f98/experiments/semantic-egraphs/proof/BooleanSignatures.lean#L146-L176
[proof-boundary]: https://github.com/TheDarkLightX/ZenoFCIS/blob/a69ed8db594d95279a46bff0f65185ef67d51f98/experiments/semantic-egraphs/proof/README.md#L50-L89
[proof-replay]: https://github.com/TheDarkLightX/ZenoFCIS/blob/a69ed8db594d95279a46bff0f65185ef67d51f98/experiments/semantic-egraphs/proof/replay.sh#L13-L66
[cargo]: https://github.com/TheDarkLightX/ZenoFCIS/blob/a69ed8db594d95279a46bff0f65185ef67d51f98/experiments/semantic-egraphs/Cargo.toml#L10-L17
[lock]: https://github.com/TheDarkLightX/ZenoFCIS/blob/a69ed8db594d95279a46bff0f65185ef67d51f98/experiments/semantic-egraphs/Cargo.lock
[workspace-license]: https://github.com/TheDarkLightX/ZenoFCIS/blob/a69ed8db594d95279a46bff0f65185ef67d51f98/Cargo.toml#L43-L50
