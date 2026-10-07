# Acceptance and future evaluation protocol

Status: executable fixture checks available; production qualification and research runs unrun.
This document is a proposed preregistration template, not experiment authorization.
Smaller V2 and the existing 2.1 prerequisite/delivery order remain unchanged.

## A. Reproduce the development catalog

From this directory:

```sh
python3 check_cases.py
```

The script validates typed topological nodes, profile limits and exact integer strings,
exhausts every declared product, compares eager checked-i64 observations and checks
stored first witnesses. It writes `fixture-check.json` and exits nonzero on a mismatch.
To recreate authored data, run `python3 generate_cases.py` followed by
`python3 check_cases.py --write-witnesses`, then the ordinary checking command.
Generation overwrites these development fixtures and initially clears their witnesses;
it does not generate a held-out set. Freeze/review digests after any regeneration.
A separately written companion oracle confirmed the same catalog before integration.
Reproduce it with `python3 independent_check.py cases.json independent-fixture-check.json`.
The small Python utility neither implements the complete production decoder/schema
nor qualifies Rust correspondence, production receipts, memory bounds or metering.

## B. Production supplied-pair acceptance contract (future implementation)

```text
freeze original bytes, ABI/domains, profile, checker identity, objective, budgets
bounded-admit original; invalid original => Refused, with measured refusal work
bounded-admit actual supplied candidate; invalid candidate => Refused
require candidate ABI/domains exactly equal original ABI/domains
preflight complete product and reserve sufficient deterministic checking work
for ordinal in the qualified full-product order:
    charge before work; unavailable work => Inconclusive
    evaluate every stored original and candidate node eagerly in stored order
    compare complete typed success tuple or exact admitted scalar error
    Boolean unexpected evaluation error => Inconclusive
    first mismatch => Different(bound ordinal, tuple, both observations)
verify terminal enumeration state and expected/visited rows
charge receipt work; exhaustion => Inconclusive
construct private Equivalent only here, bound to both actual artifacts and request
select only if both original cost guards hold and incumbent lexicographic cost decreases
```

This is the required acceptance algorithm, not a newly installed command.
A checker error, Budget refusal, timeout or panic never becomes a scalar Arithmetic error.
Checked-i64 scalar errors are semantic observations; equal errors can establish equality
only after complete admission/coverage under that separately qualified profile.
Reference and InputDomain on admitted valid inputs indicate a correspondence problem.
The qualified enumeration must be bijective: final row counts alone are insufficient.
Witness feedback requires fresh replay from bound immutable bytes.
Receipt replay re-admits and rechecks all rows and deterministic receipt fields.
Hashes bind custody/identity; they never replace full observation comparison.
Any source, policy, ABI or semantics change invalidates dependent evidence.

## C. Qualification scenarios outside optimization denominators

