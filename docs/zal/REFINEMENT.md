# From a ZAL statement to a checked contract

Ask: **Approve an order only when the supplied authorization fact is true.**

The [order statement](../../integrations/zal/examples/order.zal) declares four
states, three events and one Boolean observation. Its `zal 1` header selects
the finite-machine theory `zal/finite-fsm/1`. In its English view, the relevant
rule reads:

> Rule approve_order: when the state is pending and the event is approve, if authorized, move to approved.

The explicit default rejects every input for which no rule applies, keeping
the state unchanged. Overlapping rules are refused; their written order gives
no priority. `authorized` is an observation supplied by the caller. This
statement does not authenticate the caller or establish where that fact came from.

Choose the view you prefer, export the contract, then check it:

```sh
python3 integrations/zal/run.py render integrations/zal/examples/order.zal --to english
python3 integrations/zal/run.py check integrations/zal/examples/order.zal
python3 integrations/zal/run.py export integrations/zal/examples/order.zal --out order-contract
python3 integrations/zal/run.py qualify integrations/zal/examples/order.zal --out order-check.json
```

`qualify` needs a built `zeno-fcis` CLI; its `--cli` option selects that binary.
It runs the normal contract generator, contract review and generated application
tests. The exported `project.zeno` declares types and laws; `v2/policy.json`
contains the synthesized transition policy. The source map relates the numeric
identifiers in that contract to the ZAL names. ZAL states the intended behavior;
the implementation may use its own structure, subject to the chosen check.

For a requirement stated as a formula, use the [progression ladder](ladder/README.md).
Its formula, English readings and question share a declared theory and vocabulary.
Rendering a requirement does not automatically produce a contract that satisfies it.
The finite machine and the formula ladder are separate, explicit profiles.

## The relation being checked

Let Z be the finite ZAL model, C the generated contract, E the numeric encoding
in `zal-source-map.json`, and D the reference input domain. An input is a
state, event and complete observation record; profile 2 also has bounded integer
state and input values. The independent generated-application check requires:

1. `genesis(C) = E(initial(Z))`, including every declared state variable.
2. For every `u` in D, `observe(Authority(C, E(u))) = E(decide(Z, u))`.
3. The reference file is exactly the table computed from Z, in canonical order;
   removing, repeating or changing a row is refused before decisions are checked.

Here `observe` includes decision class, reason, complete successor state and
the empty delivery list required by these machine profiles. Reject preserves
the entire pre-state. D includes unreachable inputs: profile 1 uses the whole
declared product; profile 2 uses every input whose pre-state satisfies its
declared invariants. Profile 2 records both the declared and admissible counts.
Genesis and commit laws keep held states within those invariants. A failed
reference comparison prints the input tuple and the differing outcome.

The table is computed by the ZAL interpreter, separately from policy generation;
the native Authority executes the contract. The generated Rust test binds both
the initial values and the exact reference text. It is executable finite
comparison evidence, not a proof about arbitrary generated code. Changing both
the test and its expected values would change the subject being reviewed.

This strengthening adds checks; it does not change ZAL transitions, numeric
identifiers, rejection order or existing domain/resource bounds. Generated test
bytes and checker identities intentionally change. Bound machine indexes are
immutable: callers cannot change decision behavior while retaining a model revision.
Per-call input copies and returned records are separate from that policy.

## A different concrete representation

The native `contract refine-check` checks two contracts and an explicit state
map `rho: S_C -> S_A + invalid`. Its [reservation example](../../examples/contract-refinement/reservation/abstraction.json)
uses different concrete and abstract representations. The map covers every
declared concrete state and every declared abstract state has a preimage.
Genesis must be admitted on both sides and related by rho. Command and context
domains must match. For every concrete state and every input:

- A mapped state must yield the same class and reason, related successors,
  and identical ordered effects and deliveries, including their identities.
- An invalid state must refuse with the designated invariant law and preserve
  its state. Ordinary rejections on either side must preserve their state too.
- Unknown or exhausted evaluation is inconclusive, never a refinement pass.

The native checker pins both projects, both policies and the map, retains work
counts, and reports the first finite mismatch. Its limits include 4,096 states,
64 inputs and 262,144 rows; requested budgets can stop earlier. This relation
is stronger than the ZAL table check in its representation and effect scope.
It still does not equate running time, resource usage, publication identities,
authentication or deployment authority. The map must be supplied and checked;
similar names alone do not establish a refinement.

## Replay the refusal checks

With `ZAL_NATIVE_CLI` set to the built CLI, run:

```sh
python3 -m unittest discover -s integrations/zal -p test_refinement_binding.py -v
```

The controls generate and compile ordinary contracts before running the named
`zal_whole_declared_domain_matches_authority` test. A changed initial state,
an approval without authorization, a missing reference row and a repeated row
must each fail that test. The unchanged order models in both profiles pass.
These checks exercise the finite declared examples; they are not a security
claim about facts outside the supplied model.

## Lineage and scope

**Background.** Niklaus Wirth, “Program Development by Stepwise Refinement,”
*Communications of the ACM* 14(4), 1971, describes development through successive
refinements. Matthew B. Dwyer, George S. Avrunin and James C. Corbett, “Patterns
in Property Specifications for Finite-State Verification,” *ICSE*, 1999,
organizes recurring temporal requirements as patterns. Tobias Kuhn, “A Survey
and Classification of Controlled Natural Languages,” *ACM Computing Surveys*
46(3), article 41, 2014, surveys constrained natural-language systems.

**What is new here.** This integration binds ZAL initial values and finite
reference decisions to the generated application test. Prior-art search pending.

**Scope.** Exhaustive comparison applies only to the declared finite input
domain and the observations listed above. Temporal fairness and liveness need
their own execution checks; they are not established by a one-step comparison.
