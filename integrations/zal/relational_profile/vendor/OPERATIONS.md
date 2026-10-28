# Public operations and replay

`python3 run.py REQUEST.json` selects the operation from its schema. Use `-` for
stdin. Requests reject duplicate keys and unknown fields. Naturals use canonical
decimal strings. Rationals must be reduced and canonically spelled.

## Count

Schema: `zal/relational-query/1`. Exact fields:

```text
schema, logic, signature, formula, operation, domain_size, weights, context_id
```

`operation` is `count`; `logic` is `zal/c2/1` or `zal/c2mod/1`. Each signature entry
has `id` and integer `arity` (1 or 2). Every predicate has a `[true,false]` rational
weight pair. See [the JSON schema](schemas/relational-query.schema.json) and the
two `examples/*-count.json` files. `ExactCount` reports a completed algorithm
result with its domain; it is not a proof certificate or normalized probability.

## Evaluate a model

Schema: `zal/relational-evaluation/1`. Exact fields:

```text
schema, logic, signature, formula, domain_size, context_id, model
```

Formula syntax is shared with counting. There are no weights. The model provides
every predicate as a list of true tuples, with entity indices represented as
natural strings. For example, at size 3:

```json
{
  "Owner": [["0"]],
  "Car": [["1"], ["2"]],
  "Has": [["0", "1"], ["0", "2"]]
}
```

Other in-domain atoms are false in this explicitly complete model. The format
does not silently apply closed-world assumptions to an informal story. Missing
predicates, duplicate tuples, incorrect arity or out-of-domain entities reject.
`Evaluated` returns a Boolean; false is a valid completed evaluation.

## Compare meanings

Schema: `zal/relational-comparison/1`. Exact fields:

```text
schema, logic, signature, domain_size, context_id,
assumptions, left, right, relation
```

The three formula fields must be closed in the same vocabulary. `relation` is
`equivalence` or `implication`, with implication directed from left to right.
Weights are intentionally absent. The four partition counts are:

| Field | Left | Right |
|---|---|---|
| `both_true` | true | true |
| `left_only` | true | false |
| `right_only` | false | true |
| `both_false` | false | false |

Only models satisfying the assumptions enter the partition. `EquivalentAt` or
`ImpliesAt` reports nonvacuous agreement at the stated size. `RefutedAt` returns
a complete counterexample, the true assumption and each original formula's
Boolean result. `VacuousAt` reports no admissible models, with `holds: null`.
This is not all-size equivalence, trace refinement or executable-code correctness.

`--max-models` sets the comparison/Boolean-assignment cap from 1 to 1,000,000.
The default work limit is 10,000,000. Comparison refuses before enumeration when
the complete structure space or conservative work estimate exceeds the budget.
No smaller domain is substituted. A structured count can succeed even when the
complete-enumeration comparison is outside budget.

## Status and evidence

Exit 0 means completed count/evaluation or a nonvacuous relation success. Exit 1
means a relation refutation. Exit 2 means vacuity, invalid/unsupported input or
incomplete execution. Read the JSON status as well as the exit code.

Resource exhaustion, invalid input and internal disagreement contain no complete
count or partition. Evaluation/comparison responses bind the formal request,
context, implementation, shell and limits. The shell independently checks
coverage totals, partition consistency, relation direction and witness shape.
Independent original-formula tests also evaluate serialized counterexamples.

## Discussion and replay

The [README example](README.md#discuss-two-meanings) preserves the original
readings and their CNL lowering alongside the formal request/result. The
`ladder_adapter.py` options `--compare-to`, `--compare-rung`, `--assuming`,
`--assumption-rung` and `--relation` select this workflow. Omitting assumptions
means true, and that choice is recorded explicitly.

`replay_result.py RECORD.json` re-executes the formal request and compares the
exact semantic result, including its retained counterexample. It reports whether
the implementation binary matches separately. A changed request binding rejects;
a changed semantic result is a mismatch. Replay does not establish authorship,
human approval or correspondence of arbitrary prose to the formula. Inspect the
retained context/grammar/adapter bindings when assessing the lowering step.
