# ZAL prototype source map

This map records the ZenoFCIS source as the ZAL prototype inspected it on
2026-10-06, during V2.1 development and before the V2 release candidates.
File and line references point to that inspection revision and may have
moved since. The prototype is separate from the library and does not change
a running application, release, database or publication route.

The paths below are those of the inspection revision. Of the files named
there, `contract/rules.rs`, `contract/model.rs` and `contract/expr.rs` are in
`crates/zeno-fcis-contract/src/internals/` in this release.

## Implemented surfaces

| Boundary | Actual source at the inspection revision | Meaning for ZAL |
| --- | --- | --- |
| Original declarations | `crates/zeno-fcis-spec/src/{lexer,parser,ast,elaborate,logic}.rs` | `.zeno` already describes types, reasons, channels, scoped laws and claims. It is not an unrestricted programming language. |
| Factory inputs | `crates/zeno-fcis-cli/src/contract.rs:68`, `contract/rules.rs:19` | `project.zeno`, `v2/policy.json`, optional schema origin, and retained adoption artifacts form a contract. Rules use schema `zeno-fcis/template-declarative-policy/2`; unknown and duplicate keys are refused. |
| Contract generation | `crates/zeno-fcis-cli/src/contract.rs:255`, `contract_files.rs:188` | Pure generation consumes supplied contents, checks catalog binding, emits schema, Rust declarations and library-encoded policy. The file shell writes them. Generation does not certify human intent. |
| Factory finite profile | Historical `contract/model.rs:22`, `contract/expr.rs`; [current model source](../crates/zeno-fcis-contract/src/internals/model.rs) | At most 32 inputs, 16 outputs and 256 eager scalar nodes. Boolean and checked i128 computations are supported. Context facts are explicit inputs. Eager overflow/refusal is observable. |
| Meaning review | `crates/zeno-fcis-cli/src/review_command.rs:58`, `contract/review.rs`, `contract/review/packet.rs:43` | Decision tables, example disagreements, mutations and replayable distinguishing inputs. Reports distinguish full-domain and boundary-set searches and have `authority: none`. Findings and law refusals affect the command result. |
| Export and scaffolding | `crates/zeno-fcis-cli/src/contract_files.rs:227`, `:317` | `new --contract --source` builds a generated application around the existing route; `contract export-program` emits the current canonical decision graph. |
| Executable scalar program | `crates/zeno-fcis-synthesis/src/finite/ir.rs:54`, `:110` | Owned admitted DAG; `execute_v2` uses the actual metered evaluator. A surface renderer cannot substitute its own evaluator for factory evidence. |
| Replacement checker | `crates/zeno-fcis-cli/src/transform.rs:190`, `:443` | Exhaustive complete declared input-domain comparison, under eager semantics and a nonbinding Step limit; canonical receipt and replay. Output/failure equivalence is separate from sealed usage preservation. The checker is a tested judge, not itself a proved checker core. |
| Optimization | `crates/zeno-fcis-cli/src/optimize/`, `optimize_command.rs`, `loop_command.rs` | Bounded e-graph and neural proposals go through the replacement checker; attempts/resume have durable accounting. No optimality or convergence claim. Hosted provider is separately gated. |
| Adoption | `crates/zeno-fcis-cli/src/contract_adopt.rs:78`, `contract/adoption.rs` | Replays exact receipts, updates the contract lineage, and requires an explicit Step-usage decision. A ZAL discussion agreement is not this adoption authority. |
| Normal public API | `crates/zeno-fcis/src/program.rs:36` | Reexports the existing `bind_catalog`, `bind_program`, `ProgramDefinition`, `Invocation`, `Program`, and `Publication` family. Declarations and `policy_bytes` remain untrusted proposals. |
| Authority | `crates/zeno-fcis-synthesis/src/finite/execution_v2/authority/bound.rs:79`, `:186`, `:222` | Exact catalog binding, original-envelope evaluation, required laws/genesis, and private publication custody. No application callback supplies the decision. |
| Publication | `crates/zeno-fcis-synthesis/src/finite/execution_v2/authority/publication.rs:49` | Private non-Clone capability carrying the actual committing evaluation, original poststate and ordered deliveries. Reports or serialized subjects cannot construct it. |
| Persistence and upgrade | `crates/zeno-fcis-shell-sqlite/src/v2.rs:452`, `v2/upgrade.rs:93`, `:364` | Mandatory capability-based SQLite route, exact identity/replay, and same-schema upgrade premises. Data migration and changed-business-rule evolution were added later as `contract evolve` (see [rule changes](CLI_REFERENCE.md#rule-changes-for-live-stores) and [data migrations](CLI_REFERENCE.md#data-migrations-and-renames)). |
| Existing agent bridge | `integrations/mcp/zeno_fcis_synthesis.py`, `zeno_fcis_transform.py` | Fixed specification/check/synthesis/loop commands with no publication claim. Existing MCP registration is optional; the ZAL MVP need not modify accounts or MCP configuration. |

`docs/CONTRACT_RULES.md` describes ordered cases: the first matching case wins
and the final case must be literal `true`. ZAL can choose stricter source
semantics (reject overlapping enabled transitions) before lowering, and retain
that choice in its versioned profile. It must not silently inherit precedence.
`crates/zeno-fcis-cli/contract-app/tests/decisions.rs` tests each independent
decision example against the real Authority, genesis and a persistent journey.

## Later releases

This inspection predates the public V2.2 and V2.3 release candidates. They add,
among other things, examples-first contract drafting (`contract draft`),
change classification (`contract diff`), store evolution (`contract evolve`),
a delivery relay and a catalogue of finite core components. See the
[changelog](../CHANGELOG.md) and the [candidate record](RELEASE_CANDIDATES.md);
this map does not re-inspect them.

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

The 2.6 additions are separate modules. `explain.py` supplies `zal/why-not/1`,
`zal/what-if/1` and `zal/narrative/1`; `constructs.py` holds the construct
registry and its generated `zal/help/2` guide; `ladder_tools.py` binds ladder
statements to revisions for the `zal_ladder_*` tools. `mcp.py` adds four
explanation tools and six ladder tools after the existing ones, and `terminal.py`
adds `whynot`, `whatif`, `story`, `guide` and `ladder`. `check_zal_tools.py`
runs their planted-defect controls on an isolated copy. `behavior.py` is
unchanged by these additions. `explain2.py` gives profile 2 the same
explanations, with schemas `zal/why-not/2`, `zal/what-if/2` and
`zal/narrative/2`, through `run.py story`, `why-not` and `what-if`.

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
[grammar](zal/GRAMMAR.md) and [specification](zal/SPECIFICATION.md)
describe this implemented scope.

`factory.py` lowers to `project.zeno`, `v2/policy.json`, a feasible
`tests/decision-examples.txt` journey, a whole-domain reference table, source
map and generated Authority comparison test. Qualification runs actual contract
generation, drift checking, advisory review, scaffolding and app tests against
the pinned source. Interpreter checks remain distinct from actual Authority
correspondence. No specification result is an agreement, Authority, Publication,
replacement receipt or deployment approval. The generated test obtains its
Authority through the existing library route. See [usage](zal/README.md).

Profile `zal/finite-fsm/2` (2.6) is a separate module, `behavior2.py`;
`behavior.py` and profile 1 results are unchanged. It adds bounded integer
variables and inputs with range traps, transition requirements over pre-state,
event, outcome and post-state, implication and equivalence, and temporal
properties: always, never, inclusive within k and strong next checked exactly
over every execution, and eventually and strong until only under declared
fairness, with fair lasso counterexamples. `behavior2.py:check`, `:explain`,
`:trace_properties` and `:semantic_diff` are deterministic and use no
language model. `run.py` selects the profile by its header. `factory.py`
lowers variables, inputs, invariants and requirements to state and context
fields, cases and laws over the admissible reference domain; temporal
properties and fairness refuse as unsupported lowering.
`oracles/temporal_oracle.py` compares machine verdicts with brute-force lassos
and finite words; `check_behavior2.py planted` runs nine planted defects.

The 2.6 checker identity `zal/checker/3` hashes one source closure for all
three packages: `behavior.py`, `behavior2.py`, `workflow.py`, `factory.py`,
`shared.py`, `terminal.py`, `mcp.py`, `workspace.py`, `server.py` and
`workspace.html`; the explanation and ladder tool modules `explain.py`,
`explain2.py`, `constructs.py`, `ladder_tools.py`, `ladder.py`, `ladder_en.json` and
`temporal_lowering.py`; the withdrawal pilot's `withdrawal_pilot.py`,
`withdrawal_pilot.lean` and `withdrawal_pilot_refutation.lean`; and the finite
types, typed-machine and effects modules `adt.py`, `typed2.py`, `effects.py` and
`effects_guide.py`. Changing any of them changes the identity and makes earlier
evidence stale.

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
must be reported separately.

## Withdrawal temporal pilot

The [withdrawal pilot](zal/ladder/WITHDRAWAL_PILOT.md) binds two
`zal/ladder-temporal/1` statements to the withdrawal-queue template. Its sources:

| Source | Role |
| --- | --- |
| `crates/zeno-fcis-generated-code-tests/examples/withdrawal_queue_decisions.rs` | Decision graph of `templates/withdrawal-queue/src/v2_contract.rs` through its checked Authority |
| `integrations/zal/withdrawal_pilot.py` | Observation adapter and its obligations, tick projection, consequent lowering, Lean rows; in the `zal/checker` identity |
| `crates/zeno-fcis-formal-tools/examples/withdrawal_temporal_pilot.rs` | Every word of the tick projection replayed through `src/model_diagnostics.rs`; Lean tree inventory |
| `crates/zeno-fcis-formal-tools/src/translation_tests.rs` | `ladder_claim_lean_export_from_environment` applies the existing Lean exporter to the lowered consequent |
| `integrations/zal/withdrawal_pilot.lean`, `withdrawal_pilot_refutation.lean` | Controller model, response theorem and the planted-defect refutation; in the `zal/checker` identity |
| `integrations/zal/test_withdrawal_pilot.py`, `check_withdrawal_pilot.py` | Named checks, the length-12 run and the planted defects |

## Finite types, typed machines and effect signatures

Three 2.6 profiles share one type language. `zal/finite-types/1` declares
aliases, records and variants of bounded integers, Booleans, byte-capped and
exact bytes and bounded lists; recursion is refused and each type reports its
exact count of values. `zal/typed-fsm/1` elaborates typed variables and inputs
exactly to an unchanged `zal/finite-fsm/2` machine. `zal/effects/1` declares the
shell's effect ports and lowers them to the `zeno-fcis/shell-effects/1` record.
See [types and effects](zal/EFFECTS.md).

| Source | Role |
| --- | --- |
| `integrations/zal/adt.py` | Type declarations in both surfaces, exact counts, enumeration and values; in the `zal/checker` identity |
| `integrations/zal/typed2.py` | Typed variables and inputs, elaboration to profile 2, decoded verdicts and explanations; in the `zal/checker` identity |
| `integrations/zal/effects.py` | Effect signatures, the named structural checks and the record lowering; in the `zal/checker` identity |
| `integrations/zal/effects_guide.py` | Construct registry, `zal/help/2` guide, port and type explanations; in the `zal/checker` identity |
| `integrations/zal/examples/shell_effects.zal`, `relay_typed.zal` | The ten shipped ports in ZAL; a typed relay over the transport's answer |
| `crates/zeno-fcis-shell/examples/shell_effects_registry.rs` | Prints the built-in registry, its identity, its names and the SHA-256 of `registry.rs` for the drift test |
| `integrations/zal/test_adt.py`, `test_typed2.py`, `test_effects.py`, `check_effects.py` | Round trips, counts against enumeration, the byte-for-byte lowering and drift tests, help drift and the planted defects |

## Definition of done for the prototype

The primary-literature review, bounded grammar, paired canonical surfaces and
revision-bound dialogue are present for review. Completion still requires
exact-source validation of terminal/MCP behavior and optional UI, actual factory
qualification, independent review, and inspection of the required repository
checks. Keep fixture transport, installed protocol, live model and stock-harness
evidence separate. No merge, release, deployment, persistent
account/configuration change or live application adoption is part of this
prototype.
