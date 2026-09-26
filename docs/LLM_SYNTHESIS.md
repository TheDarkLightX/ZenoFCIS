# Synthesis-first agents

The checked-in [skill](../skills/zenofcis-synthesis-first/SKILL.md) tells an
LLM to synthesize a complete pure decision core when the declared domain fits,
and to state the remaining trusted boundary when it does not. The local
[MCP server](../integrations/mcp/zeno_fcis_synthesis.py) exposes the CLI's
assessment, synthesis, and exact-target verification tools over stdio. Neither
file is installed automatically by the Rust library.

Install the `zeno-fcis` CLI and Python 3.10+ first. With `uv`, register the MCP
server in Claude Code from a cloned repository:

```sh
claude mcp add zenofcis-synthesis -- uv run --no-project --with 'mcp==2.2.0' \
  python /absolute/path/to/ZenoFCIS/integrations/mcp/zeno_fcis_synthesis.py
```

For other MCP hosts, use that same command and arguments as a stdio server in
the host's MCP configuration. If `zeno-fcis` is not on the host's PATH, set
`ZENO_FCIS_CLI` to the absolute CLI binary path in the MCP configuration.
The server does not run a shell or take arbitrary command arguments from the
LLM. The operator chooses the CLI executable; the tools take only a problem
path, output path, and a registered target language.

The order-fulfillment template demonstrates the workflow: its 1,728-input
branch selector is synthesized and checked in all three targets, and its
executed adapter is compared with an independent full-transition model over
all 1,440 lawful state/command/context combinations. Account-lockout still
uses a hand-written decision: its raw timestamp fields make direct exhaustive
synthesis exceed this profile's tuple limit. A finite time-fact projection
would need its own soundness proof before it could support the same claim.

Inventory-reservation synthesizes the complete finite decision over 864 inputs,
including authorization precedence, state updates, and shipment selection.
Its executed adapter is compared with an independent model on every admitted
input. The proof is relative to the reviewed finite contract and the admitted
schema; it does not certify external authentication or shipment delivery.

For Codex, install the skill from this checkout with:

```sh
mkdir -p ~/.codex/skills/zenofcis-synthesis-first
cp skills/zenofcis-synthesis-first/SKILL.md \
  ~/.codex/skills/zenofcis-synthesis-first/SKILL.md
```

Other agents can load the same `SKILL.md` as project guidance. Clone or update
the repository to keep the skill and MCP server together. Review a new
`synthesis.json` as a specification of intended behavior; generated code is
only proved against that contract on its admitted finite inputs. Before using
the result as a complete application, separately check any raw-input
abstraction, adapter, laws, external effects, and exact emitted target.

Developers can exercise the MCP protocol and its refusal of modified artifacts
with `ZENO_FCIS_CLI=/absolute/path/to/zeno-fcis uv run --no-project --with
'mcp==2.2.0' python integrations/mcp/test_server.py`.
