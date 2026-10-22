# Knowledge from checked work

The engine keeps two kinds of knowledge. Exact facts enter after their artifacts
replay. Strategy lessons enter advice only after a frozen held-out ON/OFF
assessment. Program acceptance stays with the exact checker. Queue priority,
corroboration, review notes and points cannot promote a strategy.

## A lesson's journey

A configured loop consults the explicit private layers, runs its checked work, and
writes one inbox proposal plus bounded outcome and episode records. An episode
records the task binding and scope, steps, advice use, counterexamples, stopping
bound, verdict, receipt hash and deterministic counters. Missing observations stay
missing. An incomplete episode can be retained for diagnosis but cannot serve as
evidence. Iteration records retain available candidate-space sizes or checked
factor counts, counterexample elimination, satisfied constraints, term size,
abstraction size, spuriousness and per-iteration work. Untracked values stay
`null`; a known zero elimination stays zero. Stage summaries do not invent inner
solver observations. These data support later value-model experiments without
changing the assessment gate.

For example, two contributors encounter the same stopping bound on distinct subject
revisions. Failure mining proposes a `limit` or `defect` record with the episodes as
evidence. Curation normalises declared scope and deduplicates the structured
statement. Distinct subject revisions, sessions, contributors, tool versions and
environments are retained as evidence-family metadata; repeating the same family
counts once. These labels do not authenticate independence. Corroboration changes
queue priority, never advice eligibility.

The candidate proposes a concrete optional-stage order or skip. Assess it on members
that were not used to propose it. Both modes consume identical input bytes, seeds
and budgets. Each pair runs again in separate contained processes; a mismatch
refuses the assessment. The record binds the frozen suite, task and output digests,
checker and engine identities, policy and both observations. No elapsed-time
measurement enters usefulness.

A helpful candidate is promoted only when no applicable task gets worse and at least
one gets strictly better. A harmful candidate is rejected. No applicable task
or an unchanged result is inconclusive and leaves the candidate proposed. A
resource stop cannot pass. A reviewer may veto or ask for a narrower scope with
episode references; approval supplies no promotion capability.

Continuing the example, assessed sibling lessons with the same effect and compatible
scopes can propose their declared `isa` parent. Their assessed members become
proposing members for that new item: the class lesson must be assessed on different
members. In this illustrative story, that fresh class assessment passes and
justifies the class lesson. Mixed positive and negative episodes can instead
propose specialisations along one declared scope dimension. A proposed scope must
exclude the known harmful cases under the same applicability relation, including
capability sets and `isa` inheritance. Growth creates proposals, not evidence for those
proposals. When a tool version changes, the affected justification and its
descendants move to pending; unrelated branches remain available. This is an
illustrative workflow, not a claim that a mined rule generalises before its own
assessment.

```sh
zeno-fcis kb init --out private/kb.layer.json
zeno-fcis kb curate --source candidates.json --kb private/kb.layer.json
zeno-fcis kb assessment-queue --suite suite.json --kb private/kb.layer.json
zeno-fcis kb assess --suite suite.json --candidate lesson-id --kb private/kb.layer.json
zeno-fcis kb explain lesson-id --kb private/kb.layer.json
zeno-fcis kb replay-assessment --assessment ASSESSMENT_SHA256 --suite suite.json --kb private/kb.layer.json
zeno-fcis kb grow --kb private/kb.layer.json
zeno-fcis kb telemetry --kb private/kb.layer.json
zeno-fcis kb refresh --kb private/kb.layer.json
```

Create the private parent directory outside the checkout with permissions `0700`
before using these write commands. `--kb PATH` is repeatable for reads; assessment
updates require exactly one explicit layer. `ZENO_FCIS_KB` supplies paths only when
none are passed. No private directory is searched implicitly.

