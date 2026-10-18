# Validate the experimental ladder

From the repository root, with the pinned Rust dependencies available locally:

```sh
python3 -B integrations/zal/check_ladder_temporal.py native-build
python3 -B -m unittest discover -s integrations/zal -p test_ladder.py
python3 -B -m unittest discover -s integrations/zal -p test_ladder_temporal.py
python3 -B integrations/zal/check_ladder_temporal.py planted --logs /tmp/zal-defect-logs
python3 -B integrations/zal/check_ladder_temporal.py suite
python3 tools/atdd.py run --scenario zal-dialogue
```

`CARGO_TARGET_DIR` can select an existing build directory. The build helper uses
the pinned toolchain, locked offline dependencies, at most two jobs, debug
information disabled, and a six-GiB free-space floor. The inherited real capture
check is opt-in through `ZENO_CAPTURE_TEST_CLI` and requires systemd containment. The native diagnostic example reuses the
current temporal evaluator and typed Rust parser/exporter; it has authority
none and offers no certificate or publication path.

M0 retains its original tests and fixed bounds: exhaustive trees through five
nodes and 20,000 seeded trees through depth six. The extended temporal signature
includes M0 integer terms, counts, object binders, predicates, state functions,
a definition, two Boolean event atoms, all four temporal constructors and
within 0, 1 and 2. Quantifiers inside time are excluded by the declared type
rules. Both universal and existential trace binders are included.

The complete mixed enumeration contains 6,366 closed well-typed statements
through five total nodes, including the trace binder: 44,562 rung checks.
A separate 20,000-tree mixed sample has total depth at most six and seed
20261017. Larger fixed fixtures cover temporal properties over rigid object
identities, primed state functions and counts. Collisions between distinct
expanded normalized meanings are failures. These are bounded checks, not an
all-formula completeness proof.

An independent test-only truth definition covers all 340 two-atom Boolean
words of lengths 1 through 4, at every position, against 36 selected operator
and nested formulas: 45,072 native comparisons. Additional fixtures cover
strong endpoints, deadline equality, strict four-event order, missing until
witnesses, wrong request identities, undefined arithmetic, missing data,
resource exhaustion, repeated alarms, UI hiding and initial disaster.
SMT and Lean source fixtures exercise the expanded temporal operators under
the existing evidence policy; generated source is not a checked theorem.

Expansion boundaries include 256 objects with balanced universal/existential
claims, refusal of 257 objects, and the native shape boundary at 186 versus
187 expanded equivalences (4,091 versus 4,113 nodes). Native depth, export
operation and intermediate-byte refusals have named checks. Registered Plain
and Question readings are tested in both temporal modes and across horizons;
explicit profile/theory mismatches and malformed replay inputs are refused.

Isolated planted controls cover weak next, missing until witnesses,
first-parse acceptance and a resettable illustrative alarm budget. M0 controls
also challenge comparison wording, first-parse acceptance and prime-sensitive
idioms. Each must fail its named test. Live source stays unchanged, and the
ordinary final tests must still pass afterward. The budget fixture is a test
model, not an implemented withdrawal controller.

Documentation checks are `python3 tools/check_public_docs.py`,
`python3 tools/check_library_docs.py`, the bounded release privacy check, and
`git diff --check`. A release requires the separate complete current
[contribution checklist](../../../CONTRIBUTING.md), including full acceptance
on its exact source. These development commands alone prepare no release.
