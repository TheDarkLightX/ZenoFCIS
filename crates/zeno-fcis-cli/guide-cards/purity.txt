```json
{
  "name": "purity",
  "title": "Report ambient effects in Rust decision code",
  "synopsis": "zeno-fcis purity PATH... --format json",
  "engine": "Static rule table over parsed Rust source: clocks, environment, files, I/O, network, processes, threads, randomness, hash-map iteration, shared state, raw addresses, unsafe and foreign code; crate directories also get the confinement check.",
  "use_when": [
    "check decision code for nondeterminism",
    "find ambient effects in Rust",
    "confinement of a library crate",
    "purity of a generated application"
  ],
  "not_for": [
    "a proof of determinism",
    "runtime behaviour or repeated-execution probing",
    "non-Rust sources"
  ],
  "inputs": [
    "PATH: a Rust file, a directory of Rust files, or a crate directory with Cargo.toml; repeatable. A crate is also checked for confinement: library-only, src/lib.rs root, no_std, forbid(unsafe_code), no include!/#[path], a completely read manifest whose dependencies are the library's semantic crates. Every [patch] section is refused (2.7.0); see docs/FACTORY_GAPS_27.md F27-30."
  ],
  "outputs": [
    "stdout zeno-fcis/purity-report/1 with --format json: status and one row per finding with its rule and location; writes nothing"
  ],
  "exit_codes": {
    "0": "clean or confined",
    "1": "violations: at least one error-level finding",
    "3": "unreadable: a path cannot be read or listed, holds no Rust source, or is a symbolic link the walk refuses",
    "64": "invalid command-line usage"
  },
  "verdicts": {
    "clean": "No error-level finding in the listed files; warnings (floating point, DefaultHasher) may remain. Not a proof of determinism.",
    "confined": "A crate directory passed the rule table and the confinement check.",
    "violations": "Read each finding's rule and location; move the effect out of the decision code and rerun.",
    "unreadable": "Supply readable regular Rust sources or a crate directory; symbolic links inside a walked directory are refused."
  },
  "limits": {
    "paths": "unbounded; each file read whole",
    "other_caps": "unknown"
  },
  "refusals": {
    "patch-section": "A manifest with any [patch] table is refused, including the binding `new` writes for a generated application; run purity on the Rust files directly until F27-30 lands.",
    "symbolic-link": "Replace the link with the file or directory it names."
  },
  "cost": {
    "measured_units": "unknown",
    "example_wall_ms": "unknown",
    "telemetry": "PENDING: populate from source-bound invocation telemetry"
  },
  "steps": [
    {
      "command": "zeno-fcis purity crates/zeno-fcis-cli/templates/durable-counter/src/program.rs crates/zeno-fcis-cli/templates/durable-counter/src/laws.rs --format json",
      "exit": 0,
      "expected": {
        "status": "clean"
      },
      "contains": []
    }
  ],
  "example": {
    "command": "zeno-fcis purity crates/zeno-fcis-cli/src/main.rs --format json",
    "exit": 1,
    "expected": {
      "status": "violations"
    },
    "contains": []
  },
  "sources": {
    "synopsis": "docs/CLI_REFERENCE.md:309-333; docs/DETERMINISM.md; tools/atdd.py determinism scenario",
    "engine": "docs/CLI_REFERENCE.md:309-333; docs/DETERMINISM.md; tools/atdd.py determinism scenario",
    "inputs": "docs/CLI_REFERENCE.md:309-333; docs/DETERMINISM.md; tools/atdd.py determinism scenario",
    "outputs": "docs/CLI_REFERENCE.md:309-333; docs/DETERMINISM.md; tools/atdd.py determinism scenario",
    "exit_codes": "docs/CLI_REFERENCE.md:309-333; docs/DETERMINISM.md; tools/atdd.py determinism scenario",
    "verdicts": "docs/CLI_REFERENCE.md:309-333; docs/DETERMINISM.md; tools/atdd.py determinism scenario",
    "limits": "docs/CLI_REFERENCE.md:309-333; docs/DETERMINISM.md; tools/atdd.py determinism scenario",
    "refusals": "docs/CLI_REFERENCE.md:309-333; docs/DETERMINISM.md; tools/atdd.py determinism scenario"
  },
  "commands": [
    "purity"
  ],
  "pitfalls": "A clean result is checked against the rule table only; it is not a proof of determinism. Only error-level rules decide the status, so read warnings too. Read the result scope before revising the input. No report grants runtime or publication authority."
}
```

