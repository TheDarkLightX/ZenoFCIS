# Independent Lens catalog performance qualification

Fresh execution on October 31, 2026 reproduced a local performance improvement
for the batch catalog producer in draft PR #170. The measured baseline is
`8de5a6543f93353d2f4563b17badfde285ce46a2`; the measured candidate is
`48358c98fabbd8bc03edc972c077aef945cd2f16`. Production source was not edited.
This directory adds scripts, negative tests and retained evidence.

Independent means a separate execution environment and fresh records. The full
workloads reuse the upstream standard-library worker and scoped semantic
projection. These results are not an independent formal proof of every Lens.

## Measurements and scope

The new environment used Python 3.12.14 built with Clang 22.1.3 on Linux x86-64,
an AMD EPYC 9V74 host, and a container whose PID 1 is `supervisord`. Each
measurement worker used one CPU, 1 GiB address space, 90 CPU seconds and a
120-second wall deadline. Source roots were separate clean Git worktrees.
All 239 frozen files per backend, workloads, original helper bytes and the
reviewed interpreter binary were checked before and after execution.

| Complete export and fresh replay | Original baseline / candidate | Original ratio | Independent baseline / candidate | Independent ratio | Independent time reduction |
| --- | ---: | ---: | ---: | ---: | ---: |
| Five-object library | 38.323215 / 15.362162 s | 2.49× | 21.030241 / 8.427503 s | 2.495430× | 59.926742% |
| Frozen latch/draft library | 15.054669 / 4.531029 s | 3.32× | 7.977810 / 2.711319 s | 2.942409× | 66.014238% |

All table values are medians of three executions per backend. The six-worker
order was baseline, candidate, candidate, baseline, baseline, candidate.
The latch/draft workload was already public and frozen; it is not a newly
unseen holdout for this investigation. The original numbers were recomputed
from the existing raw records, separately from the new measurements.

Warm catalog construction had independent pooled medians of **41.339257 ms**
and **4.423639 ms**, a **9.345079×** ratio. The original pooled medians were
92.452878 and 6.913816 ms. Each backend has 75 observations within three worker
clusters; the 75 observations are not 75 independent process executions.
No confidence interval or population-wide speed claim is inferred.

The six independent workers observed peak RSS between 42,404 and 43,028 KiB.
This does not establish a memory reduction. No worker required a cleanup signal
or left an observed survivor. Wall timings for process startup and auxiliary
checks are recorded separately from export/replay timings.

Raw samples and descriptive aggregates are in
[records-20261031/performance-independent.json](records-20261031/performance-independent.json).
The original results remain in [the original experiment](../README.md).

## Catalog-size investigation

[catalog_sizes.py](catalog_sizes.py) calls each pinned `default_registry`
function and its actual validators. Scoped test suppliers vary the number of
structural and finite definitions; the producer body is never rewritten.
The 15-definition point uses the unchanged real suppliers. Other points use
synthetic suppliers, so these are not measurements of a public configurable
catalog API. There is always one unconditional category definition: an empty
catalog is outside this fixture and the current default producer.

Each point has 30 observations per backend with AB and BA orders alternating,
one CPU and no profiler during timings. Output catalogs matched at every point.

| Definitions | Baseline median ms | Candidate median ms | Ratio |
| ---: | ---: | ---: | ---: |
| 1 | 0.3639 | 0.2696 | 1.35× |
| 2 | 0.9189 | 0.4476 | 2.05× |
| 3 | 1.7096 | 0.6313 | 2.71× |
| 4 | 2.7085 | 0.8313 | 3.26× |
| 8 | 10.7449 | 1.8965 | 5.67× |
| 15, real catalog | 40.0177 | 4.1289 | 9.69× |
| 16 | 42.4747 | 4.2478 | 10.00× |
| 32 | 170.8640 | 8.5121 | 20.07× |
| 64 | 700.5144 | 17.3755 | 40.32× |

No median regression or crossover was observed at these nine tested points.
This does not cover every possible definition payload, every size, other
interpreters or hardware, cold-start behavior, or the full factory.
The admitted registry is bounded at 64 definitions; no unbounded whole-program
asymptotic advantage is claimed. The 65-definition refusal was checked separately.
All samples, identities and profiled call counts are retained in
[sizes.json](records-20261031/sizes.json).

## Semantics, identities and ownership

Only `default_registry` differs among 17 production functions in
`lens_library.py`; the other 16 and all other top-level AST nodes are unchanged.
The independent real-catalog counts are 255 → 30 definition validations,
30 → 1 complete registry validations, and 15 → 0 public registration calls.
The catalog remains 9,523 encoded bytes, including its trailing newline, with
SHA-256 `718d412ef7338523a439141507ff7db0408284d491b817620d816183d3a2689a`.

The differential checks retained exact catalog and fallback bytes, eight public
refusal cases and exact adapter admission results. All compared packet semantic
projections, request identities, statuses and mathematical evidence matched.
Each packet independently replayed with its own source before comparison.
Only the documented producer/application/receipt identity fields were projected
out; the producer pin sets differed only at `zal_notation/lens_library.py`.

The five-object exports retained 69 files and 2,660,741 bytes. Latch/draft exports
retained 32 files and 1,489,238 bytes. These totals matched across both backends
and every repetition. Exact artifact bytes are source-specific, so byte equality
across different producers is not claimed.

Additional controls verified eight checked receipts per source against their
own implementation and rejected all eight from the other implementation in
each direction. Own-source packets replayed; both cross-source packet replays
refused. The existing browser tests also rejected source, catalog, target,
receipt, status-scope, JavaScript and HTML mutations even after manifest resealing.
Imported browser code was not executed.

