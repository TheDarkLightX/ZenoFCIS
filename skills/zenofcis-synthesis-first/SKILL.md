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

The generated V2 path uses `v2/policy.json`, schema declarations and generated
`v2_contract.rs`. Its descriptor binds through `catalog::bind_original` and
`authority::bind`. The library's `v2_authority::Authority` evaluates decisions,
laws and resource usage and issues publication capabilities. The generated
`v2::V2SqliteShell` accepts those capabilities for durable commits. Follow the
generated template's imports and ownership; do not supply a decision callback,
law verdict, claimed usage or publication constructor.

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
