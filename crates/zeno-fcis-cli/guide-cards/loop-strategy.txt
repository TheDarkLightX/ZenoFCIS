```json
{
  "name": "loop-strategy",
  "title": "Select bounded strategies or inspect structural usage facts",
  "synopsis": "zeno-fcis loop strategy (--tasks JSON | --kind KIND --problem JSON | --kb-op OP)",
  "engine": "Task-mode development selection freezes a stage sequence before evaluation. Named modes use typed bounded plans. Knowledge mode performs structural inference only.",
  "use_when": [
    "select a sequence of proposal stages",
    "diagnose finite faults",
    "query strategy knowledge"
  ],
  "not_for": [
    "ranking as program acceptance",
    "evaluation changing the development winner",
    "knowledge facts as execution authority"
  ],
  "inputs": [
    "Choose one mode. --tasks is zeno-fcis/loop-strategy/1 with exactly one development and one evaluation entry, each a relative original path and inline hash-bound pipeline proposal. References must have distinct bytes; runtime --depth must be 1 (five candidate sequences).",
    "--kind split-by-cases/diagnose/ask-when-it-matters uses zeno-fcis/loop-strategy-problem/1. Split problem fields are schema/learn/config; diagnose uses schema/circuit/vector/observations/max_faults; ask uses schema/background/readings/query. Split requires --original; complete reference capture, transform check and replay qualify its result. Diagnose/ask only propose explanations, probes or answers.",
    "--kb-op query/check/plan/advise/explain uses structural usage facts; query/explain take --query, plan takes --have/--want. Global --kb chooses at most seven regular external layers outside the checkout. Total source text, including the compiled example, must fit 65,536 bytes. --out applies to task/named modes and requires a new directory. All files <=65,536 bytes; no session or attempts."
  ],
  "outputs": [
    "Task/named stdout zeno-fcis/loop-doors/2, door=strategy. detail includes selection/evaluation or typed plan/trace and the relevant status, checks, work/limits. Knowledge query/check/plan/explain normally use loop-doors/2; advice and no-plan results can use zeno-fcis/usage-knowledge/1. authority=none.",
    "Where integer_encoding is present, scalar report integers use safe-number-or-decimal-string/1. Program JSON bounds/literals remain decimal strings; node references remain JSON integers."
  ],
  "exit_codes": {
    "0": "Completed report; read the operation-specific verdict and scope.",
    "1": "Refused input or refuted proposal, except the explicitly documented aggregate cases.",
    "2": "Inconclusive, incomplete or unavailable containment; no new acceptance.",
    "3": "I/O failure; see operation-specific exceptions.",
    "64": "Invalid command-line usage."
  },
  "verdicts": {
    "evaluated": "Task exit 0 can include unqualified evaluation tasks; inspect each task check. The development winner remains frozen.",
    "qualified/refuted": "Split-by-cases exits 0/1 after capture and original-bound check/replay.",
    "question/proposed-diagnosis/proposed-answer/proposed-ambiguous": "Named advisory outcomes exit 0 and qualify no program.",
    "proposed-query/consistent/proposed-plan/advised": "Completed structural knowledge results exit 0; read rows, problems, steps or advice. None qualifies a program.",
    "inconsistent": "Structural knowledge check exits 1 when it reports problems.",
    "inconclusive/refused": "Exit 2/1 respectively, with no new acceptance. A bounded knowledge no-plan result is inconclusive with reason kb-bounded-no-plan."
  },
  "limits": {
    "file_bytes": 65536,
    "program_inputs_cap": 32,
    "program_outputs_cap": 16,
    "program_nodes_cap": 256,
    "max_tuples": {
      "default": 131072,
      "cap": 131072
    },
    "max_gate_pairs": {
      "default": 1000000,
      "cap": 4000000
    },
    "max_search_work_per_stage": {
      "default": 50000000,
      "cap": 100000000
    },
    "max_cells": {
      "default": 4194304,
      "cap": 16777216
    },
    "max_assignments": {
      "default": 2000,
      "cap": 20000
    },
    "max_terms": {
      "default": 128,
      "cap": 512
    },
    "max_checks": {
      "default": 24,
      "cap": 30
    },
    "deadline_ms": {
      "default": 22000,
      "cap": 22000
    },
    "invocation_control_frames": 62,
    "runtime_depth": 1,
    "development_tasks": 1,
    "evaluation_tasks": 1,
    "knowledge_steps_cap": 16,
    "knowledge_steps_default": 8,
    "knowledge_states_cap": 4096,
    "knowledge_layers_cap": 7,
    "knowledge_total_source_bytes_cap": 65536,
    "knowledge_rows_cap": 8192,
    "knowledge_bindings_cap": 4096,
    "knowledge_work_cap": 2000000
  },
  "refusals": {
    "strategy-runtime-depth-must-be-one": "Use --depth 1.",
    "strategy-subject-overlap-or-duplicate": "Use distinct development/evaluation references.",
    "output-must-be-new-directory": "Choose a new output directory."
  },
  "cost": {
    "units": "Read reported work reservations and scope; these are not total CPU-time measurements.",
    "timing": "No cross-host timing claim."
  },
  "steps": [],
  "example": {
    "command": "zeno-fcis loop strategy --kb-op query --query 'strategy(S)'",
    "exit": 0,
    "expected": {
      "schema": "zeno-fcis/loop-doors/2",
      "status": "proposed-query"
    },
    "contains": []
  },
  "sources": {
    "synopsis": "crates/zeno-fcis-cli/src/loop_strategy.rs; crates/zeno-fcis-cli/src/loop_strategy_profiles.rs; crates/zeno-fcis-cli/src/loop_strategy_knowledge.rs",
    "engine": "crates/zeno-fcis-cli/src/loop_strategy.rs; crates/zeno-fcis-cli/src/loop_strategy_profiles.rs; crates/zeno-fcis-cli/src/loop_strategy_knowledge.rs",
    "inputs": "crates/zeno-fcis-cli/src/loop_strategy.rs; crates/zeno-fcis-cli/src/loop_strategy_profiles.rs; crates/zeno-fcis-cli/src/loop_strategy_knowledge.rs",
    "outputs": "crates/zeno-fcis-cli/src/loop_strategy.rs; crates/zeno-fcis-cli/src/loop_strategy_profiles.rs; crates/zeno-fcis-cli/src/loop_strategy_knowledge.rs; crates/zeno-fcis-cli/src/knowledge_command.rs",
    "exit_codes": "crates/zeno-fcis-cli/src/loop_strategy.rs; crates/zeno-fcis-cli/src/loop_strategy_profiles.rs; crates/zeno-fcis-cli/src/loop_strategy_knowledge.rs; crates/zeno-fcis-cli/src/knowledge_command.rs",
    "verdicts": "crates/zeno-fcis-cli/src/loop_strategy.rs; crates/zeno-fcis-cli/src/loop_strategy_profiles.rs; crates/zeno-fcis-cli/src/loop_strategy_knowledge.rs; crates/zeno-fcis-cli/src/knowledge_command.rs",
    "limits": "crates/zeno-fcis-cli/src/loop_strategy.rs; crates/zeno-fcis-cli/src/loop_strategy_profiles.rs; crates/zeno-fcis-cli/src/loop_strategy_knowledge.rs",
    "refusals": "crates/zeno-fcis-cli/src/loop_strategy.rs; crates/zeno-fcis-cli/src/loop_strategy_profiles.rs; crates/zeno-fcis-cli/src/loop_strategy_knowledge.rs"
  },
  "commands": [
    "loop strategy"
  ],
  "pitfalls": "Selection ranks proposals already checked at their own gates. Structural facts, typed traces, diagnosis and proposed answers confer no program acceptance. Task exit 0 does not promise every evaluation task qualified."
}
```