A minimal candidate source has this shape. `proposing_tasks` and `families` are
required declarations: empty means no proposing observations; otherwise include
every known task digest and evidence family. A family records `subject` (revision
digest), `session`, `actor`, `tool` and `environment`. Use actual lowercase SHA256
values for digests. Mechanical statement hashes and lifecycle fields are computed by
curation, which always proposes.

```json
{
  "schema": "zeno-fcis/kb-candidates/1",
  "isa": [],
  "candidates": [{
    "id": "try-one-optimizer",
    "subject": "pipeline",
    "scope": {"family": ["bounded-counter"], "capability": ["immutable-proposal"]},
    "effect": "helps",
    "action": {"skip": ["canonical"], "order": []},
    "proposing_tasks": [],
    "families": []
  }]
}
```

`integrations/knowledge/import_sources.py` imports explicitly selected Commons
records or one inbox proposal to a new private candidate file. It opens the Commons
database read-only and pins a read transaction. Structured scope and actions are
preserved; prose supplies neither applicability nor an invented strategy. Factory
events need structured door, class, subject revision and failure fields. Repetitions
across distinct subjects produce proposals with evidence references. `kb
ingest-episodes --source FILE` imports explicitly selected episode observations; `kb
grow` also mines those. Import never writes to the source store and cannot promote.

## Formal model and authority

A strategy statement consists of subject, claim, normalised scope, effect and
optional-stage action. Its canonical hash excludes lifecycle, evidence-family counts
and assessment references. The current adapter's subject is `pipeline`; its action
can order or skip `complete`, `repair`, `canonical` and `resubstitute`. It cannot
replace the checker or its receipt replay.

An owned candidate moves through these states:

- `proposed`: eligible for assessment, absent from advice;
- `promoted`: justified by its own passing assessment;
- `rejected`: a measured regression or review veto;
- `pending`: a stale justification or observational regression needs reassessment;
- `refuted`: an exact claim has a replaying counterexample; that claim stays refuted.

The assessment record is the inspectable assessed state between proposal and
outcome. Inconclusive is an assessment outcome, not promotion. A lesson's
justification contains its assessment, dependency hashes and parent statements.
Retraction is the least fixed point of those dependency edges. Read operations
abstain immediately from stale advice; `kb refresh` persists the retraction history.
Applied observational outcomes are compared with the lesson's own assessed baseline
on comparable subject-and-scope observations. An unseen subject has no paired
baseline; it supplies observational evidence rather than a fabricated regression
comparison. Regressions trigger reassessment and never promote by themselves.

For every applicable held-out task, compare exact verdict class, every declared work
counter, and both node count and encoded byte size when a result is accepted. No
measure may worsen; at least one must strictly improve. Decisive but different
verdict classes are not interchangeable. A policy's optional minimum effect in basis
points limits improvements credited as useful; it does not permit a regression. A
budget stop in an applicable comparison, an assessment resource stop or an
incomplete suite cannot promote. The checker decides correctness independently of
this threshold.

The suite and record declare all per-task limits and the total reserved work budget.
Assessment records retain consumed loop counters and assessment comparison charges,
including the repeated independent replay. Reserved work is recorded separately.
Curation work and all earlier assessment work at the KB revision are retained too;
failed and inconclusive learning is not free. Completed runs in an interrupted
comparison remain bound as partial observations, with their counters and episodes.
They are observational, outside comparable complete-suite profiles. Replaying such
a record checks its completed observations; it does not reproduce a historical
scheduler timeout. Missing worker consumption stays unknown behind the reservation.
Artifact validation can also refuse: the measured assessment stays inconclusive,
without new certified facts or promotion. These are declared deterministic
accounting units, not CPU instructions.

### Exact facts and reuse

`proved`, `refuted` and `certified` facts are keyed by exact subject digest and
checker/engine identity. They carry premises and a replay artifact. A proof of
finite equivalence proves that property for that pair, not an efficiency claim or a
capability on other subjects. Counterexamples refute the exact recorded claim
immediately. Certified bounded not-found and telemetry observations are facts about
that run and its budget only.

