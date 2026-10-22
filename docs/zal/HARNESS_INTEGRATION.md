# ZAL harness integration for 2.3

Status: implemented locally; final qualification pending, 2026-10-06. This additive feature uses the
existing finite-FSM prototype. It does not expand the fixed 2.2 stage-one journey.

## Deliverable and acceptance

1. A setup command creates a private project workspace, a Codex project MCP
   entry, a Claude project MCP entry, and the same specification skill for both.
   Existing unrelated configuration and reviewed/pending state are preserved.
   Planning performs no writes; each applied file is replaced atomically.
   Interrupted multi-file setup can be resumed; it is not a transactional store.
2. All seven actual tools work through a standard MCP SDK client, including
   stale-revision refusal, comparison witnesses and unaccepted proposals.
   Native configuration inspection is separate from a live-model tool exchange.
3. Human review can accept an initial model after its exact display. Approval
   records bind the model, canonical meaning and checker identity. Tool/command
   permission remains separate from agreement.
4. Workspace export requires that exact reviewed revision, no pending proposal,
current checker evidence and a supported profile. Export verification binds
   every generated declaration and detects changes or stale review. The actual
   factory remains the execution/qualification route.

Run focused Python, MCP, UI and oracle checks for affected changes. Run the
complete registered ATDD suite when preparing a new version for release,
including release candidates, on the exact source (see [AGENTS.md](../../AGENTS.md)). Independent review
and hosted checks qualify publication separately. Current Codex/Claude sessions
must reload their configuration before the new tool catalog is visible.

## Setup and use

Python 3.11 or later is required. Keep the source checkout at a stable absolute
path; the entries refer to it. From that checkout:

```sh
python3 integrations/zal/setup.py --project /path/to/your/project --harness both
python3 integrations/zal/setup.py --project /path/to/your/project --harness both --apply
```

`--harness codex` or `--harness claude` installs only that harness. An optional
`--model /path/to/model.zal` supplies a new workspace's seed; it never replaces
existing state. The default order seed is an example with zero application
invariants. Its consistency check does not prove an unstated authorization policy.

Setup writes `.codex/config.toml` and `.agents/skills/zal-authoring/SKILL.md` for
Codex, `.mcp.json` and `.claude/skills/zal-authoring/SKILL.md` for Claude, and private
state under `.zeno-fcis/zal/`. Changed files have private backups. Detected concurrent
configuration edits and symlinked managed paths refuse. No credentials, provider
calls, global registrations, trust changes or approval-policy changes are needed.
Machine-specific MCP paths belong in local configuration, not a shared commit.

Project configuration loading follows the harness's own trust and permission
settings. Inspect `codex mcp get zal --json` or `claude mcp get zal` from the target
project before starting a fresh session. Codex supports a project-scoped
[MCP config](https://learn.chatgpt.com/docs/extend/mcp?surface=cli); Claude supports
[project MCP entries](https://code.claude.com/docs/en/mcp). A successful inspection
does not establish a model tool exchange. Start a new session to load the skill.

Ask the coding agent to read the shared ZAL model and explain an exact decision.
For a change it compares and proposes; the person separately runs the recorded
review command from `.zeno-fcis/zal/install.json`:

```sh
python3 /path/to/source/integrations/zal/terminal.py review --workspace /path/to/project/.zeno-fcis/zal/workspace.json
```

The person can supply independent examples in that terminal. These illustrative
commands show the format; their labels are not evidence of a real owner's input:

```text
label approval_allowed pending approve authorized=yes expect Accept approved - "illustrative supplied intent"
label approval_refused pending approve authorized=no expect Reject pending not_enabled "illustrative supplied intent"
labels
```

Supply the expected class, successor, reason (`-` for an Accept) and provenance
explicitly. Reusing a case ID replaces that supplied case. The bounded corpus is
preserved across proposals, rejection, acceptance and reload. Comparison replays
the same unchanged expectations against both meanings, reporting conflicts and
out-of-domain cases without generating new labels. These are cooperative intent
assumptions, never authenticated owner identity or approval. Visible conflicts
remain visible after review; binding a corpus does not claim its cases agree.
Meaning acceptance and reviewed declaration export may proceed with a displayed
conflict or out-of-domain case. The receipt binds that supplied corpus; it does
not certify that the owner examples passed or that Python requirements are proved.

Use `candidate`, then `accept` or `reject`. For the initial model use `show`,
review the canonical English and checks, then `accept-current`. The coding agent
refreshes with `zal_read` after the review. Do not let it simulate this gesture.

The preview separates stored check freshness, review bindings and factory
`not-run`. Reading or reloading does not refresh old checks. Restart after checker
source drift; run `check` for an explicit preview and review again to bind current
evidence. Changed examples invalidate old review/export bindings. For a pending
candidate, propose again with the current corpus, then display and review it.
For current meaning, use `show` and `accept-current`. Acceptance binds the corpus
that was displayed, so another client's label change requires a new display.
Explanation and trace views separate supplied observations from authenticated
facts; this profile supplies no authenticated facts, even for `authorized=yes`.
Browser declaration downloads send the displayed revision and example digest.
Both browser and CLI exports check the review binding and refuse stale review or
a pending candidate; a disabled browser button is not the export gate.

For the exact reviewed revision shown by `zal_read`:

```sh
python3 /path/to/source/integrations/zal/workspace.py export --workspace /path/to/workspace.json --revision FULL_REVISION --out /path/to/empty-contract
python3 /path/to/source/integrations/zal/workspace.py verify --workspace /path/to/workspace.json --revision FULL_REVISION --out /path/to/empty-contract
```

These commands emit/check declarations and `zal-review.json`; the factory status
remains `not-run`. Use the existing actual contract-generation and Authority-test
route next. No command here adopts a program or publishes an application.

## Verification

The `zal-dialogue` acceptance scenario includes Python preservation/refusal tests,
the repository's pinned standard MCP SDK exercising all seven tools, Node UI
checks, three independent finite-family oracles and actual sample-domain factory
replay. The SDK test simulates explicit human terminal input; it is not evidence
that a person approved a real application. Run it with repository dependencies
installed and the project's pinned Node version:

```sh
python3 tools/atdd.py run --scenario zal-dialogue
```

The Python tests cover changed/stale/legacy approval, failed rendering, concurrent
events and configuration edits, pending work preservation, unsupported export,
and tampered/missing/extra/symlinked files. They do not prove the Python checker.

## Boundary and next contracts

The tool's Python checker is tested, not a machine-checked theorem. The Rust
Authority remains the verified execution boundary. Current review is a trusted
local gesture; same-user processes can forge files or invoke a review client.
The export gate cannot authenticate a human. No provider or browser model call
is required for setup and deterministic tests.

A later 2.3 contract should protect reviewer authorization and the approval store
outside the coding agent's write authority. Its receipt must bind model/profile,
checker, requirements/examples, reviewer identity and intended realization;
replay and forged/stale approvals must refuse before artifact promotion. This
requires a chosen deployment and credential boundary, not just another skill.

Wider profiles depend on their own semantics and actual lowering qualification:
checked integer widths and eager traps; explicit effects; transition requirements
and independent owner examples; concurrency/event scheduling; temporal scopes.
Neither these profiles nor authentication are supplied by this addon.
Examples-first contract specification (`contract draft`) is a separate feature with
its own acceptance scenario.
