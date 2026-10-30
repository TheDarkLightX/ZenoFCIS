```json
{
  "name": "contract-draft",
  "title": "Draft a contract from intent, rules and labelled examples",
  "synopsis": "zeno-fcis contract draft start|propose|questions|label|check|finalize --session DIR ...",
  "engine": "A local specification conversation with explicit operations: start fixes intent, project and limits; propose runs generate contract and the library binding on a complete rules file; questions returns the review packet; label records expected decisions; check and finalize require every distinguishing input labelled and no failing comparison. No hosted model.",
  "use_when": [
    "draft a new contract from requirements",
    "examples-first contract drafting",
    "propose a complete rules file and get questions",
    "finalize a drafted contract directory"
  ],
  "not_for": [
    "rules with existing adoption or evolution histories (use the lineage workflows)",
    "automatic prose interpretation or labelling",
    "runtime publication or deployment approval"
  ],
  "inputs": [
    "start --session DIR --intent FILE --project FILE --provenance TEXT [--examples FILE] [--rounds 1..8, default 4] [--max-tuples 1..4096, default 4096]. propose --revision SHA256 --rules FILE --provenance TEXT (a complete v2/policy.json). label --revision SHA256 --examples FILE --provenance TEXT, lines `inputs | class reason poststate | deliveries`. questions|check --session DIR. finalize --revision SHA256 --out NEW_DIR (must not exist). At most 1024 examples and 1024 label submissions, 1 MiB per source or provenance file, 16 MiB transcript."
  ],
  "outputs": [
    "stdout zeno-fcis/contract-draft/1 with authority none and hosted_model off: status started, proposal-assessed, assessed, labels-recorded, finalization-refused, draft-finalized or refused; revision, attempts, rounds, max_tuples and detail (ready, questions, comparisons, review packet)"
  ],
  "exit_codes": {
    "0": "operation completed; for check, the draft is ready",
    "1": "check or finalize not ready, invalid rules, stale revision, exhausted rounds or a refused operation",
    "3": "filesystem failure (session, lock or output)",
    "64": "invalid command-line usage"
  },
  "verdicts": {
    "started": "Session created; use the returned revision for propose.",
    "proposal-assessed": "The assessment ran; detail.ready says whether finalize would succeed. Exit 0 here does not mean ready.",
    "assessed": "questions or check: read detail.questions and comparisons; check exits 1 unless ready.",
    "labels-recorded": "Labels bound to the revision; rechecked against every later proposal.",
    "finalization-refused": "Missing labels, stale revision or failing comparisons; repeat questions, label or propose.",
    "draft-finalized": "A new contract directory with project, rules, examples, generated files, intent, transcript and draft-finalized.json; review intent separately before zeno-fcis new.",
    "refused": "Read reason; a malformed proposal still consumes a round."
  },
  "limits": {
    "rounds": "1-8",
    "max_tuples": "1-4096",
    "examples": 1024,
    "label_submissions": 1024,
    "file_bytes": 1048576,
    "transcript_bytes": 16777216
  },
  "refusals": {
    "stale-revision": "Pass the revision from the latest response.",
    "draft-lock-present": "A crashed process left draft.lock; remove it only after checking that no operation owns it.",
    "output-exists": "finalize --out must name a path that does not exist, not even an empty directory."
  },
  "cost": {
    "measured_units": "unknown",
    "example_wall_ms": "unknown",
    "telemetry": "PENDING: populate from source-bound invocation telemetry"
  },
  "steps": [
    {
      "command": "zeno-fcis contract draft start --session $WORK/draft --intent crates/zeno-fcis-cli/tests/fixtures/contract-draft/durable-counter.intent.txt --project crates/zeno-fcis-cli/templates/durable-counter/project.zeno --examples crates/zeno-fcis-cli/templates/durable-counter/tests/decision-examples.txt --provenance \"prerecorded template examples; no live owner\" --rounds 2 | tee $WORK/start.json",
      "exit": 0,
      "expected": {
        "status": "started"
      },
      "contains": []
    }
  ],
  "example": {
    "command": "zeno-fcis contract draft propose --session $WORK/draft --revision \"$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))[\"revision\"])' $WORK/start.json)\" --rules crates/zeno-fcis-cli/tests/fixtures/contract-draft/durable-counter.rules.json --provenance \"complete fixture proposal from the retained prose description\"",
    "exit": 0,
    "expected": {
      "status": "proposal-assessed"
    },
    "contains": []
  },
  "sources": {
    "synopsis": "docs/CLI_REFERENCE.md:2767-2843; docs/CLI_REFERENCE.md:34-38; crates/zeno-fcis-cli/tests/contract_draft.rs",
    "engine": "docs/CLI_REFERENCE.md:2767-2843; docs/CLI_REFERENCE.md:34-38; crates/zeno-fcis-cli/tests/contract_draft.rs",
    "inputs": "docs/CLI_REFERENCE.md:2767-2843; docs/CLI_REFERENCE.md:34-38; crates/zeno-fcis-cli/tests/contract_draft.rs",
    "outputs": "docs/CLI_REFERENCE.md:2767-2843; docs/CLI_REFERENCE.md:34-38; crates/zeno-fcis-cli/tests/contract_draft.rs",
    "exit_codes": "docs/CLI_REFERENCE.md:2767-2843; docs/CLI_REFERENCE.md:34-38; crates/zeno-fcis-cli/tests/contract_draft.rs",
    "verdicts": "docs/CLI_REFERENCE.md:2767-2843; docs/CLI_REFERENCE.md:34-38; crates/zeno-fcis-cli/tests/contract_draft.rs",
    "limits": "docs/CLI_REFERENCE.md:2767-2843; docs/CLI_REFERENCE.md:34-38; crates/zeno-fcis-cli/tests/contract_draft.rs",
    "refusals": "docs/CLI_REFERENCE.md:2767-2843; docs/CLI_REFERENCE.md:34-38; crates/zeno-fcis-cli/tests/contract_draft.rs"
  },
  "commands": [
    "contract draft"
  ],
  "pitfalls": "A proposed answer is not a label. Labels are never retracted; contradictory labels need a new session. Neither the parser nor claimed provenance authenticates an owner. Read the result scope before revising the input. No report grants runtime or publication authority."
}
```

