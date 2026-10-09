# Tau proof-carrying synthesis bridge

Status: experimental evidence adapter. Authority: **none**.

`tools/tau_fcis.py` captures a Tau synthesis run as a replayable ZenoFCIS evidence bundle. It is intentionally outside the existing `zeno-fcis-formal-tools` authority path: a successful Tau result is retained as a **proposed synthesis artifact**, not as a `BackendCertificate`, project-law proof, release authorization, settlement authorization, or value-moving permission.

The bridge is meant to answer a narrow question reproducibly:

> Given these exact Tau/Tau-codegen/Spot executable bytes, this exact Tau specification, these execution bounds, and these explicitly bound environment values, what program and winning-strategy artifact did Tau produce?

## Why this exists

Tau's active development line adds full reactive synthesis, C++ code generation, strategy export, richer logical domains, and a plugin architecture. Those capabilities are useful to ZenoFCIS only if their outputs can be treated as inspectable external evidence rather than silently trusted runtime authority.

The bridge therefore follows the ZenoFCIS pattern:

```text
Tau spec + exact executable bytes + bounded invocation
    -> private executable snapshots
    -> tau_codegen synthesis
    -> generated C++ + HOA strategy + raw logs
    -> content-bound evidence receipt
    -> deterministic replay
    -> independent review/promotion (separate, not implemented here)
```

## Capture

```bash
python3 tools/tau_fcis.py capture \
  --spec demos/tau-proof-carrying-synthesis/mirror.tau \
  --tau /absolute/path/to/tau \
  --tau-codegen /absolute/path/to/tau_codegen \
  --ltlsynt /absolute/path/to/ltlsynt \
  --tau-source-commit <40-lowercase-hex-commit> \
  --out /tmp/tau-fcis-run
```

The output directory must not already contain files. On a successful run the default policy requires both `program.h` and `strategy.hoa`. Use `--allow-no-strategy` only for a deliberately strategy-free experiment; doing so weakens the replay surface and is recorded in the receipt.

If Tau needs an additional runtime setting, pass it explicitly:

```bash
--env NAME=value
```

The receipt stores the **SHA-256 of the value**, not the value itself. Replay requires the same variable name and the same value hash. Do not use this as a secret-management mechanism; command histories and process environments have their own exposure risks.

## Replay

```bash
python3 tools/tau_fcis.py replay \
  --bundle /tmp/tau-fcis-run \
  --tau /absolute/path/to/tau \
  --tau-codegen /absolute/path/to/tau_codegen \
  --ltlsynt /absolute/path/to/ltlsynt
```

Replay first validates the receipt, every retained artifact hash, and every supplied tool hash. It then snapshots the supplied tool bytes again, reruns code generation in a private temporary directory, and requires the generated program and strategy to match the original bundle byte-for-byte.

A mismatch exits with code `2` and is blocked.

## Bundle format

The format identifier is:

```text
zeno-fcis/tau-synthesis-evidence/1
```

A bundle contains:

- `subject.tau` — exact specification bytes;
- `program.h` — synthesized C++ header when Tau emitted one;
- `strategy.hoa` — exported winning strategy when Tau emitted one;
- `stdout.bin` / `stderr.bin` — raw `tau_codegen` output;
- `tau-version.stdout.bin` / `tau-version.stderr.bin` — raw version probe output;
- `receipt.json` — canonical JSON evidence record.

The receipt includes two hashes with different purposes:

1. `evidence_subject_sha256` binds the portable semantic subject: specification hash, exact executable hashes, declared Tau source commit, stable invocation configuration, classification, and generated program/strategy hashes. Machine-local executable paths and elapsed time do not enter this identity.
2. `receipt_sha256` binds the complete retained run record, including log artifact hashes and observed duration. It therefore identifies that particular observation rather than a portable semantic subject.

## Tool snapshot boundary

Configured executables must be ordinary regular files, not symlinks, and are limited to 512 MiB. The bridge hashes each executable while copying it into a private directory, rehashes the private copy, and executes only that copy. This prevents a path from being replaced between admission and execution.

For LTL synthesis, supply `--ltlsynt`. The private process `PATH` contains only the snapshotted tool directory. Other environment variables are cleared except a minimal platform/temp allow-list and values explicitly supplied with `--env`.

The current adapter does **not** snapshot the host kernel, dynamic loader, shared libraries, or every transitive runtime dependency. Those remain outside the receipt and are stated as nonclaims.

## Source-commit claim boundary

`--tau-source-commit` is required because the source revision matters for review, especially while Tau's plugin/synthesis branches are moving quickly. In v1 it is deliberately recorded as:

```text
caller_declared_metadata_only
```

The executable SHA-256 is the actual executed code identity. The bridge does not claim that it has proven the supplied binary was built from the declared source revision. A later integration may bind a reproducible build or independently retained build provenance.

## Classification

A run is `proposed_synthesis` only when:

- the snapshotted `tau --version` probe succeeds;
- `tau_codegen` exits successfully within the configured timeout;
- a nonempty generated program exists; and
- by default, a nonempty HOA strategy exists.

Otherwise the retained run is `blocked`. Typical reasons are `timeout`, `tau_codegen_failed`, `missing_generated_program`, and `missing_strategy`.

`proposed_synthesis` is intentionally not called `proved`, `verified`, `authorized`, or `safe`.

## What this does not prove

A successful replay does not prove that:

- the English/product requirement was translated correctly into Tau;
- Tau's algorithms or the selected Boolean-algebra/plugin implementation are sound;
- the compiler toolchain that later compiles `program.h` preserves semantics;
- the generated controller is safe outside the modeled input/state domain;
- the Tau binary corresponds to the declared source commit;
- the artifact has any ZenoFCIS publication, settlement, release, or value-moving authority.

Promotion into an existing ZenoFCIS evidence or authority type must remain a separate independently checked step.

## Tests

Run the self-contained adversarial harness:

```bash
python3 -m unittest -v tools/tests/test_tau_fcis.py
```

The tests use fake executable files and do not require Tau to be installed. They pin:

- successful capture and deterministic replay;
- tampered-spec rejection;
- changed-tool rejection;
- missing-strategy fail-closed retention;
- symlinked-tool rejection;
- receipt tamper rejection;
- hash binding of additional environment values; and
- refusal to overwrite a nonempty output directory.

A future integration workflow should add an exact Tau source pin and replay a real synthesized example after the upstream branch/toolchain interface stabilizes.
