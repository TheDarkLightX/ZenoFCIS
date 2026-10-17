# Scoped usage knowledge

Configured loops consult curated usage records before work and propose new
observations for review. Knowledge can change proposal routing; exact program
checkers still decide acceptance. A derivation explains the loaded facts and
rules. It does not establish that an empirical lesson generalizes.

## Commands and configuration

```sh
zeno-fcis kb advise --task 'task(subject,check-program).'
zeno-fcis kb advise --task 'task(subject,observe). task(tool,zeno-fcis). task(version,v241). task(environment,sandbox).'
zeno-fcis kb plan --have model,observations --want answer --task 'task(subject,observe). task(tool,zeno-fcis). task(version,v241). task(environment,sandbox).'
zeno-fcis kb explain 'usage_applies(L,S)' --task 'task(subject,check-program).'
zeno-fcis kb check --kb team.layer.json --kb personal.layer.json
zeno-fcis loop open --original original.zcve --session session --kb personal.layer.json --kb-task 'task(tool,zeno-fcis). task(version,v241). task(environment,host).'
```

`--kb PATH` is repeatable. `ZENO_FCIS_KB` supplies a platform path-separated
list when no paths are passed. These settings apply to `kb` and `loop`.
No private location is searched automatically. Raw UTF-8 knowledge files and
explicit JSON layer descriptors must live outside a ZenoFCIS checkout.
Descriptors name their knowledge file and validation files explicitly; neither
an inbox nor a directory of lessons is scanned.

The compiled public example loads first, followed by the given layers in order.
A descriptor has this shape; replace the hash placeholders with actual SHA256
values:

```json
{
  "schema": "zeno-fcis/kb-layer/1",
  "knowledge": {"path": "lessons.kb", "sha256": "KNOWLEDGE_SHA256"},
  "validations": [{"path": "validation.json", "sha256": "VALIDATION_SHA256"}]
}
```

Paths in a descriptor are relative to that descriptor. A raw knowledge file's
inbox is beside that file. A descriptor's inbox is beside the descriptor. Use a
root-level descriptor when the inbox should belong to the whole knowledge base.

The older `loop strategy --kb-op query|check|plan` commands remain available.
They also accept `advise` and `explain`; `--kb-task` supplies their task facts.
Read-only knowledge operations do not open a contained loop session.

## Vocabulary

The usage schema is `zeno-fcis/usage-knowledge/1`. It adds thirteen record
predicates to the existing structural catalog, plus `task/2` for a request.

| Predicate | Meaning |
| --- | --- |
| `lesson(Id, Subject)` | Stable identifier and applicable tool or class |
| `scope(Id, Key, Value)` | A required scope condition |
| `strength(Id, Level)` | `declared`, `observed`, `measured`, `preregistered`, or `proposed`; not a truth score |
| `effect(Id, Kind)` | `helps`, `hurts`, `neutral`, `defect`, or `limit` |
| `evidence(Id, Recipe, Hash)` | Evidence binding; a command string alone is insufficient |
| `record(Id, State, Schema, Dependencies, Mode, Family)` | Promotion state, `usage-v1`, dependency hash, evidence role, evidence family |
| `isa(Child, Parent)` | Directional class membership, closed transitively |
| `limit(Id, Metric, Maximum, Unit)` | Nonnegative bound with `count`, `bytes`, or `work` units |
| `blocker(Id, Environment)` | A declared environment exclusion |
| `cost(Id, Metric, Amount)` | Declared or observed nonnegative data, not a calibrated estimate |
| `prefer(Id, Preferred, Other)` | Explicit advisory order; no default inference |
| `supersedes(New, Old)` | Replacement only where the validated new record applies |
| `conflict(A, B)` | Explicit opposition, optionally derived by safe rules |
| `task(Key, Value)` | Facts supplied for this request |

States are `proposed`, `promoted`, and `rejected`. Evidence modes are
`observation`, `exact-precondition`, and `reproduced-blocker`. Only the latter
two may enforce exclusions, and only after validation. Observation records
remain advisory even when their effect is `limit`. Repeating an evidence
record supplies no extra strength or replication weight.

Identifiers are nonempty lowercase letters, digits, and hyphens, up to 96
bytes. A lesson has one subject, strength, effect, and lifecycle record.
Missing evidence, invalid enums, dangling or cyclic supersession, an `isa`
cycle, conflicting scalar scopes, and unresolved overlapping conflicts fail `check`. Promoted records
without current validation also fail `check`. Proposed records never become
advice merely because their syntax is valid. Integrity failures conservatively
abstain from all actionable advice; they never select a conflicting record.

