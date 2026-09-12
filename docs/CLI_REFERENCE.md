# `zeno-fcis` CLI reference

The `zeno-fcis-cli` package in `1.0.0` publishes the `zeno-fcis` binary.
It pins `clap = 4.6.1` without environment parsing or color output.

```text
zeno-fcis describe [COMMAND...]
zeno-fcis new <dir> --template minimal|mini-determinator|durable-counter
zeno-fcis check [project.zeno] [--format human|json]
zeno-fcis generate [project.zeno] --out <dir> [--check] [--format human|json]
zeno-fcis graph [project.zeno] --format dot|mermaid|json
zeno-fcis explain [project.zeno] [--code CODE] [--format human|json]
zeno-fcis prove [project.zeno] --claim ID|all --backend cvc5|z3|lean|all [--tools FILE]
zeno-fcis counterexample [project.zeno] --claim ID --backend cvc5|z3 [--tools FILE]
zeno-fcis doctor [--tools FILE]
zeno-fcis backend list
zeno-fcis backend inspect|verify [--tools FILE]
zeno-fcis backend inventory-lean ROOT [--format human|json]
```

`new` refuses a nonempty target. `check` parses and elaborates in one command.
`generate` replaces each deterministic Rust/manifest file atomically;
`--check` writes nothing and reports drift. `graph` and `explain` are derived
diagnostic views. `prove` and `counterexample` use only the separate checked
tools manifest and retain process records below `.zeno-fcis/evidence`.

Project check, explanation, and generation results use schema
`zeno-fcis/cli/1`, deterministic field ordering, and no terminal color. Successful
JSON graphs use `zeno-fcis/graph/1`; backend manifests and inventories use their
own documented formats. Human output is deterministic for the same input.

`check`, `explain`, and `graph --format json` also report project-read failures
as JSON on stdout, with `status: "error"`, `error.code: "project-read-failed"`,
the input path, and a diagnostic message. These failures return exit code `3`;
the message can include operating-system-specific details. Human mode retains
stderr diagnostics.

CLI source acquisition reads at most the one-MiB source budget plus one byte.
An oversized file fails with exit `3` before UTF-8 interpretation or parsing;
an exact-limit file proceeds to normal parser validation. The in-memory parser
continues to return its existing source-limit diagnostic for oversized strings.

The development `durable-counter` template includes authored shapes, reviewed
runtime laws, generated typed transitions, and a SQLite lifecycle. It uses the
current checkout's schema-lowering and complete-invocation APIs. Run
`python3 tools/check_generated_application.py` from the repository to compile
and exercise it as an isolated consumer of this exact source. See the
[template README](../crates/zeno-fcis-cli/templates/durable-counter/README.md)
for its bounded semantics and local demonstration limits.

## Agent discovery and generation results

`describe` emits `zeno-fcis/cli-description/1` for the command tree, or a selected
path such as `describe backend verify`. The parser supplies argument names,
requiredness, positions, arity, choices, and defaults. Command effects identify
reads, writes, tool execution, and the `generate --check` read-only condition.
An argument position is one-based; `null` denotes an option. An arity maximum
of `null` means an unbounded value count. An empty `choices` array means no
closed list is exposed by the parser, so command-specific semantic validation
may still reject a value. Descriptive help text is not an executable instruction.
`subcommand_required` distinguishes groups that require a child command from
commands that can be invoked on their own.
Unknown command paths return versioned JSON and exit `64`. Discovery reads no
project or tool input and grants no authority. These interfaces are available
in the 1.0.0 CLI.

`generate --format json` returns `status: "generated"`; with `--check` it
returns `current` (exit `0`) or `drift` (exit `1`). Results include `path`,
`output`, `artifacts`, and `drift` arrays. Missing or changed files are drift.
Other artifact-read failures return `error.code: "artifact-read-failed"` and
exit `3`, while write failures use `artifact-write-failed`. Comparisons read at
most the expected artifact length plus one byte, and checks create no files.
Source diagnostics retain `status: "invalid"` and exit `1` in JSON mode.
Generation failures use `generation-failed` and exit `3`. The default human
output remains available. See the [agent recovery loop](LLM_USAGE.md).