```sh
zeno-fcis kb certify --original original.zcve --candidate candidate.zcve --kb private/kb.layer.json
zeno-fcis kb facts --subject SUBJECT_SHA256 --kb private/kb.layer.json
zeno-fcis kb reuse --fact FACT_SHA256 --subject original.zcve --kb private/kb.layer.json
```

`certify` replays finite transform equivalence or its counterexample without
requiring usefulness assessment. `--refutes lesson-id` additionally requires that
lesson's exact `equivalent:SUBJECT_SHA256:CANDIDATE_SHA256` claim to match; it
retracts that claim's dependents. `reuse` checks the supplied subject and current
checker and replays the artifact again. Merely retrieving a fact never accepts its
result. The current reuse adapter handles these finite artifacts; other bounded-run
results need their recorded task replay. Lean and Verus proof adapters are not
supplied by this implementation.

### Frozen cases and the aggregate measure

The public regression suite is
`crates/zeno-fcis-cli/tests/data/knowledge-suite/suite.json`. It contains six small
native loop-door, core-component and template-derived tasks. The retained initial
suite also includes counter-template completion. Every task has a digest, input
digest, scope, fixed seed and budgets. Capacities one and two are development
subjects; the current revision uses capacities four, six and seven and three other
pinned programs as held-out subjects. The current resource revision declares a
22-second process allowance; the earlier 10-second suite and its safe inconclusive
attempts are retained. The proposing split, seeds, work limits and actions stayed
the same across that operational revision. The retained initial revision used
capacities three, five and eight; after its broad shortcut was rejected, all its subjects
became proposing subjects for the next revision. The engine refuses a task or
subject revision used to propose the candidate, including parent cases used for
generalisation. Renaming a subject does not make it held out. Cases retain their
ON/OFF observations for exact-scope reuse.

To add tasks, create a new frozen suite revision before observing its outcomes,
retain the previous suite, and declare the proposing/held-out split. Once a result
informs a proposal, reserve different members for the next assessment. Public
regression fixtures are not a secret or representative population.

```sh
zeno-fcis kb assess --suite suite.json --kb private/kb.layer.json
```

Without `--candidate`, this runs all current promoted lessons against none and
records the effect at that KB revision. Only this frozen ON/OFF result is evidence
for a **compounding** claim: tasks solved and deterministic work on that suite.
Inspect `aggregate`: it also reports assessment and curation work, including prior
learning and recorded proposing-run work, and the ON cost with that overhead. A
reduction in loop work alone does not establish a net saving or a benefit on other
tasks.

The recorded six-task example has these outcomes:

| Candidate | Held-out outcome | Reason |
| --- | --- | --- |
| Skip the canonical stage for bounded counters | Promoted | Three applicable tasks use 138→92, 186→124 and 210→140 work units with unchanged accepted program costs |
| Skip resubstitution for bounded counters | Rejected | Work falls, but accepted programs grow from 20→23 nodes and 22→24 nodes, with larger encoded sizes |
| Scope absent from the suite | Inconclusive; stays proposed | No applicable task |

With only the helpful lesson promoted, ALL versus NONE solves 5/6 tasks in both
modes: total loop work is 6,983 versus 7,161, a reduction of 178 units (about 2.49%).
The aggregate records 28,480 assessment units, 91,834 prior assessment units and
268 curation units, making the learning-inclusive ON total 127,565. Completed
work from an earlier inconclusive attempt remains in that history. These figures
describe this frozen revision; they establish neither a net saving nor an effect
on a representative deployment population.

### How to read a loop's value

```sh
zeno-fcis kb leaderboard --kb private/kb.layer.json
zeno-fcis kb leaderboard --population observational --kb private/kb.layer.json
```

