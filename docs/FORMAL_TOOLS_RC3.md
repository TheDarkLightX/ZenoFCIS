# Formal tools in RC3

ZenoFCIS `1.0.0-rc.3` publicly qualifies CVC5 `1.3.3`, Z3 `4.16.0`, and Lean
`4.30.0`. `zeno-fcis-formal-tools` is a standard-library shell around the pure
exporters. It cannot construct `BackendCertificate` or production authority.

## Tools manifest

Executable configuration is separate from `.zeno` source:

```json
{
  "format": "zeno-fcis/tools/1",
  "tools": [
    {
      "backend": "cvc5",
      "path": "/absolute/path/to/cvc5",
      "version": "1.3.3",
      "sha256": "64-lowercase-hex-characters",
      "timeout_ms": 30000,
      "max_output_bytes": 1048576,
      "allowed_axioms": []
    }
  ]
}
```

Unknown fields, duplicate backends, wrong versions, invalid hashes, zero or
oversized bounds, and invalid axiom names are rejected. Lean axiom names form
an exact allow-list after canonical sorting. Before every run the adapter
rechecks that the path is a bounded regular file and reads at most 512 MiB. It
hashes those bytes, copies them to a private executable, and uses that same copy
for the version check and the requested run. Replacing the configured path after
admission cannot change the executed bytes. Process environments are cleared.

## Export and execution

`export_smt` accepts only relational and finite claims selecting CVC5 or Z3.
It emits deterministic SMT-LIB that asserts the negation of the claim together
with checked `i128` definedness. Finite obligations cover every nonempty trace
length through the declared horizon.

A preflight walk stops export before recursive rendering when a claim exceeds
256 temporal steps, 4,096 formula nodes, depth 256, one million conservative
render operations, or 16 MiB of generated source. Named predicate identifiers
use a length-prefixed hexadecimal encoding, so distinct source names stay
distinct in solver input.

CVC5 runs with `--safe-mode=safe`, proof production, and Alethe output. Z3 runs
with `-in -smt2`.

`export_lean` accepts only unbounded temporal claims selecting Lean. It emits
the exact claim ID and translates projections, relational atoms, checked
arithmetic, bounded sums and quantifiers, and every temporal operator into
kernel-checkable source. The generated source ends with an axiom-report
command. A generic relational placeholder is never substituted for the typed
claim.

The timeout clock starts before process creation and covers input delivery,
execution, and output collection. A child that never reads its input is killed
and reaped when the same configured timeout expires.

No shell is used. Missing tools, hash or version mismatch, crash, timeout,
output overflow, `unknown`, unsupported proof output, failed model replay, or
an axiom report different from the configured exact set remains blocked or
failed. Z3 UNSAT is differential evidence and remains blocked because RC3 does
not qualify a Z3 proof checker. CVC5 UNSAT is only `ProposedUnsat` when
proof-shaped output is present.

SAT output is accepted as a refutation only after the model is normalized and
replayed through the built-in evaluator against the exact typed claim. Named
host predicates cannot be reconstructed from an SMT model and therefore block
replay.

## Retention

Successful process invocation retains exact generated source, checked tool
identity, stdout, stderr, classification, and a normalized replayed
counterexample when present. The directory name is the content hash of the
complete run record. Retention is diagnostic evidence only.

The permanent `formal-tools` workflow downloads the exact Linux x86-64
releases and checks the SHA-256 values in
`release/formal-tools-linux-x86_64.sha256`. It then kernel-checks a generated
Lean obligation and runs CVC5 and Z3 translation checks with those exact
executables. The checksums are those published in the official GitHub release
metadata.

## Nonclaims

Tool agreement does not establish translation correctness beyond the tested
fragment, unbounded temporal truth, allowed deployment effects, or production
authorization. Independent backend verification remains responsible for any
promotion into existing evidence types.
