```json
{
  "name": "assemble",
  "title": "Build a closed experimental runtime assembly",
  "synopsis": "zeno-fcis assemble ASSEMBLY.json --out DIR --scratch DIR [--source TREE]",
  "engine": "The pinned runtime assembler (tools/runtime_assembler.py, digest-bound into the CLI) admits a reviewed zeno-fcis/app-assembly/1 description, refuses ambient build inputs, builds the generated application with the app factory, runs the native contract review, ZenoUI checks, HTTP/SQLite journeys, interruption tests and a copied-binary launcher journey, then packages failure-atomically.",
  "use_when": [
    "assemble the task-status example",
    "build a runtime assembly profile",
    "package a generated application with evidence",
    "reviewed assembly description"
  ],
  "not_for": [
    "a universal application framework or completed qualification",
    "publication authority (an assembly manifest never authorises one)",
    "quick checks: the build takes a release binary build and real-process tests"
  ],
  "inputs": [
    "ASSEMBLY.json: at most 64 KiB, closed schema with no duplicate keys, produced by `python3 -B tools/runtime_assembler.py --cli CLI --prepare` and reviewed by the owner (configuration, exact identities, experimental profile). --out must not exist; --out and --scratch must be outside the source tree. --source: the reviewed source tree, default the tree this CLI was built from. Source files are bounded to 128 MiB and the CLI is stream-hashed with a 1 GiB limit. Compiler, linker and flag variables and any Cargo configuration file under CARGO_HOME or a parent of the paths refuse by name; build.jobs and CARGO_BUILD_JOBS are accepted."
  ],
  "outputs": [
    "--out/package.json binds the final files, assembly.json the selected inputs, evidence/checks.json the actual command outcomes and log identities, evidence/server.log the launcher log; on failure the failed command's diagnostic and stage logs are retained beside --out and no package is published"
  ],
  "exit_codes": {
    "0": "assembly built; profile qualification remains incomplete (see package.json)",
    "1": "the pinned assembler in --source differs from the one compiled into this CLI; rebuild the CLI",
    "2": "python3 could not be started",
    "3": "the assembler refused or a build, test or packaging step failed; read the retained diagnostic",
    "64": "invalid command-line usage"
  },
  "verdicts": {
    "built": "Hashes establish content identity, not behaviour, authentication or whole-program proof. Two-clean-build reproducibility and offline installation remain qualification gaps.",
    "refused": "The assembler names the refusal, for example AssemblyAmbientCargoConfiguration:<file>:<setting> or AssemblyArguments; fix the input and rerun."
  },
  "limits": {
    "description_bytes": 65536,
    "source_file_bytes": 134217728,
    "cli_hash_bytes": 1073741824,
    "profiles": "reviewed catalog only"
  },
  "refusals": {
    "AssemblyArguments": "Pass --assembly, --out and --scratch together (or --prepare alone) to the assembler.",
    "AssemblyAmbientCargoConfiguration": "Remove or move the named Cargo configuration setting; only build.jobs is accepted.",
    "pinned-assembler-differs": "The source tree's tools/runtime_assembler.py is not the one this CLI was built with; rebuild the CLI from that tree."
  },
  "cost": {
    "measured_units": "unknown",
    "example_wall_ms": "unknown",
    "telemetry": "PENDING: populate from source-bound invocation telemetry",
    "note": "a complete assembly builds a release binary and runs real-process tests; minutes, not seconds"
  },
  "steps": [
    {
      "command": "python3 -B tools/runtime_assembler.py --cli \"$(command -v zeno-fcis)\" --prepare > $WORK/task-assembly.json",
      "exit": 0,
      "expected": {},
      "contains": []
    }
  ],
  "example": {
    "command": "zeno-fcis assemble $WORK/missing-assembly.json --out $WORK/task-app --scratch $WORK/task-build",
    "exit": 3,
    "expected": {},
    "contains": [
      "assembly refused"
    ]
  },
  "sources": {
    "synopsis": "docs/RUNTIME_ASSEMBLY.md; docs/RUNTIME_ASSEMBLER_DESIGN.md; crates/zeno-fcis-cli/src/app_factory.rs assemble; tools/runtime_assembler.py",
    "engine": "docs/RUNTIME_ASSEMBLY.md; docs/RUNTIME_ASSEMBLER_DESIGN.md; crates/zeno-fcis-cli/src/app_factory.rs assemble; tools/runtime_assembler.py",
    "inputs": "docs/RUNTIME_ASSEMBLY.md; docs/RUNTIME_ASSEMBLER_DESIGN.md; crates/zeno-fcis-cli/src/app_factory.rs assemble; tools/runtime_assembler.py",
    "outputs": "docs/RUNTIME_ASSEMBLY.md; docs/RUNTIME_ASSEMBLER_DESIGN.md; crates/zeno-fcis-cli/src/app_factory.rs assemble; tools/runtime_assembler.py",
    "exit_codes": "docs/RUNTIME_ASSEMBLY.md; docs/RUNTIME_ASSEMBLER_DESIGN.md; crates/zeno-fcis-cli/src/app_factory.rs assemble; tools/runtime_assembler.py",
    "verdicts": "docs/RUNTIME_ASSEMBLY.md; docs/RUNTIME_ASSEMBLER_DESIGN.md; crates/zeno-fcis-cli/src/app_factory.rs assemble; tools/runtime_assembler.py",
    "limits": "docs/RUNTIME_ASSEMBLY.md; docs/RUNTIME_ASSEMBLER_DESIGN.md; crates/zeno-fcis-cli/src/app_factory.rs assemble; tools/runtime_assembler.py",
    "refusals": "docs/RUNTIME_ASSEMBLY.md; docs/RUNTIME_ASSEMBLER_DESIGN.md; crates/zeno-fcis-cli/src/app_factory.rs assemble; tools/runtime_assembler.py"
  },
  "commands": [
    "assemble"
  ],
  "pitfalls": "Review the prepared description before building: the assembler never executes manifest-provided commands, but it does build and run the described application. Diagnostic and mocked unit checks are not installed-app evidence. The retained scaffold keeps absolute Cargo paths to the reviewed tree and is not a portable source release. Read the result scope before revising the input. No report grants runtime or publication authority."
}
```

