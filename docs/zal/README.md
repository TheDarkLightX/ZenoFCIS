# ZAL: state the intent, then check the contract

ZAL states what must hold. A contract and its code must refine that statement.
Start with the requirement and its declared theory, then choose the reading
that is easiest to understand. The [progression ladder](ladder/README.md)
presents one formula as symbols, English or a question without changing its
declared meaning. Its legend explains the symbols; shorter readings keep the
same definitions and assumptions.

Ask: **Approve an order only when the supplied authorization fact is true.**

The [order example](../../integrations/zal/examples/order.zal) is a finite
behavior model for that requirement. Its English rule says:

> Rule approve_order: when the state is pending and the event is approve, if authorized, move to approved.

Its `zal 1` header selects the finite-machine theory `zal/finite-fsm/1`.
The explicit default rejects other inputs and keeps the state unchanged.
Inspect the statement, export the contract, then run the comparison:

```sh
python3 integrations/zal/run.py render integrations/zal/examples/order.zal --to english
python3 integrations/zal/run.py export integrations/zal/examples/order.zal --out order-contract
python3 integrations/zal/run.py qualify integrations/zal/examples/order.zal --out order-check.json
```

The contract is `order-contract/project.zeno` plus `v2/policy.json`.
`qualify` needs the built CLI and checks the ZAL initial state and every declared
input against the generated application's Authority. A mismatch names the
input and differing result. See the [exact refinement relation](REFINEMENT.md)
for scope, limits and refusal examples. The supplied `authorized` fact is not
itself authentication.

Finite machine profiles have symbolic and controlled-English views; they are
explicit behavior models, not descriptions of core implementation structure.
The formula ladder and those machine profiles remain separate. Built-in help,
translation and input explanations work locally without an LLM. These are
experimental interfaces, not a released ZenoFCIS API. A terminal or browser is
optional for the file-based journey above.

Read the [specification](SPECIFICATION.md), [exact grammar](GRAMMAR.md),
and [factory source map](../ZAL_SOURCE_MAP.md).
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

### Ask why not, what if, and for the story

Three further explanation commands use the same typed model and no provider:

```text
whynot pending approve authorized=no want Accept
whynot draft approve authorized=no want Accept approved scope input
whatif pending approve authorized=no change authorized=yes
story
story trace submit authorized=no then approve authorized=yes
```

`whynot` searches a stated finite scope for the fewest input changes that give
the wanted outcome. `want` takes `Accept [TARGET]`, `Reject [REASON]` or
`rule ID`. The default scope keeps the state and event fixed and tries every
context valuation; `scope input` searches every declared input. It reports each
minimal input, the rule that would fire and that rule's statement, and says
plainly when no input in the scope (or in the whole declared domain) gives it.
For the first example it reports one change, `authorized` from no to yes, and
`approve_order`. Inputs are hypotheses: each witness also says whether its
source state is reachable.

`whatif` applies every named change (`fact=yes`, `state:NAME`, `event:NAME`)
and compares the two decisions rule by rule. Undeclared names are refused, and
a change to the value already supplied is reported as having no effect.