Profiles are per loop configuration and problem class. The held-out view uses the
latest recorded nonempty suite revision, so earlier proposing subjects do not leak
into a later portfolio comparison. Held-out certified facts and field observations
are separate populations. Primary order is solved count descending, PAR-2 work
ascending, closed gap descending, then lexical. Profiles and points are descriptive
trial menus, never strategy advice or acceptance gates. Their selection bias remains
visible.

On the frozen example's three bounded counters, OFF and the helpful ON
configuration each solve 3/3. Their PAR-2 sums are 534 and 356; since every task
solves, these are also their total solution work. Median/p90 work is 186/210
versus 124/140; ERT is 534/3 versus 356/3. VBS and SBS both have PAR-2 sum 356,
so closed gap is undefined. Four recorded configurations each solve all three
tasks: each receives coverage-game Shapley credit 3/4, with zero unique solves
and zero marginal coverage. All receive 79 points, grade B: rounded points do not
distinguish their small budget-relative efficiency differences. The harmful
configuration has the same lower work and points as the helpful one, yet fails
assessment on program cost.

For an unsolved applicable task, PAR-2 charges twice its declared work budget. A
refusal is outside scope and is omitted from that loop's PAR-2 denominator; it
still reduces applicability. Unavailable observations are `null`.

| Metric | Definition or interpretation |
| --- | --- |
| Effectiveness | Solved/rate, decisive verdicts, counterexample yield, unique solves, marginal coverage and exact coverage-game Shapley credit |
| Efficiency | Nearest-rank median/p90 work to solve; PAR-2 charges unsolved applicable tasks twice their budget; ERT is total consumed work divided by solves |
| Anytime | Solve counts at declared budget points and exact step-profile area normalised by budget and applicability |
| Portfolio | VBS is per-task best; SBS is best fixed configuration, on a complete common applicable matrix; closed gap is `(SBS-loop)/(SBS-VBS)`, undefined when the denominator is zero |
| Performance profile | Fraction within factors 1, 2, 4 and 10 of best known solved work |
| Quality | Accepted node-cost gap to best recorded result; checked node-improvement rate |
| Reliability | Receipt replay, refusals, applicability, classes with positive marginal coverage and recorded version regressions |
| Cost | Calls, tokens, reserved money and owner questions per solve only where invocation accounting is available; unknown values stay `null` |
| Knowledge impact | Each lesson's bound assessment and ON/OFF delta |

The scalar work view sums the declared loop counters with unit weights; the Pareto
gate continues to inspect every counter independently. Counterexample prefix work is
unknown when the underlying loop did not record it.

Loop value points range from 0 to 100. The displayed components have weights 40%
solved rate, 25% PAR-2 efficiency, 20% marginal contribution and 15% receipt replay.
Efficiency is `1 - PAR2/(2*sum of applicable budgets)`, floored at zero. Grades are
A at 85, B at 70, C at 50, and D below 50. Weights and components are in each
profile and are recomputed on demand. **Points never promote, veto or exclude
anything.** They are a reading aid, not the objective threshold.

The requested external references could not be verified from primary sources in the
offline source tree. The attribution remains **unverified**, including PAR-2/SAT
competitions; ERT/COCO-BBOB; Dolan and More (2002), performance profiles; Frechette
et al. (AAAI 2016), Shapley portfolio analysis; and the VBS/SBS closed-gap
convention. The formulas above are the implementation contract; the bibliographic
attributions require verification before use as publication evidence.

## Difference from 2.4 and the earlier knowledge engine

The earlier engine provided typed Datalog applicability, provenance, replayed
positive/boundary cases, layering, automatic consultation and inbox outcomes. It did
not assess empirical usefulness. Those logical checks remain; a pair of
reproductions does not by itself qualify a strategy for advice now.

