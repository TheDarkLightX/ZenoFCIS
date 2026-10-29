# Explained ontology classification

This optional ZAL profile helps a person find shared structure and ask why a
classification follows. It extends the included semantic and notation libraries
with the positive ontology fragment from [Crucible PR44](https://github.com/TheDarkLightX/Zeno-Crucible/pull/44).
The existing abstraction, story, category and Lens APIs remain available.

For example, the [DeFi arithmetic requirements](../examples/ontology/defi_arithmetic.english.txt)
declare an unsigned swap balance and a signed PnL amount. Both need wide
arithmetic, while their signed and unsigned requirements remain distinct:

```text
Axiom "wide-requirement": if an individual is "Amount" and has "usesRepresentation" some "FullWidth", then it is "NeedsWideArithmetic".
```

ZAL returns the rule, the amount's declared type, its representation link, and
the representation's established width. It can show exactly why both objects
share that class. The classification does not make their domains interchangeable
or prove an arithmetic implementation. The full example retains those limits
in its source metadata.

## Authoring and questions

From `integrations/zal`:

```sh
python3 -m zal_notation ontology-parse --source examples/ontology/defi_arithmetic.english.txt
python3 -m zal_notation ontology-classify --source examples/ontology/defi_arithmetic.english.txt
python3 -m zal_notation ontology-browse --source examples/ontology/defi_arithmetic.english.txt --output /tmp/zal-defi-ontology
python3 -m zal_notation ontology-replay --directory /tmp/zal-defi-ontology
```

Open the generated `index.html` to select two individuals and follow a class
through its premises. The browser displays a captured run; Python recomputes
the semantics and artifact bytes during replay. Export requires a fresh
directory. The original illustrative training/programming example is available
through `ontology-demo --output /tmp/zal-ontology-demo`.

The actual ZAL MCP bridge exposes `zal_ontology_parse`, `zal_ontology_classify`,
`zal_ontology_query`, `zal_ontology_explain`, `zal_ontology_replay`, and
`zal_ontology_validate_record`. These operate on supplied inert documents and
do not read or change the behavioral review workspace. Existing finite-FSM
review and acceptance stay separate. A running installed server needs a fresh
process to load the updated code; the separately hosted Abstraction Workspace
is not automatically upgraded by this source integration.

`sharing.toolkit()` also lists these operations. The new package CLI's `toolkit`,
`discuss --request FILE`, and `tools-stdio` expose this ontology family. The last
command uses bounded newline-delimited JSON, rather than MCP transport. General
notation and library operations retain their Python APIs.

## Meaning and limits

Names are explicit JSON-quoted symbols; each English statement occupies one
line. Fixed templates lower to `zal.ontology/1`, profile
`zal.ontology/positive-rl/1`. There is no free-text fallback, implicit declaration
or interpretation of origin metadata as logic. Rendering and reparsing retain
the canonical document for inputs within the English surface's limits.

The admitted rules are named subclass, conjunction and independent existential
antecedents, subproperty, domain/range, and disjointness witnesses. Guards inspect
existing individuals and links; they do not create witnesses. Each new fact
retains one deterministic acyclic derivation. The [OWL 2 RL specification](https://www.w3.org/TR/owl2-profiles/#OWL_2_RL)
provides the rule direction; this implementation admits a smaller explicit
fragment. Equality, existential heads, arbitrary SWRL, cardinalities, remote
imports and general RDF ingestion are unsupported.

`classified` means the admitted named ground closure completed without a
disjointness conflict. `incomplete` retains an explicit budget reason and unknown
consistency. `inconsistent` retains a contradiction witness and does not apply
logical explosion. Unsupported constructs receive no supported-prefix closure.
Missing facts remain unknown in every case. Matching replay preserves the
original status and cannot turn an incomplete result into a classification.

Admission allows at most 64 classes, 32 properties, 128 individuals, 128 axioms,
1,024 assertions and eight guards per classification rule. Runs stop at 1,024
facts or one million charged logical work units. Finite ground facts and
monotone insertion give a termination argument for this fragment. These logical
bounds do not prove wall-time or memory isolation, or a performance crossover.

Turtle and expanded JSON-LD export the asserted ontology with exact-document
URNs. No external vocabulary identity is silently assigned. The fixed explicit
record validator implements selected [SHACL Core](https://www.w3.org/TR/shacl/)
constraints. A forged receipt can have conforming fields while its behavioral
claim fails independent finite-relation replay; the demo keeps these questions
separate.

## Evidence boundary

The host binds the ontology, budgets, complete result and six installed source
files. It captures source identity at initialization and checks it before and
after classification. Restart after implementation edits. Those hashes provide
integrity/replay metadata, not loaded-bytecode attestation or authorship.

Component provenance is recorded in `ONTOLOGY_UPSTREAM.json`. Local validation
is recorded separately from the original upstream tests and native Boolean
experiment. The native experiment does not certify the complete Python reasoner.
Source-level reasoning, tests and independent comparisons do not become a
kernel proof, behavioral equivalence, human agreement or execution authority.
Browser runtime requires separate qualification; Python packet replay alone is
not browser execution. Full application lowering and the full
2.7 release obligations remain open.
