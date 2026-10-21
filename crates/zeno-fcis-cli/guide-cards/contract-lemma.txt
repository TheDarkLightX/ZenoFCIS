```json
{
  "name": "contract-lemma",
  "title": "Judge invariants and transition relations",
  "synopsis": "zeno-fcis contract lemma DIR --lemma FORMULA [--transition]",
  "engine": "Finite initiation and consecution over library decisions; --transition checks actual before/after decisions.",
  "use_when": [
    "prove an invariant over a small domain",
    "find a counterexample trace",
    "check a transition relation"
  ],
  "not_for": [
    "unbounded parameter theorem",
    "an independently checked SMT proof"
  ],
  "inputs": [
    "Contract DIR as review; --lemma is a project state formula (post paths), repeatable; --transition permits pre, command, context and post. File caps: unknown."
  ],
  "outputs": [
    "stdout zeno-fcis/contract-lemma/2 or zeno-fcis/contract-transition-lemma/2: read status, lemmas, initiation and counterexample trace"
  ],
  "exit_codes": {
    "0": "requested result completed",
    "1": "invalid specification or refutation",
    "2": "blocked or inconclusive; no acceptance",
    "3": "filesystem or execution failure",
    "64": "invalid command-line usage",
    "65": "formula or contract has no contract form"
  },
  "verdicts": {
    "inductive": "Declared finite domain only; advisory, authority none. Preserved from genesis; use scenario to test intent.",
    "false": "Individual lemma verdict; a reachable witness refutes it. Top-level status is refuted; replay and revise.",
    "refuted": "At least one lemma is false; inspect lemmas and replay its trace.",
    "not-inductive": "True on reachable states but not closed over admitted states; strengthen lemma.",
    "checked": "Transition relation holds on the declared finite domain.",
    "inconclusive": "No invariant result; inspect bound.",
    "undefined": "Formula has no value; fix strict arithmetic.",
    "vacuous": "No qualifying transition; revise relation."
  },
  "limits": {
    "max_tuples_default": 1048576,
    "other_caps": "unknown"
  },
  "refusals": {
    "65": "Use the project formula grammar; run contract lemma --help."
  },
  "cost": {
    "measured_units": "unknown",
    "example_wall_ms": "unknown",
    "telemetry": "PENDING: populate from source-bound invocation telemetry"
  },
  "steps": [
    {
      "command": "zeno-fcis contract lemma examples/guide-counter --lemma 'post.100.110 <= 2'",
      "exit": 0,
      "expected": {
        "status": "inductive"
      },
      "contains": []
    }
  ],
  "example": {
    "command": "zeno-fcis contract lemma examples/guide-counter --lemma 'post.100.110 <= 1'",
    "exit": 1,
    "expected": {
      "status": "refuted",
      "lemmas.0.verdict": "false"
    },
    "contains": []
  },
  "sources": {
    "synopsis": "docs/CONTRACT_JUDGES.md:131-144; zeno-fcis contract lemma --help",
    "engine": "docs/CONTRACT_JUDGES.md:131-144; zeno-fcis contract lemma --help",
    "inputs": "docs/CONTRACT_JUDGES.md:131-144; zeno-fcis contract lemma --help",
    "outputs": "docs/CONTRACT_JUDGES.md:131-144; zeno-fcis contract lemma --help",
    "exit_codes": "docs/CONTRACT_JUDGES.md:131-144; zeno-fcis contract lemma --help",
    "verdicts": "docs/CONTRACT_JUDGES.md:131-144; zeno-fcis contract lemma --help",
    "limits": "docs/CONTRACT_JUDGES.md:131-144; zeno-fcis contract lemma --help",
    "refusals": "docs/CONTRACT_JUDGES.md:131-144; zeno-fcis contract lemma --help"
  },
  "commands": [
    "contract lemma"
  ],
  "pitfalls": "Read the result scope before revising the input. No report grants runtime or publication authority."
}
```

# Judge invariants and transition relations

## Use when

prove an invariant over a small domain; find a counterexample trace; check a transition relation.

## Not for

unbounded parameter theorem; an independently checked SMT proof.

## Inputs

Contract DIR as review; --lemma is a project state formula (post paths), repeatable; --transition permits pre, command, context and post. File caps: unknown.

Engine: Finite initiation and consecution over library decisions; --transition checks actual before/after decisions.

Synopsis: "zeno-fcis contract lemma DIR --lemma FORMULA [--transition]"

Set WORK to a fresh writable directory outside the source tree. Run commands from the repository root with zeno-fcis on PATH.

## Steps

```sh
zeno-fcis contract lemma examples/guide-counter --lemma 'post.100.110 <= 2'
```

Expected: exit 0; {"status": "inductive"}.

## Verdicts

`inductive`: Declared finite domain only; advisory, authority none. Preserved from genesis; use scenario to test intent.

`false`: Individual lemma verdict; a reachable witness refutes it. Top-level status is refuted; replay and revise.

`refuted`: At least one lemma is false; inspect lemmas and replay its trace.

`not-inductive`: True on reachable states but not closed over admitted states; strengthen lemma.

`checked`: Transition relation holds on the declared finite domain.

`inconclusive`: No invariant result; inspect bound.

`undefined`: Formula has no value; fix strict arithmetic.

`vacuous`: No qualifying transition; revise relation.

Exit codes: {"0": "requested result completed", "1": "invalid specification or refutation", "2": "blocked or inconclusive; no acceptance", "3": "filesystem or execution failure", "64": "invalid command-line usage", "65": "formula or contract has no contract form"}

Outputs: ["stdout zeno-fcis/contract-lemma/2 or zeno-fcis/contract-transition-lemma/2: read status, lemmas, initiation and counterexample trace"]

## Cost

{"example_wall_ms": "unknown", "measured_units": "unknown", "telemetry": "PENDING: populate from source-bound invocation telemetry"}

Limits: {"max_tuples_default": 1048576, "other_caps": "unknown"}

## Coordinate

Run Commons `advise` for the task first. Compute the exact subject digest, take an exclusive lease `loop:<subject digest>`, and respect existing leases. After checking, add a result record with the command, subject, verdict, scope and retained evidence; release the lease. Telemetry and advice grant no checker authority.

## Pitfalls

Read the result scope before revising the input. No report grants runtime or publication authority.

Refusals and remedies: {"65": "Use the project formula grammar; run contract lemma --help."}

## Worked example

```sh
zeno-fcis contract lemma examples/guide-counter --lemma 'post.100.110 <= 1'
```

Expected: exit 1; {"lemmas.0.verdict": "false", "status": "refuted"}.

## See also

Sources: {"engine": "docs/CONTRACT_JUDGES.md:131-144; zeno-fcis contract lemma --help", "exit_codes": "docs/CONTRACT_JUDGES.md:131-144; zeno-fcis contract lemma --help", "inputs": "docs/CONTRACT_JUDGES.md:131-144; zeno-fcis contract lemma --help", "limits": "docs/CONTRACT_JUDGES.md:131-144; zeno-fcis contract lemma --help", "outputs": "docs/CONTRACT_JUDGES.md:131-144; zeno-fcis contract lemma --help", "refusals": "docs/CONTRACT_JUDGES.md:131-144; zeno-fcis contract lemma --help", "synopsis": "docs/CONTRACT_JUDGES.md:131-144; zeno-fcis contract lemma --help", "verdicts": "docs/CONTRACT_JUDGES.md:131-144; zeno-fcis contract lemma --help"}

Run `zeno-fcis guide find "prove an invariant over a small domain"` for related cards.