# Draft a contract from intent, rules and labelled examples

## Use when

draft a new contract from requirements; examples-first contract drafting; propose a complete rules file and get questions; finalize a drafted contract directory.

## Not for

rules with existing adoption or evolution histories (use the lineage workflows); automatic prose interpretation or labelling; runtime publication or deployment approval.

## Inputs

start --session DIR --intent FILE --project FILE --provenance TEXT [--examples FILE] [--rounds 1..8, default 4] [--max-tuples 1..4096, default 4096]. propose --revision SHA256 --rules FILE --provenance TEXT (a complete v2/policy.json). label --revision SHA256 --examples FILE --provenance TEXT, lines `inputs | class reason poststate | deliveries`. questions|check --session DIR. finalize --revision SHA256 --out NEW_DIR (must not exist). At most 1024 examples and 1024 label submissions, 1 MiB per source or provenance file, 16 MiB transcript.

Engine: A local specification conversation with explicit operations: start fixes intent, project and limits; propose runs generate contract and the library binding on a complete rules file; questions returns the review packet; label records expected decisions; check and finalize require every distinguishing input labelled and no failing comparison. No hosted model.

Synopsis: "zeno-fcis contract draft start|propose|questions|label|check|finalize --session DIR ..."

Set WORK to a fresh writable directory outside the source tree. Run commands from the repository root with zeno-fcis on PATH.

## Steps

```sh
zeno-fcis contract draft start --session $WORK/draft --intent crates/zeno-fcis-cli/tests/fixtures/contract-draft/durable-counter.intent.txt --project crates/zeno-fcis-cli/templates/durable-counter/project.zeno --examples crates/zeno-fcis-cli/templates/durable-counter/tests/decision-examples.txt --provenance "prerecorded template examples; no live owner" --rounds 2 | tee $WORK/start.json
```

Expected: exit 0; {"status": "started"}.

## Verdicts

`started`: Session created; use the returned revision for propose.

