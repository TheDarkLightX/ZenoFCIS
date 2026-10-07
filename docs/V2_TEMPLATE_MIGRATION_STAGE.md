# Six original-template V2 policies

The six `templates/*/src/v2_contract.rs` modules declare the original applications as data for the library's checked catalog and Authority. `Contract::new()` owns only the input/output type tables. `descriptor()` supplies declarative branches and laws. `checked_authority(&descriptor)` always checks the complete retained original schema and full policy bytes before calling the library constructor. It accepts no caller-selected evaluator identity, mapper, candidate, report, or law verdict.

The ordinary `authority`, `create`, `invoke` and `journey` sources now select the checked V2 route. The caller owns `Contract`, its borrowed `Descriptor`, and then the library `Authority`; no callback, global cache or leaked metadata is used. `create` requires a genuine `GenesisPublication`, and `invoke` passes the exact snapshot envelope plus original command/context envelopes to `publish`. Only a genuine `Publication` reaches the V2 SQLite shell. The current runnable sources passed six complete original consumer suites and six fresh actual CLI normal-route suites. Every fresh generated/bound source and final lock byte equals the complete tested consumer inventory. The transfer receipt and fresh native lifecycle/journey plus strict Clippy/Wasm receipts are preserved separately. The final private Value/codec API port and root combined qualification remain pending; initial declaration comparisons alone do not qualify a runnable route.

## Preserved object

Every original schema-admitted prestate, command and context determines the ordered decision class/reason, complete successor and changed-field patch, effects, and ordered outbox. An invariant-unlawful prestate remains an input: a business reject may succeed, a transition that repairs it may succeed, and an unlawful successor must fail its law. No initial-state invariant is assumed at ingress. The complete Description includes every original name and unused definition. All payload-free `data` and variant command types remain **Sum**, never Enum.

| Template | Original roots and domains | Producer |
| --- | --- | --- |
| durable-counter | count/failures 0..3; two commands; Boolean context | finite scalar graph |
| inventory-reservation | both stocks 0..5; quantity 1..3; four actions | finite scalar graph |
| order-fulfillment | six statuses, attempts 0..3, six actions, three callers | finite scalar graph |
| account-lockout | failures 0..2; clocks 0..4102444800; deadline 0..4102445700 | fixed library Account abstraction |
| withdrawal-queue | balance/amount fields 0..4; command amount 1..2; pause 0..2; two lanes and priority | finite scalar graph |
| agent-treasury-guard | balances 0..20; spent 0..4; ticks 0..11; three model identities; prices 1..2; all original amount fields | finite scalar graph |

The original `.zeno` laws are lowered without changing their formulas or applicability. Treasury laws 500 through 508 retain committing/genesis, committing, accepting and failure scopes exactly. Original rejecting no-authority requirements, absent failure branches, exact genesis, and complete decision correspondence are added as actual library law programs. Conditional `ObserveWhen` nodes read an optional post/payload/reason only under the actual prior Boolean branch condition. Counts and emission conditions are checked separately; a literal default cannot excuse a required missing delivery. Guard-true observation errors remain errors.

The withdrawal controller is represented by the direct finite scalar rules for pause, due lanes, must-serve and priority. Independent comparisons use the original retained contract transition table and certified strategy table. The original Python product-graph checker remains the independent liveness oracle, including fixed-priority and alarm-deferential negative controls. A finite table comparison and a conditional recurrence result do not establish fairness of an external keeper. Its environment must continue delivering ticks, as in the original application.

## Simplicity and assurance

Previously a reader had to follow five application transition adapters (four calling synthesized transitions and one handwritten account transition), the inventory application's earlier library finite-decision route, and six executable application law checkers. The new runtime route uses six declarative policies with complete raw bindings, two existing library producer profiles, the complete-candidate builder and the library law engine. Each field and delivery is explicit. There is no template-local runtime interpreter, observation callback, cache or mutable execution state.

