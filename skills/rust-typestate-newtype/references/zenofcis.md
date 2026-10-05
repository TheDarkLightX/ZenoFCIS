# Applying the pattern in ZenoFCIS

First inspect the checkout's `AGENTS.md`, public program API, and supported profile. The V2 implementation uses these patterns already; this note does not claim that a particular release or remote branch contains it.

| Existing boundary | Where to inspect | What it contributes |
| --- | --- | --- |
| Original declaration admission | `crates/zeno-fcis/src/program.rs`, `execution_v2/catalog.rs` in the synthesis crate | `bind_catalog` returns `CheckedCatalog` after library checks; callers cannot fabricate its private fields |
| Complete descriptor admission | `execution_v2/composition.rs` and `composition/outcome.rs` | Private `BoundCore` holds the immutable admitted descriptor; the Verus type invariant states admission |
| Bound application program | `execution_v2/authority/bound.rs` | `bind_program` returns `Program` with the library-derived identity and checked core |
| Actual committing artifact | `execution_v2/authority/publication.rs` | Only the bound evaluation path creates a private, non-Clone `Publication`; a report or candidate is not a committing capability |
| Durable application | `crates/zeno-fcis-shell-sqlite/src/v2.rs` | The shell consumes a publication and checks identity, invocation kind, and current state before committing |

`execution_v2/` is under `crates/zeno-fcis-synthesis/src/finite/`. Read actual checks and contracts before describing their exact coverage. A checked catalog by itself does not authorize invocation, commit, or delivery.

Use the existing declarative `ProgramDefinition` → `bind_catalog` → `bind_program` → evaluation/publication route. Keep application behavior in the supported decision graph and laws, and use the repository's synthesis workflow where suitable. This skill does not verify arbitrary handwritten Rust or provide a new Rust typestate generator.

Do not add decision or law callbacks, unchecked constructors, caller-supplied evaluator identity, report-to-capability conversions, or a parallel authority family. Do not recreate separate genesis/transition wrapper hierarchies just to increase type-level distinctions: the simplified route uses one tagged publication and enforces invocation kind at the shell boundary.

Preserve runtime checks for policy identity, replay, current state, resource limits, and actual effect execution. Keep type/privacy checks, runtime tests, and formal evidence distinct. Full business requirements and external effects are not proved merely because the Rust API enforces construction order.
