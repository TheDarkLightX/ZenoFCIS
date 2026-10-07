# ZAL prototype source map

Inspection revision: `3245e62dc0b9fed5e87f4e55e9465fb44a03381c`,
`agent/v21-factory-20261004`, 2026-10-06. This is a development branch,
not a released V2.1 API. Remote `main` was
`b3b6bbf03315e89288b8d3ab119242152887e513`. V2 PR 119's
`8eda3096cab51ea69601a6e6c5c34ee60e86e840` is an ancestor of this revision.
Factory PR 120 remains open. The prototype is a separate branch and does not
change a running application, release, database or publication route.

## Implemented surfaces

| Boundary | Actual source at the inspection revision | Meaning for ZAL |
| --- | --- | --- |
| Original declarations | `crates/zeno-fcis-spec/src/{lexer,parser,ast,elaborate,logic}.rs` | `.zeno` already describes types, reasons, channels, scoped laws and claims. It is not an unrestricted programming language. |
| Factory inputs | `crates/zeno-fcis-cli/src/contract.rs:68`, `contract/rules.rs:19` | `project.zeno`, `v2/policy.json`, optional schema origin, and retained adoption artifacts form a contract. Rules use schema `zeno-fcis/template-declarative-policy/2`; unknown and duplicate keys are refused. |
| Contract generation | `crates/zeno-fcis-cli/src/contract.rs:255`, `contract_files.rs:188` | Pure generation consumes supplied contents, checks catalog binding, emits schema, Rust declarations and library-encoded policy. The file shell writes them. Generation does not certify human intent. |
| Factory finite profile | `crates/zeno-fcis-cli/src/contract/model.rs:22`, `contract/expr.rs` | At most 32 inputs, 16 outputs and 256 eager scalar nodes. Boolean and checked i64 computations are supported. Context facts are explicit inputs. Eager overflow/refusal is observable. |
| Meaning review | `crates/zeno-fcis-cli/src/review_command.rs:58`, `contract/review.rs`, `contract/review/packet.rs:43` | Decision tables, example disagreements, mutations and replayable distinguishing inputs. Reports distinguish full-domain and boundary-set searches and have `authority: none`. Findings and law refusals affect the command result. |
| Export and scaffolding | `crates/zeno-fcis-cli/src/contract_files.rs:227`, `:317` | `new --contract --source` builds a generated application around the existing route; `contract export-program` emits the current canonical decision graph. |
| Executable scalar program | `crates/zeno-fcis-synthesis/src/finite/ir.rs:54`, `:110` | Owned admitted DAG; `execute_v2` uses the actual metered evaluator. A surface renderer cannot substitute its own evaluator for factory evidence. |
| Replacement checker | `crates/zeno-fcis-cli/src/transform.rs:190`, `:443` | Exhaustive complete declared input-domain comparison, under eager semantics and a nonbinding Step limit; canonical receipt and replay. Output/failure equivalence is separate from sealed usage preservation. The checker is a tested judge, not itself a proved checker core. |
| Optimization | `crates/zeno-fcis-cli/src/optimize/`, `optimize_command.rs`, `loop_command.rs` | Bounded e-graph and neural proposals go through the replacement checker; attempts/resume have durable accounting. No optimality or convergence claim. Hosted provider is separately gated. |
| Adoption | `crates/zeno-fcis-cli/src/contract_adopt.rs:78`, `contract/adoption.rs` | Replays exact receipts, updates the contract lineage, and requires an explicit Step-usage decision. A ZAL discussion agreement is not this adoption authority. |
| Normal public API | `crates/zeno-fcis/src/program.rs:36` | Reexports the existing `bind_catalog`, `bind_program`, `ProgramDefinition`, `Invocation`, `Program`, and `Publication` family. Declarations and `policy_bytes` remain untrusted proposals. |
| Authority | `crates/zeno-fcis-synthesis/src/finite/execution_v2/authority/bound.rs:79`, `:186`, `:222` | Exact catalog binding, original-envelope evaluation, required laws/genesis, and private publication custody. No application callback supplies the decision. |
| Publication | `crates/zeno-fcis-synthesis/src/finite/execution_v2/authority/publication.rs:49` | Private non-Clone capability carrying the actual committing evaluation, original poststate and ordered deliveries. Reports or serialized subjects cannot construct it. |
| Persistence and upgrade | `crates/zeno-fcis-shell-sqlite/src/v2.rs:452`, `v2/upgrade.rs:93`, `:364` | Mandatory capability-based SQLite route, exact identity/replay, and same-schema upgrade premises. Data migration and changed-business-rule evolution are future features. |
| Existing agent bridge | `integrations/mcp/zeno_fcis_synthesis.py`, `zeno_fcis_transform.py` | Fixed authoring/check/synthesis/loop commands with no publication claim. Existing MCP registration is optional; the ZAL MVP need not modify accounts or MCP configuration. |

