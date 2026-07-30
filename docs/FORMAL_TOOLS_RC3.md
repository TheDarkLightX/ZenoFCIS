# Formal tools in RC3

ZenoFCIS `1.0.0-rc.3` pins and exercises process adapters for CVC5 `1.3.3`,
Z3 `4.16.0`, and Lean `4.30.0`. Each adapter gives its result a deliberately
limited classification. `zeno-fcis-formal-tools` is a standard-library shell
around the pure exporters. It cannot construct `BackendCertificate` or
production authority.

## Tools manifest

Executable configuration is separate from `.zeno` source:

```json
{
  "format": "zeno-fcis/tools/2",
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

Lean also requires the exact distribution root and its portable tree hash:

```json
{
  "backend": "lean",
  "path": "/absolute/path/to/lean-4.30.0/bin/lean",
  "version": "4.30.0",
  "sha256": "64-lowercase-hex-characters",
  "runtime": {
    "root": "/absolute/path/to/lean-4.30.0",
    "tree_sha256": "64-lowercase-hex-characters"
  },
  "timeout_ms": 30000,
  "max_output_bytes": 1048576,
  "allowed_axioms": ["Quot.sound", "propext"]
}
```

Compute that tree hash with the public CLI:

```bash
zeno-fcis backend inventory-lean /absolute/path/to/lean-4.30.0
```

The human form prints the tree hash, file count, and total bytes. The
`--format json` form prints the canonical `zeno-fcis/toolchain-inventory/1`
record.

RC3 qualifies the official Lean `4.30.0` Linux x86-64 archive recorded in
`release/formal-tools-linux-x86_64.sha256`. Its required portable tree hash is
recorded in `release/lean-4.30.0-tree.sha256` and exposed as
`LEAN_LINUX_X86_64_TREE_SHA256`. A different inventory can support repeatable
local checks, while its proof outcome remains blocked as unqualified evidence.

### Moving from tools/1

RC3 accepts `zeno-fcis/tools/2`. A tools/1 manifest produces exit code `2`
with the expected and actual formats:

```text
tools manifest blocked: WrongFormat { expected: "zeno-fcis/tools/2", actual: "zeno-fcis/tools/1" }
```

CVC5 and Z3 entries keep the same fields. A Lean entry adds the `runtime`
object shown above. Its `root` is the distribution passed to
`backend inventory-lean`, and `tree_sha256` is the command's reported hash.
Run `backend inspect` to check the manifest structure, then run `doctor` to
recheck the configured files.

Unknown fields, duplicate backends, wrong versions, invalid hashes, zero or
oversized bounds, and invalid axiom names are rejected. Lean axiom names form
an exact allow-list after canonical sorting. Before every run the adapter
rechecks that the path is a bounded regular file and reads at most 512 MiB. It
hashes those bytes, copies them to a private executable, and uses that same copy
for the version check and the requested run. Replacing the configured path after
admission cannot change the executed bytes. For Lean, the adapter rejects links
and special files, copies every admitted distribution file into a private tree,
checks the canonical tree hash, and uses that private tree as the sysroot.
Runtime trees are limited to 25,000 files, depth 64, 4 GiB total, 512 MiB per
file, and 4,096 UTF-8 bytes per relative path. Process environments are cleared.

## Export and execution

`export_smt` accepts only relational and finite claims selecting CVC5 or Z3.
It emits deterministic SMT-LIB that asserts the negation of the claim together
with checked `i128` definedness. Finite obligations cover every nonempty trace
length through the declared horizon.

A preflight walk stops export before recursive rendering when a claim exceeds
256 temporal steps, 4,096 formula nodes, depth 256, one million conservative
render operations, or 16 MiB of generated source. The Lean renderer also
checks its operation and byte budgets while constructing each term, so a wide
bounded formula stops before building an oversized intermediate string. Named
predicate identifiers use a length-prefixed hexadecimal encoding, so distinct
source names stay distinct in solver input.

CVC5 runs with `--safe-mode=safe` and proof production. Z3 runs
with `-in -smt2`.

`export_lean` accepts only unbounded temporal claims selecting Lean. It emits
the exact claim ID and has translations for projections, relational atoms,
checked arithmetic, bounded sums and quantifiers, and every temporal operator.
The generated source ends with an axiom-report command. A generic relational
placeholder is never substituted for the typed claim.

Deterministic Lean source tests currently cover exact claim identity,
projection paths, equality, and `always`. The pinned Lean workflow
kernel-checks the representative Mini Determinator claim 501 through both the
library and the CLI. The remaining translation branches are present in the RC3
exporter, with operator-complete source and kernel acceptance required before
stable V1.

The timeout clock starts before process creation and covers input delivery,
execution, and output collection. On Unix, every tool runs in a new process
group. Completion, timeout, and output failure all kill the remaining members
of that process group and reap the direct child, including when a descendant
keeps an output pipe open. A hostile descendant that creates a new session or
process group can leave this boundary. Running untrusted binaries requires a
stronger operating-system sandbox. Platforms without implemented process-tree
containment fail before starting a tool.

No shell is used. Missing tools, hash or version mismatch, crash, timeout,
output overflow, `unknown`, unsupported proof output, failed model replay, or
an axiom report different from the configured exact set remains blocked or
failed.

| Backend output | RC3 classification | CLI result |
| --- | --- | --- |
| CVC5 UNSAT with proof-shaped output | `ProposedUnsat` | Retain the proposal and return exit code `2`. The proof output is not independently checked. |
| Z3 UNSAT | `Blocked(UnsupportedEvidence)` | Retain the blocked run and return exit code `2`. |
| CVC5 or Z3 SAT with a replayed model | `Refuted` | Retain the normalized counterexample. `prove` returns `1`; `counterexample` returns `0`. |
| Qualified Lean kernel success with the configured exact axiom report | `KernelChecked` | Retain the run and return exit code `0` from `prove`. |
| Custom Lean tree reports kernel success | `Blocked(UnsupportedEvidence)` | Retain the run and return exit code `2`. |

SAT output is accepted as a refutation only after the model is normalized and
replayed through the built-in evaluator against the exact typed claim. Named
host predicates cannot be reconstructed from an SMT model and therefore block
replay.

## Retention

Every completed process invocation retains exact generated source, checked
tool identity, classification, and each decision, evidence, or kernel phase.
`formal-run-record.bin` contains the canonical bytes whose commitment names the
directory, so a reviewer can recompute the identity without reconstructing an
exit status from prose.
Each phase has its own exact input, stdout, and stderr file. The `stdout` and
`stderr` files are stable aliases for the final phase. A normalized replayed
counterexample is included when present. Lean runs also retain
`toolchain.json`, including every relative path, length, executable flag, file
hash, and the tree hash. The directory name is the content hash of the complete
run record. A complete bundle is written to a private staging directory and
published with one rename. Existing content at that hash must match byte for
byte. Retention is diagnostic evidence only.

The permanent `formal-tools` workflow downloads the exact Linux x86-64
releases and checks the SHA-256 values in
`release/formal-tools-linux-x86_64.sha256`. The portable Lean distribution hash
is recorded in `release/lean-4.30.0-tree.sha256`. The workflow checks a generated
Lean obligation through the private-snapshot adapter and runs CVC5 and Z3
translation checks with those exact executables. The archive checksums are
those published in the official GitHub release metadata.

## Nonclaims

Tool agreement does not establish translation correctness beyond the tested
fragment, allowed deployment effects, or production authorization. A
kernel-checked Lean run establishes the generated theorem under its reported
axioms. Review of the translation from the typed ZenoFCIS claim remains a
separate task.

The Lean tree hash covers the files in the admitted Lean distribution. The host
kernel, loader, and shared system libraries are outside that tree hash. The
workflow records its host platform so another reviewer can reproduce the same
boundary. Independent backend verification remains responsible for any
promotion into existing evidence types.