Lesson metadata and hierarchy edges are declared facts. User rules may derive
conflicts and extension views; they cannot manufacture lifecycle or validation
records. The internal `usage_` namespace and stored `task` facts are reserved.

## Applicability and absence

Supported scope dimensions are `tool`, `version`, `family`, `environment`,
`capability`, and `prerequisite`. Every scope row must match. Tool and family
values can match through directional `isa`; versions and environments match
exactly. Capabilities and prerequisites may have multiple required values.
The scalar task dimensions and each numeric measurement have one value.

An omitted scope dimension imposes no restriction. Explicit `any` requires a
known value for that dimension. An omitted task fact or the value `unknown`
does not satisfy a condition. Missing assumptions are returned separately;
they never establish permission or safety. No similarity search supplies scope.

Numeric facts include their unit in the key: a `tuples` limit with unit `count`
uses `task(tuples-count,4096)`. Bytes are a different measurement. Equality at
the maximum is allowed. An unknown size is reported without enforcing a
numeric exclusion. Subject membership alone never discharges other conditions.

Advice contains all and only applicable, promoted, currently validated records.
A validated narrow replacement defeats an older record only in their matching
overlap. Outside it, the older record remains available. An unpromoted or stale
replacement does not retire it. Conflicts are checked over overlapping scopes,
including a known common subclass; disjoint versions do not conflict.

The layer override key is a lesson ID with its complete record block. A later
block wins the effective data view. Changed blocks produce a layer conflict;
both versions are retained in the explicit query report, and advice abstains.
Use distinct IDs and scoped `supersedes` for a deliberate narrow exception.
Layers cannot replace executable steps, checker gates, or the public type
catalog. The complete reserved structural relations still have to match code.

## Logic and explanations

The language is function-free, safe, stratified Datalog over finite constants.
Positive strata reach least fixed points; negation refers only to completed
lower strata. Negation through recursion is refused. The closed-world meaning
of `not p(...)` is “not derivable in this snapshot,” never “false in reality.”

Constants are names, JSON strings, and signed 64-bit integers. `=`, `!=`, `<`,
`<=`, `>`, and `>=` are supported. Ordering requires integers and performs no
arithmetic or coercion. Named head, negative, and comparison variables must
occur in positive literals. `_` is anonymous in queries and literals, and is
refused in heads and comparisons. Every predicate has one arity.

Each answer has one deterministic finite derivation: ground conclusion, rule
index, bindings, source layer, parent facts, grounded negative lookups, and
typed comparisons. Parent facts precede conclusions. Exclusions also expose
the constraint and observed task inputs. Override reports retain both sides;
ranking reports state the declared ordering data and tie-break. Layer hashes
identify the frozen source snapshot. The engine does not enumerate all proofs.

## Evidence and invalidation

Curators promote records explicitly. A versioned replay recipe binds the lesson
statement, runner, checker binary, declared inputs and dependencies, expected
exit, and a JSON output assertion. Each actionable record needs a positive case
and a nearby negative or boundary case. The structured statement excludes the
self-referential evidence and lifecycle hashes; changing scope, effect, limits,
or preferences invalidates its old validation.

`kb replay --recipe recipe.json --out validation.json` runs an approved bounded
shell adapter. It never executes a raw lesson command string and never promotes
a lesson. The first external runner, `zeno-fcis-readonly-v1`, admits fixed native
`transform check --original FILE --candidate FILE` and
`contract review DIR [--gaps]` shapes. It refuses export, arbitrary
tools, and free-form command flags. Cases have bounded input/output sizes and
process deadlines. Missing inputs, changed hashes, timeouts, and assertion
mismatches remain distinct failures. Declare the full relevant source and
policy dependencies; the recipe does not infer that dependency inventory.
Both transform subjects must be hashed case inputs. Contract review requires
its `project.zeno`, `v2/policy.json`, and examples file when present as inputs.

A recipe uses schema `zeno-fcis/kb-recipe/1`, `runner`, `lesson`, `family`, `mode`,
`checker_sha256`, `dependencies`, `statement`, `positive`, and `negative`.
Each input is `{ "path": "...", "sha256": "..." }`. A statement row is
`{ "predicate": "scope", "arguments": ["lesson-id", "version", "v241"] }`.
A case supplies `arguments`, `inputs`, `expected_exit`, and
`assertion: { "pointer": "/status", "equals": "expected-status" }`.
Use the actual checker response schema for the assertion.