# Report ambient effects in Rust decision code

## Use when

check decision code for nondeterminism; find ambient effects in Rust; confinement of a library crate; purity of a generated application.

## Not for

a proof of determinism; runtime behaviour or repeated-execution probing; non-Rust sources.

## Inputs

PATH: a Rust file, a directory of Rust files, or a crate directory with Cargo.toml; repeatable. A crate is also checked for confinement: library-only, src/lib.rs root, no_std, forbid(unsafe_code), no include!/#[path], a completely read manifest whose dependencies are the library's semantic crates. Every [patch] section is refused (2.7.0); see docs/FACTORY_GAPS_27.md F27-30.

Engine: Static rule table over parsed Rust source: clocks, environment, files, I/O, network, processes, threads, randomness, hash-map iteration, shared state, raw addresses, unsafe and foreign code; crate directories also get the confinement check.

Synopsis: "zeno-fcis purity PATH... --format json"

Set WORK to a fresh writable directory outside the source tree. Run commands from the repository root with zeno-fcis on PATH.

## Steps

```sh
zeno-fcis purity crates/zeno-fcis-cli/templates/durable-counter/src/program.rs crates/zeno-fcis-cli/templates/durable-counter/src/laws.rs --format json
```

Expected: exit 0; {"status": "clean"}.

## Verdicts

`clean`: No error-level finding in the listed files; warnings (floating point, DefaultHasher) may remain. Not a proof of determinism.

`confined`: A crate directory passed the rule table and the confinement check.

`violations`: Read each finding's rule and location; move the effect out of the decision code and rerun.

`unreadable`: Supply readable regular Rust sources or a crate directory; symbolic links inside a walked directory are refused.

Exit codes: {"0": "clean or confined", "1": "violations: at least one error-level finding", "3": "unreadable: a path cannot be read or listed, holds no Rust source, or is a symbolic link the walk refuses", "64": "invalid command-line usage"}

Outputs: ["stdout zeno-fcis/purity-report/1 with --format json: status and one row per finding with its rule and location; writes nothing"]

## Cost

{"example_wall_ms": "unknown", "measured_units": "unknown", "telemetry": "PENDING: populate from source-bound invocation telemetry"}

Limits: {"other_caps": "unknown", "paths": "unbounded; each file read whole"}

## Coordinate

Run Commons `advise` for the task first. Compute the exact subject digest, take an exclusive lease `loop:<subject digest>`, and respect existing leases. After checking, add a result record with the command, subject, verdict, scope and retained evidence; release the lease. Telemetry and advice grant no checker authority.

## Pitfalls

A clean result is checked against the rule table only; it is not a proof of determinism. Only error-level rules decide the status, so read warnings too. Read the result scope before revising the input. No report grants runtime or publication authority.

Refusals and remedies: {"patch-section": "A manifest with any [patch] table is refused, including the binding `new` writes for a generated application; run purity on the Rust files directly until F27-30 lands.", "symbolic-link": "Replace the link with the file or directory it names."}

## Worked example

```sh
zeno-fcis purity crates/zeno-fcis-cli/src/main.rs --format json
```

Expected: exit 1; {"status": "violations"}.

## See also

Sources: {"engine": "docs/CLI_REFERENCE.md:309-333; docs/DETERMINISM.md; tools/atdd.py determinism scenario", "exit_codes": "docs/CLI_REFERENCE.md:309-333; docs/DETERMINISM.md; tools/atdd.py determinism scenario", "inputs": "docs/CLI_REFERENCE.md:309-333; docs/DETERMINISM.md; tools/atdd.py determinism scenario", "limits": "docs/CLI_REFERENCE.md:309-333; docs/DETERMINISM.md; tools/atdd.py determinism scenario", "outputs": "docs/CLI_REFERENCE.md:309-333; docs/DETERMINISM.md; tools/atdd.py determinism scenario", "refusals": "docs/CLI_REFERENCE.md:309-333; docs/DETERMINISM.md; tools/atdd.py determinism scenario", "synopsis": "docs/CLI_REFERENCE.md:309-333; docs/DETERMINISM.md; tools/atdd.py determinism scenario", "verdicts": "docs/CLI_REFERENCE.md:309-333; docs/DETERMINISM.md; tools/atdd.py determinism scenario"}

Run `zeno-fcis guide find "check decision code for nondeterminism"` for related cards.
