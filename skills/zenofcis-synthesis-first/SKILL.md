---
name: zenofcis-synthesis-first
description: Propose complete ZenoFCIS checked program declarations, use supported library execution and finite synthesis, and keep evidence separate from runtime publication and trusted shell effects.
---

# ZenoFCIS synthesis first

Specify the complete pure transition before implementation: every legal original
pre-state, command and explicit context, decision class/reason, successor state,
ordered effects/deliveries, required transition laws and genuine genesis laws.
Review intent independently and retain separately authored acceptance examples or
a reference model. An LLM proposal does not establish requirements adequacy.

Use `discover_program_authoring` or `zeno-fcis describe` to inspect the actual CLI.
Use `create_program_project` or `zeno-fcis new --template durable-counter` for a
complete checked declaration example. The other supported normal examples are
account-lockout, order-fulfillment, inventory-reservation, withdrawal-queue and
agent-treasury-guard. `check_project_spec` or `zeno-fcis check --format json`
checks `.zeno` authoring diagnostics; it does not admit a runtime program.

The normal Rust API is `zeno_fcis::prelude` and `zeno_fcis::program`. Propose the
complete original schema description/bytes, `ProgramDefinition`, original policy
bytes and original state/command/context frame and channel links. Preserve every
legal input and intended successful outcome. `policy_bytes` only serializes a
proposal. Call the actual library `bind_catalog` and `bind_program`; never supply
an evaluator, observer, law verdict, claimed usage or candidate sealer.
Generated adapters are declaration data and retain complete original envelopes.

Use the library's supported closed finite/control IR or full-width account
producer. Unsupported schemas/profiles return named refusals; explain the missing
support without granting an alternative commit capability. The Account producer
owns its raw-input abstraction; a newly proposed small fact projection needs its
own independent correspondence proof. Never replace the full domain with a
smaller set or an all-Reject policy to make a checker pass.

`Program::publish` and `publish_genesis` execute actual original envelope
admission, declared footprints/private resource meter, complete decision and
required laws before a private Publication can exist. Rejections and technical
refusals grant no committing publication. Context authentication, trusted version
selection, durable replay/state comparison and physical effects belong to the
host. A tool report or passing source test is not that runtime capability.

Standalone finite synthesis remains useful. Assess the Cartesian spaces with
`assess_finite_problem` or the CLI. `finite-i64/1` has 16 fields per side, 256 nodes
per graph, 65,536 inputs, 4,096 outputs and 100 million graph-node work units.
When the complete reviewed relation fits, use `synthesize_finite_core`/`synth run`,
`synth run --check` and `verify_finite_core`/`synth verify` on the exact target.
Retain independent full-decision/state/reason/delivery comparisons over every
admitted input. Finite target evidence does not authorize arbitrary generated Rust
or replace checked Program binding, laws or complete raw-input correspondence.

Report authoring proposals, finite exhaustive results, source-bound proof
qualification, genuine runtime Publications and trusted shell effects separately.
Timeouts, incomplete searches, unchecked solver answers and passing samples are
not proof acceptance. Preserve artifact drift refusals and exact source/tool
identities. A Wasm compilation does not establish native/Wasm execution parity or
a universal Rust code-generation theorem.

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
duplicates and malformed proposals consume them.

Use the local [MCP server](https://github.com/TheDarkLightX/ZenoFCIS/blob/main/integrations/mcp/zeno_fcis_synthesis.py) or the same
CLI directly. Installation and tool usage are documented in
[docs/LLM_SYNTHESIS.md](https://github.com/TheDarkLightX/ZenoFCIS/blob/main/docs/LLM_SYNTHESIS.md). Neither the skill nor MCP
substitutes for independent intent/specification review.
