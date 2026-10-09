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

Three transform-loop tools drive the bounded optimization loop
(`zeno-fcis loop`, see the [CLI reference](CLI_REFERENCE.md)):
`transform_request(original_path, session_path, profile, attempts, checks,
deadline_ms)` admits an original and opens a session; `transform_candidate
(session_path, candidate_path | candidate_json_path)` spends one attempt on a
candidate the agent supplies, as canonical bytes or in the fixture vocabulary,
and returns checker-derived feedback (a replayed counterexample, an admission
refusal, an incomplete check, or an equivalence with its cost and selection
reason) together with the current incumbent's program; `transform_replay
(session_path)` re-admits the session, verifies its ledger and replays the
incumbent before reporting it. The tools accept typed paths, closed profile
names and bounded integers only; there is no argument by which a caller can
assert that a candidate passed, and an agent's explanations are provenance,
not evidence. Hosted model providers stay disabled. A `best-checked-so-far`
incumbent is functionally equal to the original on its declared domain under
eager semantics and no more costly in nodes and bytes; it is not optimal,
not a new application version, and grants no authority. The handlers live in
`integrations/mcp/zeno_fcis_transform.py` and are tested without the MCP SDK by
`python3 integrations/mcp/test_transform_tools.py` (set `ZENO_FCIS_CLI` or build
the CLI first); `test_server.py` lists them with the other tools.

Six examples-first drafting tools use the same CLI through the existing transform
integration: `contract_draft_start(session_path, intent_path, project_path,
provenance, examples_path="", rounds=4, max_tuples=4096)`,
`contract_draft_propose(session_path, revision, rules_path, provenance)`,
`contract_draft_questions(session_path)`,
`contract_draft_label(session_path, revision, examples_path, provenance)`,
`contract_draft_check(session_path)`, and
`contract_draft_finalize(session_path, revision, out_path)`. Start from an intent
file and fixed declarations, supply a complete rules proposal, present F2's
questions to the owner, then submit explicitly supplied expected decisions.
Repeat with the returned exact revision. Hosted models stay off; an MCP agent
is the external proposer. Do not convert a suggested answer into an owner label.

Draft reports separate assessed proposals from a ready/finalized draft. Every
label's supplied provenance is an assumption, not authentication or adoption
permission. The mutable session grants no authority. Finalization rechecks all
labels and questions, writes a new draft only on agreement, and remains advisory.
See the [CLI workflow, bounds and concurrency contract](CLI_REFERENCE.md#examples-first-contract-drafting).
`DraftToolsTest` in `test_draft_tools.py` checks wrapper routing/refusals with
mocked processes; those controls do not establish native CLI behavior. Its
`NativeDraftPathTest` requires a built CLI and checks existing-path preservation
through the wrappers. The G7 acceptance scenario also exercises native drafting
and generated apps. None of these fixtures establishes a real owner conversation.

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

## Proof and specification review

Two additional skills are available from the same checkout:

- [fixed-spec-proof-review](../skills/fixed-spec-proof-review/SKILL.md) checks
  the exact theorem, transitive definitions, permitted assumptions, and
  proof replay. A compiled proof of a weakened or changed obligation is
  insufficient.
- [specification-non-vacuity](../skills/specification-non-vacuity/SKILL.md)
  checks that required success remains possible, premises and relevant
  states are meaningful, and deny-all variants fail independent examples.

They complement the synthesis-first skill and work across projects. They
provide review instructions, not a new application authority or automatic
proof service. For project-local Codex installation, copy each directory
into `.agents/skills/`; for Claude Code use `.claude/skills/`. Preserve any
existing skill with that name and keep the source revision with the copied
instructions. Start a fresh harness session to load newly installed skills.

The [frozen proof challenge](../experiments/proof-challenge/README.md) records
the pinned Lean Comparator pilot and its planted controls. Its small Boolean
model is separate from the Rust core. The runner accepts only its reviewed
fixtures, and the development shim provides no sandbox for arbitrary agent
submissions. Follow that README for reproduction and the named trust boundary.