| ID | Controlled scenario | Required outcome/evidence |
| --- | --- | --- |
| Q01 | Candidate shrinks MAX endpoint away from I08 | Refused schema/domain change before comparison |
| Q02 | Empty interval versus zero input fields | Empty interval refused; B10 executes one empty tuple |
| Q03 | Enumerator omits B09 all-true row | Coverage mutant detected; no Equivalent |
| Q04 | Enumerator repeats a row and omits another at unchanged count | Bijection/native-oracle test fails; count-only guard inadequate |
| Q05 | Reordered or renamed input/output ABI | Refused exact descriptor mismatch |
| Q06 | Same ABI but output roots reordered | B14 Different; full ordered tuple retained |
| Q07 | Duplicate output root silently deduplicated | Wrong root count/schema refused or full-tuple mismatch |
| Q08 | Unsupported dead opcode, forward reference, bad type or root | Admission refusal, including unreachable nodes |
| Q09 | Boolean Eq/Int/Add hidden in unused node | Boolean profile refusal |
| Q10 | Wrong encoding/tag/version, trailing data, noncanonical bytes | Decoder refusal; exact canonical re-encoding |
| Q11 | 256/257 nodes, 16/17 outputs, 6/7 Bool inputs, 64 KiB/+1 bytes | Exact valid bound admitted if other gates pass; excess refused |
| Q12 | Full i64 interval, product overflow, 65,536/65,537 tuples | Correct cardinality; oversized refused; permitted size still subject to work limits |
| Q13 | Candidate artifact changes after receipt; stale checker/dependency pin | Replay refusal; no trusted incumbent |
| Q14 | Forged receipt count, tuple trace, costs, error, usage or completion | Recompute deterministic fields and reject each mutation |
| Q15 | Supplied receipt bytes claim private result or authority | No constructor/capability path; fresh check required |
| Q16 | Force equal digest labels on unequal observations | Full observation comparison still detects Different |
| Q17 | Timeout/truncation/panic at last row or receipt construction | Inconclusive; no new accepted replacement |
| Q18 | Compare only successes or erase all scalar error tags | I08 detects skipped error rows; I14 detects erased tags; I07 checks legitimate equal-error acceptance |
| Q19 | Make Select lazy or prune unused arithmetic | I10/I11 reject changed eager semantics |
| Q20 | Use arbitrary precision or wrapping arithmetic | I08/I09/I12 detect missing checked-i64 overflow |
| Q21 | Wrong application Step usage or charge-after-work | I01 x+0 versus x differs in exact application observations |
| Q22 | Rewrite resource refusal to success without refinement contract | Reject application promotion; functional equivalence is insufficient |
| Q23 | Forged caller meter or wrong refused attempted charge | Meter ownership/accounting qualification fails; not a scalar result |
| Q24 | Eight invalid/duplicate/timed-out proposals | All consume attempt allowance; no ninth attempt |
| Q25 | Improvement followed by failure/regression/late reply | Return only genuine earlier checked incumbent with matching receipt |
| Q26 | Changed request/resume ledger rollback or insufficient replay allowance | ResumeRefused/ResumeInconclusive; no inherited trusted receipt |
| Q27 | Worker exceeds memory/deadline or leaves descendants | Qualified shell limits and cleanup; unsupported enforcement means unavailable |
| Q28 | Application reason/effect order/law/identity differs | Full application comparison rejects; no authority from functional receipt |

These are designed scenarios, not executed mutation results.
Attach each planted defect to its intended qualification gate before running it.
Freeze defect IDs, source locations, expected witness and kill criterion first.
A surviving mutant is not silently discarded; classify equivalent mutant only with evidence.
Report killed/survived/inconclusive counts per defect family and false acceptance separately.
Never put a fault-injected checker on an authority or publication path.
Meter tests use the current library-owned execution route; do not add a second meter.

## D. Cost and result accounting

Record actual node count of every stored instruction once and production canonical bytes.
Do not substitute JSON length, expression-tree cost, solver objective or receipt length.
For each checked equivalent Q require N(Q)<=N(P0), B(Q)<=B(P0), at least one strict,
and (N(Q),B(Q)) lexicographically below the incumbent to select it.
A tie keeps the incumbent; no graph churn is counted as improvement.
Record raw proposals, semantic/checker outcomes, every selection decision and returned fallback.
Report original cost, raw candidate cost, selected cost, and both component deltas.
Refused/inconclusive/different candidates retain their costs where admission measured them;
unknown costs remain unknown, never zero. Invalid originals cannot yield checked fallback.
An equivalent but larger candidate is EquivalentWithoutImprovement, not a checker failure.
For every original optimization task, a failed search contributes zero checked gain and its
failure class. Preserve no-improvement controls in the denominator.
Deliberately incorrect supplied candidates and Q01–Q28 have separate qualification denominators.
Do not turn rejection of an incorrect target into an optimizer success.

## E. Frozen resource ceilings for the first supported Boolean study

Use these existing loop ceilings, or stricter operator limits fixed before the run:

| Item | Ceiling |
| --- | --- |
| Artifact | 64 KiB; Boolean structural limits; 64 tuples |
| Attempts/checks | 8 attempts and at most 8 candidate checks per task/run |
| Checker work | 1,000,000 units/check; 8,000,000/session including replay |
| Checker worker | 2 seconds, 64 MiB |
| Search worker | 5 seconds, 512 MiB including descendants, one solver thread |
| Entire session | 20 seconds; cleanup latency separately recorded |
| Model calls | At most 4; 4,096 input and 2,048 output tokens each |
| Model aggregate | At most 24,576 reserved tokens/session |
| Retained data | 512 KiB transcript; 2 MiB request/artifact/receipt/witness storage |
| Hosted spending now | Zero; disabled |