# Build a closed experimental runtime assembly

## Use when

assemble the task-status example; build a runtime assembly profile; package a generated application with evidence; reviewed assembly description.

## Not for

a universal application framework or completed qualification; publication authority (an assembly manifest never authorises one); quick checks: the build takes a release binary build and real-process tests.

## Inputs

ASSEMBLY.json: at most 64 KiB, closed schema with no duplicate keys, produced by `python3 -B tools/runtime_assembler.py --cli CLI --prepare` and reviewed by the owner (configuration, exact identities, experimental profile). --out must not exist; --out and --scratch must be outside the source tree. --source: the reviewed source tree, default the tree this CLI was built from. Source files are bounded to 128 MiB and the CLI is stream-hashed with a 1 GiB limit. Compiler, linker and flag variables and any Cargo configuration file under CARGO_HOME or a parent of the paths refuse by name; build.jobs and CARGO_BUILD_JOBS are accepted.

Engine: The pinned runtime assembler (tools/runtime_assembler.py, digest-bound into the CLI) admits a reviewed zeno-fcis/app-assembly/1 description, refuses ambient build inputs, builds the generated application with the app factory, runs the native contract review, ZenoUI checks, HTTP/SQLite journeys, interruption tests and a copied-binary launcher journey, then packages failure-atomically.

Synopsis: "zeno-fcis assemble ASSEMBLY.json --out DIR --scratch DIR [--source TREE]"

Set WORK to a fresh writable directory outside the source tree. Run commands from the repository root with zeno-fcis on PATH.

## Steps

```sh
python3 -B tools/runtime_assembler.py --cli "$(command -v zeno-fcis)" --prepare > $WORK/task-assembly.json
```

Expected: exit 0; [].

## Verdicts

`built`: Hashes establish content identity, not behaviour, authentication or whole-program proof. Two-clean-build reproducibility and offline installation remain qualification gaps.

`refused`: The assembler names the refusal, for example AssemblyAmbientCargoConfiguration:<file>:<setting> or AssemblyArguments; fix the input and rerun.