`docs/CONTRACT_RULES.md` describes ordered cases: the first matching case wins
and the final case must be literal `true`. ZAL can choose stricter source
semantics (reject overlapping enabled transitions) before lowering, and retain
that choice in its versioned profile. It must not silently inherit precedence.
`crates/zeno-fcis-cli/contract-app/tests/decisions.rs` tests each independent
decision example against the real Authority, genesis and a persistent journey.

## V2.2 is a plan, not an installed API

`docs/V2_1_V2_2_PLAN.md` is the 2026-10-05 plan of record and supersedes
`V2_1_FACTORY_PLAN.md`. F1-F7 implementations are present in the inspected
tree (F7 commit `1c60424`), while exact-head review, CI and release status remain
separate. G1-G14 describe future V2.2 capabilities. In particular:

- G7, examples-first authoring and owner labels, directly motivates behavioral dialogue.
- G8, structural change classification and plain-language diff, motivates revision views.
- G3 owns wider and compound values and is the only planned core-changing feature.
- G10/G11 own stronger checker work accounting and a verified checker core.
- G14 covers behavior-changing evolution, symbolic checks and operational app tools.

No separate V2.2 branch or production entry point was identified in the remote
branch inventory. Planning prose is not evidence that these features execute.

## Additive prototype implemented for review

`integrations/zal/` contains a standard-library Python research prototype,
outside the verified core and root Cargo manifest. `behavior.py` parses two
deterministic surfaces into one bounded, owned typed model; `workflow.py`
separates proposals, evidence and human agreement. `shared.py`, `terminal.py`
and `mcp.py` provide the terminal-first cooperative workspace and seven
read/check/help/explain/compare/trace/propose tools. There is no MCP acceptance
tool. `behavior.py:help_topic` supplies deterministic `zal/help/1` grammar and
typed-object help; `behavior.py:explain` evaluates one complete input and
exposes source/event matches, recursive guard truth, exact outcome/default and
frame. `terminal.py` routes `?`, `man`, `help TOPIC`, `legend TOPIC`, canonical
`translate` and `why` without a provider. `server.py` exposes the same help and
explanation functions to the optional browser.

The browser's visible help controls and pointer/focus previews resolve canonical
typed objects or grammar topics. Local `Explain next input` and
`Why this recorded step?` are distinct from `Ask Codex`. No editable-source-span
mapping or language server is implemented. `workspace.html` rejects stale help
responses across request generations, model revisions, selection and dismissal.
The help renderer uses the checker source closure as its implementation identity;
this is separate from the model digest and grants no evidence or review authority.

The implemented `zal/finite-fsm/1` profile has finite control-state/event enums,
fresh Boolean context, total Boolean guards, fixed targets, noncommitting
reasoned rejection and an explicit default. State invariants are checked over
the complete finite graph, with raw Accept targets checked separately. The
checker exposes invariant count and separate advisories for no application
invariants, impossible-at-source guards, unreachable sources and invariants true
throughout the declared state domain. These do not add acceptance obligations.
The seed has no application invariants: weakening its approval guard to `true`
passes consistency while the semantic diff exposes unauthorized approval under
the supplied false authorization observation. Effects,
fairness/liveness, arithmetic and concurrency can be retained as unsupported
obligations; wider executable semantics are refused. The
[grammar](zal/GRAMMAR.md), [specification](zal/SPECIFICATION.md) and
[research/provenance](zal/RESEARCH.md) now describe this implemented cut.

`factory.py` lowers to `project.zeno`, `v2/policy.json`, a feasible
`tests/decision-examples.txt` journey, a whole-domain reference table, source
map and generated Authority comparison test. Qualification runs actual contract
generation, drift checking, advisory review, scaffolding and app tests against
the pinned source. Interpreter checks remain distinct from actual Authority
correspondence. No authoring result is an agreement, Authority, Publication,
replacement receipt or deployment approval. The generated test obtains its
Authority through the existing library route. See [usage](zal/README.md).