## Tools manifest

V1 uses tools-manifest format `zeno-fcis/tools/2`. A tools/1 manifest is
rejected with exit code `2` and an error that names both formats:

```text
tools manifest blocked: WrongFormat { expected: "zeno-fcis/tools/2", actual: "zeno-fcis/tools/1" }
```

To move a tools/1 manifest to tools/2:

1. Change its top-level `format` field to `zeno-fcis/tools/2`.
2. Keep existing CVC5 and Z3 entries unchanged.
3. Add `runtime.root` and `runtime.tree_sha256` to every Lean entry.
4. Run `zeno-fcis backend inspect --tools zeno-fcis.tools.json` to check and
   print the canonical manifest.
5. Run `zeno-fcis doctor --tools zeno-fcis.tools.json` to recheck each
   executable, version, hash, and Lean runtime.

Compute the Lean tree hash with the same bounded inventory code used before
every Lean run:

```bash
zeno-fcis backend inventory-lean /absolute/path/to/lean-4.30.0
```

Human output has this form, with values computed from the selected tree:

```text
lean tree_sha256 <tree-sha256>
files <file-count>
total_bytes <byte-count>
```

Use `--format json` to print the canonical
`zeno-fcis/toolchain-inventory/1` record, including every admitted file.

## Exit classes

| Code | Meaning |
| ---: | --- |
| 0 | requested positive result completed |
| 1 | invalid specification or refuted claim |
| 2 | blocked, indeterminate, missing evidence, or unavailable tool |
| 3 | tool, filesystem, or bounded execution failure |
| 64 | invalid command-line usage |

An exit code does not create formal evidence or authority. In particular, a
successful graph or generation command says only that the requested derived
view was produced.

Formal commands apply the exit classes to each backend result:

| Backend result | `prove` | `counterexample` | Meaning |
| --- | ---: | ---: | --- |
| CVC5 returns UNSAT with proof-shaped output | 2 | 2 | The proposal and output are retained. V1 does not independently check the proof. |
| Z3 returns UNSAT | 2 | 2 | The result remains blocked because V1 has no Z3 proof checker. |
| CVC5 or Z3 returns SAT and the built-in evaluator replays the model | 1 | 0 | A normalized counterexample is retained. |
| Qualified Lean returns `KernelChecked` with the configured exact axiom report | 0 | unavailable | The generated theorem passed the Lean kernel check under the recorded RC3 Linux x86-64 toolchain identity. |
| A custom Lean tree reports kernel success | 2 | unavailable | The run is retained as unqualified evidence. |
| Tool is missing, reports `unknown`, or supplies unsupported evidence | 2 | 2 | The requested result remains blocked. |
| Tool crashes, times out, exceeds a bound, or encounters a filesystem failure | 3 | 3 | The bounded execution failed. |

Exit code `0` from a Lean `prove` command covers the generated theorem, the
qualified Lean `4.30.0` Linux x86-64 distribution, and the configured exact
axiom report. Translation review and any promotion into existing ZenoFCIS
evidence types remain separate steps.

## Finite synthesis

`zeno-fcis synth discover` describes the shared finite profile, current Rust,
Python and JavaScript adapters, and their callable interfaces.
`synth run PROBLEM --target LANGUAGE --out DIR` performs
complete bounded semantic checking and emission; add `--check` for read-only
regeneration comparison. `synth verify PROBLEM --target LANGUAGE --out DIR`
rechecks the artifacts and executes the complete finite corpus. Optional
`--tool PATH` selects the compiler/interpreter and `--receipt PATH` creates a
separate new receipt file outside DIR. Both commands accept
`--max-assignments` and `--max-steps`. See
[language-neutral synthesis](LANGUAGE_NEUTRAL_SYNTHESIS.md) for the JSON
contract, distinct failure outcomes, and target-conformance boundary.
