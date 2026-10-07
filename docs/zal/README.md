# ZAL: terminal-first behavior review

ZAL is an additive research prototype for discussing a small, precise behavior
before realizing it through ZenoFCIS. Two deterministic surfaces, symbolic and
controlled English, describe one `zal/finite-fsm/1` model. Start in a terminal;
the browser is optional. Built-in help, translation and input-specific
explanations work locally without an LLM. This is not a released ZenoFCIS API.

Read the [specification](SPECIFICATION.md), [exact grammar](GRAMMAR.md),
[research and provenance](RESEARCH.md), and [factory source map](../ZAL_SOURCE_MAP.md).
Commands below run from the repository root. Python 3.11+ and a POSIX system
are required for the terminal/shared-file path (`fcntl` advisory locking).
The Python runtime uses only the standard library. Factory qualification also
requires the repository's pinned Rust 1.97.1 and dependencies.

## 1. Inspect a model without a provider

```sh
python3 integrations/zal/run.py legend
python3 integrations/zal/run.py check integrations/zal/examples/order.zal
python3 integrations/zal/run.py render integrations/zal/examples/order.zal --to english
```

The seed has four states, three events and one fresh Boolean context fact:
24 raw input tuples. It declares no application invariant. A passing report
therefore establishes the reported finite checks, not an unstated safety goal.
Rendering and checking make no model calls and grant no production authority.

## 2. Open the human review terminal

Create a private temporary directory and one workspace. Keep the printed path
for the second terminal that will run your coding harness.

```sh
ZAL_WORKDIR=$(mktemp -d "${TMPDIR:-/tmp}/zal.XXXXXX")
export ZAL_WORKSPACE="$ZAL_WORKDIR/workspace.json"
python3 integrations/zal/terminal.py init --workspace "$ZAL_WORKSPACE"
printf 'Workspace: %s\n' "$ZAL_WORKSPACE"
python3 integrations/zal/terminal.py review --workspace "$ZAL_WORKSPACE"
```

`init --source PATH` can seed another symbolic model. Initialization refuses
an existing workspace. Keep this directory if you want to resume it later;
`review` reloads it without reinitializing.

At the `zal>` prompt, try:

```text
show
select rule:approve_order
? and
man rule:approve_order
legend frame
translate english
why
why pending approve authorized=no
why pending approve authorized=yes
check
trace submit authorized=no then approve authorized=no
candidate
```

The default display uses controlled English, selected-rule context, readable
state traces, concrete witnesses and scoped check labels. It does not require
writing JSON or copying a revision hash. The trace starts at the declared initial
state. Each step supplies every declared context fact, even when that rule does
not use it. `why` displays built-in help for the selected object. With an explicit
state, event and complete context, it explains the exact modeled decision:
source/event matches, each guard's truth, the enabled rule or default,
outcome/reason and frame. The examples above reject with `not_enabled` and keep
`pending` for `authorized=no`, then accept into `approved` for `authorized=yes`.
These are assumed inputs, not authenticated authorization or observed events.
`check` checks the current model; `candidate` shows a staged model's checks
and semantic diff. The paired legend shows both surfaces; `--json` also includes
the complete selectable typed-ID list and both model projections.

### Built-in language help

`? TOPIC`, `man TOPIC`, `help TOPIC` and `legend TOPIC` share one deterministic
resolver. Bare `?` or `man` lists topics; bare `help` also prints review commands.
Bare `legend` shows the full paired legend. Examples include `? context`,
`? and`, `man &`, `? +`, `? ->`, `man rule:approve_order` and `help default`.
These commands run inside `zal>`; they do not invoke the OS manual or a shell.

Exact typed IDs and the built-in IDs `initial`, `default` and `frame` take
precedence. Other bare names resolve only when unique and not colliding with a
grammar topic. Use `state:NAME` or `event:NAME` to disambiguate such names.
`grammar:TOPIC` explicitly stays in grammar space, including when a declared
name collides with a grammar topic; it does not resolve typed IDs. Unknown or
stale IDs are refused. Operator help describes
the predicate role: English `and`/`is` also occur as fixed statement text;
`+` separates state and event, and `->` introduces an outcome.