# Select bounded strategies or inspect structural usage facts

## Use when

select a sequence of proposal stages; diagnose finite faults; query strategy knowledge.

## Not for

ranking as program acceptance; evaluation changing the development winner; knowledge facts as execution authority.

## Inputs

Choose one mode. --tasks is zeno-fcis/loop-strategy/1 with exactly one development and one evaluation entry, each a relative original path and inline hash-bound pipeline proposal. References must have distinct bytes; runtime --depth must be 1 (five candidate sequences).

--kind split-by-cases/diagnose/ask-when-it-matters uses zeno-fcis/loop-strategy-problem/1. Split problem fields are schema/learn/config; diagnose uses schema/circuit/vector/observations/max_faults; ask uses schema/background/readings/query. Split requires --original; complete reference capture, transform check and replay qualify its result. Diagnose/ask only propose explanations, probes or answers.

--kb-op query/check/plan/advise/explain uses structural usage facts; query/explain take --query, plan takes --have/--want. Global --kb chooses at most seven regular external layers outside the checkout. Total source text, including the compiled example, must fit 65,536 bytes. --out applies to task/named modes and requires a new directory. All files <=65,536 bytes; no session or attempts.

Engine: Task-mode development selection freezes a stage sequence before evaluation. Named modes use typed bounded plans. Knowledge mode performs structural inference only.

