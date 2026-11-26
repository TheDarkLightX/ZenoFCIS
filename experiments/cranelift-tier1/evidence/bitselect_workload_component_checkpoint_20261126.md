# 2026-11-26 — Sightglass component checkpoint

Continue existing draft PR #197, not a new experiment branch or upstream submission.

## Executed evidence
Run [45795585224](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/45795585224), job 139825273319, head 6cccd061580dceb8ac2059401ef94b4eff34c879, completed unsuccessfully. Artifact 15103893904 has reported SHA256 3081699e3d26482cf0205041bc7bebcc0eecd033a4cb71d16317916748ff423d (metadata only, not independently downloaded in this run). Job logs confirm four baseline rows:

| Module | CLIF files per setting | bitselect lines | .text bytes per setting |
| --- | ---: | ---: | ---: |
| shootout/shootout-xchacha20.wasm | 101 | 0 | 57344 |
| libsodium/libsodium-core5.wasm | 133 | 0 | 90112 |

Each compiled at speed and speed-and-size; no optimized variant executed. Third entry cm-online-stats failed: component model support was disabled at compile time. This is a harness failure, not a counterexample or a completed workload A/B. No real workload savings claimed.

## Source audit and next run
Pinned Wasmtime b738a89287bb78e51c06ac9624014bbda3c1429f; Sightglass f0d2040f4b1c521be5b763241f5c3b1146efff97. At that Wasmtime pin:
- src/commands/compile.rs dispatches components through compile_component_serialized only with component-model feature.
- Cargo.toml provides component-model. Added it to minimal build.
- crates/cranelift/src/compiler.rs finish_with_info writes context.func after compile_maybe_cached; dump is a static post-compilation IR observation, not a dynamic hit count or direct rule-fire trace.
- cranelift/codegen/src/opts/selects.isle contains eight commuted forms of (c & x) | (~c & y) -> bitselect, all guarded by ty_vec128. Thus those particular rules do not create scalar i64 bitselect from scalar mask expressions; not a proof that no other route exists.

Workflow correction commit [7c9f506881112860d6d4a70450d2594a71196f47](https://github.com/TheDarkLightX/ZenoFCIS/commit/7c9f506881112860d6d4a70450d2594a71196f47) enables component-model, records individual compile errors and incomplete pair count explicitly, retains per-module static SSA-assignment opcode histogram and binary version bytes, and still fails the overall run if any of the 12 pairs are missing. No execution result for the correction confirmed at checkpoint. The histogram is a simple text-derived static count, not complete instruction parsing or dynamic profiling.

Next: inspect this exact workflow commit's push-triggered run and catalog before rerunning. If no scalar reachability and identical text, preserve negative evidence rather than expanding synthetic qualification indefinitely. Use the static opcode histogram to prioritize a frequent integer lowering or inspect scalar mask-expression reachability for a separately scoped rewrite, with actual compiler cost and independent proof. No speculative upstream PR; personal review and VeriISLE gaps remain. Do not confuse section padding with symbol code size. Prior synthetic readiness and verification receipts remain unchanged.
