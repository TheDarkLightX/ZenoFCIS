---
name: zenofcis-synthesis-first
description: Build ZenoFCIS applications from checked declarations, choose an optimization or synthesis loop for finite programs, and use counterexamples to repair proposals without weakening their contracts.
---

# ZenoFCIS synthesis first

Use the installed CLI's `zeno-fcis describe` or MCP
`discover_program_authoring` to discover supported commands and their task
guidance. Check each command's domain and resource limits before a search.

## Choose by the missing result

| Task | Start here | What to do with the result |
|---|---|---|
| Build a durable application | `new --template durable-counter`, then edit the declarations and `generate contract` | Review complete decisions, reasons, successors, effects and laws |
| Find behavior the laws leave open | `contract review APP --gaps` | Inspect admitted alternatives; propose laws and recheck them against the contract |
| Resolve competing intended behaviors | `loop question --contract APP` | Show the concrete input and alternatives to the user; do not choose policy silently |
| Check a proposed replacement | `transform check` | Repair a refuted candidate using the reported input and differing outcomes |
| Find a smaller equivalent program | `contract export-program`, then `optimize` | Replay the receipt with `transform replay`; adoption is separate |
| Keep a resumable improvement search | `loop open`, then `candidate` or `run`, then `resume` | Use counterexamples and admission refusals to guide the next bounded proposal |
| Infer a rule from finite examples | `loop learn` | Resolve distinguishing questions; example agreement alone is not whole-domain correctness |
| Complete an expression with holes | `loop sketch` | Check the completed program against the full intended contract |

Read `--help` for the selected command's arguments. A loop is propose, check,
inspect feedback, revise. State a bounded objective before running it; keep the
exact inputs and checker outputs so another agent can replay the result.

## Construct the core from declarations

Specify every legal state, command and explicit context, each decision class
and reason, successor, ordered effects, transition laws and genesis laws.
Keep separately authored examples or a reference model to challenge the
requirements. Do not narrow the domain or replace success with rejection to
make a check pass.

Standalone finite synthesis remains useful. Assess the Cartesian spaces with
`assess_finite_problem` or the CLI. `zeno-fcis/finite-i128/1` has 16 fields per side, 256 nodes
per graph, 65,536 inputs, 4,096 outputs and 100 million graph-node work units.
When the complete reviewed relation fits, use `synthesize_finite_core`/`synth run`,
`synth run --check` and `verify_finite_core`/`synth verify` on the exact target.
Retain independent full-decision/state/reason/delivery comparisons over every
admitted input. Finite target evidence does not authorize arbitrary generated Rust
or replace checked Program binding, laws or complete raw-input correspondence.

The generated V2 path uses `v2/policy.json`, schema declarations and generated
`v2_contract.rs`. Its descriptor binds through `catalog::bind_original` and
`authority::bind`. The library's `v2_authority::Authority` evaluates decisions,
laws and resource usage and issues publication capabilities. The generated
`v2::V2SqliteShell` accepts those capabilities for durable commits. Follow the
generated template's imports and ownership; do not supply a decision callback,
law verdict, claimed usage or publication constructor.

To shrink an admitted finite program, use `transform_request`,
`transform_candidate` and `transform_replay` (or `zeno-fcis loop`). Propose
complete programs with exactly the original's inputs and outputs; the loop
checks each one against the original on every input tuple and keeps a
replacement only when it is equivalent and smaller in nodes or bytes within the
original's bounds. Read the typed feedback as checker facts and your own
reasoning as advisory: a replayed counterexample names the first differing
tuple, a refusal names the admission rule, and `best-checked-so-far` means
equal on the declared domain and no larger, not optimal and not adopted by any
application. At most eight attempts and eight checks are available per session;
duplicates and malformed proposals consume attempts but do not necessarily use a
full checker call. Replay and preparation use their own retained work reservations.
`checked-improvement` and `equivalent-without-improvement:<reason>` exit 0;
`different`, `refused` and canonical/JSON duplicate outcomes exit 1. Expression
and sketch duplicates with independent check/replay confirmation exit 0; failed
expression/sketch proposals exit 1. Incomplete or closed sessions
exit 2; I/O errors exit 3. `loop run` returning 0 means the driver completed,
including runs with no improvement or failed proposals.

Run `zeno-fcis loop --help`, `zeno-fcis loop COMMAND --help` and
`zeno-fcis guide show loop-workspace` to choose the exact surface. The CLI
reference's “Which loop to use” table distinguishes capture, learning, session
checks, pipelines, rules, strategy modes, family/transfer, meaning/evaluate,
questions, restated rules and decomposition. Capture owner `whole`/`open`/`status`
are separate from public `packet` and proposer `candidate`; keep owner evidence
private. Example fit, meaning hashes, structural knowledge and simulation do not
supply a transform verdict. `learn --original` and split-by-cases additionally
need complete-reference capture and original-bound check/replay. `loop duel`
does not exist; the duel guide describes a protocol and a small-model solver
example, not an executable alternating driver.

`check --format json` diagnoses the specification; it does not admit a runtime
program. `publish` and `publish_genesis` perform the checked execution. Supplied
authorization and time remain data: caller authentication and physical effects
belong to the host.

## Interpret checker feedback precisely

For optimization, export the original program and retain it unchanged. Supply
complete candidates with the same declared inputs and outputs. A checked
improvement is equivalent over that finite domain and smaller under the
reported cost; it is not a global optimum. An unsupported original names an
instruction the optimizer cannot handle; increasing a budget cannot add support.
An incomplete search is inconclusive, not evidence that no improvement exists.

Arithmetic refusal is observable behavior. For example, removing an overflowing
instruction can turn refusal into success even if its value was not selected.
Use the exact transform checker instead of assuming ordinary algebra preserves
checked execution. Replay each accepted receipt with its original and candidate
bytes before using it; `contract adopt` is a separate, checked operation.

For standalone relation synthesis, first use `assess_finite_problem` or CLI
validation to assess the declared domain and work budget. `synth run --check`
and `synth verify` check the supported target and profile. Keep that claim
separate from arbitrary Rust code, application integration and shell effects.

Report incomplete searches, example agreement, exhaustive finite results,
source proof qualification and runtime publication separately. Neither the LLM
nor a tool report supplies release or application authority.

Use the [MCP setup and synthesis guide](https://github.com/TheDarkLightX/ZenoFCIS/blob/main/docs/LLM_SYNTHESIS.md)
and [CLI reference](https://github.com/TheDarkLightX/ZenoFCIS/blob/main/docs/CLI_REFERENCE.md)
for installation, artifact formats and command-specific limits.
