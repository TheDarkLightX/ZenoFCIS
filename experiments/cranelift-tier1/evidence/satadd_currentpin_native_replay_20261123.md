# Tier-1 Cranelift signed-i64 saturation-clamp: current-upstream executable qualification (2026-11-23)

**Research only; not a Wasmtime submission.** This document records a new successful pinned compiler A/B and an independent replay of the resulting machine-code objects. The rule is a **previously authored one-rule ISLE proposal**, not a newly synthesized ZenoFCIS result. Nothing here establishes a throughput win or an architecture-neutral profitable optimization.

## Exact revisions and artifacts

- ZenoFCIS experiment head used by CI: `3988929316efba6ce037994af9a2b95c43dbbc95`.
- Wasmtime/Cranelift: `fb792bf7a143801c22c0ba5b16fe2c6085a8598b`; pristine `cranelift/codegen/src/opts/selects.isle` Git blob `5ec428ceefedfc89c49392875a3fad7299a5f884`.
- [Successful CI #45107263287](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/45107263287): checkout source pin; Z3 QF_BV sign-clamp check; compile and test identical signed-i64 `satadd.clif` with pristine and experimental Cranelift; independent finite oracle and byte measurement.
- [Original 17-member build artifact #14792876411](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/45107263287/artifacts/14792876411), SHA-256 ZIP `3237acce6771e55cb6702b2111ce9fcb82ebe75a6e40fc01b21661f1617d6a4a`. Download is subject to GitHub artifact retention.

## Same CLIF; actual compiler-generated x86_64 bytes

Target `x86_64`; Cranelift setting `opt_level=speed_and_size`.

| Exact metric | Pristine Cranelift | Experimental single-rule Cranelift | Reduction |
| --- | ---: | ---: | ---: |
| ELF function symbol bytes | 56 | 39 | 17 bytes |
| Decoded executable instruction bytes through first RET | 46 | 39 | 7 bytes |
| Instructions through first RET | 13 | 12 | 1 |

The extra 10 bytes in the pristine function symbol are **not** executable instruction bytes through the first RET. Neither metric is CLIF opcode count, throughput, or whole-program savings.

Full object SHA-256s: pristine `5179f3985e49772b4656ca261e64eb0d8ba57802b78abc9c1ee02d2d7cf03268`; experimental `06e9bb768ebb06a1067b1a1bb23bcb48e5bfbcd04e4281db07332e22d501b960`.

## Independent native replay (separate from CI)

The original artifact was downloaded and its receipt pins and hashes cross-checked. Both original unmodified ELF objects were independently linked into Linux x86_64 shared objects and invoked via their native SysV ABI. A separately written Python arbitrary-precision reference computed `max(INT64_MIN,min(INT64_MAX,x+y))`. Test inputs: **484** distinct 22-by-22 boundary-grid pairs and **100,000** deterministic pseudo-random 64-bit pairs, for **100,484 pairs per object / 200,968 total native calls**, with **zero mismatches**. Separate `readelf -Ws` and `objdump -dw` decoding reproduced 56/39 symbol bytes, 46/39 executable-prefix bytes, and 13/12 instruction counts.

Local replay artifacts:
- Audit source: `audit_satadd_currentpin.py`, SHA-256 `a6b6149a8adb3869e327fba81fb72e89f3657418ec69bb8bf6a34076cefbe1d7`.
- JSON receipt: `audit_receipt_run8.json`, SHA-256 `9f0b4583887414c7b9b506711a59e3f9e6cd9fd3f09dac6e2a6ef600debdd9b2`.
- Reproduction: download artifact #14792876411, unzip into a fresh directory, and run `python3 audit_satadd_currentpin.py <artifact-directory> --out audit_receipt_run8.json` on Linux x86_64 with `clang`, `readelf`, `objdump`.

The CI proof receipt contains `unsat` for the **pure signed-i64 sign-clamp equivalence** (QF_BV, 30-second limit, Z3 4.15.4.0). The independent native replay **did not rerun the SMT solver**; it checked proof/patch/CLIF receipt hashes and executed finite native checks. This is not a full-state machine equivalence proof. Full ISLE chain proof, undefined/poison and target assumption coverage, non-x86 target effects, register/flag/ABI obligations in all contexts, performance regression and compile-time impacts remain unproven.

## Decision

**Current-source compiler A/B passes; keep draft/research status.** The smaller emitted function is confirmed at the new source pin with additional independent native execution. Before human upstream submission: measure representative workload speed/size under relevant targets, quantify compile-time and code-quality regressions, assess applicability and flag/poison semantics, and seek maintainer input on an architecture-neutral mid-end rule that can be profitable on multiple backends.

No edits were applied to Wasmtime or ZenoFCIS `main`; CI patches only a disposable checkout. Upstream Wasmtime agents must not open PRs or comment, per its pinned `AGENTS.md` and Bytecode Alliance policy.
