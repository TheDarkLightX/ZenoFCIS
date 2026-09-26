---
name: zenofcis-synthesis-first
description: Build or revise ZenoFCIS application decision cores; prefer checked finite synthesis when the complete pure transition fits, and report exact assurance boundaries when it does not.
---

# ZenoFCIS synthesis first

For a ZenoFCIS application, specify the complete pure transition before writing
decision code: admitted pre-state, command, explicit context, decision and
reason, post-state, and planned effects/outbox. Treat the specification as a
reviewed requirement, not as evidence merely because an LLM wrote it. Keep
independent decision examples or a separately written reference model.

Assess the Cartesian input and output spaces and grammar/work limits with
`assess_finite_problem` or the CLI. The current `finite-i64/1` profile has
16 fields per side, 256 nodes per graph, 65,536 inputs, 4,096 outputs, and
100 million graph-node work units. If the complete core fits, synthesize it,
run `synth run --check` on the checked-in artifact, and run `synth verify` on
the exact emitted target. Verify the adapter's full decision, state update,
reasons, and effect plan against an independent model over **every admitted
finite input**. Preserve catalog admission, laws, and commit authority.

For large numeric domains, a smaller fact domain is acceptable only when its
projection is independently proved to preserve the raw decision and output.
If this or another obligation is missing, label the result a checked finite
subcore and keep the rest as trusted code; never market the whole core as
formally guaranteed. A timeout, incomplete search, solver `unsat` without a
checked proof, or a passing sample test is not synthesis success.

Use the local MCP server in
[`integrations/mcp/zeno_fcis_synthesis.py`](../../integrations/mcp/zeno_fcis_synthesis.py)
when installed, or the same `zeno-fcis synth` CLI directly. The MCP tools
report CLI evidence; they do not grant application authority. The full tool
and installation guide is
[`docs/LLM_SYNTHESIS.md`](../../docs/LLM_SYNTHESIS.md).