Help describes the current canonical model, not a dirty editor buffer or text
under an arbitrary source position. `translate english` and `translate symbolic`
render that same model with no inferred rewrite. Help, translation and `why`
leave selection, proposals, agreement and the exact displayed-review binding
unchanged. They neither display a candidate for acceptance nor accept one.
The shared-file adapter may still rewrite its snapshot during a read.

Structured help uses `zal/help/1`, carrying the full model revision, profile,
resolved topic and renderer source identity, paired canonical examples, notes
and available topics. The renderer identity uses the checked source closure;
it is distinct from the model digest. `--json` exposes the recursive guard truth
tree for concrete explanations. Missing/extra context keys, non-Boolean values,
undeclared state/event and overlapping enabled rules are refused. There is no
provider fallback or implicit use of a previous trace's inputs.

For a compact shell walkthrough after leaving the interactive loop with `quit`:

```sh
printf '%s\n' '? and' 'man rule:approve_order' 'translate english' \
  'why pending approve authorized=no' 'why pending approve authorized=yes' \
  'check' 'quit' | python3 integrations/zal/terminal.py review --workspace "$ZAL_WORKSPACE"
```

### Check scope and the intent boundary

Completed `check` reports warn when no application invariants are declared. Other
advisories distinguish a guard never true at its source, an unreachable source,
and an invariant true on every declared state. They do not turn intentional
unreachable rules or domain properties into errors. Overlap and violated
invariants remain counterexamples.

The seed demonstrates why a consistency pass is not an authorization proof.
This local, no-LLM comparison weakens only the approval guard in a separate file:

```sh
sed 's/\[authorized\]/[true]/' integrations/zal/examples/order.zal > "$ZAL_WORKDIR/weakened.zal"
python3 integrations/zal/run.py check "$ZAL_WORKDIR/weakened.zal"
python3 integrations/zal/run.py diff integrations/zal/examples/order.zal "$ZAL_WORKDIR/weakened.zal"
```

The weakened model still passes the declared finite checks because the seed
contains no application invariant. The diff exposes the changed behavior:
`pending / approve / authorized=false` changes from default Reject to
Accept/`approved`. Review expected and forbidden scenarios independently;
the checker cannot supply an unstated requirement. This example does not stage
or accept a workspace candidate.

### Stage and review a change

To stage a manual edit, copy the seed to a separate file, edit it, then enter
`propose /absolute/path/to/candidate.zal symbolic` (or `english`) in the review
terminal. The review prompt is not a shell: substitute real paths yourself.
A candidate must change the canonical revision. There is one pending candidate;
a later valid proposal replaces it, so inspect `candidate` immediately before
deciding.

Display the candidate here, review its meaning and evidence, then choose
`accept` or `reject`:

```text
candidate
accept
```

The client remembers the exact base, candidate and event cursor displayed in
this terminal. It never silently accepts an unseen newer candidate. Any
intervening mutation requires `candidate` and review again; harmless reads do
not change that binding. Acceptance rechecks the current checker source identity and
refuses stale, consumed or counterexample-bearing candidates. An unresolved
obligation can be accepted as a requirement, but remains `unsupported` and
cannot lower. `cancel` rejects the pending candidate without changing the
current meaning; Ctrl-C cancels the current input. `quit` leaves the review loop.
Agreement remains separate from evidence and `realization: not-run`.

For low-level use, `accept FULL_BASE_REVISION FULL_CANDIDATE_REVISION` and the
equivalent `reject` command remain available, as do JSON-array traces. Add
`--json` to `terminal.py review` for structured output. The MCP tools remain
the provider-independent structured interface.

Long-lived clients pin their checker source closure at process start. If those
files change, restart the affected clients and restage/review the candidate;
old evidence must not be relabeled with a new checker identity.

## 3. Use the same workspace from an existing coding harness

The provider-independent MCP bridge runs this command as a stdio child:

```sh
python3 /absolute/path/to/repository/integrations/zal/mcp.py \
  --workspace /absolute/path/to/workspace.json
```

