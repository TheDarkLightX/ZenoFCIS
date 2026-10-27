```json
{
  "name": "knowledge",
  "title": "Consult scoped usage advice",
  "synopsis": "zeno-fcis kb advise --task FACTS",
  "engine": "Finite structured queries plus replay-bound held-out ON/OFF assessment; independent checkers retain program acceptance authority.",
  "use_when": [
    "choose a checker from usage advice",
    "inspect scoped knowledge",
    "find a supported loop plan",
    "assess a knowledge candidate"
  ],
  "not_for": [
    "proof from retrieved advice",
    "population-wide usefulness from the public regression suite"
  ],
  "inputs": [
    "FACTS: structured task predicates. Assessment needs an explicit frozen --suite and one private --kb layer; see docs/KNOWLEDGE.md for the checked journey."
  ],
  "outputs": [
    "stdout zeno-fcis/usage-knowledge/1 for advise; inspect status, provenance and authority; read-only advise writes no application files."
  ],
  "exit_codes": {
    "0": "requested result completed",
    "1": "invalid specification or refutation",
    "2": "blocked or inconclusive; no acceptance",
    "3": "filesystem or execution failure",
    "64": "invalid command-line usage"
  },
  "verdicts": {
    "complete": "Advice/query finished; run the recommended exact checker.",
    "inconclusive": "Bound or no plan; supply missing facts or choose a supported plan.",
    "refused": "Repair layer integrity or input syntax; inspect kb --help."
  },
  "limits": {
    "all_defaults_and_caps": "unknown"
  },
  "refusals": {
    "assessment-resource-stop": "Retain measured and reserved work and the inconclusive record; inspect the reported resource cause before another bounded attempt."
  },
  "cost": {
    "measured_units": "unknown",
    "example_wall_ms": "unknown",
    "telemetry": "PENDING: populate from source-bound invocation telemetry"
  },
  "steps": [
    {
      "command": "zeno-fcis kb advise --task 'task(subject,check-program).'",
      "exit": 0,
      "expected": {},
      "contains": []
    },
    {
      "command": "zeno-fcis kb assess --help",
      "exit": 0,
      "expected": {},
      "contains": [
        "--suite",
        "--candidate"
      ]
    }
  ],
  "example": {
    "command": "zeno-fcis kb assess --help",
    "exit": 0,
    "expected": {},
    "contains": [
      "--suite",
      "--candidate"
    ]
  },
  "sources": {
    "synopsis": "docs/KNOWLEDGE.md; zeno-fcis kb --help",
    "engine": "docs/KNOWLEDGE.md; zeno-fcis kb --help",
    "inputs": "docs/KNOWLEDGE.md; zeno-fcis kb --help",
    "outputs": "docs/KNOWLEDGE.md; zeno-fcis kb --help",
    "exit_codes": "docs/KNOWLEDGE.md; zeno-fcis kb --help",
    "verdicts": "docs/KNOWLEDGE.md; zeno-fcis kb --help",
    "limits": "docs/KNOWLEDGE.md; zeno-fcis kb --help",
    "refusals": "docs/KNOWLEDGE.md; zeno-fcis kb --help"
  },
  "commands": [
    "kb",
    "kb advise",
    "kb plan",
    "kb replan",
    "kb explain",
    "kb check",
    "kb query",
    "kb replay",
    "kb assess",
    "kb assessment-queue",
    "kb leaderboard"
  ],
  "pitfalls": "Read the result scope before revising the input. No report grants runtime or publication authority."
}
```

# Consult scoped usage advice

## Use when

choose a checker from usage advice; inspect scoped knowledge; find a supported loop plan; assess a knowledge candidate.

## Not for

proof from retrieved advice; population-wide usefulness from the public regression suite.

## Inputs

FACTS: structured task predicates. Assessment needs an explicit frozen --suite and one private --kb layer; see docs/KNOWLEDGE.md for the checked journey.

Engine: Finite structured queries plus replay-bound held-out ON/OFF assessment; independent checkers retain program acceptance authority.

Synopsis: "zeno-fcis kb advise --task FACTS"

Set WORK to a fresh writable directory outside the source tree. Run commands from the repository root with zeno-fcis on PATH.

## Steps

```sh
zeno-fcis kb advise --task 'task(subject,check-program).'
```

Expected: exit 0; [].

```sh
zeno-fcis kb assess --help
```

Expected: exit 0; ["--suite", "--candidate"].

## Verdicts

`complete`: Advice/query finished; run the recommended exact checker.

`inconclusive`: Bound or no plan; supply missing facts or choose a supported plan.

`refused`: Repair layer integrity or input syntax; inspect kb --help.

Exit codes: {"0": "requested result completed", "1": "invalid specification or refutation", "2": "blocked or inconclusive; no acceptance", "3": "filesystem or execution failure", "64": "invalid command-line usage"}

Outputs: ["stdout zeno-fcis/usage-knowledge/1 for advise; inspect status, provenance and authority; read-only advise writes no application files."]

## Cost

{"example_wall_ms": "unknown", "measured_units": "unknown", "telemetry": "PENDING: populate from source-bound invocation telemetry"}

Limits: {"all_defaults_and_caps": "unknown"}

## Coordinate

Run Commons `advise` for the task first. Compute the exact subject digest, take an exclusive lease `loop:<subject digest>`, and respect existing leases. After checking, add a result record with the command, subject, verdict, scope and retained evidence; release the lease. Telemetry and advice grant no checker authority.

## Pitfalls

Read the result scope before revising the input. No report grants runtime or publication authority.

Refusals and remedies: {"assessment-resource-stop": "Retain measured and reserved work and the inconclusive record; inspect the reported resource cause before another bounded attempt."}

## Worked example

```sh
zeno-fcis kb assess --help
```

Expected: exit 0; ["--suite", "--candidate"].

## See also

**Background.** Jon Doyle, “A Truth Maintenance System,” Artificial
Intelligence 12(3), 1979, describes dependency maintenance. Maarten H. van
Emden and Robert A. Kowalski, “The Semantics of Predicate Logic as a
Programming Language,” Journal of the ACM 23(4), 1976, is background for
logical fixed points.

**What is new here.** The engine adds source-bound held-out strategy
assessment and independently replayed exact facts. The comparative
assessment is **prior-art search pending**.

**Scope.** The help example discovers an available command and performs no
assessment. Use the explicit private-layer journey for measured evidence;
advice and descriptive points supply no program acceptance authority.


Sources: {"engine": "docs/KNOWLEDGE.md; zeno-fcis kb --help", "exit_codes": "docs/KNOWLEDGE.md; zeno-fcis kb --help", "inputs": "docs/KNOWLEDGE.md; zeno-fcis kb --help", "limits": "docs/KNOWLEDGE.md; zeno-fcis kb --help", "outputs": "docs/KNOWLEDGE.md; zeno-fcis kb --help", "refusals": "docs/KNOWLEDGE.md; zeno-fcis kb --help", "synopsis": "docs/KNOWLEDGE.md; zeno-fcis kb --help", "verdicts": "docs/KNOWLEDGE.md; zeno-fcis kb --help"}

Run `zeno-fcis guide find "choose a checker from usage advice"` for related cards.
