# ZenoFCIS 2.7 factory gap register

All twenty-one rows remain open. This is the development backlog for full 2.7.
Implementation, focused tests, native qualification, proof and release readiness
have separate requirements. See the [continuation guide](DEVELOPMENT_27.md) for
source locations and reproduction commands; bind results to the actual commit.

P0 identifies a major assurance or release blocker, P1 a required capability or
integration, and P2 a targeted improvement or research qualification. Priority
does not waive an accepted requirement.

## Progress recorded on 2026-10-29

These are completed substeps and remaining obligations, not closed release rows.
Native test results below belong to the stated frozen source; subsequent
formatting is a separate reviewed difference, not a fresh test pass.

| Area | Established evidence | Next required proof or integration |
| --- | --- | --- |
| Existing parser certificate checker | The actual Rust `check_derivation` and `check_derivation_prepared` already have Verus soundness contracts: acceptance implies a complete grammar derivation, correct root, ordered full token coverage and no unused node/edge records. | Preserve those results and their source bindings when integrating additional lowering proofs. This part of the parser was already proved. |
| Native configuration admission | The parser gate checks all bound Rust sources before preparing tools, using exact declaration-bound configuration exceptions. On source `8fbd2fedb01199cc8b911ac111100b8108f41b39`, 82 focused gate test cases passed, including refusal of a native-only branch inserted into the actual syntax checker. | Review exceptions with configuration changes and replay the full parser qualification on the final release subject. This source-dialect guard does not prove Rust/Verus compiler or erasure correspondence. |
| Helper-node lowering | A separate local source-bound frontier harness passed 71 verification sections, including its existing certificate dependencies, under the original resource limits. Model source SHA-256: `603f5a07b74d4c92f48401c12268868b916615e71985b562361d7278124a239f`. | Integrate the shared function into the production `Lowerer::parts` wrapper; qualify its callers, grammar binding, semantic mutants and operational allocation behavior. Complete subsequent typed-AST/elaboration/runtime correspondence. The 71 sections are not 71 new independent parser theorems. |
| Full-width values and schemas | Explicit Value2/codec2/Schema2 implementations exist. Frozen source `08a8fc6aa9e1ad32bfe8127acd6fcc3e2e21ef30fef6d1fe0e3f06be0cb9fae8` passed 14 legacy unit executions, 40 enabled-profile unit executions, 10 public consumer tests, two documentation checks and a no-std check, with source/tool postchecks and owned-process cleanup. The 40 include the same 14 legacy methods under the enabled feature. | Prove the actual Schema2 constructor/closure, full-width comparison/range validation, canonical encoding and owned witness/envelope bindings. Existing schema-binding theorems concern the earlier finite canonical profile; they do not establish Schema2 refinement. Then complete wide execution, laws, Authority, synthesis and applications. |
| Shared full-width comparator | The actual shared generic limb comparator passed its complete isolated Verus harness with both entry/loop obligations and all three intended semantic defects rejected. Frozen native source `66955d656527d0a3a99f3b75fca0bc407780f99b32c775594d054093097459bd` passed 128 test executions across value/codec/schema/consumer suites and an allocation-only build. See the [numeric qualification](NUMERIC_256_FOUNDATIONS_27.md#shared-comparison-proof). | Preserve C1 when qualifying the type/order lifting below and remaining arithmetic callers. C1 alone is a lexical comparison theorem. |
| Actual unsigned/signed numeric order | The actual constructors/views, sign test and comparisons passed a whole 21-condition proof with exact typed/body coverage. All seven actual-method defects were rejected semantically. Frozen native source `4d8bba31eb96c3720affeffde7af6d9ebca483edc187d844bbad1cbf3ae36699` passed 128 test executions and the allocation-only build. See the [type/order qualification](NUMERIC_256_FOUNDATIONS_27.md#actual-type-and-numeric-order-proof). | Prove the other arithmetic methods, public associated constants/conversions, complete Schema2 refinement, and the remaining execution/Authority/application chain. Checked raw-bit endpoint lemmas do not certify the separate public constants. Compiler/erasure correspondence remains an assumption. |
| Machine-code connection | Draft PRs [159](https://github.com/TheDarkLightX/ZenoFCIS/pull/159) and [161](https://github.com/TheDarkLightX/ZenoFCIS/pull/161) contain checked x86-64/RV64I word-kernel search/replay and its ZAL connection; dedicated machine checks passed at PR161 revision `47470843f69a8cfe5b8555818170d0a77da01955`. | Complete broader target/ABI/refinement, embedded execution and normal application integration. |
| Wide program and shell work | Separate local implementation candidates exist for a numeric-only wide profile and actual Channel/Authority trace correspondence. | Compile and prove the coherent wide driver and its default-profile correspondence; qualify the concrete shell candidate against actual native handlers and recovery. Source-only candidates are not accepted runtime or proof results. |

### Proof-first next steps

- [x] Identify the existing syntax-certificate theorem and retain its scope.
- [x] Require source-bound native configuration admission before parser proof tools or compilation.
- [x] Obtain a successful whole-harness helper-frontier proof without increasing its solver budget or weakening its statement.
- [ ] Integrate and qualify the production lowering wrapper and its remaining proof links.
- [x] Obtain source-bound native evidence for the new Schema2 structural admission route.
- [x] Prove the actual shared generic limb comparator and qualify its native callers with retained semantic controls.
- [x] Lift comparison to the actual unsigned/signed numeric types with checked constructors/views/sign and independently rejected method defects.
- [ ] Establish Schema2 refinement of the actual Rust implementation; retain constructor, comparator, encoder, resource and witness dependencies explicitly.
- [ ] Prove the refactored default numeric driver and the separate full-width profile before treating either new route as qualified.
- [ ] Complete full-width laws, exact source/schema/program binding, Authority, synthesis/code generation and complete DeFi applications.
- [ ] Freeze the full release assembly and complete release, platform and evidence gates.

Tests remain useful for specific integration boundaries and independent negative
controls. Additional test counts do not discharge an open refinement theorem.

| ID | Priority | Gap | Current boundary and next required evidence |
| --- | --- | --- | --- |
| F27-01 | P0 | Complete native-checker refinement | The existing Rust syntax-certificate checker is proved; a separate local helper-frontier harness now passes 71 verification sections. Integrate its production lowering wrapper and close the remaining typed AST, elaboration, runtime and deep-AST correspondence, source/premise bindings, semantic mutants and aggregate proof gate. |
| F27-02 | P0 | Worker isolation and installed routing | Protected dispatch, channels, supervision and explicit native checks exist. Internal fork/init lifetime, executable privilege metadata, account/runtime closure, aggregate multiprocess limits and installed factory routing remain open. |
| F27-03 | P0 | Durable effect authority, publication and fencing | Outbox, pending-intent and reservation machinery exists. Close exact publication/recovery/ACK correspondence, revocation and late work, durable grants, receiver-enforced fencing, restore and upgrade. An epoch or local lock alone is insufficient. |
| F27-04 | P0 | Current-program subjects across consumers | Several lineage consumers have source changes and tests. Check every supported judge against the intended adopted program, with source-bound tampering and changed-program controls, or explicitly refuse unsupported lineage. |
| F27-05 | P1 | General ZAL meaning to complete applications | Multiple authoring and abstraction profiles exist. Finite-FSM3 authoring/export and native profile2 app admission are distinct routes. Establish qualified bridges with explicit meaning preservation, assumptions and ambiguity/refusal handling beyond current finite examples. |
| F27-06 | P1 | Effectful ZAL apps and plan review | Typed effects and native outbox machinery exist. The finite app lowerer emits empty outboxes. Bind effect declarations, destinations, payloads, order, identity and UI confirmation; qualify real retry, ACK, restart and uncertain-outcome journeys. |
| F27-07 | P1 | Grouped UI through generated-app qualification | Source includes one selected form with separate command drafts and a focused-input ownership repair. Complete native/Wasm generation and actual keyboard, viewport, confirmation, lost-response, principal-recovery and durable-restart checks on changed source. |
| F27-08 | P1 | Multi-entity and data-rich apps | Maps, tallies and collection models are reusable. The new fixed-table descriptor/presentation binding needs native and complete-app qualification. Dynamic keyed read models, pagination, snapshots, scoped commands and richer recovery remain separate work. |
| F27-09 | P1 | Ordered collection runtime framing | Ordered language/query helpers and equality-key routes have different scopes. Complete ordered snapshot/read/write admission, bounds, alias/capacity behavior, atomic correspondence and independent runtime controls. |
| F27-10 | P1 | Wider typed UI idioms and interactions | Shared forms, displays and specialized idioms exist. Catalog presence and synthetic fixtures do not qualify each family. Obtain actual native captures and check units, roles, actions, effects, coverage, masking, consent and browser behavior for useful new families. |
| F27-11 | P1 | Lens self-use and maintained integration | Lens/library/browser and optional category source are included. Retain exact source/dependency and runtime-approval boundaries; qualify current identities and distinguish the custom viewer from factory-synthesized layouts. Finite views do not establish infinite mathematical claims. |
| F27-12 | P1 | Checked relevance, availability and UX | Grouping, navigation, confirmation and uncertainty copy have implementation paths. Input removal still needs complete refusal/effect/mask-aware evidence. Qualify remaining Lens viewport/focus behavior and preserve exact confirmed packet identity. |
| F27-13 | P1 | Optimized programs through normal consumers | Restricted full-ABI experiments do not establish a general replacement. Preserve class, reason, successor, effects and metering; qualify normal catalog, adoption and dispatcher consumption over complete app domains. |
| F27-14 | P1 | Scaling and sound proof reuse | Composition and symbolic/collection routes need implementation correspondence, aggregate budgets and independently checked reuse conditions. Compare heterogeneous workloads against the strongest applicable qualified baseline; retain unknown baselines and regressions. |
| F27-15 | P1 | Exact numeric meaning in generic ZAL ingestion | Exact-token and profile-specific checks do not establish exact generic JSON ingestion. Preserve or refuse unsupported numeric lexemes and check source/model/canonical agreement across runtimes. |
| F27-16 | P1/P2 | Practical efficiency and development tools | Bounded search does not establish end-to-end speedup. Qualify actual tool startup and resource enforcement; assess correctness, complexity, complete costs, held-out crossover and assurance independently. Keep slower and incomplete results. |
| F27-17 | P0 | Release, platform and evidence closure | This checkpoint is incomplete. Freeze final assembly, run full ATDD and affected proof/native/platform/package checks, bind source/artifacts and complete reviewed publication/release gates. Earlier platform artifacts do not qualify changed 2.7 inputs. |
| F27-18 | P1 | Backend-independent shell and PostgreSQL | Pure ports/programs provide a seam; apps and protected-owner persistence retain SQLite coupling. Qualify a common observation/refusal profile and real PostgreSQL atomicity, replay, CAS, history, outbox, ACK, concurrency/recovery/deployment, plus owner/factory/upgrade integration. |
| F27-19 | P1 | First-class U256/I256 throughout the factory | Explicit owned Value2, codec2 and Schema2 now have source-bound native evidence. Their complete native refinement proofs remain open. Complete exact-width program/JSON/ABI/meter profiles, arithmetic/encoding/witness proofs, collections/summaries, SMT/synthesis, Authority and complete Rust/Wasm/app/UI support. Preserve old bytes, traps and finite-profile caps. |
| F27-20 | P1 | Complete ZenoDEX and DeFi factory targets | Bounded models do not establish full-width app or settlement correctness. Deliver swap/liquidity, signed PnL/funding and collateral/mint/redeem with units, scales, rounding, unsigned transfers, carry bounds, alias/framing/conservation, authenticated roles, replay, uncertainty and coherent effect review. Qualify selected backend/chain assumptions separately. |
| F27-21 | P1 | Machine-code synthesis and embedded factory integration | Experimental LLM/raw-byte and bounded-search loops emit x86-64/RV64I register kernels and linkable objects, with independent full-word SMT checking and replay. Complete target/ABI/refinement, qualified embedded execution, broader kernel and normal application integration, source/artifact correspondence and measured comparisons. A working word-kernel loop does not establish arbitrary embedded apps or globally optimal binaries. See the [machine-code route](MACHINE_CODE_SYNTHESIS_27.md). |

See the [runtime checklist](RUNTIME_ISOLATION_27_PROGRESS.md),
[assembly](RUNTIME_ASSEMBLY.md), [refinement](RUNTIME_REFINEMENT.md),
[app profile](../integrations/zal/APP_PROFILE.md), [shell effects](SHELL_EFFECTS.md)
and [feasibility standard](RESEARCH_FEASIBILITY.md).
Keep private diagnostics outside publication inputs and follow the
[publication policy](PUBLICATION_POLICY.md).
