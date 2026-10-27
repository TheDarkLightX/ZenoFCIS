```json
{
  "name": "contract-probe",
  "title": "Ask an exact question about a finite contract",
  "synopsis": "zeno-fcis contract probe implies DIR --if FORMULA --then FORMULA",
  "commands": ["contract probe"],
  "engine": "Exhaustive finite contract observation through the library Authority and law evaluator.",
  "inputs": ["DIR/project.zeno and DIR/v2/policy.json; formulas use declared numeric field paths."],
  "outputs": ["zeno-fcis/contract-probe/2; result.answer, witnesses or counts; authority: none"],
  "exit_codes": {"0": "answered query or yes", "1": "no", "2": "vacuous or inconclusive", "3": "IO failure", "64": "usage", "65": "malformed judge input"},
  "verdicts": {"yes": "The queried proposition holds on the declared finite domain.", "no": "The proposition fails; retain the witness or exhaustive result.", "vacuous": "No qualifying tuples or pairs; no positive verdict.", "inconclusive": "The declared enumeration limit prevented an answer."},
  "limits": {"max_tuples_default": 1048576},
  "refusals": {"malformed": "Correct the formula or contract; no verdict was established."},
  "cost": {"scope": "Declared domain enumeration; depends also compares selected input pairs."},
  "use_when": ["check a contract implication", "find a reachable contract state", "test whether a decision depends on an input", "count contract decisions"],
  "not_for": ["runtime authorization", "unbounded domains", "interpreting a zero count as a refuted implication"],
  "steps": [{"command": "zeno-fcis contract probe implies examples/guide-counter --if true --then true", "exit": 0, "expected": {"result.answer": "yes"}}],
  "example": {"command": "zeno-fcis contract probe implies examples/guide-counter --if true --then false", "exit": 1, "expected": {"result.answer": "no", "authority": "none"}},
  "sources": {"engine": "crates/zeno-fcis-cli/src/contract/review/probe.rs", "exit_codes": "docs/CLI_REFERENCE.md#exit-codes"}
}
```

## Use when

Check implications, reachability or input dependence, or query exact counts.

## Not for

Runtime authorization or claims beyond the declared finite domain.

## Inputs

Use the contract directory and its declared numeric field paths in formulas.

## Steps

```sh
zeno-fcis contract probe implies examples/guide-counter --if true --then true
```

Expected: exit 0; `result.answer` is `yes`.

## Verdicts

For `implies`, `reachable` and `depends`, `yes` exits 0 and `no` exits 1.
Vacuous and inconclusive results exit 2. `explain`, `count` and `decisions`
exit 0 when answered; read the returned counts, including `undefined` and
`refused`. A `post.` value can be undefined on a rejected decision.

## Cost

Enumeration is bounded by `--max-tuples`; `depends` also compares input pairs.

## Coordinate

Retain the contract, command, packet and exit status together for replay.

## Pitfalls

An exit code grants no authority. A zero count is an answered query.

## Worked example

```sh
zeno-fcis contract probe implies examples/guide-counter --if true --then false
```

Expected: exit 1, with the first counterexample in `result.input`.

## See also

[Exit codes](../CLI_REFERENCE.md#exit-codes) define the shared convention.
Background: IEEE and The Open Group, *POSIX.1-2024*, Shell Command Language,
2024, defines shell exit status conventions. This interface assigns verdict
classes within those conventions; its scope is the reported finite query.