Reserve before dispatch; failures, duplicates, refusals and uncertain remote completion consume
allowances. No hidden retries, enlarged timeout, discarded warmup failure or automatic reset.
A search strategy need not spend unused allowances. All arms receive the same total ceilings.
Include parsing, solver setup, extraction, cleanup and adapter overhead in their proper timings.
Peak RSS sampling is descriptive; enforce an owned cgroup or qualified equivalent.
If limits cannot be enforced, mark the arm unavailable, not as a successful zero-cost run.
Checked-i64's tuple ceiling alone does not promise feasibility under these work limits.
A later i64 study must preflight a separately qualified work policy and freeze its own limits;
it must not silently raise Boolean limits or present a resource refusal as semantic failure.

## F. Future data freeze and six-arm comparison

A bounded proposed first held-out Boolean study uses 12 independent provenance/family groups,
8 independently accepted tasks per group (96 total), and 5 recorded repetitions per arm.
This is a planning default, not an accomplished dataset, power claim or executed study.
If 12 independent groups cannot be obtained, preregister a smaller descriptive study before
exposing outcomes; do not relabel 96 renamed/constant-substituted formulas as independent tasks.
The seed pairs and old 100-case corpus are calibration only and excluded from this test set.
Freeze split groups before generation using the union rules in DESIGN section 10.
Freeze task bytes/contracts, seeds, provider/model versions, decoding settings, host/toolchain,
source/dependency closure, warmup policy, arm order, run count and stop rule before dispatch.
Independently accepted contracts must precede optimizer output; rejected requests stay in the
intake/exclusion ledger with reasons. No case is excluded because search later fails.

The arms remain: (1) local-only, (2) fixed rewrite e-graph, (3) semantic e-graph,
(4) model-only whole proposals with checker, (5) hybrid without feedback,
(6) hybrid with replay-verified feedback. Pin all baseline implementations.
Use identical original contracts, objective, checker and aggregate budgets.
Freeze seeded balanced execution order to reduce order/host effects.
Record all 5 repetitions, including deterministic-arm repetitions; they are not 5 new tasks.
Hosted mode requires separate authorization, disclosure policy and enforceable spend ceiling.
This design does not authorize its possible 2,880 task/arm/repetition sessions.
Stop exactly at the frozen run schedule or the authorized aggregate limit; explain missing runs.
A safety/correctness failure stops the affected arm; retain attempted and planned denominators.

Scheduler/CERES is a separate orthogonal ablation, not a seventh neural arm.
Within each scheduler comparison hold the rules, extractor, checker, artifacts and ceilings fixed.
Any rule/extractor or budget change makes a different exploratory study, not scheduler evidence.
Obtain the full algorithm, exact implementation and dependency/license pins before using CERES.
No CERES performance or memory gain has been measured here.

## G. Metrics, uncertainty and diagnostic usefulness

Primary search metrics: family-balanced mean checked node reduction and improvement frequency.
Report canonical-byte deltas alongside nodes, including guard failures and equal-node byte wins.
For task/run normalize node reduction by original nodes; failure/no improvement is zero gain.
Average repetitions within each task, tasks within each declared independence group, then groups.
Report micro totals and per-family distributions too; do not hide large hard families in one mean.
Secondary metrics: raw invalid/refused/inconclusive rates, calls/tokens/money, total time,
peak memory, checked time to first equivalence and to first improvement, and human review time.
Report time-to-event censoring at the fixed session cap, completion fractions and survival curves;
do not drop timeouts or assign them a falsely observed success time.
Unknown billing retains reservations and an unknown actual field; missing telemetry is explicit.

Use paired arm differences on the same tasks. Proposed uncertainty: 10,000 bootstrap samples
of the frozen independence groups, seed 20261004, retaining their tasks and all repetitions.
Report percentile 95% intervals for family-balanced differences, group count and sensitivity.
With few groups, intervals are descriptive/unstable; they are not broad population guarantees.
Provider drift, changing hosted state and run order remain reported reproducibility limitations.
Predeclare six-arm contrasts of interest; label unplanned pairwise comparisons exploratory.
No best-of-five selection, retry-level independence, post-hoc case deletion or success-only mean.

Diagnostic usefulness is evaluated separately by planted-defect detection, false acceptance,
coverage of named boundaries, discrimination among fixed strategies and independent maintainer
judgment of project-derived cases. Publish failures and null strategy differences.
No composite effectiveness score or Herbie equivalence claim follows from fixture validation.
Native/oracle comparisons, proof/body coverage, mutations, ATDD and exact-head CI remain required
for any enabled implementation slice. Missing/unrun gates remain explicit, never presumed passed.
