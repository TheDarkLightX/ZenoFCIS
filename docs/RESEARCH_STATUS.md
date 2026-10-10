# Research and integration status

Status: 10 October 2026. The published [V2.2 and V2.3 release candidates](RELEASE_CANDIDATES.md)
are available now. Several newer experiments have also produced useful results,
but their evidence and integration status differ from those of the candidates.

The private results below are reported in the owner's
[Library research record](https://chatgpt.com/space/page_af6f39ee8f94819184996404b53d47b1).
That record was read for this documentation update; its source archives and proof
packets were **not independently downloaded or replayed in this checkout**. The
link may require access to the owner's Library. Archive identities below support
handoff and later verification; a matching hash alone does not qualify a proof.
These experiments are not included in the published candidates by virtue of this
documentation update.

## What has been achieved

| Work | Reported result | Current boundary |
|---|---|---|
| Native finite composition checker | Checks genuine FCIS Authorities against leaf contracts and checks that connected guarantees establish consumer assumptions | Private, restricted acyclic finite checker; no composite runtime or Publication |
| Signed receipt verifier | Authenticates a canonical statement under an independently supplied trusted key and exact Ed25519 acceptance profile | Private, stateless authentication; no freshness, durable acknowledgement or authority token |
| Crucible resource models | Source-bound Kani campaigns for exact resource counts four, five and six, with separate obligations and semantic controls | Bounded typed models; no general resource-count theorem or FCIS integration |
| Native law diagnostics | Uses the actual law evaluator to show outcome sensitivity over an explicit finite frame table | Private advisory CLI prototype; no whole-contract adequacy proof |
| ZOS resource shadow oracle | Calls the qualified count-four core and compares hypothetical routes under four consumable quotas | Offline advisory experiment; no real resource enforcement or dispatch |
| Completion-buffer optimization | Measured improvements on one workload, positive core proofs, scoped Miri checks and a reviewed fixture migration | Mutation and broader integration qualification remain incomplete |

## Native finite composition checker

The private experiment uses V2.3 RC source
`dd04cbcb3f7d8694de5f79bc1280b9252fdd147b`, leaving its original 2,195 files
unchanged. It separates two obligations: each leaf's actual implementation must
satisfy its own contract, and the surrounding guarantees must establish the next
leaf's input domain and assumptions. It retains joint correlations and complete
canonical native subjects. Failed assumptions, missing coverage, cycles, state
aliases, Bool/Int coercions and exhausted checking budgets cannot yield success.

Its supported graphs are finite, eager, unconditional and acyclic. Every leaf
owns a distinct scalar state slot initially zero; wires copy typed values. The
supported outcome is Accept without a reason, effects or outbox. Shared state,
sequential state reuse, concurrency, liveness and arbitrary Rust are outside this
profile. The finite induction argument was reviewed, not machine-proved. The
separate traversal oracle still shares the native Authority, root enumeration
and envelope helpers.

Reported checks include 31 negative controls, 13 independently authored semantic
cases, eight source-integrity mutations, eight compiled checker mutants and four
API capability controls. A synthetic width-16 graph with two useful roots used
304 transition evaluations instead of 1,024, with four genesis evaluations on
both paths. Median cold process time including serialization was 83.82 ms versus
247.09 ms. Relation checks increased from 1,024 to 20,528; interface products can
still grow exponentially. These measurements apply to that fixture, not every
composition.

This establishes a useful experiment toward [V2.3 composition](V2_3_PLAN.md).
It does not complete the six planned coupled applications or create a composite
Authority or Publication. Source checking is performed at build time; full
upstream release qualification was not run for this additive experiment.

## Signed receipts and durable effects

The private RC-bound verifier authenticates a 355-byte canonical payload through
a 422-byte versioned signing frame. Its statement binds deployment, Authority
identity commitment, genesis, delivery, entry, receiver, key, key epoch, request
sequence and challenge. The received key label cannot select the trusted key.
The pinned `ed25519-dalek=2.2.0` strict acceptance profile is part of its identity.

Reported evidence includes 19 regressions in debug and release, 1,067 provider
cases, 3,352 refused single-bit corruptions, 17 compiled mutation controls and
ten API controls. Sixteen mutants demonstrated unintended acceptance; the other
demonstrated changed refusal precedence. Five complete fixtures also passed
OpenSSL verification.

Replaying a valid receipt succeeds again: the verifier is stateless. A signature
does not establish statement truth, external execution, freshness, durable commit
or correct key custody. It issues no Publication, Delivered or Checkpoint token.
Integration needs independently retained expectations, atomic replay handling
and key lifecycle rules. Cargo feature unification and provider/toolchain trust
remain explicit dependencies; these tests are not a cryptographic security proof.

## Crucible resource models and the shadow oracle

The private count-four/count-five campaign reports 47 qualified terminal runs:
18 positives, 27 intended semantic mutation failures and two passing abstracted
callers under a bad-counter mutant. A passing caller does not replace the required
failing real-callee control. Count six has its own binding and 28 runs: 12
positives, 15 semantic controls and one passing abstracted caller.

Each model fixes the active resource count. Valid capacities are 1 through 8,
positive demand is bounded by capacity, and typed Guard/Debit programs have
lengths up to 12. Admission and execution obligations include raw malformed
inputs, full certificate preservation, complete outcomes, refusal priority,
conservation and unchanged balances with zero charge on refusal. The
`36^4`, `36^5` and `36^6` valid-specification counts describe symbolic domains,
not enumerated native test counts.

Kani 0.68.0, CBMC 6.11.0, the Rust frontend, solver and experimental contract
translation remain trusted. Generator coverage and admission/execution
composition have reviewed arguments, not a machine-checked source-translation
theorem. The count-six packet has narrower tool-payload provenance than the
later four/five campaign. Separate exact-count results cannot be combined into
an unbounded or single-binding counts-two-through-six theorem.

The ZOS shadow oracle calls the unchanged count-four core to evaluate each route
against the same original immutable balances. Its four quotas are abstract
positive consumables; they are not asserted measurements of CPU, money or RAM.
Reported checking covers 38,880 native resource results, 87,480 route reports and
3,240 comparisons with the pinned scalar coordinator, plus seven mutation
controls. Those 129,600 rows cover the stated finite snapshots, not histories or
concurrent execution. The wrapper has no inherited or newly qualified Kani proof.
The existing proposal consumer does not enforce its multi-resource evidence.

## Law diagnostics and completion optimization

The private `contract diagnose-law` command evaluates one focal law first with a
fresh private meter over supplied frames. For `on reject = context.102 == 0`, it
can show a frame where Accept skips the law and Reject is refused. The existing
body-only diagnostic and strict authoring rejection remain unchanged. Reported
checks include 400 CLI tests, 40 independent native adversarial cases and seven
replay-verifier mutants; 11 upstream ignored tests were not counted as passes.
These witnesses do not establish original law ordering, whole-law-set success,
reachable executions, adequacy, progress or mandatory acceptance.

The completion-buffer experiment measured search at 10.73 to 5.48 ms and
verification at 11.43 to 6.55 ms on a 4,096-state, 16-command, 10-node workload.
It reduced allocations but increased peak live memory by 144 bytes. Smaller
workloads sometimes regressed. Both pristine RC and a separate integration
candidate passed fresh positive Authority proofs with 835 verified and zero
errors. The pristine RC also had stale coverage metadata; old receipts are not
transferred to new source identities.

Qualification remains incomplete. The Authority mutation campaign stopped at
its sixth control: a deliberately broken reason variant produced ordinary
failure diagnostics, but its underlying solver queries returned UNKNOWN rather
than a SAT counterexample. No SAT-backed control credit is granted. This is not
a failed positive core proof. Native tests independently caught the reason loss.
Fourteen other affected gates remain unqualified pending attribution repairs.
The fixture-generation migration and all 31 scoped strict-provenance Miri tests
passed their stated checks, but do not close broader adoption, journey, catalogue
or release obligations.

## Public Crucible increment

[Zeno-Crucible PR #19](https://github.com/TheDarkLightX/Zeno-Crucible/pull/19), at
`d625d695e80b79f0275f1b935f55248325c33b82`, is a separate public implementation
increment. At this status date it is open, with eight hosted checks passing.
It includes a model-proposed counter improvement checked over all 36 inputs,
generated-app tests and an actual store-upgrade journey. The counter shrank
from 24 nodes to 17; the existing optimizer baseline had 22 nodes. A separate
MDD-plus-optimizer result shrank a 49-node program to 48 over 432 inputs; MDD
search alone did not improve that panel.

These results demonstrate checked improvements in explicit bounded experiments.
They do not establish general optimality, finish a general data-structure
synthesizer, or integrate Crucible into the normal FCIS publication path. The
private resource campaigns above are separate evidence, not automatically part
of that pull request.

## Evidence handoff

The following full archives are recorded in the private Library. They are not
public GitHub release assets. Before integrating an experiment, retrieve its
actual packet, check its inventory, review the implementation and obligations,
then run the applicable checks on the exact proposed integration tree.

| Result | Full archive | Version | SHA-256 |
|---|---|---|---|
| Composition | `zenofcis-native-composition-dd04cbc-full-20261010.zip` | 0 | `0b61280168ddb5b8e5de4854b920ae181a35993b08879fa96bd1369b84bcbf4a` |
| Signed receipts | `FCIS_signed_receipt_full_2026-10-10.zip` | 0 | `8e3278f215f7f2582babf75035727a2ccfcf0caeb72e055004af898c7c10189a` |
| Resources four/five | `crucible-count45-separated-checkpoint-20261009.tar.gz` | 6 | `15a3419e6af75d3871dc15115e2024f126ba530d849407a7f9021e9a79fe691d` |
| Resources six | `crucible-count6-separated-checkpoint-20261009.tar.gz` | 2 | `6101e116e204c598ae3fc784dfd2d70d91eba964eeee8823caba8e9035a70ad0` |
| Law diagnostics | `fcis-law-diagnostics-native-prototype-20261009.tar.gz` | 0 | `ea393428286d825cb998bf480d32427d147d98a8a23085ed7b438d6f67a61060` |
| Shadow oracle | `zos-four-resource-shadow-full-20261010.tar.gz` | 0 | `17425aa028f7c2122bba33ded39d36cb9eeff62a93754961fdc5ee47ad68e85f` |
| Completion scoped evidence | `FCIS_completion_scoped_qualification_full_v2_2026-10-10.zip` | 0 | `804a2439cced3696b2c8b34b782b0ceca024e4a9bc9d16c6b988aa3eadb293de` |

The [V2.3 plan](V2_3_PLAN.md) remains the release completion checklist. These
results inform its next implementation steps: checked component interfaces,
state and interference obligations, proof reuse with exact dependencies, and
durable receipt handling. They supply concrete starting points and retained
evidence; they do not silently remove unfinished release requirements.
