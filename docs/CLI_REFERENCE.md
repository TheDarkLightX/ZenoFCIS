# `zeno-fcis` CLI reference

The `zeno-fcis-cli` package in `1.0.0-rc.3` publishes the `zeno-fcis` binary.
It pins `clap = 4.6.1` without environment parsing or color output.

```text
zeno-fcis new <dir> --template minimal|mini-determinator
zeno-fcis check [project.zeno] [--format human|json]
zeno-fcis generate [project.zeno] --out <dir> [--check]
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
`generate` atomically writes deterministic Rust and `PROJECT_MANIFEST.zfcis`;
`--check` writes nothing and reports drift. `graph` and `explain` are derived
diagnostic views. `prove` and `counterexample` use only the separate checked
tools manifest and retain process records below `.zeno-fcis/evidence`.

Machine output uses schema `zeno-fcis/cli/1`, deterministic field ordering,
and no terminal color. Human output is deterministic for the same input.

## Tools manifest

RC3 uses tools-manifest format `zeno-fcis/tools/2`. A tools/1 manifest is
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
| CVC5 returns UNSAT with proof-shaped output | 2 | 2 | The proposal and output are retained. RC3 does not independently check the proof. |
| Z3 returns UNSAT | 2 | 2 | The result remains blocked because RC3 has no Z3 proof checker. |
| CVC5 or Z3 returns SAT and the built-in evaluator replays the model | 1 | 0 | A normalized counterexample is retained. |
| Qualified Lean returns `KernelChecked` with the configured exact axiom report | 0 | unavailable | The generated theorem passed the Lean kernel check under the recorded RC3 Linux x86-64 toolchain identity. |
| A custom Lean tree reports kernel success | 2 | unavailable | The run is retained as unqualified evidence. |
| Tool is missing, reports `unknown`, or supplies unsupported evidence | 2 | 2 | The requested result remains blocked. |
| Tool crashes, times out, exceeds a bound, or encounters a filesystem failure | 3 | 3 | The bounded execution failed. |

Exit code `0` from a Lean `prove` command covers the generated theorem, the
qualified Lean `4.30.0` Linux x86-64 distribution, and the configured exact
axiom report. Translation review and any promotion into existing ZenoFCIS
evidence types remain separate steps.
