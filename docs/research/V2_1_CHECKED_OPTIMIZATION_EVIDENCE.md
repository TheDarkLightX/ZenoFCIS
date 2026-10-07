# V2.1 checked optimization: application-derived evidence

Date: 2026-10-04. Feature: track 6, `transform` and `optimize`, including the
planned neurosymbolic proposal/check/feedback loop. Status: native research
evidence for the feature; product integration remains planned.

The withdrawal-queue result is evidence for the V2.1 feature we intend to ship:
neural and e-graph search proposes smaller pure programs, and a separate exact
checker decides whether each actual candidate preserves the declared behavior.
The checked result does not depend on trusting how search found the candidate.

## Discovery and independent checking

The owner reports that the original research result was produced by a
neurosymbolic optimizer using e-graphs. That is recorded as reported discovery
provenance. The provided four-page research brief records the candidate and
research findings; the original model/e-graph search trace was not replayed in
the native qualification below. This run does not measure the search strategy.

Root independently reproduced the OR-to-Select replacement with a bounded
pattern proposer, extracted the actual controller kernel, and applied that
replacement to the current decision scalar graph. It then checked the actual
canonical programs with the native Rust library evaluator and independently
written direct domain-rule oracles. Astra Max separately reviewed the source,
observations and recorded evidence and found no decisive blockers.

Thus the discovery attribution and the independently checked optimization are
separate evidence items. The native proposer is a reproduction mechanism; it
does not implement or rerun the upstream neural/e-graph search.

## What this demonstrates for track 6

| Feature obligation | Evidence obtained |
| --- | --- |
| Useful smaller candidate | Kernel 16 → 7 nodes; retained controller 69 → 60; current decision scalar graph 106 → 100 |
| Complete finite scalar equality | All 16 kernel, 384 controller and 1,296,000 current-graph tuples match the direct rule oracles within the recorded native scope |
| Cost measured on actual artifacts | Canonical bytes decrease 1,063 → 763, 4,090 → 3,790 and 6,686 → 6,486 respectively; existing importer round-trips every artifact |
| Acceptance independent of proposal origin | The native comparison evaluates the actual original and candidate; it does not accept an e-class membership, search verdict or claimed rewrite result |
| Observable resource differences retained | Reduced node counts change successful Steps and boundary refusals; padded controls retain tested observations but increase encoded size |
| Wrong candidates detected | Wrong OR and reordered-output controls differ; invalid graphs, malformed inputs, eager traps and coverage defects are checked |
| Source-bound reproducibility | Frozen source/dependency manifest, locked offline builds, actual canonical artifacts and commands; independent Astra Max source/evidence review |

These are exhaustively checked scalar optimizations under the declared finite
domains and trusted native execution. They supply a concrete application-derived
case beyond the earlier synthetic development seeds. They establish neither
physical speed nor global optimality, broad optimizer effectiveness, or the
soundness of an unimplemented product acceptance API.

The 69-node subject is the retained six-output controller fixture. The current
production template uses the separately checked nine-output 106-node scalar
descriptor. Its 1,296,000-row product includes law-invalid and unreachable
states; scalar coverage is not a complete application-publication argument.

## How it feeds implementation

Use the four-input/three-output Boolean kernel as a real-source regression for
the first `FunctionalBoolV1` milestone. It fits that profile's input, output,
node and tuple bounds. Keep its canonical original/candidate bytes, complete
16-row comparison and wrong-candidate witness. This supplements stage 6.1
qualification; it does not complete it.

Keep the controller and current mixed-input graph as separately scoped research
subjects. The current graph's cardinality exceeds the planned checked-i64
transform limit; this experiment does not enlarge any product limit or bypass
profile admission. Preserve the resource counterexample as an application
adoption regression. Padding provides a useful no-improvement control: equal
node count and larger canonical bytes must not count as an accepted cost win.

Product work still needs the qualified checker/custody boundary, measured
checker-work limits, canonical equivalence receipt and replay, bounded neural
and e-graph proposal adapters, checked fallback, and the selected profile's
source/body proof and integration gates. Application adoption separately needs
complete decision observations and the reviewed resource/identity contract.
Original search revisions, model/rules/extractor configuration and trace should
be retained when qualifying the proposer, alongside its actual candidates.

## Records and claim wording

- [Native benchmark and reproduction](../benchmarks/withdrawal-queue/README.md)
- [Frozen native evidence](../evidence/v2_1_benchmarks/withdrawal-native-20261004.json)
- [Actual canonical artifacts](../benchmarks/withdrawal-queue/artifacts/manifest.json)
- [Independent Astra Max review](../benchmarks/withdrawal-queue/REVIEW.md)
- [Transform contract and delivery stages](../V2_1_TRANSFORM_DESIGN.md)
- [Neurosymbolic loop design](../neurosymbolic-loop/DESIGN.md)

Suggested description: **application-derived research evidence for
neurosymbolically discovered, independently checked functional-core optimization**.
State that discovery provenance is owner-reported and native checking was
independently reproduced. The production feature and hosted search evaluation
remain planned. This evidence adds no smaller-V2 release condition.