Twelve new catalog tests check registry/proposal ownership, independence of all
mutable list fields across rows and invocations, unchanged adapter constants,
duplicate and malformed fixed suppliers, exact admission matching, refusal
precedence, no input mutation on refusal, optional import fallback, unexpected
import failures, and the 64/65 boundary. They passed against both pinned roots
in normal and optimized Python. The 46 existing affected tests passed in each
mode, with modules run as separately bounded groups. Five measurement-pin tests
passed in each mode: exact bindings, wrong descriptor, changed worker recipe,
changed workload and changed interpreter digest. Three namespace ownership
controls passed in each mode, including a deadline, observed-descendant cleanup,
an unrelated live sentinel, and failure of identity translation.

The evidence summary is [checks-summary.json](records-20261031/checks-summary.json).
Its `pass` field means the expected diagnostic outcomes reproduced, including
expected nonzero evaluator diagnostics. Its explicit `qualification_failures`
remain failures. Per-command `RESULT.json` records retain actual exits and limits.

## Environment blocks and unresolved qualification

The original supervisor's check-only operation passed. Its unchanged ordinary
launch refused with **Actual host required**, because PID 1 is not `systemd`.
That protocol remains unreproduced in this environment. No host check was removed
or spoofed. The supplemental supervisor is [container_measurements.py](container_measurements.py).

The first supplemental attempt also refused: the original ownership helper
indexes procfs using subprocess IDs, but this environment mounts procfs from an
outer PID namespace. A follow-up preflight found `task/*/children` unavailable.
The separate [namespace_process.py](namespace_process.py) helper binds pidfd
targets through fdinfo, start ticks and parent links, and signals only retained
pidfds. It tracks observed descendants, not arbitrary adversarial process trees.
The original worker and all published helpers remain byte-identical. These
supplemental results are not a successful run of the original host supervisor.
Failed setup evidence is retained in [attempt-01](records-20261031/attempt-01/)
and [the preflight record](records-20261031/ownership-preflight-failures.json).

The initial-name restriction also remains: native finite contracts whose initial
state is `latched` are refused with **unsupported machine layout or initial state**
at both revisions. Initial `unseen` is the positive control. This restriction
does not constrain general ZAL authoring. An initial supplemental fixture failed
earlier at enum canonicalization; its source/log are retained, and a corrected,
sorted fixture reproduced the actual adapter restriction without changing either
source tree.

The evaluator source-generation check independently failed at **both** revisions:

```text
python3 -B tools/check_authority_v2.py --check-sources
ValueError: compiled evaluator digest differs from approved source generation
```

That failure predates this optimization relative to the measured baseline and
has not been repaired by this qualification. Hosted records associated with the
inspected head contain **10 successful, 32 failed and 4 cancelled workflow runs**.
The three ZAL jobs in the main CI run passed, including Lens replay; its Rust
job failed at evaluator identity, and later Rust checks were skipped. Its actual
checkout was merge commit `8ad91f76188aed7dc84af4172ff9565ad67b7cf4`, whose tree
matches the inspected head exactly. Other hosted failures are retained by name
and URL; their individual root causes were not diagnosed here. Newly added CI
coverage is not represented as a hosted pass by these local records.

These results support the measured local improvement. Mathematical novelty,
universal speedup, production qualification, full CI acceptance, whole-factory
acceleration and complete allocation/cold-start models remain unestablished.

## Reproduction

Prepare clean separate Git worktrees at the baseline and candidate commits above.
Use a reviewed local interpreter. Choose a fresh evidence directory outside both
source roots. The preparation helper copies exact upstream recipes/workloads and
the supplemental scripts, then prints the regenerated descriptor hash:

```sh
python3 -I -B prepare_reproduction.py \
  --baseline-root /absolute/baseline --candidate-root /absolute/candidate \
  --python /absolute/reviewed/python --output /absolute/fresh-evidence
cd /absolute/fresh-evidence
python3 -I -B published-recipe/run_measurements.py \
  --pins-sha256 PRINTED_DESCRIPTOR_HASH --check-only
```

On a supported systemd host, run the original supervisor without `--check-only`.
On a Linux container with pidfds and readable procfs parent/namespace metadata,
the distinct supplemental route is:

```sh
python3 -I -B container_measurements.py --recipe published-recipe \
  --pins-sha256 PRINTED_DESCRIPTOR_HASH
python3 -I -B run_checked_command.py --output checks/sizes \
  --cpu-seconds 180 --wall-seconds 240 -- \
  /absolute/reviewed/python -I -B catalog_sizes.py \
  --baseline-root /absolute/baseline --candidate-root /absolute/candidate \
  --output sizes.json
python3 -I -B run_independent_checks.py \
  --baseline-root /absolute/baseline --candidate-root /absolute/candidate \
  --recipe published-recipe
python3 -I -B test_measurement_pins.py --recipe published-recipe
python3 -O -I -B test_measurement_pins.py --recipe published-recipe
python3 -I -B check_initial_state.py --root /absolute/candidate --output initial-state-candidate.json
```

Run the initial-state control against the baseline too, with a different output
filename. The measurement and checked-command supervisors retain partial results
and refuse existing result paths. Helper tests intentionally signal only their
own spawned control processes. The recorded local descriptor paths are provenance;
regenerate them for a new host without changing expected source/workload/helper
hashes. Exact producer/artifact identities, logs, raw observations, source audit
and failed attempts are retained under [records-20261031](records-20261031/).
