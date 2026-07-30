# Tutorial: see formal tools fail closed

List the exact backend versions supported by RC3:

```bash
cargo +1.97.1 run --quiet -p zeno-fcis-cli --locked -- backend list
```

```text
cvc5 1.3.3
z3 4.16.0
lean 4.30.0
```

Now request a proof without configuring a tool:

```bash
cargo +1.97.1 run --quiet -p zeno-fcis-cli --locked -- \
  prove examples/minimal/project.zeno --claim 500 --backend cvc5
```

The command returns exit code `2` and prints:

```text
tools manifest blocked: Io("No such file or directory (os error 2)")
```

No proof, certificate, or production permission is created. Missing
configuration is a visible blocked result.

## Bind one exact executable

Tool paths live in a separate JSON file:

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

Check it, then run the exact claim:

```bash
cargo +1.97.1 run --quiet -p zeno-fcis-cli --locked -- \
  doctor --tools zeno-fcis.tools.json
cargo +1.97.1 run --quiet -p zeno-fcis-cli --locked -- \
  prove examples/minimal/project.zeno --claim 500 --backend cvc5 \
  --tools zeno-fcis.tools.json
```

Before every run, ZenoFCIS checks the file type, hash, and reported version.
It starts the tool with a fixed argument list and an empty environment. It
never passes the command through a shell.

## Follow the result

```text
checked claim
    -> exact SMT-LIB or Lean source
    -> checked tool identity
    -> bounded process run
    -> parsed output
    -> model replay when a solver finds a counterexample
    -> retained record named by its content hash
```

A missing tool, wrong hash, wrong version, timeout, crash, oversized output,
`unknown`, mismatched claim, failed model replay, or rejected Lean axiom report
stays blocked.

CVC5 and Z3 receive finite claims. The generated SMT covers every nonempty
trace length through the stated horizon and keeps checked `i128` arithmetic
visible to the solver. Lean receives unbounded proof requests. Its generated
source contains the original projection paths and relational formula, so a
reader can inspect the proposition that the kernel checked.

Solver agreement supplies useful evidence for the translated claim. A separate
backend verifier is responsible for creating any ZenoFCIS certificate types.

The [formal-tools reference](../FORMAL_TOOLS_RC3.md) gives the full JSON
schema, fixed arguments, output rules, retained-file layout, and exact scope of
each result. The [CLI reference](../CLI_REFERENCE.md) defines exit codes.
