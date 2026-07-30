# Tutorial: run formal tools and read each result

List the exact backend versions supported by RC3:

```bash
cargo +1.97.1 run --quiet -p zeno-fcis-cli --locked -- backend list
```

```text
cvc5 1.3.3
z3 4.16.0
lean 4.30.0
```

## See the successful path

With the qualified Lean distribution and tools manifest prepared below, this
command asks Lean to check claim 501 from the Mini Determinator:

```bash
cargo +1.97.1 run --quiet -p zeno-fcis-cli --locked -- \
  prove examples/mini-determinator/project.zeno --claim 501 --backend lean \
  --tools zeno-fcis.tools.lean.json
```

The successful path prints:

```text
lean claim 501: generated theorem kernel checked with the qualified RC3 toolchain identity and exact axiom report; production authority unchanged
```

This result covers the exact generated theorem, qualified Lean tree, and axiom
report. It leaves production authority unchanged. The next sections show how
to prepare the tool identity and how blocked solver results appear.

## Ask CVC5 about one finite claim

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

Exit code `2` tells automation that the requested proof remains blocked. The
retained source and solver output let a reviewer inspect exactly what ran.

## Ask Lean to check one generated theorem

Download the official Lean `4.30.0` Linux x86-64 archive, verify its recorded
archive checksum, and unpack it. RC3 qualifies this exact distribution:

```bash
curl --fail --location --output lean-4.30.0-linux.zip \
  https://github.com/leanprover/lean4/releases/download/v4.30.0/lean-4.30.0-linux.zip
printf '%s  %s\n' \
  3ffb3dc406912936a6b30885ce47a349c7ed8ee7e4e4dfac7361a497608bc8d1 \
  lean-4.30.0-linux.zip | sha256sum --check -
unzip lean-4.30.0-linux.zip
```

The same archive checksum is recorded in
`release/formal-tools-linux-x86_64.sha256`. Compute both manifest hashes, then
compare the tree inventory with RC3's recorded trust anchor:

```bash
sha256sum /absolute/path/to/lean-4.30.0/bin/lean
cargo +1.97.1 run --quiet -p zeno-fcis-cli --locked -- \
  backend inventory-lean /absolute/path/to/lean-4.30.0
test "$(cargo +1.97.1 run --quiet -p zeno-fcis-cli --locked -- \
  backend inventory-lean /absolute/path/to/lean-4.30.0 | \
  awk '/^lean tree_sha256 / { print $3 }')" = \
  "$(tr -d '\n' < release/lean-4.30.0-tree.sha256)"
```

The inventory command prints values in this form:

```text
lean tree_sha256 <tree-sha256>
files <file-count>
total_bytes <byte-count>
```

For the qualified archive, the tree hash is
`5dc9cab14b1a15fc8d6cfc3f1c1b627c0c74facb23465fb9463c42554a807f5b`.
The counts depend on the distribution. Copy the verified tree hash into
`zeno-fcis.tools.lean.json`:

```json
{
  "format": "zeno-fcis/tools/2",
  "tools": [
    {
      "backend": "lean",
      "path": "/absolute/path/to/lean-4.30.0/bin/lean",
      "version": "4.30.0",
      "sha256": "64-lowercase-hex-characters-from-sha256sum",
      "runtime": {
        "root": "/absolute/path/to/lean-4.30.0",
        "tree_sha256": "5dc9cab14b1a15fc8d6cfc3f1c1b627c0c74facb23465fb9463c42554a807f5b"
      },
      "timeout_ms": 30000,
      "max_output_bytes": 1048576,
      "allowed_axioms": ["Quot.sound", "propext"]
    }
  ]
}
```

Check the distribution and run the unbounded claim from the Mini Determinator
project:

```bash
cargo +1.97.1 run --quiet -p zeno-fcis-cli --locked -- \
  doctor --tools zeno-fcis.tools.lean.json
cargo +1.97.1 run --quiet -p zeno-fcis-cli --locked -- \
  prove examples/mini-determinator/project.zeno --claim 501 --backend lean \
  --tools zeno-fcis.tools.lean.json
printf 'exit %s\n' "$?"
```

A successful kernel check prints the first line below. The `printf` command
prints the exit code on the second line:

```text
lean claim 501: generated theorem kernel checked with the qualified RC3 toolchain identity and exact axiom report; production authority unchanged
exit 0
```

The retained directory contains the generated theorem and the full checked
toolchain inventory:

```text
examples/mini-determinator/.zeno-fcis/evidence/<content-hash>/
  formal-run-record.bin
  record.json
  source
  stderr
  stdout
  toolchain.json
  transcript-01-kernel-input
  transcript-01-kernel-stderr
  transcript-01-kernel-stdout
```

Before each run, ZenoFCIS checks the file type, hash, and exact reported
version. It starts the tool with a fixed argument list and an empty environment.
Lean runs from a checked private copy of its distribution. Commands go directly
to the selected executable.

`backend inventory-lean` can also describe a custom Lean tree. That hash gives
repeatable local consistency and lets `doctor` detect later changes. A custom
tree is outside the qualified RC3 identity, so its successful kernel output
remains blocked and `prove` returns exit code `2`.

## See missing configuration fail closed

Requesting a proof without a tools manifest returns exit code `2`:

```bash
cargo +1.97.1 run --quiet -p zeno-fcis-cli --locked -- \
  prove examples/minimal/project.zeno --claim 500 --backend cvc5
```

```text
tools manifest blocked: Io("No such file or directory (os error 2)")
```

No proof, certificate, or production permission is created. Missing
configuration is a visible blocked result.

## Move a tools/1 manifest to tools/2

RC3 reports both schema names when it reads an old manifest:

```text
tools manifest blocked: WrongFormat { expected: "zeno-fcis/tools/2", actual: "zeno-fcis/tools/1" }
```

Change the top-level `format` value to `zeno-fcis/tools/2`. CVC5 and Z3 need no
other field changes. Lean also needs the `runtime` object shown above. Check the
result with:

```bash
cargo +1.97.1 run --quiet -p zeno-fcis-cli --locked -- \
  backend inspect --tools zeno-fcis.tools.json
```

## Read solver and kernel outcomes

| Result | Command exit | What ZenoFCIS keeps |
| --- | ---: | --- |
| CVC5 UNSAT proposal | `2` | Exact SMT-LIB, checked tool identity, and proof-shaped output |
| Z3 UNSAT | `2` | A blocked process result because RC3 has no Z3 proof checker |
| Replayed CVC5 or Z3 SAT model | `1` from `prove`, `0` from `counterexample` | Normalized counterexample and exact run |
| Lean `KernelChecked` | `0` from `prove` | Generated theorem, exact axiom report, and Lean tree inventory |

CVC5 and Z3 receive relational or finite claims. Finite SMT covers every
nonempty trace length through the stated horizon and keeps checked `i128`
arithmetic visible. Lean receives unbounded proof requests. Its generated
source contains the original projection paths and relational formula.

A missing tool, wrong hash, wrong version, timeout, crash, oversized output,
`unknown`, mismatched claim, failed model replay, or rejected Lean axiom report
stays blocked or fails with a nonzero exit.

These runs supply reviewable evidence for the translated claim. A separate
backend verifier is responsible for creating any ZenoFCIS certificate types.

The [formal-tools reference](../FORMAL_TOOLS_RC3.md) gives the full JSON
schema, fixed arguments, output rules, retained-file layout, and exact scope of
each result. The [CLI reference](../CLI_REFERENCE.md) defines exit codes.