`proposal-assessed`: The assessment ran; detail.ready says whether finalize would succeed. Exit 0 here does not mean ready.

`assessed`: questions or check: read detail.questions and comparisons; check exits 1 unless ready.

`labels-recorded`: Labels bound to the revision; rechecked against every later proposal.

`finalization-refused`: Missing labels, stale revision or failing comparisons; repeat questions, label or propose.

`draft-finalized`: A new contract directory with project, rules, examples, generated files, intent, transcript and draft-finalized.json; review intent separately before zeno-fcis new.

`refused`: Read reason; a malformed proposal still consumes a round.

Exit codes: {"0": "operation completed; for check, the draft is ready", "1": "check or finalize not ready, invalid rules, stale revision, exhausted rounds or a refused operation", "3": "filesystem failure (session, lock or output)", "64": "invalid command-line usage"}

Outputs: ["stdout zeno-fcis/contract-draft/1 with authority none and hosted_model off: status started, proposal-assessed, assessed, labels-recorded, finalization-refused, draft-finalized or refused; revision, attempts, rounds, max_tuples and detail (ready, questions, comparisons, review packet)"]

## Cost

{"example_wall_ms": "unknown", "measured_units": "unknown", "telemetry": "PENDING: populate from source-bound invocation telemetry"}

Limits: {"examples": 1024, "file_bytes": 1048576, "label_submissions": 1024, "max_tuples": "1-4096", "rounds": "1-8", "transcript_bytes": 16777216}

## Coordinate

Run Commons `advise` for the task first. Compute the exact subject digest, take an exclusive lease `loop:<subject digest>`, and respect existing leases. After checking, add a result record with the command, subject, verdict, scope and retained evidence; release the lease. Telemetry and advice grant no checker authority.

## Pitfalls

A proposed answer is not a label. Labels are never retracted; contradictory labels need a new session. Neither the parser nor claimed provenance authenticates an owner. Read the result scope before revising the input. No report grants runtime or publication authority.

Refusals and remedies: {"draft-lock-present": "A crashed process left draft.lock; remove it only after checking that no operation owns it.", "output-exists": "finalize --out must name a path that does not exist, not even an empty directory.", "stale-revision": "Pass the revision from the latest response."}

## Worked example

```sh
zeno-fcis contract draft propose --session $WORK/draft --revision "$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["revision"])' $WORK/start.json)" --rules crates/zeno-fcis-cli/tests/fixtures/contract-draft/durable-counter.rules.json --provenance "complete fixture proposal from the retained prose description"
```

Expected: exit 0; {"status": "proposal-assessed"}.

## See also

Sources: {"engine": "docs/CLI_REFERENCE.md:2767-2843; docs/CLI_REFERENCE.md:34-38; crates/zeno-fcis-cli/tests/contract_draft.rs", "exit_codes": "docs/CLI_REFERENCE.md:2767-2843; docs/CLI_REFERENCE.md:34-38; crates/zeno-fcis-cli/tests/contract_draft.rs", "inputs": "docs/CLI_REFERENCE.md:2767-2843; docs/CLI_REFERENCE.md:34-38; crates/zeno-fcis-cli/tests/contract_draft.rs", "limits": "docs/CLI_REFERENCE.md:2767-2843; docs/CLI_REFERENCE.md:34-38; crates/zeno-fcis-cli/tests/contract_draft.rs", "outputs": "docs/CLI_REFERENCE.md:2767-2843; docs/CLI_REFERENCE.md:34-38; crates/zeno-fcis-cli/tests/contract_draft.rs", "refusals": "docs/CLI_REFERENCE.md:2767-2843; docs/CLI_REFERENCE.md:34-38; crates/zeno-fcis-cli/tests/contract_draft.rs", "synopsis": "docs/CLI_REFERENCE.md:2767-2843; docs/CLI_REFERENCE.md:34-38; crates/zeno-fcis-cli/tests/contract_draft.rs", "verdicts": "docs/CLI_REFERENCE.md:2767-2843; docs/CLI_REFERENCE.md:34-38; crates/zeno-fcis-cli/tests/contract_draft.rs"}

Run `zeno-fcis guide find "draft a new contract from requirements"` for related cards.
