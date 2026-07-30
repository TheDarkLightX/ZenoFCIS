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
zeno-fcis backend list|inspect|verify [--tools FILE]
```

`new` refuses a nonempty target. `check` parses and elaborates in one command.
`generate` atomically writes deterministic Rust and `PROJECT_MANIFEST.zfcis`;
`--check` writes nothing and reports drift. `graph` and `explain` are derived
diagnostic views. `prove` and `counterexample` use only the separate checked
tools manifest and retain process records below `.zeno-fcis/evidence`.

Machine output uses schema `zeno-fcis/cli/1`, deterministic field ordering,
and no terminal color. Human output is deterministic for the same input.

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