Synopsis: `zeno-fcis loop strategy (--tasks JSON | --kind KIND --problem JSON | --kb-op OP)`

Run from the repository root with zeno-fcis on PATH. Set WORK to a fresh writable scratch directory outside the source tree.



## Steps

## Verdicts

`evaluated`: Task exit 0 can include unqualified evaluation tasks; inspect each task check. The development winner remains frozen.

`qualified/refuted`: Split-by-cases exits 0/1 after capture and original-bound check/replay.

`question/proposed-diagnosis/proposed-answer/proposed-ambiguous`: Named advisory outcomes exit 0 and qualify no program.

`proposed-query/consistent/proposed-plan/advised`: Completed structural knowledge results exit 0; read rows, problems, steps or advice. None qualifies a program.

`inconsistent`: Structural knowledge check exits 1 when it reports problems.

`inconclusive/refused`: Exit 2/1 respectively, with no new acceptance. A bounded knowledge no-plan result is inconclusive with reason kb-bounded-no-plan.

Exit codes: {"0": "Completed report; read the operation-specific verdict and scope.", "1": "Refused input or refuted proposal, except the explicitly documented aggregate cases.", "2": "Inconclusive, incomplete or unavailable containment; no new acceptance.", "3": "I/O failure; see operation-specific exceptions.", "64": "Invalid command-line usage."}

Outputs: Task/named stdout zeno-fcis/loop-doors/2, door=strategy. detail includes selection/evaluation or typed plan/trace and the relevant status, checks, work/limits. Knowledge query/check/plan/explain normally use loop-doors/2; advice and no-plan results can use zeno-fcis/usage-knowledge/1. authority=none.

## Cost

Read reported work reservations and scope; these are not total CPU-time measurements.

Limits: {"deadline_ms": {"cap": 22000, "default": 22000}, "development_tasks": 1, "evaluation_tasks": 1, "file_bytes": 65536, "invocation_control_frames": 62, "knowledge_bindings_cap": 4096, "knowledge_layers_cap": 7, "knowledge_rows_cap": 8192, "knowledge_states_cap": 4096, "knowledge_steps_cap": 16, "knowledge_steps_default": 8, "knowledge_total_source_bytes_cap": 65536, "knowledge_work_cap": 2000000, "max_assignments": {"cap": 20000, "default": 2000}, "max_cells": {"cap": 16777216, "default": 4194304}, "max_checks": {"cap": 30, "default": 24}, "max_gate_pairs": {"cap": 4000000, "default": 1000000}, "max_search_work_per_stage": {"cap": 100000000, "default": 50000000}, "max_terms": {"cap": 512, "default": 128}, "max_tuples": {"cap": 131072, "default": 131072}, "program_inputs_cap": 32, "program_nodes_cap": 256, "program_outputs_cap": 16, "runtime_depth": 1}

## Coordinate

Retain the command, exit status, verdict, declared scope and supporting receipt when passing a result to another workflow. Reports and suggestions do not grant adoption or publication authority.

## Pitfalls

Selection ranks proposals already checked at their own gates. Structural facts, typed traces, diagnosis and proposed answers confer no program acceptance. Task exit 0 does not promise every evaluation task qualified.

Refusals and next steps: {"output-must-be-new-directory": "Choose a new output directory.", "strategy-runtime-depth-must-be-one": "Use --depth 1.", "strategy-subject-overlap-or-duplicate": "Use distinct development/evaluation references."}

## Worked example

```sh
zeno-fcis loop strategy --kb-op query --query 'strategy(S)'
```

Expected: exit 0; {"schema": "zeno-fcis/loop-doors/2", "status": "proposed-query"}.

## See also

**Background.** John R. Rice, “The Algorithm Selection Problem,” Advances in Computers 15, 1976, is background for selecting algorithms by measured behavior; Ross Tate et al., “Equality Saturation: A New Approach to Optimization,” ACM POPL, 2009, is background for proposal search.

**What is new here.** This card documents the implemented interface; it adds no algorithm or priority claim.

**Scope.** Selection ranks proposals already checked at their own gates. Structural facts, typed traces, diagnosis and proposed answers confer no program acceptance. Task exit 0 does not promise every evaluation task qualified.

Sources: crates/zeno-fcis-cli/src/loop_strategy.rs; crates/zeno-fcis-cli/src/loop_strategy_profiles.rs; crates/zeno-fcis-cli/src/loop_strategy_knowledge.rs

[CLI reference](../CLI_REFERENCE.md); [loop overview](loop-workspace.md).