Validation files retain the recipe binding, dependency digest, both observed
outputs and assertions, and last validation time. Loading a layer rechecks the
bindings and current dependencies. A change makes that validation stale pending
explicit replay. There is no global age decay. Validation files are local
curator evidence, not authenticated receipts or program publication authority.
A replay validates the recorded cases, not an empirical generalization.

The compiled example has a separate `compiled-public-policy-v1` runner. It
replays fixed positive and boundary fixtures against the actual pure engine
for the public declarations only. Its evidence hashes bind the public source;
it cannot admit a private record or establish real host availability. The
example's sandbox restriction is a declared demonstration policy. Its work
units are declared ordering data. They are not private measurements.

## Planning and automatic feedback

Planning first retains the structural catalog check and existing type algebra.
It filters evidenced environment blockers and exceeded numeric limits. Search
finds a shortest monotone type path. Declared preference penalties, declared
work units, and lexical ties order the frontier; this is not a cost optimum or
an empirical strategy recommendation. Preferences attached to a defect or
negative effect do not recommend that observed bad ordering.

Execute only `next_step`, then use `kb replan` with the artifacts actually
produced. A failed or unknown step contributes no imagined products. Artifacts
in a plan are names, not checked runtime values. Program qualification still
requires the unchanged exact checker and matching full-domain replay.

Configured loop invocations consult before dispatch. Program pipelines and
strategy trials filter and order their optional stages from that advice, and
re-evaluate guards after actual search and checker outcomes. A blocked direct
operation is skipped. Unavailable advice retains the ordinary checked proposer
path. The contained child checks the parent's snapshot binding before using
its own acquired data. No mutable global knowledge cache is retained.

After each configured invocation, exactly one structured proposed lesson is
written to the last layer's `inbox/`, alongside private observation evidence.
The files record consultation, application or blocking, observed checker fields
when available, and costs. Session accounting counters are labeled cumulative,
separately from invocation counters. A loop outcome never invents a matched
replay, and invocation duration excludes proposal-file I/O. Missing checker observations remain unknown. They
never imply success. No file is added to promoted lessons. Inbox directories
require private permissions and files use exclusive creation. This does not
promise crash-safe exactly-once feedback or an authenticated curator.

## Privacy and bounds

Only the public example is compiled or packaged. User knowledge is acquired
from explicit external files. Ordinary loop diagnostics and receipts carry
paths, hashes, and public status metadata; lesson text, identifiers, scopes,
and rule traces stay out of them. Explicit `kb` responses contain requested
private data: keep their destination private.

MCP exposes only `kb_advise`, `kb_plan`, and `kb_explain`, marked read-only.
These calls do not write proposals, replay recipes, or promote records. They
use only the paths given to that call. Other MCP loop tools suppress ambient
KB configuration, so they do not create a KB write path. A client can still
record a tool response; read-only access does not control a client's logs.
The engine has no upload operation.

The pure caps remain 64 KiB of combined knowledge text, eight layers, 8,192
tokens per parsed text, 128 rules including the fixed usage rules, 128
predicates, 4,096 declared facts, arity eight, body length sixteen, and 32
named variables. Relations admit at most 8,192 rows; joins and query answers
at most 4,096 bindings. Planning has at most 4,096 states and sixteen steps.
Trace copying is charged and bounded. The work ceiling is 2,000,000 logical
units per consultation, including public policy replay preparation. Stops
retain charged work and return no partial answers or candidate plan.

A contained invocation consults in the parent and child separately, with each
cost recorded separately. Search/check budgets remain their own accounting;
logical work is not measured CPU time. Acquisition and completion deadlines
suppress expired results without a host wall-time guarantee. The loader caps
external validation files at 32. It does not discover facts from prose, learn
rules, implement stable models, decay knowledge by age, estimate success, or
establish a compounding advantage.

A successful configured `loop open` or `loop capture open` freezes the KB
snapshot and task configuration for later commands on that session. A sibling
`SESSION.knowledge.json` contains only the private configuration path and hash;
the task facts and explicit KB paths live in the last layer's private inbox.
This sidecar is outside the authoritative session store. Later commands reuse
that configuration. Changed layers, validation dependencies or task context
make consultation unavailable; ordinary exact checks remain available. Open a
new session to change the frozen context. `--no-kb` explicitly disables automatic
consultation, including stored session configuration. MCP loop adapters always
use this flag. Stateless commands use one snapshot per invocation.

The neutral learner example also requires family `finite-learner`, capability
`finite-domain`, and prerequisite `vocabulary`. These are explicit descriptions
of the finite vocabulary workflow, not facts guessed from a task's text. For
that example, pass all three alongside tool, version, and environment facts;
missing any one abstains. Its transitive learner membership preserves every
one of those conditions.