`kb migrate --source old.layer.json --out private/kb.layer.json` preserves the
version-one knowledge text as pending legacy evidence. It does not silently promote
curator decisions. The retained legacy facts and rules remain available to explicit
logical queries and explanations as a pending read projection; they cannot activate
strategy advice. Version-one files remain readable for explicit queries and replay,
but their external validations cannot activate strategy advice. Translate the
desired routing into candidate records and assess it. The compiled public
declarations remain a separate fixed demonstration policy; they are not a measured
empirical generalisation.

The logical language remains finite, function-free, safe stratified Datalog.
Positive strata reach a least fixed point; negation reads completed lower strata.
Recursion through negation refuses. Closed-world absence means not derivable in this
snapshot, never false in reality. Each answer retains one bounded derivation with
rule, premises, source layer, negative lookups and typed comparisons. `kb explain
'usage_applies(L,S)' --task 'task(subject,check-program).'` continues to provide
those witnesses; a lesson identifier shows its assessment, episodes and parent
chain.

Scope dimensions remain `tool`, `version`, `family`, `environment`, `capability` and
`prerequisite`. Every declared condition must match. Tool and family follow
directional `isa`; version and environment match exactly. An explicit `any` requires
a known fact. Missing or `unknown` task facts do not satisfy a requirement. Numeric
K1 limits retain explicit units. Similarity, global age decay and strength vetoes
are not implemented. Conflicts are reported; incompatible overlapping promoted
actions refuse publication.

## Storage, privacy and supported scope

Version-two descriptors contain `schema: zeno-fcis/kb-layer/2` and one `state`
path/hash binding to an immutable content-addressed snapshot. A writer locks the
layer, compares the expected descriptor hash, validates the complete snapshot,
writes and fsyncs it, runs `kb check` on the staged descriptor, then renames and
syncs the descriptor. Failure retains or restores the old complete layer. Readers
verify the descriptor and snapshot; a half-written data file is never published as
the current layer. Locks are process-owned; immutable old snapshots support
recovery. This is local filesystem durability, not distributed consensus or
authenticated provenance.

If assessment publication loses a lock or revision race, the command refuses and
retains an unpublished private state snapshot with a path and hash. The response
keeps its assessment and measured work. The recovery snapshot is not a layer
descriptor and cannot activate advice. Inspect it and the current revision before
retrying. A filesystem failure can also prevent retention; that refusal is returned
with the assessment, without a durability claim. Unpublished attempts remain
separate recovery evidence, outside the published revision history.

All acquisition uses explicit files outside a checkout. Private text, scope,
episodes and traces stay in those private layers and inboxes. Ordinary loop logs and
receipts carry paths, hashes and public status only. Explicit `kb` responses contain
the requested private data; keep their destination private. Nothing is uploaded or
embedded in a package. Only the public neutral example and public regression
fixtures are shipped.

MCP retains read-only advice, planning and explanation. One additional
`kb_assessment_view` exposes assessments, queue, telemetry or the leaderboard. It
cannot assess, import, certify, grow, refresh or write a layer. Other MCP loop
adapters suppress ambient and saved-session knowledge configuration. Read-only
access cannot control a caller's own logging.

The K1 logical limits remain 64 KiB combined text, eight layers, 8,192 tokens per
text, 128 rules and predicates, 4,096 facts and bindings, arity eight, sixteen body
literals and planning steps, 32 variables, 8,192 relation rows, 4,096 planning
states and 2,000,000 logical work units. Acquisition and process deadlines refuse
expired results; they are not a host wall-time guarantee. Version-two snapshots are
limited to 16 MiB, 128 candidates and assessments, 64 tasks per suite, 128 hierarchy
edges, 4,096 retained episodes/facts and bounded per-task output. All returned data
is owned; no mutable global knowledge cache or caller alias supplies decision
authority.

The current assessment adapter covers finite guarded program pipelines and finite
equivalence/counterexample artifact replay. It does not establish whole-tool
readiness, universal loop quality, a cost optimum, a production release, or a
benefit beyond the declared frozen comparison. Source-bound assessment evidence must
be refreshed after engine or checker changes.