Normally the harness launches it; there is no HTTP service to start. The bridge
uses newline-delimited MCP JSON-RPC, advertises protocol `2024-11-05`, and exposes
only these seven tools:

| Tool | Required arguments | Result/purpose |
| --- | --- | --- |
| `zal_read` | `{}` | Current model/revision, typed IDs, selection, pending proposal and evidence. |
| `zal_check` | `revision` | Finite checks on that exact current revision. |
| `zal_help` | `revision`, `topic` | Built-in grammar or typed-object help; use an empty topic for the index. |
| `zal_explain` | `revision`, `state`, `event`, `context` | Exact decision and recursive guard evaluations for one complete Boolean input. |
| `zal_compare` | `revision`, `syntax`, `surface` | Compare a complete candidate surface without staging it. |
| `zal_trace` | `revision`, `inputs` | Replay an array of `{event, context}` inputs from the initial state. |
| `zal_propose` | `base_revision`, `object_ids`, `message`, `syntax`, `surface` | Parse and stage a complete candidate with checks and diff; never accept it. |

`syntax` is `symbolic` or `english`. Copy `model.revision` from `zal_read`;
`object_ids` must refer to the current model. Surface text follows the grammar,
not an arbitrary prose edit or a JSON patch. Tool discovery and JSON Schema do
not themselves establish that the harness granted permission to call a tool.

Ask in your usual coding conversation, for example: “Read the ZAL workspace.
Use `zal_help` for `rule:approve_order`, then `zal_explain` for pending/approve
with authorized=false, on its exact revision.
If I ask for a change, stage it for my separate terminal review.” The loop is:

1. The harness calls `zal_read`, then checks, compares or traces that revision.
2. It calls `zal_propose` for an explicitly discussed change.
3. You inspect `candidate` and accept or reject the exact revisions in the human
   terminal. A command/tool permission prompt is not semantic acceptance.
4. The harness calls `zal_read` again before continuing on the new meaning.

This is a pull workflow. There are no hooks, Channels,
unsolicited model turns, change subscriptions or auto-refresh into a conversation.
Neither the shared store nor the bridge calls a provider. The harness controls
what it sends to its model, using its existing account and permissions.

### Project addon for Codex and Claude Code

The [2.3 addon guide](HARNESS_INTEGRATION.md) provides persistent project-local
MCP entries and a shared authoring skill. Run `integrations/zal/setup.py` from a
stable source checkout, first without `--apply` to inspect the plan. It preserves
unrelated configuration and pending work, and refuses a differing existing ZAL
entry or skill. The older per-invocation instructions below remain available.
Installation does not change tool approval policies or call a model.

### Codex: per-invocation configuration