The contract generator, `zeno-fcis generate contract` (pure functions in the CLI crate's `contract` module), is a development tool outside the runtime core. Its output remains untrusted until catalog admission and independent conformance checks. It does not evaluate input tuples. It takes policy bytes only from `authority::policy_bytes`, and writes them only after the library's catalog binding has accepted them with the generated descriptor and schema. Independent Rust reference rules and retained examples do not use the generator or its expression trees. Million-tuple withdrawal/treasury comparisons execute the protected framed core using the descriptor obtained from the already checked Authority; they omit repeated serialization of the complete embedded source identity. Every retained example separately checks production Authority evaluation and whole-subject replay. Counter/inventory/order/account comparisons also use Authority evaluation. This distinction is explicit in the receipts and is not a claim that every large-domain tuple was independently sealed. Original `.zeno`, example tables, synthesis specifications and controller artifacts remain intact. The prior public implementation is retained under an explicit `legacy` namespace for independent oracle tests. Normal entry points do not call its program or law code; no original oracle tests are feature-disabled.

Independent acceptance checks are retained: original schema admission, exact original-schema/Description correspondence, complete policy equality, typed candidate checks, every original scoped law, required law metadata, and private Authority identity construction. No check was deleted on the strength of fewer lines. The complete declaration law is an additional consistency condition; it is not independent proof that the declaration expresses the owner's intent.

## Explicit changes

V2 source, policy and evaluator identities are intentionally new. V2 reports charge all original input fields in the library ingress order, then actual scalar steps, candidate construction, and actual law work. These reports are not byte-identical to the legacy adapters' conditional observations or their 32/64-step tutorial budgets. New finite limits are derived from the complete schema byte widths, graph sizes, required candidate/write/delivery counts and a stated conservative read allowance. They do not narrow input domains. `Byte` covers exactly the three maximum original envelopes; `Candidate=1`, `Write=state-field count`, `Effect=1`, and unused WitnessByte/Depth are zero. The emitter reports the concrete per-template totals.

Decision values, reason precedence, successor fields, patches and original delivery contents are preservation claims subject to the independent checks. The V2 delivery idempotency atom is the explicit ordinal zero. It is **not** the original delivery ID. The root V2 shell derives the domain-separated delivery ID from the genuine publication subject and exact original `OutboxEntry` bytes. It reconstructs and delivers that actual entry; the template has no output encoder. The interrupted-delivery journey calls the root checked `deliver_next_memory_unacknowledged`, reopens with the same immutable Authority and destination ledger, retries through normal delivery, and repeats the saved acknowledgment. No clock or ordinal is a globally unique substitute.

## Evidence and replay

Run from the checkout with pinned Rust 1.97.1. Cargo uses `--locked --offline`, jobs 1, test threads 1, `CARGO_INCREMENTAL=0`, and one shared target directory, `$SHARED_TARGET`. Every heavy suite holds `/tmp/zenofcis-v2-parallel/heavy-check.lock` for its entire execution.

```
for template in crates/zeno-fcis-cli/templates/*/; do cargo +1.97.1 run --locked --offline -q -p zeno-fcis-cli -- generate contract "$template" --check; done
cargo +1.97.1 test --locked --offline -p zeno-fcis-cli --bin zeno-fcis contract::
cargo +1.97.1 test --locked --offline -p zeno-fcis-cli --test contract_generation
CARGO_TARGET_DIR="$SHARED_TARGET" CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 flock /tmp/zenofcis-v2-parallel/heavy-check.lock cargo +1.97.1 test --locked --offline -p zeno-fcis-synthesis --test v2_template_contracts -- --test-threads=1 --nocapture
```

`zeno-fcis generate contract <template-dir>` is the authoring command for `v2/schema.zcve`, `src/v2_contract.rs` and `v2/policy.zcve`; with `--check` it changes nothing and names each file that differs. Changing a declaration requires reviewing the regenerated source and policy bytes and rerunning all source-bound checks. Each template build also compares the retained SC bytes to a fresh `lower_schema` result, an independent check of the generator's schema encoding. `compiled_contracts_reproduce_their_committed_policy_bytes` recomputes every template's policy bytes from its compiled source, separately from the generator's own model.

The result manifest under `/tmp/zenofcis-v2-parallel/templates/` records exact commands, stdout/stderr, exit status, owned sources, original oracle inputs, and library dependencies. Only completed checks listed there count as evidence. Planned test coverage is not a successful result. Treasury partition checks are not a full Cartesian-domain proof. Account's library abstraction proof does not alone prove the chosen template policy or every original intent claim.

## Normal route and root handover

```rust
let contract = v2_contract::Contract::new();
let descriptor = contract.descriptor();
let authority = authority(&descriptor)?;
let mut shell = create(path, &authority)?;
```

The shell borrows Authority for its entire lifetime. Genesis input framing is a local host operation; the shell copies the exact initial envelope from the consumed genuine capability. Invocation retains the original prestate, command and context byte slices across its first publication and exact `replay_publication`. It never decodes and re-encodes the prestate for execution. Typed snapshot decoding is solely presentation. A business Reject publishes nothing; every technical refusal returns the actual evaluation/refusal diagnostic. The displayed class comes from the actual library candidate.

Snapshot `version` counts successful publications, and `pending` counts outstanding deliveries. Original journey summary names, decision sequences and expected values remain unchanged. The new schema-v7 history binds one original replay key to each genuine committed publication. Reopening recomputes the complete history using the same immutable Authority. Host authentication, SHA-256 implementation, SQLite atomicity, input framing/presentation and independently durable destination idempotency remain named shell assumptions.

1. Root applied the concrete CLI registry proposal for the six V2 modules/artifacts, six `src/legacy.rs` files and retained counter `tests/legacy_lifecycle.rs`. All six fresh CLI inventories contain these sources exactly.
2. Root supplied the coherent public-ledger API port; supply the final private Value/codec API port and requalify root SQLite, including its checked before-acknowledgment delivery hook. The templates never supply a duplicate wire adapter.
3. Current actual emitted normal applications passed the following checks; repeat them after final private API changes: original lifecycle scenarios, explicit independent legacy/oracle tests, complete current policy comparisons, strict Clippy, no-sqlite/Wasm surfaces and six-contract no_std build. Initial successful declaration/native receipts remain frozen under their earlier exact sources.
4. Regenerate the final library source closure and run pinned whole-source proofs, full ATDD, public ledger/evidence gates, integration/recovery tests and independent final review on integrated source. Root owns these gates and publication authority.