`story` turns the current check's counterexample (determinism, a reachable
invariant violation or a raw Accept that breaks an invariant) into a short
numbered story; `story candidate` does the same for the pending candidate, and
`story trace ...` narrates assumed inputs. Every sentence is a registered
template filled from the witness, paired with a symbolic line, and re-derived
from the model before it is shown. For example, adding
`invariant never_cancelled: !(state == cancelled);` to the seed gives
"Step 2: in pending, event cancel arrives with authorized=no; rule cancel_order
accepts and moves to cancelled." and `pending + cancel (authorized=false) ->
accept cancelled [cancel_order]`.

`guide TOPIC` is construct help (`zal/help/2`) generated from the construct
registry: meaning, symbolic and English forms, the model's own lines and the
typed IDs that use an operator. It resolves the same topics as `?`. Bare
`guide` lists every construct with its forms. A drift test requires a registry
entry for every statement kind, Boolean node and unsupported kind, and requires
the live parser to accept every registry form.

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
these model tools:

| Tool | Required arguments | Result/purpose |
| --- | --- | --- |
| `zal_read` | `{}` | Current model/revision, typed IDs, selection, pending proposal and evidence. |
| `zal_check` | `revision` | Finite checks on that exact current revision. |
| `zal_help` | `revision`, `topic` | Built-in grammar or typed-object help; use an empty topic for the index. |
| `zal_explain` | `revision`, `state`, `event`, `context` | Exact decision and recursive guard evaluations for one complete Boolean input. |
| `zal_compare` | `revision`, `syntax`, `surface` | Compare a complete candidate surface without staging it. |
| `zal_trace` | `revision`, `inputs` | Replay an array of `{event, context}` inputs from the initial state. |
| `zal_propose` | `base_revision`, `object_ids`, `message`, `syntax`, `surface` | Parse and stage a complete candidate with checks and diff; never accept it. |
| `zal_why_not` | `revision`, `state`, `event`, `context`, `want`, optional `scope` | Fewest input changes giving a wanted outcome in the stated scope. |
| `zal_what_if` | `revision`, `state`, `event`, `context`, `change` | Apply named changes and compare both decisions rule by rule. |
| `zal_narrate` | `revision`, `source` (`check`, `candidate`, `trace`), optional `inputs` | Numbered story of a counterexample or trace, paired with symbolic lines. |
| `zal_guide` | `revision`, `topic` | Construct help (`zal/help/2`) generated from the construct registry. |

The ladder tools work on the shipped [ladder examples](ladder/README.md) and,
when `ZAL_LADDER_DIR` names a directory, its `*.json` statements as
`project:NAME`. Each statement has its own revision: a SHA-256 of its canonical
JSON. Read first, then pass the statement ID and exact revision; a stale
revision is refused with `Stale ladder revision; call zal_ladder_read again`.
There is no ladder write or acceptance tool.

| Tool | Required arguments | Result/purpose |
| --- | --- | --- |
| `zal_ladder_read` | `{}` | Statement IDs, revisions, profiles and scope identities. |
| `zal_ladder_render` | `statement`, `revision`, optional `rung` | All seven rungs (or one), scope strip, legend and evidence status. |
| `zal_ladder_parse` | `statement`, `revision`, `rung`, `text` | Exactly one normalized tree with its uniqueness counts, or a named refusal. |
| `zal_ladder_same` | `statement`, `revision`, and `other_statement`/`other_revision` or `rung`/`text` | Same normal form and scope, with the evidence. |
| `zal_ladder_lower` | `statement`, `revision` | The inert `.zeno` claim and its obligations; authority none. |
| `zal_ladder_explain` | `statement`, `revision`, optional `rung`, `text` | Which rung admits a text, why its parse is unique, and what the scope strip means. |

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

### Optional tool integration

The [2.3 addon guide](HARNESS_INTEGRATION.md) provides persistent project-local
MCP entries and a shared specification skill. Run `integrations/zal/setup.py` from a
stable source checkout, first without `--apply` to inspect the plan. It preserves
unrelated configuration and pending work, and refuses a differing existing ZAL
entry or skill. Installation does not change tool approval policies or call a
model. A connected coding tool must refresh the exact revision before checking
or proposing a change. Tool permission and human acceptance of the statement
remain separate actions.

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

## Optional browser and provider conversation

To view and review the same existing terminal/MCP workspace:

```sh
python3 integrations/zal/run.py ui --workspace "$ZAL_WORKSPACE" --port 8765
```

Open `http://127.0.0.1:8765/`. The companion starts in local replay mode; replay
is a disclosed fixture, not a live model. It provides paired rule views, graph
and table selection, traces, witnesses, built-in contextual help and
exact-revision review. `Explain next input` evaluates the current trace state,
selected event and every Boolean checkbox; `Why this recorded step?` explains
the exact input at the trace cursor. The provider control opens a separate
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