Exit codes: {"0": "assembly built; profile qualification remains incomplete (see package.json)", "1": "the pinned assembler in --source differs from the one compiled into this CLI; rebuild the CLI", "2": "python3 could not be started", "3": "the assembler refused or a build, test or packaging step failed; read the retained diagnostic", "64": "invalid command-line usage"}

Outputs: ["--out/package.json binds the final files, assembly.json the selected inputs, evidence/checks.json the actual command outcomes and log identities, evidence/server.log the launcher log; on failure the failed command's diagnostic and stage logs are retained beside --out and no package is published"]

## Cost

{"example_wall_ms": "unknown", "measured_units": "unknown", "note": "a complete assembly builds a release binary and runs real-process tests; minutes, not seconds", "telemetry": "PENDING: populate from source-bound invocation telemetry"}

Limits: {"cli_hash_bytes": 1073741824, "description_bytes": 65536, "profiles": "reviewed catalog only", "source_file_bytes": 134217728}

## Coordinate

Run Commons `advise` for the task first. Compute the exact subject digest, take an exclusive lease `loop:<subject digest>`, and respect existing leases. After checking, add a result record with the command, subject, verdict, scope and retained evidence; release the lease. Telemetry and advice grant no checker authority.

## Pitfalls

Review the prepared description before building: the assembler never executes manifest-provided commands, but it does build and run the described application. Diagnostic and mocked unit checks are not installed-app evidence. The retained scaffold keeps absolute Cargo paths to the reviewed tree and is not a portable source release. Read the result scope before revising the input. No report grants runtime or publication authority.

Refusals and remedies: {"AssemblyAmbientCargoConfiguration": "Remove or move the named Cargo configuration setting; only build.jobs is accepted.", "AssemblyArguments": "Pass --assembly, --out and --scratch together (or --prepare alone) to the assembler.", "pinned-assembler-differs": "The source tree's tools/runtime_assembler.py is not the one this CLI was built with; rebuild the CLI from that tree."}

## Worked example

```sh
zeno-fcis assemble $WORK/missing-assembly.json --out $WORK/task-app --scratch $WORK/task-build
```

Expected: exit 3; ["assembly refused"].

## See also

Sources: {"engine": "docs/RUNTIME_ASSEMBLY.md; docs/RUNTIME_ASSEMBLER_DESIGN.md; crates/zeno-fcis-cli/src/app_factory.rs assemble; tools/runtime_assembler.py", "exit_codes": "docs/RUNTIME_ASSEMBLY.md; docs/RUNTIME_ASSEMBLER_DESIGN.md; crates/zeno-fcis-cli/src/app_factory.rs assemble; tools/runtime_assembler.py", "inputs": "docs/RUNTIME_ASSEMBLY.md; docs/RUNTIME_ASSEMBLER_DESIGN.md; crates/zeno-fcis-cli/src/app_factory.rs assemble; tools/runtime_assembler.py", "limits": "docs/RUNTIME_ASSEMBLY.md; docs/RUNTIME_ASSEMBLER_DESIGN.md; crates/zeno-fcis-cli/src/app_factory.rs assemble; tools/runtime_assembler.py", "outputs": "docs/RUNTIME_ASSEMBLY.md; docs/RUNTIME_ASSEMBLER_DESIGN.md; crates/zeno-fcis-cli/src/app_factory.rs assemble; tools/runtime_assembler.py", "refusals": "docs/RUNTIME_ASSEMBLY.md; docs/RUNTIME_ASSEMBLER_DESIGN.md; crates/zeno-fcis-cli/src/app_factory.rs assemble; tools/runtime_assembler.py", "synopsis": "docs/RUNTIME_ASSEMBLY.md; docs/RUNTIME_ASSEMBLER_DESIGN.md; crates/zeno-fcis-cli/src/app_factory.rs assemble; tools/runtime_assembler.py", "verdicts": "docs/RUNTIME_ASSEMBLY.md; docs/RUNTIME_ASSEMBLER_DESIGN.md; crates/zeno-fcis-cli/src/app_factory.rs assemble; tools/runtime_assembler.py"}

Run `zeno-fcis guide find "assemble the task-status example"` for related cards.
