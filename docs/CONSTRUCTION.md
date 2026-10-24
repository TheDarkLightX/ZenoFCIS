# Construction from requirements

Describe what a component must do. Construction searches different internal
layouts and checks each proposed implementation against the frozen meaning over
its entire declared finite domain.

## The user journey

Start with a one-line ask: "a priority collection with insert, cancel, change
priority and remove lowest; at most two items, three identities, two priorities,
and two insertion tickets per instance."

Settle the choices that affect behavior before construction: priority 0 is lower,
equal priorities use arrival order, and Change preserves arrival. Insert rejects
Duplicate before Full before Exhausted. Missing Cancel or Change rejects Missing;
empty Remove rejects Empty. Same-priority Change accepts and emits its event.
Every Accept emits one event containing kind, identity, priority and arrival;
unused priority and arrival fields use sentinel 2. Reject commits nothing.

The meaning is frozen as a 45-state, 16-command reference table with 720 rows.
The requirements bundle contains that reference, the abstract contract, reviewed
T/I grammars, frozen abstraction maps, source pins and checker qualification.
The owner approval binds the exact bundle manifest and retained meaning decision.
These inputs must be prepared and reviewed before invoking construction.
Search requires `preflight/preflight.json` beside the bundle; both come only from
the repository's construction driver. The command accepts this explicit bundle;
it does not interpret free-form prose or create an owner approval.

Given a frozen bundle and its approval in the same evidence directory, run
from the parent of that directory, outside the source checkout:

```sh
zeno-fcis construct --family priority-collection \
  --requirement requirements/bundle \
  --owner-approval requirements/owner/approval.json \
  --layout T --out requirements/T
```

Choose T for two slots indexed by arrival, or I for three cells indexed by
identity. Use a fresh output directory for each invocation. The bundle and its
qualification must match the current source and executable; a stale bundle
refuses. Retain the bundle's enclosing evidence directory and referenced files.
The current workflow requires the matching source checkout and local Git
inventory; it is not a standalone binary-only workflow. A binary built without a
recorded source tree refuses with `construction-build-tree-unrecorded`.

Construction starts with the sixteen genesis examples. It fills typed holes in
rejection guards, counter updates and cell updates. A failed candidate's first
native counterexample becomes another example, and construction starts a fresh
search. There are at most 32 attempts per layout and 50 million proposal-work
units per attempt, shared across output solves. A limit stop remains incomplete.

Exit 0 means `candidate-checked`: the run's final attempt names the generated
`candidate/project.zeno`, `candidate/v2/policy.json`, exhaustive `native.json`,
compiled and lowered programs, and transform receipt/check/replay records.
`run.json` retains all attempts and their counterexamples and measured work.
Exit 1 is a failed gate, 2 is incomplete or no hit, 65 is malformed or unsupported
input, and 3 is an I/O or subprocess failure. Unsupported families refuse with
the requested family name before creating output. Existing output is refused.

A checked candidate is one layout's result. Whole-workflow qualification also
requires both layouts, replay, planted faults, comparison-disable tests and
independent review. The command does not grant that review or install the result.

## The formal model

The abstract machine A has state `(n, M)`: a counter n in 0..2 and a partial map
from three identities to `(priority, arrival)`. Distinct arrivals are less than n;
there are 45 states. There are sixteen commands. Its step returns class, reason,
successor, effects and an ordered event list.

A concrete layout C has 147 raw states for T or 375 for I. An explicit abstraction
map alpha sends each raw state to its abstract state, or null for an invalid
encoding. Each layout has 45 valid states; T has 102 invalid states and I has 330.
Genesis corresponds, and every abstract state has a concrete representative.

For every raw state and command, [contract refinement](CLI_REFERENCE.md#contract-representation-refinement-26-unreleased)
executes the actual native Authorities. Valid encodings must agree exactly on
class, reason, successor abstraction and all ordered effects and event fields.
Rejection preserves the concrete state. A null encoding must refuse with the
designated law violation and commit nothing. Usage includes refused work.

The check visits all 2,352 T rows or 6,000 I rows. Full one-step refinement plus
corresponding genesis gives matching traces by induction inside this fixed
machine. This is checked exhaustively over the declared domain; it is not a
parametric theorem. Lowering check and replay separately confirm that the
materialized scalar program matches its lowered graph on every raw input.

## How this differs from bounded search

Repair, sketch and program search usually preserve one input/state layout.
Construction permits the implementation's state to differ from its specification.
An abstraction map connects those state spaces, and invalid stored encodings are
part of the comparison. One frozen meaning can admit multiple designs.

The construction search is bounded and untrusted. Examples guide proposals;
only exhaustive native refinement admits them. Finding no candidate within the
budget does not establish impossibility. Exact reasons and ordered events are
part of correctness, alongside the successor state.

## Scope

Only the priority-collection family and its fixed capacity-two envelope are
supported. There are at most two successful insertions during an instance's
lifetime; removing an item does not restore an insertion ticket. This feature
does not construct arbitrary applications or prove arbitrary capacities,
migrations, compiler correctness or external event delivery.

Planted controls cover reject-all, forgotten content, a missing operation,
strict-priority insertion, wrong tie order, wrong reason, invalid encoding and
source substitution. Each comparison also has a test-only suppression control.
At capacity two, a non-strict interior-insertion fault has no witness; the
strict-priority control makes no claim to detect that fault. Production exposes
no comparator bypass. Review and release qualification remain separate gates.

The construction driver's scenario tests, its frozen-reference tests and the
planted-control meta check need the retained construction evidence: the frozen
bundle and its previous version, the evidence directory, and the driver built
from the same source (`cargo +1.97.1 build -p zeno-fcis-cli --example
construction_pilot`). They are ignored by default. Set `E`,
`ZENO_FCIS_CONSTRUCTION_DRIVER`, `ZENO_FCIS_CONSTRUCTION_BUNDLE`,
`ZENO_FCIS_CONSTRUCTION_PREVIOUS_BUNDLE` and `ZENO_FCIS_CONSTRUCTION_EVIDENCE`,
then run them with `-- --include-ignored`; without that evidence each fails
instead of skipping. The meta check also requires an F51 qualification of the
current source, so it refuses a bundle qualified on an earlier source.