The factory-qualified subset requires at least two states to satisfy the
unchanged scaffold's independent rejected-genesis check. The language itself
still admits and checks one-state models. Factory node/shape limits are separate.
`run.py qualify` locates the debug CLI through `CARGO_TARGET_DIR` when set;
an explicit `--cli` takes precedence.

`acceptance/features/zal_dialogue.feature` and the `zal-dialogue` registration
in `tools/atdd.py` connect the Python suite, deterministic Node UI checks, pinned
CLI build and actual sample-domain factory replay to repository acceptance.
Run `python3 tools/atdd.py run --scenario zal-dialogue` for that slice;
`run --all` includes it. Registration is not evidence of a successful final run,
native browser coverage or live-harness interoperability.

## Installed Codex harness evidence

Historical local inspection found `codex-cli 0.146.0`. Its `app-server` supports
`stdio://`; its schema generator supplied the installed protocol. Stable
methods include `initialize`, `initialized`, `thread/start`, `turn/start`,
`turn/steer`, and `turn/interrupt`. `turn/start.outputSchema` constrains the
final assistant message to a supplied JSON Schema. `turn/steer` requires an
`expectedTurnId`; interruption requires the exact thread and turn IDs.
That historical schema lists `untrusted`, `on-request`, `never`, or a granular
approval-policy object. Schema presence is not a current compatibility promise;
the implemented client uses `on-request` and the read-only sandbox.

The implemented custom app-server client uses a stdio child and structured
responses; questions, selection, checks and agreement remain client-owned.
An earlier live structured explanation on 0.146.0 did not change the revision.
The cloud installation is 0.159.2 and must be validated independently. A schema
or handshake alone does not establish a completed live model turn.

Stock Codex consuming the ZAL MCP bridge is a different integration. The
earlier stock-CLI probe discovered `zal_read`, but its permission layer
cancelled both calls; no read/check/proposal completed. This is not successful
interoperability. A separately approved cloud retry on 0.159.2 stopped during
runtime initialization with a read-only-filesystem error, before any model/MCP
execution was observed; its workspace and configuration stayed unchanged.
Claude's MCP seam is documented but no live Claude model was
invoked. The setup guide uses per-invocation configuration, not persistent
registration or broader approvals.

The optional browser shares the terminal/MCP file only with `ui --workspace PATH`;
without that option it has an isolated in-memory session. Long-lived clients
pin checker source identity at start and refuse checking/acceptance after source
drift until restarted; candidates with old evidence still need fresh review.
The cooperative same-user shared store is not malicious-agent isolation, and
app-server's sandbox does not sandbox client-side domain tools. A model output
or command approval cannot grant semantic agreement. No evidence establishes
embedding a custom pane inside stock Codex; the UI is a companion.
Fresh native browser QA is also separate: the cloud browser was blocked before
page creation by a local socket restriction, and the supported browser refused
loopback access. DOM-double checks do not establish native rendering, keyboard,
touch, HTTP or accessibility behavior for the new help controls.

## Addon follow-up for 2.3

The [harness integration](zal/HARNESS_INTEGRATION.md) is an additive follow-up to
the archived prototype. `integrations/zal/setup.py` now prepares persistent
project-local MCP registration and a shared skill; `workspace.py` adds a reviewed,
revision/checker-bound declaration export and exact regeneration check. Terminal
review now supports explicit acceptance of a displayed initial model. It does
not add an acceptance tool or change the Rust core. Earlier cloud/harness evidence
above remains historical; current native configuration and live model results
must be reported separately. This track leaves the fixed 2.2 journey unchanged.

## Definition of done for the prototype

The primary-literature review, bounded grammar, paired canonical surfaces and
revision-bound dialogue are present for review. Completion still requires
exact-source validation of terminal/MCP behavior and optional UI, actual factory
qualification, independent review, and inspection of the required repository
checks. Keep fixture transport, installed protocol, live model and stock-harness
evidence separate. Recheck the factory branch head before the authorized stacked
draft PR. No merge, release, deployment, persistent account/configuration change
or live application adoption is part of this prototype.
