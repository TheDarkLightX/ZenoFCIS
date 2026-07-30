# Tutorial: use the CLI as a checked workflow

Start with one command:

```bash
cargo +1.97.1 run --quiet -p zeno-fcis-cli --locked -- \
  check examples/minimal/project.zeno
```

```text
checked examples/minimal/project.zeno: project=1 components=1 claims=1 unresolved_obligations=2 semantic_program_hash=57385a54387db8ad3e2da9a46a9ae22d2e72502b336120f570791d79e736b365
```

The line shows the project ID, number of components, number of claims, checks
that still need evidence, and the identity of the checked program.

For automation, request JSON:

```bash
cargo +1.97.1 run --quiet -p zeno-fcis-cli --locked -- \
  check examples/minimal/project.zeno --format json
```

```json
{"claims":1,"components":1,"path":"examples/minimal/project.zeno","project_id":1,"schema":"zeno-fcis/cli/1","semantic_program_hash":"57385a54387db8ad3e2da9a46a9ae22d2e72502b336120f570791d79e736b365","status":"valid","unresolved_obligations":2}
```

The keys appear in stable order and the schema has an explicit version.

## Turn problems into actionable output

```bash
cargo +1.97.1 run --quiet -p zeno-fcis-cli --locked -- \
  check examples/diagnostics-tour/project.zeno --format human
```

One run reports all independent problems it can retain. Use `explain` to focus
on one stable code:

```bash
cargo +1.97.1 run --quiet -p zeno-fcis-cli --locked -- \
  explain examples/diagnostics-tour/project.zeno --code ZENO-E0203
```

## Generate, view, and check

```bash
cargo +1.97.1 run --quiet -p zeno-fcis-cli --locked -- \
  graph examples/minimal/project.zeno --format mermaid
cargo +1.97.1 run --quiet -p zeno-fcis-cli --locked -- \
  generate examples/minimal/project.zeno --out generated
cargo +1.97.1 run --quiet -p zeno-fcis-cli --locked -- \
  generate examples/minimal/project.zeno --out generated --check
```

`graph` produces a review view. `generate` writes through temporary files and
renames them into place. `generate --check` only reads and reports drift.

## Use exit codes as decisions

| Code | Meaning |
|---:|---|
| `0` | The requested positive result completed. |
| `1` | The project is invalid or the checked claim has a counterexample. |
| `2` | Required evidence or a required tool is unavailable, or evaluation could not decide. |
| `3` | A tool, file, or bounded execution failed. |
| `64` | The command line is invalid. |

Treat code `2` as blocked. A build system should stop a release or deployment and
  preserve
the reason for review.

The CLI can check source, generate code, draw graphs, explain messages, invoke
approved formal tools, and retain their output. These commands cannot create
permission to update trusted state, receipts, or commits.

The [CLI reference](../CLI_REFERENCE.md) lists every command and option. The
[language tutorial](LANGUAGE.md), [composition tutorial](COMPOSITION.md), and
[formal-tools tutorial](FORMAL_TOOLS.md) provide focused examples.
