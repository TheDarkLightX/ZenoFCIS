# Checked program authoring with agents

The [skill](../skills/zenofcis-synthesis-first/SKILL.md) and local
[MCP server](../integrations/mcp/zeno_fcis_synthesis.py) use the actual `zeno-fcis`
CLI alongside the library. The LLM proposes complete declarations; the library
admits and executes them. Neither tool installs automatically with the library.

Clone the public repository and install the CLI from the same checkout:

```sh
git clone https://github.com/TheDarkLightX/ZenoFCIS.git
cd ZenoFCIS
cargo +1.97.1 install --locked --path crates/zeno-fcis-cli
```

Install Python 3.10+ and `uv`. Register the server in Claude Code:

```sh
claude mcp add zenofcis-synthesis -- uv run --no-project --with 'mcp==2.2.0' \
  python /absolute/path/to/ZenoFCIS/integrations/mcp/zeno_fcis_synthesis.py
```

Other MCP hosts use the same executable and arguments as a stdio server. Set
`ZENO_FCIS_CLI` to an absolute CLI binary path when it is not on the host's PATH.
The operator chooses the executable. Tools accept typed paths, a closed template
or target selection and documented booleans; they never invoke a shell or accept
arbitrary CLI arguments.

`discover_program_authoring` runs `zeno-fcis describe` to expose the actual
command contracts and normal program route. `create_program_project` runs `new`
with one of durable-counter, account-lockout, order-fulfillment,
inventory-reservation, withdrawal-queue, agent-treasury-guard, prepared-counter
or compliance-gateway. The destination's
parent must exist; a nonempty destination is refused without overwriting files.
`check_project_spec` runs `check --format json`, optionally with
`--require-substantive`. These are authoring results with runtime admission
explicitly not run.

Follow [the normal API migration](V2_PROGRAM_API_MIGRATION.md): independently
review complete original schema/policy bytes, closed control and branches,
footprints, reasons, channels, required transition/genesis laws and all legal
input domains. `bind_catalog` and `bind_program` call the actual checked library.
Generated `GeneratedProgramProposal`/`GeneratedDeclarations` are untrusted data;
metadata cannot invent a complete policy. Original envelopes are retained and
independently admitted during execution. A committing `Program::publish` or
`publish_genesis` can produce a private Publication only after actual execution
and required law checks. Authentication, persistence/replay/state comparison and
physical delivery remain trusted shell work.

The library supports closed finite/control declarations and a full-width Account
producer. Account-lockout uses that library-owned producer over its original
timestamp domains; it does not fall back to a handwritten authority callback.
Unsupported profiles return named refusals and grant no publication capability.
Independent acceptance models and source-bound proof qualification remain
separate from these authoring tools.

The original three finite tools remain: `assess_finite_problem`,
`synthesize_finite_core` and `verify_finite_core`. Order-fulfillment's standalone
finite synthesis covers 1,728 inputs; inventory-reservation covers 864. Exact
target verification executes every admitted input, and artifact drift is refused.
The original independent transition comparisons remain required. These results
are finite behavior evidence relative to reviewed contracts, not a universal
code-generation theorem or automatic authority for generated native code.

Install the skill in Codex from this checkout:

```sh
mkdir -p ~/.codex/skills/zenofcis-synthesis-first
cp skills/zenofcis-synthesis-first/SKILL.md \
  ~/.codex/skills/zenofcis-synthesis-first/SKILL.md
```

Other agents can load the same `SKILL.md` as project guidance. Keep server, CLI,
library and skill revisions consistent. Report authoring diagnostics, finite
exhaustive evidence, qualified source-bound proofs, runtime Publications and
shell effects separately. A passing sample, timeout or unchecked solver answer
is not proof acceptance; tool use does not replace independent intent review.

Exercise the real MCP protocol, all original finite cases and the normal
authoring/refusal tools with:

```sh
ZENO_FCIS_CLI=/absolute/path/to/zeno-fcis uv run --no-project --with 'mcp==2.2.0' \
  python integrations/mcp/test_server.py
```
