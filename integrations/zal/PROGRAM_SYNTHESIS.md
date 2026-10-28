# Finite eligibility synthesis pilot

`program_synthesis.py` prepares a finite relation for the existing
`zeno-fcis synth` command. It is an opt-in pilot for a single fixed event's
Accept bit, not a complete application generator or a runtime authority.

The supported input is a checked ZAL2 machine with one bounded integer
variable, one Boolean fact explicitly bound as server-context authorization,
and no other context inputs, invariants, temporal properties, games, fairness,
or unresolved obligations. The fixed-event Cartesian domain has at most 32
inputs; the complete model has at most 256. The adapter refuses unsupported
shapes, model-check failures, ambiguous binding, and oversized graphs. This
pilot also refuses a fixed-event projection with no Accept or no Reject case;
those may be valid specifications, but require a different qualification.

The adapter enumerates the existing ZAL interpreter's decisions on every
fixed-event input. It maps control states to the factory's numeric identifiers,
keeps the integer variable and Boolean context in an explicit tuple order, and
records the full reference outcome beside the projected Accept bit. The
relation graph comes from accepted rows; a separate complete lookup graph
serves as a reference program. The bounded candidate grammar is an AND of
three interchangeable Boolean predicates chosen from all declared-domain
integer equalities, strict comparisons against thresholds above each domain's
minimum, the fact, its negation, and Boolean
constants. It supplies no selected predicate or handwritten candidate.

For the five-state, three-value, one-fact profile this grammar has 18 choices
per hole and 5,832 assignments. The declared ceilings are 6,000 assignments,
8,000,000 synthesis steps, and 256 nodes per graph. Other shapes may be
refused when their declared grammar or graph exceeds those bounds. A complete
search failure concerns only that grammar; a budget stop is inconclusive.

```sh
python3 -B integrations/zal/program_synthesis.py \
  --source MODEL.zal --binding BINDING.json --event EVENT --out NEW_INPUT_DIR

zeno-fcis --no-kb synth run NEW_INPUT_DIR/problem.json --target python \
  --out NEW_ARTIFACT_DIR --max-assignments 6000 --max-steps 8000000
zeno-fcis --no-kb synth verify NEW_INPUT_DIR/problem.json --target python \
  --out NEW_ARTIFACT_DIR --max-assignments 6000 --max-steps 8000000 \
  --receipt NEW_CONFORMANCE_RECEIPT.json
zeno-fcis --no-kb synth run NEW_INPUT_DIR/reference-problem.json --target python \
  --out NEW_REFERENCE_DIR --max-assignments 1 --max-steps 8000000
zeno-fcis --no-kb transform check --original NEW_REFERENCE_DIR/program.zcve \
  --candidate NEW_ARTIFACT_DIR/program.zcve --max-input-tuples 32 \
  --step-limit 256 --receipt NEW_EQUIVALENCE_RECEIPT.json
zeno-fcis --no-kb transform replay --original NEW_REFERENCE_DIR/program.zcve \
  --candidate NEW_ARTIFACT_DIR/program.zcve --max-input-tuples 32 \
  --receipt NEW_EQUIVALENCE_RECEIPT.json
```

`problem.json`, `reference-program.json`, `reference-problem.json`,
`table.json`, and `mapping.json`
bind the relation, lookup reference, table and source identities. The CLI's
selected `program.zcve`, target source, manifest, vectors, and target
conformance receipt must be retained separately. The reference JSON can be
admitted by the existing canonical program encoder or encoded through the
one-choice finite-synthesis reference shown above. Compare that resulting
program with the selected program using `transform check --receipt`, then
`transform replay`.
The deciding exhaustive check concerns only the declared fixed-event
projection. An unsupported encoding or inconclusive check is not a pass.

The mapping pins the adapter and its local interpreter sources, including
`symmetry2.py`, whose declaration parser and validator run even when the model
declares no symmetries. The isolated-process regression changes only a copied
dependency and checks that its recorded hash changes while computational
artifacts stay identical. These are local file provenance records; they do not
attest to execution on a hostile host.

The binding records the declared source of the authorization fact; it does not
authenticate it. UI fields in the binding are not consumed or validated by
this adapter. Application construction must still validate the complete
binding and use the ordinary catalog and Program/Authority path. The Boolean
projection omits other events, reason, complete next state and effects; it
cannot replace a full transition program. A future integration needs a
reviewed full-ABI adapter and complete correspondence, law and runtime checks.
