# Tutorial: run formal tools and read each result

List the pinned backend versions:

```bash
cargo +1.97.1 run --quiet -p zeno-fcis-cli --locked -- backend list
```

```text
cvc5 1.3.3
z3 4.16.0
lean 4.30.0
```

The list includes Lean's pinned identity, but the current public Lean exporter
returns `UnsupportedMode`, so `prove --backend lean` stays blocked (exit `2`).
For independently checked scalar program equivalence, use the
[certificate route](#check-scalar-program-certificates) below.

## Ask CVC5 about one relational claim

First find the executable hash:

```bash
sha256sum /absolute/path/to/cvc5
```

Create `zeno-fcis.tools.cvc5.json` and replace the path and hash with the
values from your machine:

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

Check the tool, then run claim 500 from the minimal project:

```bash
cargo +1.97.1 run --quiet -p zeno-fcis-cli --locked -- \
  doctor --tools zeno-fcis.tools.cvc5.json
cargo +1.97.1 run --quiet -p zeno-fcis-cli --locked -- \
  prove examples/minimal/project.zeno --claim 500 --backend cvc5 \
  --tools zeno-fcis.tools.cvc5.json
printf 'exit %s\n' "$?"
```

For an UNSAT answer with proof-shaped output, ZenoFCIS prints the first line
below. The `printf` command prints the exit code on the second line:

```text
cvc5 claim 500: UNSAT proposal retained; proof output was not independently checked
cvc5 claim 500 scope: no transition relation was exported, so this result holds for every bounded observation assignment and says nothing specific about this system
exit 2
```

The command keeps the proposal under:

```text
examples/minimal/.zeno-fcis/evidence/<content-hash>/
  formal-run-record.bin
  record.json
  source
  stderr
  stdout
  transcript-01-decision-input
  transcript-01-decision-stderr
  transcript-01-decision-stdout
  transcript-02-evidence-input
  transcript-02-evidence-stderr
  transcript-02-evidence-stdout
```

Exit code `2` tells automation that the requested proof is still blocked. The
retained source and solver output let a reviewer inspect exactly what ran.

Read the `scope:` line. Claim 500 is `pre.100 == pre.100`, which is true for
every observation assignment. No transition relation was exported, so the
solver result says nothing specific about the application, and `check`
reports the claim as `constant-true`. See
[law and claim substance](../CLAIM_SUBSTANCE.md).

Before each run, the adapter checks the executable's file type, its hash and
its exact reported version. It runs a private copy with fixed arguments, a
cleared environment, a timeout and an output bound.

## Use Z3 and request a counterexample

Create `zeno-fcis.tools.z3.json` with the same fields, setting `backend` to
`z3`, `version` to `4.16.0`, and using Z3's own executable path and hash.
Claim 500 in the Mini Determinator selects both SMT backends:

```bash
cargo +1.97.1 run --quiet -p zeno-fcis-cli --locked -- \
  prove examples/mini-determinator/project.zeno --claim 500 --backend z3 \
  --tools zeno-fcis.tools.z3.json
cargo +1.97.1 run --quiet -p zeno-fcis-cli --locked -- \
  counterexample examples/mini-determinator/project.zeno --claim 500 --backend z3 \
  --tools zeno-fcis.tools.z3.json
```

This reflexive claim has no counterexample, so Z3's UNSAT answer stays
blocked (exit `2`) for either command. For a refutable claim, a SAT model
counts only after the library evaluator replays it. After that replay,
`prove` exits `1` and `counterexample` exits `0`. To request a counterexample
for a claim that selects CVC5, use `--backend cvc5` and the CVC5 manifest.

## Check scalar program certificates

`transform check-certificates` independently checks complete Alethe
certificates with the pinned Carcara checker. It compares admitted scalar
programs whose domain is larger than the enumeration cap. The comparison stays
within the quantifier-free Boolean and linear integer fragment over eager
checked-i64 programs, and the Step limit must never bind.

Follow the [symbolic checks guide](../SYMBOLIC_CHECKS.md#independently-checked-scalar-certificates)
to build and identify the checker, create `checker.json`, and supply exactly
`0.alethe`, `1.alethe`, and so on in regenerated difference-query order.
The checker builder does not generate those proofs. With the matching tools
manifest and certificates prepared, run:

```bash
cargo +1.97.1 run --quiet -p zeno-fcis-cli --locked -- \
  transform check-certificates --original original.zcve --candidate candidate.zcve \
  --tools zeno-fcis.tools.cvc5.json --checker checker.json \
  --certificates certificates --out certificate-receipt.json
cargo +1.97.1 run --quiet -p zeno-fcis-cli --locked -- \
  transform replay-certificates --receipt certificate-receipt.json \
  --original original.zcve --candidate candidate.zcve \
  --tools zeno-fcis.tools.cvc5.json --checker checker.json --certificates certificates
```

A complete check exits `0` with `certificate-checked-equivalent`, evidence
`CarcaraChecked`, and receipt schema `zeno-fcis/transform-certificate-receipt/1`.
A successful replay regenerates the queries, independently rechecks every
certificate and requires byte-identical receipt contents. It then exits `0`
with `certificate-replayed`. Missing, corrupt, unsupported or incomplete
proofs, undecided queries and failed controls prevent a passing receipt.

The result covers ordered outputs and exact eager failures on the declared
domain under the scalar query/runtime refinement and the named trusted base:
Carcara and its parser/fragment guard, SMT-LIB meaning, the compiler, loader and
operating system. It does not establish Step-usage equality, source-rule meaning,
Lean kernel checking or verified compilation. The receipt grants no application
or adoption authority. `transform replay`, `contract adopt`, contract generation
and `contract refresh-receipts` require the exhaustive receipt format and refuse
this separate schema.

## See missing configuration fail closed

Requesting a proof without a tools manifest returns exit code `2`:

```bash
cargo +1.97.1 run --quiet -p zeno-fcis-cli --locked -- \
  prove examples/minimal/project.zeno --claim 500 --backend cvc5
```

```text
tools manifest blocked: Io("No such file or directory (os error 2)")
```

No proof, certificate, or production permission is created; missing
configuration shows up as a blocked result.

## Move a tools/1 manifest to tools/2

RC3 reports both schema names when it reads an old manifest:

```text
tools manifest blocked: WrongFormat { expected: "zeno-fcis/tools/2", actual: "zeno-fcis/tools/1" }
```

Change the top-level `format` value to `zeno-fcis/tools/2`. CVC5 and Z3 need no
other field changes. Retained Lean manifest entries need `runtime.root` and
`runtime.tree_sha256` as described in the [tools-manifest reference](../CLI_REFERENCE.md#tools-manifest);
these fields do not enable the unsupported public Lean exporter. Check the
result with:

```bash
cargo +1.97.1 run --quiet -p zeno-fcis-cli --locked -- \
  backend inspect --tools zeno-fcis.tools.json
```

## Read solver and certificate outcomes

| Result | Command exit | What ZenoFCIS keeps |
| --- | ---: | --- |
| CVC5 UNSAT proposal | `2` | Exact SMT-LIB, checked tool identity, and proof-shaped output |
| Z3 UNSAT | `2` | A blocked process result because `prove` has no Z3 proof checker |
| Replayed CVC5 or Z3 SAT model | `1` from `prove`, `0` from `counterexample` | Normalized counterexample and exact run |
| Public Lean export | `2` | Export is unsupported (`UnsupportedMode`); no theorem is generated |
| Complete scalar certificate check | `0` | `CarcaraChecked` receipt binding programs, domains, queries, certificates and checker identity |

For relational and finite claims, CVC5 and Z3 check exported formulas. Finite
SMT covers every nonempty trace length through the stated horizon and keeps
checked `i128` arithmetic visible. These translated claims do not include an
application transition relation.

A missing tool, wrong hash, wrong version, timeout, crash, oversized output,
`unknown`, mismatched claim or failed model replay stays blocked or fails with a
nonzero exit.

Solver runs supply reviewable evidence for the translated claim and its printed
scope. The scalar certificate route separately requires complete independent
checking; it does not turn solver-only results into checked certificates.

The [formal-tools reference](../FORMAL_TOOLS_RC3.md) gives the full JSON
schema, fixed arguments, output rules, retained-file layout, and exact scope of
each result. The [CLI reference](../CLI_REFERENCE.md) defines exit codes.