The browser's optional provider connection uses the separate
[`codex_transport.py`](../../integrations/zal/codex_transport.py) app-server
client. That mode owns a conversation; it does not attach a pane or send
events into an existing terminal session. It uses stdio, an ephemeral read-only
thread, structured final output and exact turn IDs, and refuses execution
approval requests. Actual permitted read-only tools must still be observed;
an instruction to avoid tools is not a physical isolation guarantee.
Live use is separate from default offline checks. See evidence distinctions.

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
masks, then compare the exact hashed implementation. Profile 2 has its own
oracle and planted-defect run, both also in `zal-dialogue`:

```sh
python3 integrations/zal/oracles/temporal_oracle.py
python3 -B integrations/zal/check_behavior2.py planted
```

The temporal oracle judges small machines by brute-force lasso enumeration
and finite words by the ZAL-T macro expansion, without the checker's search.
Each planted defect must fail its named test before the baseline passes. Each prints its case counts
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
check count. A pass of the DOM-double tests does not establish native pointer,
keyboard, touch or accessibility behavior.

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

## Profile 2: integers, requirements and temporal properties

A source that starts with `zal 2;` or `ZAL version 2.` uses
`zal/finite-fsm/2` ([grammar](GRAMMAR.md#profile-zalfinite-fsm2),
[semantics](SPECIFICATION.md#profile-zalfinite-fsm2-integers-requirements-and-temporal-properties)).
The file-level commands select the profile from that header:

```sh
python3 integrations/zal/run.py check integrations/zal/examples/withdrawal2.zal
python3 integrations/zal/run.py render --to english integrations/zal/examples/keeper2.zal
python3 integrations/zal/run.py diff integrations/zal/examples/order2.zal candidate.zal
python3 integrations/zal/run.py qualify integrations/zal/examples/order2.zal
```

`withdrawal2.zal` is a one-lane abstraction of the withdrawal-queue
template's alarm controller: "an alarm can delay a withdrawal but never
freeze it" is a liveness property under the fairness assumption that the
keeper keeps ticking. `keeper2.zal` is the keeper's tick-only view, where
"every request is paid within four ticks" is a bounded `within` property.
Both are review abstractions, not bindings to the template's contract, and
their properties are finite checked over the declared machine only.
`order2.zal` adds "approve only if authorized" as a transition requirement.
Weakening its approval guard to `true` now fails with a requirement
counterexample.

`lower --out DIRECTORY`, `export` and `qualify` use the existing factory
lowerer. Step safety properties `always(P)` and `never(P)` become contract
laws when they hold on every admissible step. Other temporal properties and
fairness still refuse as unsupported lowering. `refine-check SOURCE CONTRACT
--cli PATH` delegates to the existing native refinement checker. See the
[lowering guide](LOWERING.md), including its lineage in Baier and Katoen,
*Principles of Model Checking* (MIT Press, 2008), and Abadi and Lamport,
*The Existence of Refinement Mappings* (Theoretical Computer Science, 1991).
The review terminal, MCP tools and browser workspace
still use profile 1; `run.py ui` refuses a profile 2 model.

The explanations work on profile 2 models from a file, through `run.py`:

```sh
python3 integrations/zal/run.py story MODEL.zal
python3 integrations/zal/run.py story MODEL.zal --trace INPUTS.json
python3 integrations/zal/run.py why-not integrations/zal/examples/withdrawal2.zal --scope input \
  --input '{"state": "idle", "variables": {"balance": 0, "must_serve": 0, "pause": 0, "reserved": 0}, "event": "request", "context": {"alarm": false, "amount": 1}}' \
  --want '{"class": "Accept"}'
python3 integrations/zal/run.py what-if MODEL.zal --input '{...}' --change '{"variables": {"balance": 2}}'
```

`story` tells the check's counterexample. Range traps, requirement failures,
safety, strong-next and within-k violations and fair lassos each get their own
sentences. For example, removing `must_serve = 1` from the lane's `wait_last`
rule gives a lasso. After two steps, "From here the next 3 steps repeat
forever." lists the alarm cycle, and "Fairness keeper_ticks (event is tick)
holds at cycle step 1" shows that the cycle is fair. The story ends: "state is
waiting and state after this event is idle is false at every step from step 2
on, including every step of the repeating cycle, so it never happens." In
`why-not` and `what-if`, variables and integer inputs are inputs like any
other, and `want` can name `Trap` to find the nearest input whose update
leaves a range. The same commands accept profile 1 models without `variables`.

### Game properties

The withdrawal example checks whether the user can keep withdrawing while
the operator controls the other states and reopens the window infinitely
often:

```sh
python3 integrations/zal/run.py check integrations/zal/examples/withdrawal_game2.zal
python3 integrations/zal/run.py export-game integrations/zal/examples/withdrawal_game2.zal --property keep_withdrawing --out /tmp/withdrawal-game/project.zeno
zeno-fcis check --expand /tmp/withdrawal-game/project.zeno
```

The first command prints this JSON:

<!-- withdrawal-game-check-output -->
```json
{
  "status": "pass-with-scope",
  "revision": "62f361bbe3dbdb74bdbfe7050626e26f4d029990be5ddc1d37f9187dd729b8e0",
  "profile": "zal/finite-fsm/2",
  "checker": "6546f5b0528f21913187a9686ea1b5bbe1d07b74e5d2f33f69f96a1708fdd097",
  "scope": "Finite atomic steps over declared control states and integer variables; context ranges over fresh Boolean and integer inputs on every step. Ranges, invariant preservation and requirements cover every admissible configuration (declared and satisfying every invariant); properties range over every infinite execution from the initial configuration, liveness only over executions satisfying every fairness declaration; game properties range over every play from the initial configuration in which each player chooses the event in its own states and the other player chooses the context.",
  "checks": {
    "initial_nonempty": "pass-with-scope",
    "typing": "pass-with-scope",
    "determinism": "pass-with-scope",
    "update_ranges": "pass-with-scope",
    "coverage_with_explicit_default": "pass-with-scope",
    "reachable_invariants": "pass-with-scope",
    "invariant_preservation": "pass-with-scope",
    "transition_requirements": "pass-with-scope",
    "temporal_properties": "pass-with-scope",
    "game_properties": "pass-with-scope"
  },
  "input_tuples": 9,
  "configurations": 3,
  "admissible_configurations": 3,
  "reachable_configurations": 3,
  "reachable_states": [
    "open",
    "paused",
    "withdrawn"
  ],
  "unreachable_rules": [],
  "invariant_count": 0,
  "requirement_count": 0,
  "property_count": 1,
  "fairness_count": 1,
  "properties": {
    "keep_withdrawing": {
      "status": "pass-with-scope",
      "kind": "game",
      "player": "user",
      "objective": "infinitely often",
      "english": "Property keep_withdrawing: user can force (state is withdrawn) to happen infinitely often.",
      "configurations": 3,
      "winning_configurations": 3,
      "initial_wins": true,
      "solvers": [
        "fixed-point",
        "parity"
      ]
    }
  },
  "advisories": [],
  "default_count": 5,
  "default_witness": {
    "state": "open",
    "variables": {},
    "event": "pause",
    "context": {},
    "outcome": {
      "class": "Reject",
      "state": "open",
      "variables": {},
      "reason": "not_allowed",
      "rule": "default"
    }
  },
  "unresolved": [],
  "factory": "not-run",
  "authority": "none",
  "game_property_count": 1
}
```

The export command prints:

<!-- withdrawal-game-export-output -->
```json
{
  "status": "exported",
  "format": "contract-tables",
  "property": "keep_withdrawing",
  "nodes": 3,
  "out": "/tmp/withdrawal-game/project.zeno",
  "authority": "none"
}
```

When `$ZENO_FCIS_BIN` is set, the replay test runs the third command and
checks each `from_` law against the fixed-point solver. With it unset, the
external expansion is skipped; the ZAL check and table export still run. The
`check` and `export-game` output shown here is compared byte for byte by the
profile 2 game tests. The exporter supports up to 64 nodes and includes a
`stable` law that must expand to exactly `true`. Native formula expansion has
a lower practical limit: measured expansion is about 20 nodes with one goal
and one fairness assumption, and about 12 nodes with two of each. Beyond that
limit, `zeno-fcis check --expand` refuses and the ZAL check result stands.

### Bounded game example

The deadline example asks whether the user can reach `withdrawn` within one
rule application, while the operator chooses the context. It exercises the
inclusive deadline and whole-step context rule.

```sh
python3 integrations/zal/run.py check integrations/zal/examples/deadline_game2.zal
python3 integrations/zal/run.py export-game integrations/zal/examples/deadline_game2.zal --property quick_withdrawal --out /tmp/withdrawal-deadline/project.zeno
zeno-fcis check --expand /tmp/withdrawal-deadline/project.zeno
```

The first command prints:

<!-- deadline-game-check-output -->
```json
{
  "status": "pass-with-scope",
  "revision": "f9d5a4db71761f0ef6b9a5dff04bfec3189c617f56ff8cfe18b77facd49e9aef",
  "profile": "zal/finite-fsm/2",
  "checker": "6546f5b0528f21913187a9686ea1b5bbe1d07b74e5d2f33f69f96a1708fdd097",
  "scope": "Finite atomic steps over declared control states and integer variables; context ranges over fresh Boolean and integer inputs on every step. Ranges, invariant preservation and requirements cover every admissible configuration (declared and satisfying every invariant); properties range over every infinite execution from the initial configuration, liveness only over executions satisfying every fairness declaration; game properties range over every play from the initial configuration in which each player chooses the event in its own states and the other player chooses the context.",
  "checks": {
    "initial_nonempty": "pass-with-scope",
    "typing": "pass-with-scope",
    "determinism": "pass-with-scope",
    "update_ranges": "pass-with-scope",
    "coverage_with_explicit_default": "pass-with-scope",
    "reachable_invariants": "pass-with-scope",
    "invariant_preservation": "pass-with-scope",
    "transition_requirements": "pass-with-scope",
    "temporal_properties": "pass-with-scope",
    "game_properties": "pass-with-scope"
  },
  "input_tuples": 8,
  "configurations": 2,
  "admissible_configurations": 2,
  "reachable_configurations": 2,
  "reachable_states": [
    "open",
    "withdrawn"
  ],
  "unreachable_rules": [],
  "invariant_count": 0,
  "requirement_count": 0,
  "property_count": 1,
  "fairness_count": 0,
  "properties": {
    "quick_withdrawal": {
      "status": "pass-with-scope",
      "kind": "game",
      "player": "user",
      "objective": "within 1",
      "english": "Property quick_withdrawal: user can force (state is withdrawn) to happen within 1 steps.",
      "configurations": 2,
      "winning_configurations": 2,
      "initial_wins": true,
      "solvers": [
        "fixed-point",
        "parity"
      ]
    }
  },
  "advisories": [],
  "default_count": 4,
  "default_witness": {
    "state": "open",
    "variables": {},
    "event": "wait",
    "context": {
      "signal": false
    },
    "outcome": {
      "class": "Reject",
      "state": "open",
      "variables": {},
      "reason": "not_allowed",
      "rule": "default"
    }
  },
  "unresolved": [],
  "factory": "not-run",
  "authority": "none",
  "game_property_count": 1
}
```

The export command prints:

<!-- deadline-game-export-output -->
```json
{
  "status": "exported",
  "format": "contract-tables",
  "property": "quick_withdrawal",
  "nodes": 2,
  "out": "/tmp/withdrawal-deadline/project.zeno",
  "authority": "none"
}
```

The native expansion prints:

<!-- deadline-game-native-output -->
```text
zeno 1;
project 1 constant_role_hierarchy;
namespace 2 permissions;
type 100 state Roles;
type 101 command Request;
type 102 context Session;
type 105 int Parent in 0..=2;
type 106 int Role in 0..=1;
type 107 int Grant in 0..=1;
type 108 int ContextChoice in 0..=1;
// Who moves: 0 = the property's player, 1 = the other player.







field 210 100 grants_0 107;
field 211 100 grants_1 107;
field 120 101 role 106;
variant 130 102 Session none;
reason 200 permission_required precedence 0;
component 400 inheritance {
  owns 100;
  reads pre.100;
  writes post.100;
  contexts context.102;
  budget steps 4096;
}
merge [400];


// n0: configuration state=open variables=
law 600 from_n0 on commit = true;
// n1: configuration state=withdrawn variables=
law 601 from_n1 on commit = true;
```

The README replay test compares the first two command outputs byte for byte
and, when `ZENO_FCIS_BIN` is set, compares the native `from_` laws with solver
A. Bounded exports contain `r0` through `rK` and have no `stable` law.


### Several recurring goals

The recurring-goals example checks that every play visits both states infinitely often, even though the operator controls the withdrawn state:

```sh
python3 integrations/zal/run.py check integrations/zal/examples/recurring_goals_game2.zal
python3 integrations/zal/run.py export-game integrations/zal/examples/recurring_goals_game2.zal --property both_goals --out /tmp/recurring-goals/project.zeno
zeno-fcis check --expand /tmp/recurring-goals/project.zeno
```

The first command prints:

<!-- multi-goal-check-output -->
```json
{
  "status": "pass-with-scope",
  "revision": "5eb629ebc9bff7c65dc6e06613dd1126f0cf4775a080f161adabbfb7531da3af",
  "profile": "zal/finite-fsm/2",
  "checker": "6546f5b0528f21913187a9686ea1b5bbe1d07b74e5d2f33f69f96a1708fdd097",
  "scope": "Finite atomic steps over declared control states and integer variables; context ranges over fresh Boolean and integer inputs on every step. Ranges, invariant preservation and requirements cover every admissible configuration (declared and satisfying every invariant); properties range over every infinite execution from the initial configuration, liveness only over executions satisfying every fairness declaration; game properties range over every play from the initial configuration in which each player chooses the event in its own states and the other player chooses the context.",
  "checks": {
    "initial_nonempty": "pass-with-scope",
    "typing": "pass-with-scope",
    "determinism": "pass-with-scope",
    "update_ranges": "pass-with-scope",
    "coverage_with_explicit_default": "pass-with-scope",
    "reachable_invariants": "pass-with-scope",
    "invariant_preservation": "pass-with-scope",
    "transition_requirements": "pass-with-scope",
    "temporal_properties": "pass-with-scope",
    "game_properties": "pass-with-scope"
  },
  "input_tuples": 2,
  "configurations": 2,
  "admissible_configurations": 2,
  "reachable_configurations": 2,
  "reachable_states": [
    "open",
    "withdrawn"
  ],
  "unreachable_rules": [],
  "invariant_count": 0,
  "requirement_count": 0,
  "property_count": 1,
  "fairness_count": 0,
  "properties": {
    "both_goals": {
      "status": "pass-with-scope",
      "kind": "game",
      "player": "user",
      "objective": "infinitely often",
      "english": "Property both_goals: user can force (state is open) and (state is withdrawn) to each happen infinitely often.",
      "configurations": 2,
      "winning_configurations": 2,
      "initial_wins": true,
      "solvers": [
        "fixed-point",
        "parity"
      ]
    }
  },
  "advisories": [],
  "default_count": 0,
  "default_witness": null,
  "unresolved": [],
  "factory": "not-run",
  "authority": "none",
  "game_property_count": 1
}
```

The export command prints:

<!-- multi-goal-export-output -->
```json
{
  "status": "exported",
  "format": "contract-tables",
  "property": "both_goals",
  "nodes": 2,
  "out": "/tmp/recurring-goals/project.zeno",
  "authority": "none"
}
```

The native expansion prints:

<!-- multi-goal-native-output -->
```text
zeno 1;
project 1 constant_role_hierarchy;
namespace 2 permissions;
type 100 state Roles;
type 101 command Request;
type 102 context Session;
type 105 int Parent in 0..=2;
type 106 int Role in 0..=1;
type 107 int Grant in 0..=1;
// Who moves: 0 = the property's player, 1 = the other player.





field 210 100 grants_0 107;
field 211 100 grants_1 107;
field 120 101 role 106;
variant 130 102 Session none;
reason 200 permission_required precedence 0;
component 400 inheritance {
  owns 100;
  reads pre.100;
  writes post.100;
  contexts context.102;
  budget steps 4096;
}
merge [400];
// w(k,l,i)[s]: the other player can escape middle stratum (k,l) through assumption i.






// win[s]: the property player wins from s under the fairness assumptions.


law 500 domains on commit, genesis = (post.100.210 >= 0 && post.100.210 <= 1 && (post.100.211 >= 0 && post.100.211 <= 1));
law 510 stable on commit = true;
law 600 from_open on commit = true;
law 601 from_withdrawn on commit = true;
```

The README replay test compares the check and export outputs byte for byte.
When `ZENO_FCIS_BIN` is set, it also checks the native output. One `stable`
law checks the proposed per-goal strata; the tables name the recurring goals
`goal0`, `goal1`, and so on.

## Types and effect signatures

`zal/effects/1` writes the shell's effect ports in ZAL, and `zal/typed-fsm/1`
gives profile 2 variables and inputs finite types. See
[types and effects](EFFECTS.md):

```sh
python3 integrations/zal/check_effects.py native-build
python3 integrations/zal/run.py check integrations/zal/examples/shell_effects.zal
python3 integrations/zal/run.py explain integrations/zal/examples/shell_effects.zal --topic port:Commit --human
python3 integrations/zal/run.py check integrations/zal/examples/relay_typed.zal
```

The first command builds the small Rust printer that the byte-for-byte test
compares against.

## Experimental progression ladders

The [ladder prototype](ladder/README.md) presents a statement through seven
deterministic readings. Its separate temporal profile uses explicit event
order, finite or unbounded scope, and versioned assumptions. Rendering and
native-source previews have authority none. The [withdrawal
pilot](ladder/WITHDRAWAL_PILOT.md) binds the withdrawal statements to the
withdrawal-queue contract: its bounded claim is finite checked through length
12, and its unbounded claim is kernel checked by Lean under a stated fairness
premise. The vault and interface examples use proposed definitions awaiting
owner confirmation.

The same statements are available through the `zal_ladder_*` MCP tools above
and the review terminal's `ladder` commands, which bind the revision read when
the command runs:

```text
ladder list
ladder render example:temporal/vault_drain plain
ladder parse example:ladders/dex_k typed forall t in T: S(t) => x'(t)*y'(t) >= x(t)*y(t)
ladder same example:temporal/vault_drain plain No admitted operation sequence drains the vault.
ladder explain example:temporal/vault_drain No admitted operation sequence drains the vault.
ladder lower example:temporal/vault_drain
```

`ladder explain` without a rung tries every rung, names the rungs that admit
the text and gives the parse counts behind its uniqueness. `same` reports
"same" only for identical expanded normal forms with an identical scope
identity in either profile. The identity binds all declarations except the
display ID, formula text and fixtures. Different normal forms do not prove
different meanings.

## Lineage and scope

**Background.** Niklaus Wirth, “Program Development by Stepwise Refinement,”
*Communications of the ACM* 14(4), 1971, describes successive program refinement.
Matthew B. Dwyer, George S. Avrunin and James C. Corbett, “Patterns in Property
Specifications for Finite-State Verification,” *ICSE*, 1999, organizes recurring
temporal requirements as patterns. Tobias Kuhn, “A Survey and Classification of
Controlled Natural Languages,” *ACM Computing Surveys* 46(3), article 41, 2014,
surveys constrained natural-language systems.

**What is new here.** This integration connects declared statements and readable
views to existing finite checks and generated-application comparisons.
Prior-art search pending.

**Scope.** A check covers its named finite domain or explicitly stated proof
obligation. Reading, review and human acceptance grant no runtime, publication
or authentication authority. A readable requirement still needs a checked
refinement before its implementation can claim to satisfy it.