In a second shell at the repository root, set `ZAL_WORKSPACE` to the exact path
printed above. These overrides use the installed Codex 0.159.2 `--config`
interface and the documented [stdio MCP configuration](https://learn.chatgpt.com/docs/extend/mcp).
They do not write a registration to `config.toml` or widen approval policy.

```sh
export ZAL_WORKSPACE=/absolute/path/printed/by/the/review/setup/workspace.json
ZAL_ARGS=$(python3 -c 'import json,sys; print(json.dumps([sys.argv[1], "--workspace", sys.argv[2]]))' \
  "$PWD/integrations/zal/mcp.py" "$ZAL_WORKSPACE")
codex \
  -c 'mcp_servers.zal.command="python3"' \
  -c "mcp_servers.zal.args=$ZAL_ARGS" \
  -c 'mcp_servers.zal.enabled_tools=["zal_read","zal_check","zal_help","zal_explain","zal_propose","zal_compare","zal_trace"]'
```

Before starting the interactive session, the same command with `mcp get zal
--json` appended inspects the effective entry without running a model. That
configuration check was observed on 0.159.2. Use an unused server name if your
existing configuration already defines `zal`; other configured integrations
remain governed by your existing setup.

**Interoperability limit:** the earlier stock Codex probe discovered `zal_read`
but its permission layer cancelled both calls. No read/check/proposal completed.
An explicitly approved, narrowly scoped cloud retry on 0.159.2 stopped during
runtime initialization with a read-only-filesystem error, before any model or
MCP tool execution was observed. The demo workspace and configuration remained
unchanged. Neither attempt establishes an end-to-end stock-CLI pass.
Keep required approvals visible; a
blocked call is not a reason to bypass policy or install persistent permissions.

### Claude Code: documented adapter, not live-validated

The same seven tools fit Claude Code's documented [local stdio MCP
interface](https://code.claude.com/docs/en/mcp). Its [CLI reference](https://code.claude.com/docs/en/cli-reference)
documents `--mcp-config` for per-session JSON, with `mcpServers.zal` containing
`type: "stdio"`, `command: "python3"` and the same absolute-path argument array.
An installed Claude executable was not available in the cloud validation
environment. Verify that installed version's flags, configuration loading and
ordinary tool approval flow before use. No Claude model was invoked, no content
was sent to Claude, and no persistent Claude configuration was installed.

## Workspace boundary

Terminal and MCP share the same `zal/shared-workspace/1` file. Cooperative
advisory locking, bounded private snapshots and atomic replacement prevent
ordinary stale-client races; they are not malicious-agent isolation. A process
with the same user's filesystem privileges can bypass the intended review
workflow. Human acceptance here is a trusted local gesture, not a signed human
attestation. MCP has no acceptance tool, but that alone is not a security boundary.

Each operation reloads current state. Stale revisions require a fresh read and
new review. Snapshots are capped at 32 MiB and discussion history at 256 events;
lock contention refuses after two seconds. Keep sensitive model content in an
appropriately protected workspace. This prototype has no shared multi-user
access control or production durability guarantee.

## Optional browser and separate Codex custom client

To view and review the same existing terminal/MCP workspace:

```sh
python3 integrations/zal/run.py ui --workspace "$ZAL_WORKSPACE" --port 8765
```

Open `http://127.0.0.1:8765/`. The companion starts in local replay mode; replay
is a disclosed fixture, not a live model. It provides paired rule views, graph
and table selection, traces, witnesses, built-in contextual help and
exact-revision review. `Explain next input` evaluates the current trace state,
selected event and every Boolean checkbox; `Why this recorded step?` explains
the exact input at the trace cursor. `Ask Codex` is the separate provider
conversation. `Help ?` opens the topic overview; `Meaning of this object ?`
opens help for the selected typed ID. Mouse/pen pointer or keyboard focus on
objects/topics loads a persistent Context help preview. Visible controls also
work by click/tap; `?` opens the focused topic or selected object outside text
controls and the editor. Close/Escape restores the invoking focus. Help uses
the same resolver as the terminal, with stale-reply protection. It describes
canonical typed objects; there is no editable-source-span hover or LSP adapter.
With `--workspace`, domain operations use the same locked store; browser polling
observes terminal/MCP changes. Dirty source text keeps its original base and
must be explicitly discarded or rebased after another client accepts a revision.
The coding harness still needs an explicit `zal_read` refresh.

For an independent disposable demo, omit `--workspace`:
`python3 integrations/zal/run.py ui --port 8765`. That mode has an isolated
in-memory session, is not synchronized with the terminal/MCP file, and loses
its session when restarted. The UI state labels which workspace mode is active.

The browser's explicit Codex connection uses the separate
[`codex_transport.py`](../../integrations/zal/codex_transport.py) app-server
client. That mode owns a Codex conversation; it does not attach a pane or send
events into your stock Codex CLI session. It uses stdio, an ephemeral read-only
thread, structured final output and exact turn IDs, and refuses execution
approval requests. Actual permitted read-only tools must still be observed;
an instruction to avoid tools is not a physical isolation guarantee.
Live use is separate from default offline checks. See [evidence distinctions](RESEARCH.md#integration-evidence-and-open-work).

## File-level export and actual factory qualification

For the shared harness workspace, use the [reviewed export commands](HARNESS_INTEGRATION.md)
after separate terminal acceptance. They require the current exact revision and
checker binding, and verify every exported file. `show` followed by
`accept-current` reviews a seed without inventing a meaning change; it refuses a
pending proposal or an intervening event. Existing unbound approval records need
an explicit new review. Shared-file acceptance is cooperative, not authenticated.

The file-level commands below deliberately remain proposal operations.

```sh
python3 integrations/zal/run.py diff integrations/zal/examples/order.zal /path/to/candidate.zal
python3 integrations/zal/run.py export integrations/zal/examples/order.zal --out /path/to/empty-export
cargo +1.97.1 build -p zeno-fcis-cli --locked
python3 integrations/zal/run.py qualify integrations/zal/examples/order.zal --out /tmp/zal-factory.json
```

Use an absent or empty export directory. For English input add `--syntax
english`; for an English diff candidate add `--candidate-syntax english`.
Qualification honors `CARGO_TARGET_DIR` when locating `debug/zeno-fcis`; relative
target paths are resolved from the repository root. `qualify --cli PATH`
overrides that choice with the actual built executable.

The current factory-qualified subset requires at least two states: its unchanged
scaffold checks need a distinct rejected genesis. One-state models remain valid
for language checking and dialogue, but export/qualification explicitly refuse
them. Independent factory node/shape limits can also refuse an admitted model.

These commands operate on the named source file, not the shared workspace's
agreement record. Export checks the supported profile but is still a proposal;
it does not certify human acceptance. Qualification invokes actual contract
generation, drift checking, advisory review, scaffolding and generated Authority
tests. Its temporary application lockfile is normalized offline; the repository
lockfile is unchanged. Inspect every report entry and terminal status. None of
these commands adopts, publishes, upgrades, releases or deploys an application.

CLI exit codes are 0 for a completed operation, 1 for `counterexample` or
`unfinished`, 2 for refused input/operation, and 3 for `unsupported`. A diff's
`different` or `domain-changed` result is data, not an exit-code failure.

## Reproduce checks

The `zal-dialogue` scenario also runs the shipped standalone reference oracles:

```sh
python3 integrations/zal/oracles/independent_oracle.py
python3 integrations/zal/oracles/formula_oracle.py
python3 integrations/zal/oracles/explanation_oracle.py
```

They derive expected outcomes from explicit transition tables and integer truth
masks, then compare the exact hashed implementation. Each prints its case counts
and bounded scope. They cover named finite families, not all admitted programs,
human intent, external context truth, factory lowering or browser behavior.
Run without Python `-O`; the loader refuses disabled assertions.

```sh
python3 -m unittest discover -s integrations/zal -p 'test_*.py' -v
node integrations/zal/test_workspace.cjs
```

The Node command runs the actual UI script with minimal DOM doubles and fixtures
generated by the Python domain. Its deterministic checks need no browser,
Playwright, network or provider; they do not test native rendering, focus behavior
or the browser-to-server HTTP path. Use the final run's output for the current
check count. Final cloud browser QA remains blocked before page creation by a
local socket restriction; the supported browser also refused loopback access.
No new native pointer, keyboard, touch or accessibility pass is claimed.

The repository's registered acceptance scenario runs the Python suite,
deterministic Node UI checks, pinned CLI build and actual sample-model factory
replay together:

```sh
python3 tools/atdd.py run --scenario zal-dialogue
```

Its contract is [zal_dialogue.feature](../../acceptance/features/zal_dialogue.feature).
It uses the existing Node and pinned Rust toolchain; it performs no provider
inference or native browser validation. `tools/atdd.py run --all` includes this
scenario alongside the repository's other acceptance checks.

Optional real-browser checks require an already installed Playwright and Chromium. Start
a fresh companion, then run `node integrations/zal/browser_smoke.cjs
http://127.0.0.1:8765/ /tmp/zal-browser`. `CHROMIUM` can select an existing browser;
`NODE_PATH` can locate an existing Playwright installation. This command neither
installs them nor establishes a complete accessibility audit. Final repository
checks are those in [CONTRIBUTING.md](../../CONTRIBUTING.md), not just these suites.
