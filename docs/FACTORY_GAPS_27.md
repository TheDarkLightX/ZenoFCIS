# ZenoFCIS 2.7 factory gap register

All twenty-one rows remain open. This is the development backlog for full 2.7.
Implementation, focused tests, native qualification, proof and release readiness
have separate requirements. See the [continuation guide](DEVELOPMENT_27.md) for
source locations and reproduction commands; bind results to the actual commit.

P0 identifies a major assurance or release blocker, P1 a required capability or
integration, and P2 a targeted improvement or research qualification. Priority
does not waive an accepted requirement.

| ID | Priority | Gap | Current boundary and next required evidence |
| --- | --- | --- | --- |
| F27-01 | P0 | Complete native-checker refinement | Bounded kernels, formal statements and restricted native comparisons exist. Close actual parser, typed AST, elaboration, runtime and deep-AST correspondence, including source/premise bindings, semantic mutants and the aggregate proof gate. |
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
| F27-19 | P1 | First-class U256/I256 throughout the factory | 128-bit helpers and wider intermediates do not supply first-class 256-bit values. Establish versioned canonical/schema/program/JSON/ABI/meter profiles, arithmetic proofs, collections/summaries, SMT/synthesis and complete Rust/Wasm/app/UI support. Preserve old bytes, traps and finite-profile caps. |
| F27-20 | P1 | Complete ZenoDEX and DeFi factory targets | Bounded models do not establish full-width app or settlement correctness. Deliver swap/liquidity, signed PnL/funding and collateral/mint/redeem with units, scales, rounding, unsigned transfers, carry bounds, alias/framing/conservation, authenticated roles, replay, uncertainty and coherent effect review. Qualify selected backend/chain assumptions separately. |
| F27-21 | P1 | Machine-code synthesis and embedded factory integration | Experimental LLM/raw-byte and bounded-search loops emit x86-64/RV64I register kernels and linkable objects, with independent full-word SMT checking and replay. Complete target/ABI/refinement, qualified embedded execution, broader kernel and normal application integration, source/artifact correspondence and measured comparisons. A working word-kernel loop does not establish arbitrary embedded apps or globally optimal binaries. See the [machine-code route](MACHINE_CODE_SYNTHESIS_27.md). |

See the [runtime checklist](RUNTIME_ISOLATION_27_PROGRESS.md),
[assembly](RUNTIME_ASSEMBLY.md), [refinement](RUNTIME_REFINEMENT.md),
[app profile](../integrations/zal/APP_PROFILE.md), [shell effects](SHELL_EFFECTS.md)
and [feasibility standard](RESEARCH_FEASIBILITY.md).
Keep private diagnostics outside publication inputs and follow the
[publication policy](PUBLICATION_POLICY.md).
