# `zeno-fcis` CLI reference

This guide documents the `2.4.0` source. Its retained V1
protocol guidance is historical; see [the release notes](RELEASE_NOTES.md)
for the current checked API and assurance boundaries.

The `zeno-fcis-cli` package in `1.1.0` publishes the `zeno-fcis` binary.
It pins `clap = 4.6.1` without environment parsing or color output.

```text
zeno-fcis describe [COMMAND...]
zeno-fcis new <dir> --template minimal|mini-determinator|durable-counter|prepared-counter|account-lockout|order-fulfillment|inventory-reservation|compliance-gateway|withdrawal-queue|agent-treasury-guard [--source TREE]
zeno-fcis new <dir> --contract <contract-dir> [--source TREE]
zeno-fcis check [project.zeno] [--format human|json] [--require-substantive] [--require-resolved-paths]
zeno-fcis generate [project.zeno] --out <dir> [--check] [--format human|json]
zeno-fcis generate contract [<app-dir>] [--out <dir>] [--check] [--format human|json]
zeno-fcis graph [project.zeno] --format dot|mermaid|json
zeno-fcis explain [project.zeno] [--code CODE] [--format human|json]
zeno-fcis prove [project.zeno] --claim ID|all --backend cvc5|z3|lean|all [--tools FILE]
zeno-fcis counterexample [project.zeno] --claim ID --backend cvc5|z3 [--tools FILE]
zeno-fcis doctor [--tools FILE]
zeno-fcis purity <PATH>... [--format human|json]
zeno-fcis backend list
zeno-fcis backend inspect|verify [--tools FILE]
zeno-fcis backend inventory-lean ROOT [--format human|json]
zeno-fcis transform check --original FILE --candidate FILE [--step-limit N] [--max-input-tuples N] [--receipt OUT]
zeno-fcis transform check --original FILE --candidate FILE --symbolic --tools FILE [--step-limit N] [--max-input-tuples N] [--symbolic-receipt OUT] [--queries DIR]
zeno-fcis transform replay --receipt FILE --original FILE --candidate FILE [--max-input-tuples N]
zeno-fcis transform check-certificates --original FILE --candidate FILE --tools FILE --checker FILE --certificates DIR [--step-limit N] [--max-input-tuples N] [--out RECEIPT] [--queries DIR]
zeno-fcis transform replay-certificates --receipt FILE --original FILE --candidate FILE --tools FILE --checker FILE --certificates DIR [--max-input-tuples N]
zeno-fcis optimize --program FILE [--strategy FILE] [--profile functional-bool-v1|checked-i64-v1] [--with-candidate FILE]... [--candidate-out OUT] [--receipt OUT] [--max-input-tuples N]
zeno-fcis contract draft start --session DIR --intent FILE --project FILE --provenance TEXT [--examples FILE] [--rounds 4] [--max-tuples 4096]
zeno-fcis contract draft propose --session DIR --revision SHA256 --rules FILE --provenance TEXT
zeno-fcis contract draft questions|check --session DIR
zeno-fcis contract draft label --session DIR --revision SHA256 --examples FILE --provenance TEXT
zeno-fcis contract draft finalize --session DIR --revision SHA256 --out NEW_DIR
zeno-fcis contract review [<app-dir>] [--out PACKET.json] [--max-tuples N] [--mutant-programs DIR] [--format human|json]
zeno-fcis contract export-program [<contract-dir>] --out FILE [--format human|json]
zeno-fcis contract export --tau [<contract-dir>] --out DIR [--max-tuples N] [--samples N] [--format human|json]
zeno-fcis contract import --tau SPEC --out DIR [--scale-width ORIGINAL:NEW] [--domain INPUT:MIN:MAX] [--format human|json]
zeno-fcis contract adopt [<app-dir>] --candidate FILE --receipt FILE --usage preserved|new-version [--format human|json]
zeno-fcis contract refresh-receipts [<app-dir>] [--format human|json]
zeno-fcis contract diff <old-dir> <new-dir> [--format human|json]
zeno-fcis contract evolve [<app-dir>] --to <contract-dir> [--migration FILE [--shortcut FILE]...] [--format human|json]
zeno-fcis contract check-symbolic [<contract-dir>] [--strengthening FILE] [--tools FILE] [--max-tuples N] [--out REPORT.json] [--queries DIR] [--format human|json]
zeno-fcis contract check-composition MANIFEST [--aggregate DIR | --write-aggregate DIR] [--receipt FILE | --replay FILE] [--format human|json]
```

`new` refuses a nonempty target. `check` parses and elaborates in one command.
A field name must be unique within its record; other declaration names are
unique in the project. The summary line's `unresolved_obligations=N` counts
evidence that the check does not establish and generated code cannot
discharge; it is not an error. Human output explains each one on the lines
after the summary, and JSON output lists them under `obligations` as
`{kind, component, peer, claim, explanation}`. The kinds are
`complete-footprint`, one per component, which needs evidence that the
component reads and writes only what its footprint declares;
`sequential-parity`, one per project, which needs evidence that the merged
components decide as they would one after another; `parallel-law`, one per
pair of components that share a path one of them writes, or an effect or
outbox; and `assumption`, one per claim a component assumes. `check` also
classifies every law and claim by
[substance](CLAIM_SUBSTANCE.md). Human output warns on stderr about formulas
that cannot constrain any transition. JSON output adds a `substance` object,
and `--require-substantive` exits 1 with `status: "vacuous"` when any are
found. The classification reads each law's scope: a law on only some
decisions, such as an `on accept` authorization law, counts even when it
reads no outcome. It also reports law and claim paths that name no declared type or
field ([law path resolution](LAW_PATH_RESOLUTION.md)): human output warns on
stderr, JSON output adds an `unresolved_paths` object, and
`--require-resolved-paths` exits 1 with `status: "unresolved-paths"` when any
are found. `prove` prints a `scope:` line saying what a result can establish.
Relational and temporal claims are exported without a system model. An
[inductive claim](INDUCTIVE_CLAIMS.md) is exported as its induction step over
the laws it assumes. `prove` names those laws, and says what the application
must also check: that its observer reads every value the invariant reads, the
base case, and the enforcement of the assumed laws. A
replayed counterexample prints `replayed counterexample retained`. When the
claim has no value at the counterexample, because of an overflow, a division
by zero, or an inexact exact division, the line ends with the reason, and the
run record's status is `undefined`.
`generate` replaces each deterministic Rust/manifest file atomically;
`--check` writes nothing and reports drift. `generate contract` reads an
application's `project.zeno` and `v2/policy.json`, whose format the
[contract rules reference](CONTRACT_RULES.md) describes, and replaces
`v2/schema.zcve`, `src/v2_contract.rs` and `v2/policy.zcve`; the policy bytes
come from the library's encoder, and the library's catalog binding must accept
them first. When the rules list adoptions (see [Contract adoption and store
upgrades](#contract-adoption-and-store-upgrades)) it also reads each
`v2/adoptions/N/program.zcve` and `receipt.json`, replays every receipt in
order, checks each superseded version's policy against the digest its
adoption bound, and replaces each superseded version's `src/v2_contract_vN.rs`
and `v2/policy_vN.zcve`. With `--out <dir>` it writes the generated files
under `<dir>` instead, creating it when absent, and leaves the contract
directory holding only the files its author writes; with `--check`, it
compares the files there. With `--check` it writes nothing, names each
drifted file and exits 1. An invalid contract exits 1 and writes nothing. The
generator's own checks name the file and entry at fault. Among them: an
accept has no reason, and a reject or committed failure has one; constants lie
within their declared ranges; the variants of a sum the decision program reads
have consecutive IDs; a law that applies at genesis reads only `post` fields;
and no declared law shares an ID with a law generation adds (990, 991, and the
reject and committed-failure laws); a case's deliveries have increasing
ordinals; and a contract with a committed-failure case has a
`CommittedFailureEffects` law, without which framework law 908 would refuse
every committed failure. Each generated channel's idempotency domain covers
every `idempotency_ordinal` the rules use on it, and the Effect limit is the
most deliveries any case makes, so a case may deliver more than once. A
contract that passes these checks but that the library's catalog binding
refuses is reported as that refusal. The library reports only which stage
refused, so the command binds the contract again without each case delivery,
with each law's formula replaced by `true`, and without each channel, and
names the first entry without which the library admits the contract. When no
single entry accounts for the refusal, the error names none.
`new --contract` builds an application from a directory holding
`project.zeno`, `v2/policy.json`, any adoptions they list and, optionally,
`tests/decision-examples.txt`.
`new` binds every Cargo application it writes, from a contract or from the
`durable-counter`, `prepared-counter` and example templates, to a ZenoFCIS
source tree, so that it builds with no other step. The application pins each
ZenoFCIS package to the CLI's version, and without the binding Cargo would
resolve those pins to the crates published under the same versions, which
are a different release. The tree is the one `--source` names, else the tree
the CLI was built from, while that still exists; the CLI records its path
when it is compiled. The tree must be a Cargo workspace whose listed
`members` provide every needed ZenoFCIS package at the CLI's version, with a
`Cargo.lock`. `new` then:
- appends to the application's `Cargo.toml` a `[patch.crates-io]` section
  with a path entry for every ZenoFCIS package the application needs,
  directly or through the normal and build dependencies of other ZenoFCIS
  packages, in sorted order;
- uses the tree's `Cargo.lock` to retain its external package identities.
  Contract apps reconcile their feature graph with pinned offline Cargo in
  private scratch and include the application itself in the emitted lock,
  so the first build accepts `--locked --offline`. Development templates
  retain their copied source lock;
- copies the tree's `rust-toolchain.toml` when it has one.

Without a tree, `new` exits 2 and names `--source`; with a tree that cannot
bind the application, it exits 1. Either way it creates no target directory;
an existing empty target directory is preserved. `new --contract` requires
Cargo with Rust 1.97.1 and cached dependencies for offline reconciliation;
`cargo fetch` in the source tree fills that cache. A build of
the CLI with the environment variable `ZENO_FCIS_BUILD_TREE` set records that
path as its tree instead, or, when it is empty, none, so that the binary
holds no build directory; such a binary always needs `--source`. The release
binaries are built that way (`tools/rc_package.py build`). With an installed
release binary, extract the release's `source/zeno-fcis-<version>-source.tar.gz`
and pass `--source <extracted source tree>`:

```sh
tar -xzf zeno-fcis-2.4.0-source.tar.gz
zeno-fcis new my-app --contract my-contract --source zeno-fcis-2.4.0
```
 The generated README lists the build commands, which
`tools/check_app_journey.py` runs exactly as written, in a new directory, on
the app study's escrow contract; `tools/check_generated_application.py`
checks the binding of every application it builds.
`graph` and `explain` are derived
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
for its bounded semantics and local demonstration limits. In every application
template the SQLite shell is the `sqlite` feature, on by default; the core
builds without it, including for `wasm32-unknown-unknown`, which the same gate
checks.

Four more development templates build realistic applications the same way:
- [`account-lockout`](../crates/zeno-fcis-cli/templates/account-lockout/README.md):
  failed logins committed as failures, time as a context input,
  administrator unlocks, and an inductive claim over the integer ranges
  `project.zeno` declares;
- [`order-fulfillment`](../crates/zeno-fcis-cli/templates/order-fulfillment/README.md):
  a state machine with idempotent payment and shipping requests;
- [`inventory-reservation`](../crates/zeno-fcis-cli/templates/inventory-reservation/README.md):
  a synthesized core over quantities, with a conservation law;
- [`compliance-gateway`](../crates/zeno-fcis-cli/templates/compliance-gateway/README.md):
  an expert system's rule base turned into a synthesized core, where every
  decision names the rule that fired.
- [`withdrawal-queue`](../crates/zeno-fcis-cli/templates/withdrawal-queue/README.md):
  a controller step synthesized from a sketch of a fair policy, whose table
  OrbitSynthesis checks for every input sequence, so alarms delay
  withdrawals but never freeze them; a refinement law ties each tick to the
  finite model.
- [`agent-treasury-guard`](../crates/zeno-fcis-cli/templates/agent-treasury-guard/README.md):
  an AI agent as an untrusted proposer of swaps, guarded by a budget, a
  reserve, and a slippage bound; a synthesized core decides the rule
  precedence, inductive claims attested by CVC5 through `prove` state that
  the action laws keep the reserve and the daily budget, and requests to
  the destination use a typed `SwapIntent` message.

In each, `check --require-substantive --require-resolved-paths` passes, and
the same gate compiles and exercises it as an isolated consumer.

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
in the 1.1.0 CLI.

`generate --format json` returns `status: "generated"`; with `--check` it
returns `current` (exit `0`) or `drift` (exit `1`). Results include `path`,
`output`, `artifacts`, and `drift` arrays. Missing or changed files are drift.
Other artifact-read failures return `error.code: "artifact-read-failed"` and
exit `3`, while write failures use `artifact-write-failed`. Comparisons read at
most the expected artifact length plus one byte, and checks create no files.
Source diagnostics retain `status: "invalid"` and exit `1` in JSON mode.
Generation failures use `generation-failed` and exit `3`. The default human
output remains available. See the [agent recovery loop](LLM_USAGE.md).

The next patch rejects Unix named-pipe artifacts without waiting for a writer.
Failed individual file writes remove their incomplete file; a failed replacement
preserves the previous destination. Generation is atomic per file. A failure
after other files finish can leave a partial directory.

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

## Purity

`purity` parses Rust source and reports every source of nondeterminism or
ambient effect it recognizes: clocks, the environment, files, input and output,
the network, processes, threads, randomness, hash-map iteration, shared state,
raw addresses, unsafe code, and foreign code. Floating point and `DefaultHasher`
are warnings. Each PATH is a Rust file, a directory of them, or a crate
directory containing `Cargo.toml`. A crate is also checked for confinement:
- a library-only package whose root is `src/lib.rs`, with no binary target;
- unconditional `no_std` and `forbid(unsafe_code)`;
- no `extern crate std`, and no source brought in by `include!` or a
  `#[path]` attribute;
- a manifest that the check reads completely, with every dependency named as
  one of the library's semantic crates.

The status is `clean` or `confined` (exit 0), `violations` (exit 1), or
`unreadable` (exit 3). Only error-level rules decide it, so a clean or
confined result can include warnings. A path the check cannot read or list,
or one with no Rust source, makes the status `unreadable`. So does a symbolic
link to a directory or to a Rust file inside a directory it walks. JSON output
uses schema `zeno-fcis/purity-report/1`. A clean result is checked against the
rule table; it is not a proof of determinism. See
[determinism](DETERMINISM.md).

## Exit classes

`describe` reports the compiled package's `cli_version` and a
`source_identity` fingerprint of the compiled loop-program source set, the
same source binding used in strategy reports. The fingerprint identifies
that source set, rather than the whole release tree or executable; retain the
release manifest and binary digest for complete build provenance. A release
filename does not change the compiled package version.

| Code | Meaning |
| ---: | --- |
| 0 | requested positive result completed |
| 1 | invalid specification or refuted claim |
| 2 | blocked, indeterminate, missing evidence, or unavailable tool |
| 3 | tool, filesystem, or bounded execution failure |
| 64 | invalid command-line usage |
| 65 | a contract judge's input has no contract form: a formula, scenario file or proposed law, or for `profile`, `lemma`, `scenario` and `probe` the contract itself |

An exit code does not create formal evidence or authority. In particular, a
successful graph or generation command says only that the requested derived
view was produced.

Formal commands apply the exit classes to each backend result:

| Backend result | `prove` | `counterexample` | Meaning |
| --- | ---: | ---: | --- |
| CVC5 returns UNSAT with proof-shaped output | 2 | 2 | The proposal and output are retained. The current path does not independently check the proof. |
| Z3 returns UNSAT | 2 | 2 | The result remains blocked because the current path has no Z3 proof checker. |
| CVC5 or Z3 returns SAT and the built-in evaluator replays the model | 1 | 0 | A normalized counterexample is retained. |
| `--backend lean` | 2 | 2 | The current public exporter refuses this unsupported mode. The historical exporter is test-only. |
| Tool is missing, reports `unknown`, or supplies unsupported evidence | 2 | 2 | The requested result remains blocked. |
| Tool crashes, times out, exceeds a bound, or encounters a filesystem failure | 3 | 3 | The bounded execution failed. |

The retained `KernelChecked` evidence type does not provide a public Lean
exporter. For independently checked scalar program equivalence, use
`transform check-certificates` and `transform replay-certificates`; see the
[certificate route](SYMBOLIC_CHECKS.md#independently-checked-scalar-certificates)
for its restricted fragment, checker setup, receipt format and trusted base.
That receipt grants no application or adoption authority. `prove`,
`contract check-symbolic` and `transform check --symbolic` retain their existing
solver evidence classes.

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


## Contract program export

`contract export-program [DIR] --out FILE` writes the current decision
program of the contract in `DIR` (default `.`) to the new file `FILE`, in the
canonical `program.zcve` encoding that `transform check`, `transform
replay`, `optimize` and `loop` read. `DIR` holds `project.zeno`,
`v2/policy.json` and any adoptions. The command generates the contract as
`generate contract` does, replaying every adoption's receipt, and writes the
program of its last version: the bytes the next adoption's receipt names as
its original. After an adoption these are the adopted candidate's bytes. The
bytes are a function of the contract's files, so exporting twice gives the
same file. `v2/policy.zcve` is the whole policy, not a program, and the
program commands refuse it. `FILE` must not exist, as a file or a link; the
command never overwrites one. Results use schema `zeno-fcis/cli/1`:
`exported`, with the application, its version and the program's path,
bytes, SHA-256, nodes and outputs; or `error`, with `contract-invalid` or
`output-exists` (exit 1), or `contract-read-failed` or
`program-write-failed` (exit 3).

An optimization journey therefore needs no other tool:

```sh
zeno-fcis contract export-program . --out program.zcve
zeno-fcis optimize --program program.zcve --candidate-out candidate.zcve --receipt receipt.json
zeno-fcis transform replay --receipt receipt.json --original program.zcve --candidate candidate.zcve
zeno-fcis contract adopt . --candidate candidate.zcve --receipt receipt.json --usage new-version
```

Arithmetic development graphs within 256 nodes, 16 outputs and 32 inputs use
`zeno-fcis/finite-i64/3`. `contract export-program`, finite `transform check`
and `transform replay` carry that profile and its exact arithmetic. Older
readers refuse it. Legacy `optimize` and loop/search proposal profiles refuse
the new opcodes; symbolic and independent-certificate program encoders also
report unsupported. Native and proof requalification on this changed source
is pending; parser acceptance is not that qualification.

`transform`, `optimize` and `contract review` enumerate input domains, and a
release build of the CLI (`cargo build --release -p zeno-fcis-cli`) does
that several times faster than a debug build. In the app study of
2026-10-05, three reviews took 29 s, 64 s and 168 s with a debug build and
4.3 s, 8.5 s and 36 s with a release build, 4.7 to 7.5 times faster, and
their packets were byte-identical. Prefer a release build for these
commands; their results do not depend on the build.

A decision program of at most 256 nodes, 16 outputs and 32 inputs uses the
profile `zeno-fcis/finite-i64/1`. A larger one, up to 4,096 nodes, 64 outputs
and 64 inputs, uses the profile `zeno-fcis/finite-i64/2` (or `/4` when
it contains the arithmetic extension); `generate contract`
and `new --contract` refuse a contract past any of these limits and name the
counts. `export-program` writes either profile, and an application built
from either decides through the library Authority. The commands that read a
`program.zcve` file, `transform`, `optimize` and `loop`, read it with the
library importer, which admits profile 1 only. `transform check` and
`optimize` refuse a file over 64 KiB as `program-too-large` before decoding
it, and a smaller profile 2 program with status `refused`, detail `{"code":
"program-encoding", "reason": "original-not-admitted"}` and exit 1. No
transform receipt can name a profile 2 program, so `contract adopt` cannot
replace one.

## Checked program transform

`transform check` compares a supplied candidate with an original finite scalar
program. Both files use the canonical `program.zcve` encoding, at most 64 KiB,
and must pass the library importer. Their input and output domains must match
exactly, in order. The check runs both programs on each tuple of the declared
input domain, last input fastest, until the first difference. It uses the
library's metered evaluator at a budget of 256 Steps, which no admitted program
can exhaust, so unused nodes and unselected `Select` arms still trap. Each run
gives the program's result and its true Step usage. The declared
`--step-limit` (default 256) is then compared with that usage; it never
changes a result. The default domain limit is 100,000,000 tuples.

Results use schema `zeno-fcis/transform-result/1`. Checker results have a `detail`
object; supervisor failures instead carry a top-level `reason`. Once both programs are read, `check` also reports the `limits` it used
and both programs' SHA-256 digests. Input and output values and integer domain
bounds are decimal strings; Boolean positions are `"0"` or `"1"`.

| Command | Status | Exit | Meaning |
| --- | --- | ---: | --- |
| `check` | `equivalent` | 0 | Every tuple gave identical outputs or identical failures, and no tuple needs more Steps than the limit in either program. |
| `check` | `counterexample` | 1 | The first tuple whose outputs or failures differ, with both observations and their Step usage. |
| `check` | `refused` | 1 | A program failed admission or exceeds 64 KiB, the ABIs differ, or an input domain is empty. |
| `check` | `receipt-exists` | 1 | The `--receipt` path already exists, as a file or a link; nothing was checked. |
| `check` | `inconclusive` | 2 | The domain exceeds the tuple limit (`domain-too-large`); the programs agree but some tuple needs more Steps than the limit (`budget-boundary`, with counts and the smallest limit that never binds); or the defensive enumeration count failed (`coverage-mismatch`). |
| `replay` | `replayed` | 0 | The recomputed receipt is byte-for-byte identical. |
| `replay` | `replay-mismatch` | 1 | The program digests, the domain size or the recomputed receipt differ, or the check no longer gives `equivalent`. |
| `replay` | `invalid-receipt` | 1 | The file is not a readable transform receipt or exceeds 64 KiB. |
| `replay` | `inconclusive` | 2 | The domain exceeds replay's own `--max-input-tuples`; nothing was evaluated. |
| both | `io-error` | 3 | A file could not be read, is not a regular file, or the receipt could not be created. |

`--receipt OUT` creates a new file for an `equivalent` result. The receipt,
schema `zeno-fcis/transform-receipt/1`, is compact JSON with sorted keys and a
final newline. It records both programs' SHA-256 digests, sizes and node
counts, the domains, both limits, the tuples checked, the Step usage report
and the checker identity: the checker's semantics version,
`zeno-fcis/transform-check/3`, and the library's evaluator digest. The
semantics version changes only with a deliberate change of what the check
means. Known answers under `crates/zeno-fcis-cli/tests/fixtures/transform-check/`
pin it, so a refactor of the checker or a new crate version changes no
receipt.
`transform replay` first checks the receipt's program digests and domain size
against the supplied files, and the domain against its own tuple cap. Only then
does it rerun the check with the receipt's limits; it accepts the receipt only
if the recomputed bytes are identical.

The usage report gives each program's largest Step usage, the number of tuples
on which the candidate uses more Steps, and `usage_preserved`, which is true
only when both programs use the same Steps on every tuple. Usage does not
decide equivalence. V2 seals usage counters into every publication, so
adopting a candidate whose `usage_preserved` is false changes the
application's sealed observations and would be a new contract version. This
command does not adopt candidates, and a receipt grants no application or
publication authority.

### Offline parallel enumeration

This development-source feature is awaiting combined stable qualification.

`transform check --offline-parallel` partitions the same ordered domain into
ranges and merges their verified reports in ordinal order. Successful verdicts
and receipt bytes match serial checking. Operational failure can instead make
a run inconclusive, including failure in a later range of a dispatched window
that already contains a witness.

| Flag | Default | Limit |
| --- | ---: | ---: |
| `--workers` | 4 | 1–8 |
| `--chunk-tuples` | 4096 | 1–4096 |
| `--max-chunks` | 65536 | 1–65536 |
| `--work-units` | 1000000000000 | Complete reservation must fit |
| `--deadline-ms` | 60000 | 1–60000 |

These flags require `--offline-parallel`. This mode uses the Linux contained
runtime described in [Installation](INSTALLATION.md#contained-checker-runtime).
On macOS it prints one JSON result with `"status":"inconclusive"`,
`"authority":"none"`, `"reason":"containment-unavailable:platform"` and
`"accounting_complete":false`, exits 2, and checks and writes nothing; the
serial check works there.
The final `progress` object reports `reserved_work`, `setup_reserved`,
`evaluated_pairs`, `equal_prefix`, `measured_steps` (two decimal strings),
`planned_chunks`, `completed_chunks`, `active_workers`, `scheduling` (observed
CPU affinity and nice priority), and `accounting_complete`. It is final accounting, not a live progress stream.
A killed process reports incomplete accounting and cannot supply range progress.

Operational reasons include `work-exhausted`, `work-accounting-exceeded`,
`chunk-cap`, `deadline`, `worker-unavailable`, `worker-failed`,
`range-coverage`, `operational-limits`, and `operational-dimensions`.
Containment failure, abnormal exit, or incomplete teardown also returns
inconclusive, exit 2. Exhaustion never gives a verdict or a new transform
receipt. A child hands its staged receipt to the supervisor, which publishes
only after accepting the exit and teardown. Receipt publication (also in serial
mode) uses an exclusive hard link and requires filesystem hard-link support.
The parent directory is opened and synced before exposing that link. If the
post-link directory sync fails, publication removes the link and syncs the
rollback. A failed rollback is reported explicitly as `receipt-may-exist` or
`receipt-absent-durability-uncertain`; do not treat that failure as a receipt
publication acknowledgment. These guarantees require a working filesystem
and a trusted, stable parent namespace; failing storage or concurrent namespace
replacement can prevent rollback or durable confirmation.

## Symbolic per-case checks

`contract check-symbolic` and `transform check --symbolic` ask the pinned
CVC5 and Z3 one query per rule case, for domains too large to enumerate. The
[symbolic checks guide](SYMBOLIC_CHECKS.md) describes both commands, the
strengthening file format, the report schema `zeno-fcis/symbolic-check/1`,
the receipt schema `zeno-fcis/transform-symbolic-receipt/1`, the encoding's
supported constructs and its limits.

- A counterexample is replayed through the library evaluators before it is
  reported (Checked, exit 1).
- `holds` from enumeration of the whole domain is Proved (exit 0). `holds`
  from the solvers is attested by CVC5, corroborated by Z3, not proved, and
  exits 2, as a CVC5 `unsat` does for `prove`.
- Anything undecided, including a solver disagreement without a replayed
  model and a planted control that was not refuted, is `inconclusive`
  (exit 2).
- `transform check --symbolic` refuses a domain that fits
  `--max-input-tuples`; the exhaustive check decides it. Its receipt is not a
  `zeno-fcis/transform-receipt/1`: `transform replay`, `contract adopt`,
  `contract refresh-receipts` and contract generation refuse it.

Without `--symbolic`, `transform check` is unchanged.

## Checked composition

`contract check-composition MANIFEST` checks a composition of separately
bound application contracts. The manifest, schema
`zeno-fcis/checked-composition/1` or, with groups,
`zeno-fcis/checked-composition/2`, names each leaf contract's directory
(its `project.zeno` and `v2/policy.json`, relative to the manifest) with the
SHA-256 of both files and of the identity of the program they bind to, the
leaf's invariant, its assumption over its interface observation and command,
its guarantees, each tagged `StateFact` or `TransitionRelation`, the global
state slots it owns, its complete footprint and the state fields it exports
as views; and the interface edges, each feeding one context field of a
consumer leaf with a provider leaf's view read from the same pre-state.
Formulas use the contract rules' expression language and are compiled into
the library's law programs.

The profile is sequential: one leaf decides each command, peers are seen
only through views of the same pre-state, leaves own disjoint state, and
there are no effects or outbox deliveries. For each leaf, the command
compiles the contract as `generate contract` does, binds it to the library
Authority, and runs the Authority on every input of the leaf's complete
declared domain, state by command by context; a domain of more than 2^20
inputs is refused, not sampled. Every decision is replayed. It requires:

- the declared genesis satisfies the invariant, and the Authority's genesis
  and its replay succeed;
- on every input whose state satisfies the invariant and whose command and
  context satisfy the assumption: an Accept whose successor satisfies the
  invariant, or a business Reject that changes nothing; no technical
  refusal, committed failure, effect or delivery; and every transition
  relation within its scope;
- the invariant implies every StateFact on every state;
- an Accept, and a Reject when listed, on some such input;
- every write and read the Authority attempts within the declared footprint.

Across leaves, the manifest denotes a `zeno-fcis-compose` composition
specification: one component per leaf, which may write the slots it owns and
reads its peers' slots through their views, with one guarantee for each
declared guarantee and one assumption for each leaf with interface fields.
The compose crate judges that specification: two leaves that own one slot
are a write-write conflict, and every edge must name a listed provider and
one of its declared guarantees. The command also requires the owned slots to
run from 0 without a gap; exactly one edge for each context field, from a
different leaf, through a view whose domain equals the field's, with facts
that are the provider's StateFacts and read only the viewed field; and, for
every value of the consumer's interface fields and every command, that the
providers' facts imply the consumer's assumption. A cycle of edges is
accepted, because each fact follows from its own leaf's invariant, which
that leaf's genesis establishes and its admitted inputs preserve.

A `/2` manifest also lists groups and names the root group. Each group lists
its children (leaves and groups, each a child of exactly one group), its
environment inputs, its exports, an outer assumption and an outer guarantee.
A group's environment inputs are exactly the context fields of its leaves
that no edge from inside the group feeds; it names each one
`context.102.<id>`. An export is a view of one of its leaves, named
`post.100.<id>`. The outer assumption is a list of named formulas over the
environment inputs and the leaves' command fields; it cannot read the route,
which leaf a command selects. A command field is one variable shared by ID,
so every leaf that declares an ID must give it the same type. The outer
guarantee is a list of formulas over the exports. In a `/2` manifest a
context field may be
fed by no edge: it is then an environment input of every group above its
leaf, up to the root.

For each group, children first, the command checks, over every value of the
fields each formula reads, that wherever the facts of the edges feeding a
child from inside the group and the group's outer assumption hold, each
conjunct of the child's assumption holds: a leaf's assumption split at its
top-level `&&`, or a child group's outer assumption. A conjunct that does not
hold there and reads only environment inputs and command fields becomes part
of the group's outer assumption and is checked again one level up; a
conjunct that also reads a field fed inside the group is refused. The
command also checks that the children's guarantees on the group's exports (a
leaf's StateFacts, a child group's outer guarantee) imply each conjunct of
the outer guarantee. The root's outer assumption, with every conjunct that
rose to it, is the claim's condition: with none the claim is
`unconditional`; when every condition reads only environment inputs, it is
`unconditional under named external assumptions`; when one reads a command
field it is `conditional`. The root's assumption must hold on some input.
This is not the strict discharge rule of equation 2.8: an edge's facts are
the provider leaf's own StateFacts at every level, which hold because every
leaf's invariant holds jointly, and the outer guarantees are checked and
reported but discharge nothing. Groups are components of the compose
specification: a group writes nothing, guarantees its outer guarantee and
the delivery of its outer assumption to its children, and, below the root,
assumes its outer assumption. A `/1` manifest is the flat case and is checked
as before.

`--write-aggregate DIR` writes the composition's aggregate contract,
`project.zeno` and `v2/policy.json`, into `DIR` and validates it;
`--aggregate DIR` validates an aggregate contract already written. The
aggregate is an ordinary contract for `generate contract` and `new
--contract`: slot `s` is state field `1000 + s` with the owning leaf field's
type, the leaves' command fields are shared by ID, a target command field
selects one leaf, an environment input is a context field, and each leaf's
cases are guarded by its target value. The validation is structural and does
not enumerate the joint state. It compiles the aggregate through the
ordinary contract route and requires: its state fields to be the slots with
their leaves' domains and its genesis to be every leaf's genesis; the target
to range over the leaves and every other command or context field to be a
leaf's, with the same domain; no channel or delivery; that no program node
can leave the 64-bit range on any input within the declared domains; a Step
budget that covers every program and law node, and the Authority's observed
usage within it; on each route, the aggregate's program with its target
fixed to that route, reduced to canonical terms, choosing the selected
leaf's cases in the leaf's order with the same classes and reasons, giving
the leaf's successor values to the leaf's slots, reading each peer view from
the provider's slot of the same pre-state and leaving every other slot
unchanged; and every aggregate law to be a StateInvariant whose conjuncts
each read one leaf's slots and hold on every state of that leaf satisfying
its invariant. The correspondence is claimed on pre-states satisfying every
leaf's invariant on its slots. An aggregate that fails names the obligation.

An aggregate has at most 64 inputs, 64 outputs and 4,096 program nodes; a
register ring of one state field per leaf fits at most 61 leaves.

The repository's Verus harness `verification/verus/checked_assembly.rs`
proves, with the tables, the validated step equation and genesis as
premises, that every history of the aggregate keeps every leaf's invariant at
every state, and that fixing the target and reducing to canonical terms
preserves the evaluator specification's values. A conditional claim is that
theorem with the root's outer assumption among each route's admitted
commands. The trusted base, tested and not proved, is listed in the result
as `trusted_base`:

- The checker's exhaustive enumeration of each leaf's local, StateFact and
  interface tables with the leaf's bound Authority.
- The group checks of equation 2.8: their tables and the discharge and rise
  of each child's assumption.
- The assembly validator's canonical terms, route specialization and case
  pairing, compared with the library evaluator on the register ring and chain
  and constructor by constructor in unit tests.
- The assembly validator's comparison of classes, reasons, assignments and
  frames, its field, genesis and routing checks, and its law-implication
  tables.
- The interval analysis showing that no eager node can trap, and the meter
  check with its cost model of one Step per program or law node.
- The link from the bound Authorities' actual decisions (case selection,
  assignments, law and meter refusals) to the abstract decision functions of
  the Verus theorem.

The application-level safety statement is therefore machine-checked over the
evaluator specification, with the validator implementation tested, not
proved.

The result, schema `zeno-fcis/checked-composition-result/2`, records the
manifest's schema, SHA-256 and commitment, the checker's source-bound
identity, the commitment of the composition specification, each leaf's
bindings and counts, the SHA-256 of each table the checker ran, the edges and
the cycles; with groups, each group's discharged and risen conjuncts, its
tables and the claim with its conditions; with an aggregate, what the
validation established.
The identity combines the SHA-256 of the checker's own source files, the
source digests of `zeno-fcis-spec` and `zeno-fcis-compose`, and the library
evaluator's digest. `--receipt FILE` writes the result as a receipt, schema
`zeno-fcis/checked-composition-receipt/2`: compact JSON with object keys in
byte order and a final newline. `--replay FILE` runs every check again and
requires that receipt byte for byte; a difference is reported with the path
of the first value that differs.

The result grants no runtime authority. It makes no liveness, fairness,
trace, continuous-dynamics, delivery, asynchronous or shared-write coupling
claim; states outside a history from a checked genesis keep only the
conditional local statements. A conditional claim, and a claim under named
external assumptions, covers only the histories whose commands and context
satisfy the root's outer assumption; neither the checker nor the aggregate
checks those conditions when a command runs.

Exit codes: 0 `checked` or `replayed`; 1 `refused`, naming the obligation,
the leaf or edge and, where one exists, the input that shows it, or
`replay-mismatch`; 3 when a file could not be read or written; 64 for usage.

### Composed-domain manifests

A manifest with schema `zeno-fcis/checked-composition/3` is checked in the
composed-domain profile; see
[composed-domain composition](COMPOSED_DOMAIN_COMPOSITION.md). Every
participating component decides each request in merge order on one
pre-state; its inputs are bindings over the request and earlier components'
pre-state; a Reject or any refusal ends the step with no change, and the
first CommittedFailure commits the successors up to and including its own.
A component may own no state; its state is then bound read-only.

```text
zeno-fcis contract check-composition MANIFEST [--receipt FILE | --replay FILE]
    [--format human|json]
zeno-fcis contract check-composition MANIFEST --joint-table MAX_PAIRS
    [--format human|json]
```

The command validates the manifest and binds each component as a `/1` leaf
is bound, then plans the `local`, `assume` and `couple` obligations with
their exact sizes, and the tables the check builds: each component's
decisions over its local domain and each formula's values, one evaluation
per cell. Over `max_obligation` (2^22) for one obligation or table, or
`max_total` (2^25) for the obligations' sum or the tables' sum, or a lower
limit the manifest gives, the verdict is `refused` and nothing is evaluated
(`evaluated: 0`, no Authority decision, no formula value). Within budget
every obligation is checked exhaustively. The JSON report has status
`proved`, `violated` or `refused` and a result of schema
`zeno-fcis/composed-domain-result/2`: each obligation's size, status and
evaluated rows, the first witness of a violation, the planner's refusal, the
monolithic product the check did not enumerate, each component's pins and
decision-table SHA-256, the checker's identity, the claim, the trusted base
and the non-claims. `--receipt` writes `zeno-fcis/composed-domain-receipt/2`
for any verdict and `--replay` requires it byte for byte.

`--joint-table MAX_PAIRS` prints the joint step on every global state
reachable from genesis and every request, breadth first, and checks no
obligation; over `MAX_PAIRS` pairs it is refused (`joint-table-over-budget`),
not cut. `--aggregate` and `--write-aggregate` are refused for this profile
(`composed-domain-aggregate-refused`): it has no aggregate contract.

A replay carries the receipt's verdict: status `replayed` with exit 0 for a
proved check, `replayed-violated` or `replayed-refused` with exit 1, as the
check itself exits, for a violated or refused one.

Exit codes: 0 `proved`, `replayed` or `explored`; 1 `violated`, `refused`
(by the planner, at validation or for an aggregate), `replayed-violated`,
`replayed-refused` or `replay-mismatch`; 3 when a file could not be read or
written; 64 for usage.

## Checked optimizer

`optimize` searches for a smaller program equivalent to a canonical finite
scalar program and hands every candidate to the transform checker. The
optimizer is an untrusted proposer: nothing it believes about a candidate
counts, and a candidate is reported as accepted only when `transform check`'s
pure checker, run in-process over the full declared input domain, accepted it.
The command reads the program (at most 64 KiB), optionally a strategy file (at
most 64 KiB), writes the best accepted candidate and its receipt when asked,
and prints one JSON report. It adopts nothing into any application.

The engine is an in-house e-graph: a union-find over classes, hash-consed
e-nodes and congruence rebuilding over the existing ten instructions, with no
new dependency. Every class carries its scalar kind, interval bounds, its
support (the inputs it depends on) and, when the product of its support's
domains has at most 2^16 tuples, an exact table: its value, or the poison left
where an earlier `Add` or `Sub` has already trapped, on every tuple of that
product, in the checker's enumeration order restricted to the support. Tables
are kept over the minimal support, so two classes compute the same function on
the whole declared domain exactly when their kinds and tables are identical,
and a class over three inputs of a domain with millions of tuples still has an
exact table of a few dozen entries. Two classes with tables merge only when
the tables are identical, poison included. A class whose support is too large,
or that arrives after the per-search table budget (2^28 tuple evaluations and
64 MiB) is spent, keeps conservative annotations: rules merge it only when no
possible trap can poison either side, while congruence and commutativity
merge the same computation on the same operands. Every class also keeps its
values at fixed sample tuples, every tuple of a domain of at most 1,024 tuples
and 256 fixed tuples otherwise; samples bucket candidates, and different
sample values refuse any rule merge. Every `Add` or `Sub` that may overflow on
a tuple whose operands are defined is pinned: no rule removes, merges away or
folds it, and extraction emits it whether or not an output uses it.
Arithmetic is otherwise rewritten only by constant folding, and a pinned
instruction is never a constant.

A strategy is a small versioned JSON document in a closed grammar; unknown or
duplicate keys, wrong types and out-of-range values are refused, and no
user-supplied code runs. Phase names come from a closed set:

| Phase | What it does |
| --- | --- |
| `boolean` | Algebraic rules over `And`, `Not`, `Eq`, `Lt` and the Or form `Select(a, a, b)`: involution, idempotence, identity, annihilation, complement, absorption, De Morgan's Or recognition, Or commutation, factoring both ways, mux recognition and sharing-directed associativity. |
| `select` | `Select` simplification for both kinds: equal arms, constant or negated conditions, Boolean identities, and the condition's known value inside its arms. |
| `fold` | Constant folding from exact tables or interval bounds, for never-poisoned classes only. |
| `share` | Common-subexpression sharing modulo commutativity of `And` and `Eq`; structural sharing of identical instructions is inherent to the e-graph. |
| `semantic-merge` | Merges classes with identical exact tables, at any domain size, then adds single instructions over existing classes whose function an existing class already has (Not, And, Eq, Lt and Select), so extraction can use them. Candidates are bucketed by their sample values and each is confirmed by its exact table before anything is added. Skipped, and reported as such, when no class has a table. |
| `cut-rewrite` | Exact local rewriting of Boolean classes, in the style of ABC's DAG-aware rewriting: cuts of at most three leaves through each class's cheapest e-node, with their truth tables; when the leaves can never be poisoned and the table's minimum circuit for that function has fewer instructions than the chain the cut came from, the class is proposed equal to that circuit. The circuits come from a built-in table of minimum circuits for every Boolean function of up to three inputs (with and without `Eq`), generated by an exhaustive search and checked by tests. |

```json
{
  "schema": "zeno-fcis/optimize-strategy/1",
  "phases": [{"phase": "boolean", "rounds": 3}, {"phase": "cut-rewrite", "rounds": 2}],
  "limits": {"max_enodes": 20000, "max_classes": 10000,
             "max_rewrites_per_round": 10000, "max_extraction_rounds": 8,
             "max_work": 100000},
  "extractor": "dag-greedy",
  "profile": "functional-bool-v1"
}
```

| Key | Meaning and range |
| --- | --- |
| `schema` | Required; exactly `zeno-fcis/optimize-strategy/1`. |
| `phases` | Required; 1 to 16 objects, each `{"phase": NAME, "rounds": 1..=8}` with `NAME` from the table above. A round collects every match against the current graph, applies the rewrites and rebuilds; a phase stops early when a round changes nothing. |
| `limits` | Optional object; each entry is optional and defaults to the value shown, with range 1 to 100000: `max_enodes` (e-nodes ever created, retired duplicates included), `max_classes` (live classes), `max_rewrites_per_round` (rewrites proposed or completions attempted in one round), `max_extraction_rounds` (improvement rounds of `dag-greedy`), `max_work` (deterministic work in millions of steps: rule matches, completion pairs, cut evaluations and table tuple evaluations; no round starts once it is reached). A phase that reaches a limit stops and names it. `max_work` stands in for a time budget, which a deterministic search cannot read; its default does not bind in practice. |
| `extractor` | Optional; `dag-greedy` (default) or `tree`. |
| `profile` | Optional; `functional-bool-v1` or `checked-i64-v1`, the artifact profiles of the [bounded optimization loop](#bounded-optimization-loop). Instructions outside the profile (`Int`, `Add`, `Sub`, `Eq` and `Lt` for `functional-bool-v1`) are never added by any phase and have no finite extraction cost; cut rewriting uses its table without `Eq`, and may replace a stored instruction outside the profile. Every candidate must also pass the profile's gate before it is judged. |

Without `--strategy` the fixed default portfolio, version 1
(`zeno-fcis/optimize-portfolio/1`), runs three strategies in turn, each from
the original program:

1. the default strategy (`fold` 1, `share` 1, `boolean` 3, `select` 3,
   `semantic-merge` 2, `boolean` 2, `select` 2, `fold` 1);
2. the same with `select` before `boolean` in both places;
3. `fold` 1, `share` 1 and three cycles of `boolean` 3, `select` 3 and
   `semantic-merge` 2.

Each is followed by `semantic-merge` 2, `cut-rewrite` 2 and `semantic-merge` 1,
so merging by tables comes after the rules, and each runs under its own size
budget (`max_enodes` 20000) and work budget (`max_work` 400, that is 400
million steps; the most any measured run used is about 97 million). One
incumbent spans the three runs. `--strategy FILE` runs that one strategy instead.
`--profile` applies a profile to every strategy that runs; a strategy naming
another profile is `invalid-strategy`, and an original whose inputs, outputs
or domain size the profile cannot hold is `refused` (`original-outside-profile`)
before any search. After every phase a candidate is extracted, re-encoded
canonically and judged, so an early phase's candidate survives a later phase
that finds nothing better.

`--with-candidate FILE` (repeatable, at most 8 files of at most 64 KiB)
supplies programs claimed equivalent to the original, to be fused into the
search. Each is judged before any run: the checker always compares it with
the original over the full domain (and the profile's gate applies), and it
becomes the incumbent when it is smaller and better. Only an accepted program
is fused: its instructions are added to every run's e-graph, never pinned,
since it fails on exactly the original's failing tuples, which the original's
pinned instructions already keep. Each of its roots is merged with the
original's matching root when the original has no instruction that may
trap, so the checker's verdict makes the two roots equal on every tuple, or
otherwise when both roots have identical exact tables. Extraction can then
combine the best parts of every supplied program and every rewrite; the
result is never worse than the best accepted input, by the incumbent rule.

Extraction is a deterministic DAG cost search with no solver. Cost is the
pair (instruction count, canonical byte length); the byte length is exact per
instruction because the encoding is fixed-width (39 bytes for a leaf or
`Not`, 56 for a binary instruction, 73 for `Select`). The `tree` extractor
picks the cheapest tree per class bottom up, with pinned classes forced to
their pinned instruction. `dag-greedy` starts from that choice and tries, class
by class in a fixed order, every alternative e-node, keeping a switch only when
the whole shared candidate becomes strictly cheaper and never when it would
close a cycle. Instructions are emitted in a stable topological order, earliest
original position first.

Each extracted candidate is screened before judging: it must pass the
profile's gate when one is set, be componentwise no larger than the original
in instructions and bytes, strictly smaller in at least one, not byte-identical
to the original, a supplied program or an earlier candidate of any run, and
not worse than the
incumbent on (instructions, bytes). Then `transform check` runs it over the
full domain at the full Step budget with the default declared Step limit of
256, which never binds. The best accepted candidate wins by (instructions,
bytes, largest Step usage, canonical bytes). Step usage is reported from the
receipt and never compared for equivalence.

Results use schema `zeno-fcis/optimize-result/1`. Every result has a `detail`
object; once the program and strategy are read, the report also carries
`original_sha256`, `strategy_sha256` (of the strategy file, or of the built-in
portfolio document) and the checker `limits`. A searched result's `detail`
reports the original's size, the domain size and whether every class of the
original has an exact table, the strategy or portfolio document, the
termination bounds (`search`: phases requested and run, each phase's run,
rounds requested and run, saturation, the limit hit if any, rewrites, nodes
added, merges accepted and refused, cut evaluations, work, e-node and class
counts; `runs`: each run's e-graph size, merges, tables with and without an
exact table, table work and whether the table budget ran out, work and stop
reason), every candidate with its run and verdict (`accepted`,
`accepted-not-better`, `counterexample`, `inconclusive`, `refused`,
`not-smaller`, `duplicate`, `not-better-than-incumbent`, `unextractable`), each
supplied program with its digest, size, verdict and whether it was fused, and
`best` with its run and phase (or the index of the supplied program it is),
the candidate's digest, sizes, largest Step usage and its complete transform
receipt. Each run also reports how many supplied roots were merged and
refused. With a portfolio, the top-level `enodes` and
`classes` are the largest run's and the merge counts are totals.

| Status | Exit | Meaning |
| --- | ---: | --- |
| `improved` | 0 | An accepted candidate is smaller than the original. `--candidate-out` and `--receipt` were created when given. |
| `no-checked-improvement` | 2 | The search ended within its bounds without an accepted smaller candidate; the original stands. Nothing is written. |
| `inconclusive` | 2 | The domain exceeds `--max-input-tuples` (default 100,000,000); no candidate could be judged, so nothing was searched. |
| `refused` | 1 | The original failed the library importer, exceeds 64 KiB, has an empty input domain, or has an ABI or domain size the requested profile cannot hold; or more than 8 programs, or one larger than 64 KiB, were supplied. |
| `invalid-strategy` | 1 | The strategy is not a document of the closed grammar, exceeds 64 KiB, or names a profile other than `--profile`. |
| `output-exists` | 1 | A `--candidate-out` or `--receipt` path already exists, as a file or a link; nothing was searched. |
| `io-error` | 3 | A file could not be read, is not a regular file, or an output could not be created. |

The written receipt is the checker's `zeno-fcis/transform-receipt/1` for the
original and the written candidate; `transform replay` reproduces it byte for
byte. The same program, strategy and profile give byte-identical output.

On the recorded withdrawal-queue artifacts the default portfolio reaches the
7-node Boolean kernel (from 16), a 35-node retained controller (from 69; the
recorded hand candidate has 60) and an 88-node current decision graph (from
106, every one of its 1,296,000 tuples checked), each accepted with a receipt.
Over the 32 seeds of `docs/benchmarks/cases.json` the outputs total 58
instructions on the Boolean seeds and 45 on the integer seeds (an unimproved
seed counts at its own size), and over the
published 100-case corpus
(calibration only) 508 instructions; under `functional-bool-v1` every Boolean
seed and corpus output passes the profile's gate with the same totals.
[`docs/benchmarks/measure_optimizer.py`](benchmarks/measure_optimizer.py)
reproduces these figures and replays every receipt. In a release build,
measured on a shared machine as the median of three runs, the portfolio took
1.628 seconds on the controller, 30.64 seconds on the decision graph (mostly
the checker's enumeration of 1,296,000 tuples) and under 0.5 seconds on 87 of
the 100 corpus cases (at most 1.734 seconds); a debug build is several times
slower.

These are results of a bounded search, not minimality claims: tables are exact
only for supports of at most 2^16 tuples and within the table budget, larger
supports get algebraic rules under conservative trap bounds, the cut table
covers three inputs, and an accepted candidate changes Step usage, so adopting
one into an application remains a separate reviewed step that this command
does not perform.

## Bounded optimization loop

`zeno-fcis loop` runs a bounded adaptive loop: a proposer suggests
complete candidate programs and `transform check` judges every one against the
admitted original on its whole declared input domain. The loop keeps an
incumbent that is either the original, with no receipt, or a checked
replacement built from a genuine equivalence; a model, a user, a receipt file
or a "passed" flag cannot build one. Proposals are data, never code.

| Command | Effect |
| --- | --- |
| `loop open --original P --session DIR [--profile functional-bool-v1\|checked-i64-v1] [--attempts N] [--checks N] [--model-calls N] [--check-work N] [--session-work N] [--deadline-ms N]` | Admits the original under the profile (default `checked-i64-v1`), freezes the canonical request (`zeno-fcis/transform-request/1`) and its `request_id`, and creates the session directory. Limits above the ceilings (8 attempts, 8 checks, 4 model calls, 20,000 ms, and the work ceilings scaled to the original, below) are refused; lower limits win, and an omitted work limit takes its ceiling. Hosted providers, source disclosure and spending are fixed disabled. |
| `loop candidate --session DIR (--candidate C \| --candidate-json J) [--provenance FILE]` | Spends one attempt on a supplied candidate (canonical bytes, or the fixture vocabulary `{inputs, outputs, nodes, roots}` encoded canonically first) and prints typed feedback: a replayed counterexample, an admission refusal, an incomplete check, or an equivalence with its actual cost and selection reason. |
| `loop run --session DIR --proposer local\|fake\|hosted [--script S] [--hosted-config H] [--seed P]...` | Drives attempts until the proposer is exhausted or a limit stops the session. Each `--seed` program (canonical bytes, such as an `optimize --candidate-out` candidate or a `contract review --mutant-programs` mutant) is proposed first, in order, as an ordinary candidate attempt with no model call metered; a seed is never trusted, only checked. `local` is a deterministic rewriter; `fake` replays a script (`zeno-fcis/fake-provider-script/1`) and is metered as a model; `hosted` is the provider adapter, which is disabled in this build and answers "unavailable" without any network, credential or spending path. |
| `loop resume --session DIR` | Re-admits the request, verifies the ledger chain against its head, replays the incumbent through the checker and prints the report. Any failure leaves no trusted incumbent. |
| `loop encode --program J --out P` | Encodes a fixture-vocabulary program as canonical bytes after library admission; the output file is never overwritten. |

Results use schema `zeno-fcis/transform-loop/1` with `authority: none`.
`open` exits 0 when the request is admitted and 1 when it is refused.
`candidate` exits 0 for an equivalent candidate (improvement or not), 1 for a
difference, refusal or duplicate, and 2 for an inconclusive check or a closed
session. `run` exits 0 when the loop completed; `resume` exits 0 with a
replayed incumbent and 2 (`resume-refused`, `resume-inconclusive`) otherwise.
I/O failures exit 3. The report (`zeno-fcis/transform-loop-report/1`) states
`no-checked-improvement` or `best-checked-so-far`, the actual stop reason, the
receipt status, every attempt's outcome and cost, the accounting (attempts,
checks, model calls, tokens, work, unresolved reservations) and the claims the
result does not make: global optimality, convergence, success probability,
model learning, application authority, wall-time speedup.

Selection is strictly lexicographic on (stored nodes, canonical bytes): a
candidate replaces the incumbent only if neither component exceeds the
original's, at least one is below it, and the pair is below the incumbent's.
Ties keep the incumbent. Every incumbent is therefore componentwise no more
costly than the original and updates descend strictly; nothing stronger is
claimed. The equivalence is functional equality on the declared domain under
eager semantics; Step usage is reported, never compared, so adopting a
candidate remains an application decision outside this command.

The session directory holds `request.json`, `original.zcve`, the append-only
`ledger.jsonl` with its separately written `ledger.head`, `artifacts/` by
digest, `receipts/`, `witnesses/` and a transcript capped at 512 KiB. Every
attempt, model call and check is reserved in the ledger before its work and
never refunded; a crash leaves the reservation charged as unresolved.
`resume`, `candidate` and `run` each start by replaying the incumbent and
the stored counterexamples, and that replay is reserved and written to the
ledger before any of its work runs, so an interrupted replay stays charged.
The ledger records no completion for a replay and no invocation reuses an
earlier one's: each pays for its own, and when the remaining session work
cannot cover it the invocation exits 2 with `resume-inconclusive`
(`insufficient-replay-allowance`) and writes nothing. The
ledger's hash chain detects truncation and edits relative to the head; a host
that rolls back both files together is outside its detection, which is the
documented host-integrity assumption. The `--deadline-ms` limit bounds one
`run` or `candidate` invocation's loop work; an agent's thinking time between
`candidate` calls is not loop work, while attempts, checks and work caps are
session-wide and persisted. The invocation runs in a contained process with a
64 MiB memory cap and no swap. Contained loop stages request at most two
seconds of CPU for a check and five seconds for a search, clipped by the
invocation's remaining wall allowance. The supervisor samples kernel CPU:
the verified cgroup's `cpu.stat usage_usec` on systemd, or the owned trusted
finite worker's user and system ticks on POSIX. POSIX refuses unexpected
waited-child CPU; interpreter and native profiles still require systemd.
Missing, malformed or regressed counters refuse. FINISH independently resamples
and charges the counter's rounding uncertainty before permitting a result.
CPU use at or above the requested budget refuses; a conservative strict upper
bound above that budget also refuses at FINISH. POSIX rejects a stage before
starting it with `stage-cpu-resolution` when the requested CPU milliseconds
cannot cover FINISH uncertainty (20 ms for two counters at HZ=100). A 20 ms
request is admissible there, but still needs a bounded FINISH; no extra tick is
added to the minimum because the strict upper bound may equal the budget.
Systemd's one-microsecond counter policy is unchanged. A replay refused for
this reason returns `inconclusive`, an empty current-invocation `attempts`
list and the persisted accounting, including any replay reservation already
charged. Intake observations remain complete. Its report marks
`resume: "not-replayed"` and leaves `incumbent` and `trusted_incumbent` null:
no retained result is trusted without replay, and no attempt is spent.

Each loop stage also has a wall backstop of nine times its requested CPU
milliseconds plus two seconds, clipped to the whole worker's remaining time.
Thus a full check has at most 20 seconds; a full search's nominal 47 seconds
is clipped by the 25-second worker cap. No CPU increase and no valid completion
frame for the requested milliseconds plus one second refuses (3 seconds for a
full check; 6 for a full search). The supervisor services queued protocol
completion before declaring inactivity; partial or invalid frames do not
extend this bound. Waiting for a control reply has a separate 10-second
monotonic limit; wrong, closed, missing and late replies refuse. These are
checked deadlines, not real-time scheduling guarantees.

The whole loop worker has a 25-second cap from spawn, including startup.
Cleanup and absence verification fence output and receipts after execution;
they do not grant more execution time. Caller preparation precedes that clock;
on POSIX, the separate 30-second guardian clock also includes guardian
preparation. Startup ACK waiting has a 10-second monotonic deadline inside the
worker cap; no application work starts before ACK. There is no invocation retry.
The systemd startup manager query has its own 5-second ceiling, also bounded by
the worker deadline. Offline check stages keep their explicit requested wall
limit plus one second of grace and their separate 65-second worker cap.
Independent deterministic work reservations and invocation deadlines still
apply; CPU accounting does not replace them.

Operator observations are written separately under the owner-private
`/tmp/zeno-fcis-operator-UID/` directory (0700; records 0600). The versioned
`zeno-fcis/operator-containment/1` records identify the observer process and
record preparation, ACK, worker and guardian durations in milliseconds where
observed, plus the exact supervisor refusal and cleanup observation. Missing
phases remain unknown. These records supply no decision, resource accounting,
admission, receipt or publication authority. They do not enter capture JSON;
its proposer-facing refusal stays sanitized. A record-write failure sends the
operator diagnostic to stderr instead. Each complete record is flushed to an
owner-only temporary file in that same directory and renamed atomically into
its final name. Readers select the invocation before decoding; malformed
matching records still fail their checks.

A zero-remaining stage records a timeout
without starting work. Durable session records can retain earlier completed
checks even if a later stage fails; resume independently rechecks them.

Work policy `zeno-fcis/checker-work/4` defines **admission-control units**,
not elapsed CPU time or a proof that every machine instruction is metered.
`check_work` reserves import, dispatch, enumeration, merge and receipt services;
`session_work` also charges preparation and each resume replay, without refunds.
Version 4 scales the work ceilings to the original (below); every charge is
version 3's.
Measured evaluator Steps must fit the reservation at runtime; this check does
not measure every shell service. Candidate output names `detail.session.state`
and, when closed, `stop_reason`; preparation exhaustion includes the session
report and permanently closes it. Separate `acquisition` accounting bounds
bootstrap reads and reports skipped leftover staging entries (at most four).

The work ceilings scale with the original. The check ceiling is the work of
checking the original against a candidate of its own size, at least the
compiled 1,000,000 units and at most 2^28 (268,435,456). The session ceiling
funds all eight attempts even when each arrives in an invocation of its own,
as every attempt through the MCP tools does: per attempt, two checks (its own
and the invocation's replay of the incumbent) and 4,310,016 units for the
largest intake (a JSON candidate with provenance, 2,625,536), the attempt's
import preparation at the largest artifact (635,904) and the replay of eight
stored counterexamples (1,048,576). That is 50,480,128 units at the smallest
check ceiling and 4,329,447,424 at the largest. An omitted `--check-work` or
`--session-work` takes its ceiling, a larger one is refused, and
`request.json` records the values, so a reopened or resumed session is held
to the limits it was opened with. The check cap is sized to the two-second
check stage: the loop's four-worker check spent 2.2 to 3.2 seconds on
687,409,803 units for the 1,296,000-tuple decision graph before the checker
speedup, so a check at the cap should take about 0.9 to 1.3 seconds on that
machine by linear scaling. That is an estimate, not a timing guarantee; a
check that outlives its stage is reported as incomplete. Explicit limits at
rc.1's 1,000,000/8,000,000 may now be refused earlier than under rc.1, which
did not charge candidate intake/import preparation: at the maximum artifact
allowance an attempt reserves 635,904 units before checking, JSON intake
additionally prepays 2,101,248 units without provenance before any file read,
and eight such JSON attempts alone require 21,897,216 units, before checks or
resume. Silently rescaling a stored request would authorize work above its
bound, and reducing the tariff would omit bounded services.
Variable byte sizes, domain sizes and resume counts mean the old and new
units are not interchangeable. Sessions bind
this policy in their canonical request; a request that binds another version,
or an rc.1 request without one, refuses as `work-policy-changed`, must be
reopened, and cannot reuse its old request identity.

Strategy proposals in the optimizer's grammar (`zeno-fcis/optimize-strategy/1`:
named phases with bounded rounds, optional limits, a fixed extractor and an
optional profile) are admitted as data and run by the wired engine, the
checked e-graph optimizer (engine `zeno-fcis/optimize/1`, searching domains
of at most 1,000,000 tuples under the search worker's five-second deadline).
The engine runs every strategy within the request's profile, so its candidate
can pass the loop's profile admission, and fuses the session's checked
replacement, when there is one, into its search as a supplied program; it
proposes nothing unless it finds a candidate better than that replacement. A
strategy that names another profile is `strategy-unavailable`, as is a phase
outside the optimizer's table.

In a debug build the default strategy finishes within 2 seconds on every measured
program of at most 1,024 tuples; a larger domain can take longer than the
deadline, and an abandoned search is reported as a failed search, never as a
candidate. The optimizer's own checker verdict is only
provenance: the bytes it emits enter the same admission and check path as
any candidate, and only the loop's check can replace the incumbent. The MCP tools
`transform_request`, `transform_candidate` and `transform_replay` in
[LLM synthesis](LLM_SYNTHESIS.md) call `open`, `candidate` and `resume`.
[`docs/benchmarks/run_neural_loop_protocol.py`](benchmarks/run_neural_loop_protocol.py)
runs the local and fake arms of the preregistered protocol over a cases file
and reports every result, including failures.

### Loop doors (2.4.0)

The doors let a proposer submit expressions, sketches with holes and family
programs instead of whole canonical programs. They are untrusted proposers
(`zeno-fcis-loop-doors`). The checkers decide acceptance; unrealizable,
not-found, regions and repair sites are the doors' own untrusted findings. The
guide is
[ZAL loops](zal/LOOPS.md).

| Command | Effect |
| --- | --- |
| `loop candidate --session DIR --expressions FILE [--provenance FILE]` | Lowers one expression per output (`zeno-fcis/loop-expressions/1`) to a program with the original's exact domains and spends one attempt on it, like `--candidate-json`: an unreadable or over-large file is refused, and an undecodable or unknown-name document is a failed proposal that still consumes the attempt. The door's own `transform check` with replay runs before the attempt; if it does not complete (for example on a short `--deadline-ms`), the result is `inconclusive` (exit 2), the attempt is not spent and the incumbent is unchanged. On a counterexample, `detail.door.explanation` adds the failing region (a formula the doors confirm on every tuple with their own evaluator, or the shared field values), ranked repair sites in the proposer's text, and a repair. A repair is labelled `checked repair` only after `transform check` accepts it and `transform replay` matches its receipt. |
| `loop sketch --session DIR --sketch FILE [--provenance FILE]` | Spends one attempt. Solves the holes (several holes in an output: holes alone on their route derived exactly, the rest branched over effect classes; `detail.door.searched[].search` is `meanings`); a completion goes through an in-process `transform check` with replay and then the session's check. If that confirming check does not complete, the result is `inconclusive` (exit 2) with `detail.door.status` `completion-not-confirmed`, and no attempt is spent; a search that stops on its budget is `inconclusive` (exit 2, `door-search-not-completed`) and spends no attempt either. Otherwise `detail.door.status` is `completed`, `unrealizable` (with a witness input, or after a listed complete candidate space was exhausted), `not-found` (claims nothing), `inconclusive` (a budget ran out) or `refused`. |
| `loop family --instances MANIFEST --program FILE [--fit]` | No session. Judges a program over family parameters on every instance of a `zeno-fcis/loop-family-manifest/1` manifest (at most 10) with `transform check` and `transform replay`; reports nodes against each instance's optimize result when listed, and the smallest failing instance. Only a checker counterexample makes a row `refuted`; a row whose sketch the doors find unrealizable is labelled `unrealizable`. `--fit` fits each hole to `a * $P + b` on the instances not marked `"fit": false` and checks the lifted program on every instance, including those. Coverage is stated as "checked on these N instances". |
| `loop transfer --from FILE --to-instances MANIFEST` | No session. Moves a program written for one instance (whose parameters the document gives) to each listed instance: directly, then with the constants near a changed parameter opened as choice holes, then by repair. Each result is checked as above. |
| `loop meaning --original FILE (--expressions FILE \| --candidate FILE)` | No session. Keys a complete program by its observation on every declared input (every output, or the evaluator's exact failure), computed by the library evaluator as `transform check` observes it, and reports whether it means what the original means. Two programs get equal keys exactly when `transform check` finds them equivalent, within its limits. The loop workspace uses it to answer a refuted program written another way from memory. |
| `loop question --contract DIR [--answers FILE]` | No session. Groups the inputs where the declared laws and the recorded answers (`zeno-fcis/loop-answers/1`) admit more than one decision, by the case that fires there, and asks about the largest group: a formula checked to hold on exactly its inputs, an example, the decisions still admitted and the core's own. Each answer becomes laws over exactly its inputs (`detail.answers[].laws`), checked by the gaps judge against the core (a text with no exact law form, such as a truth value compared with a number, is refused); an answer that differs from the core in class, reason or, for an accept that gives one, successor is a `disagreement`, never adopted. An answer that leaves some input with no admissible decision is refused (`conflict`, exit 1). A reject reason has no law form and is recorded as a requirement only this door applies. In practice, on the 5-second stage deadline, a question with answers can stop `inconclusive` (exit 2) above roughly 10,000 state × input tuples, and admissibility is refused as inconclusive above 4,000,000 law checks. |
| `loop teach-back --contract DIR --rule FILE [--answers FILE]` | No session. Judges each rule (`zeno-fcis/loop-rules/1`) on every declared input and on the reachable states: `exact`, `holds` (with the missing region), `wrong` (exceptions grouped by the core's decision, with regions and examples) or `vacuous`; whether the requirements already force it (`required`: on the inputs where it holds, how many the requirements force it on, leave `open`, or `forbidden` it on, with an admitted decision as the witness; left out with `summary.required.not_computed` when the admissibility table is over its budget); a meaning key (equal keys give equal verdicts); and a summary of the exact and holding rules. In practice, on the 5-second stage deadline, it can stop `inconclusive` (exit 2) above roughly 10,000 state × input tuples. |
| `loop decompose --contract DIR --architecture FILE [--compose [--export DIR]]` | No session. Steps 1 to 3 for components in merge order (`zeno-fcis/loop-architecture/1`): required behaviour per component, interface counterexamples with suggested reads (and reads owned by a later component), unowned reasons, failures that must change a later-owned field, case tables projected from the core or tabulated for components without cases, and blame with regions for components with cases. Verdicts: `refused`, `unrealizable` (also whenever a structure problem or interface counterexample is found), `realizable`, `completed`, `meets-requirements`, `refuted`. Steps 1 to 3 are the door's own evaluation of the tables (`decided_by: door-simulation`), not a checker's, and do not claim that the composed architecture behaves as the core. With `--compose`, a realizable architecture whose components all have case tables becomes a `zeno-fcis/checked-composition/3` composition of generated contracts, every submitted guard kept (a catch-all reject with its own reason is appended where a last guard is not `true`); the composition checker's joint step is compared with the core on every declared input and its obligation check runs (`detail.compose`, `decided_by: composition-checker`). The verdict is the weaker of steps 1 to 3 and the composed check: `accepted` only when step 3 blames nothing, the joint step equals the core and the obligations are proved; `refuted` when either differs. `--export DIR` writes the manifest and contracts to a new directory (an existing one is refused), whatever the verdict. |

Results carry `authority: none`. Session doors use schema
`zeno-fcis/transform-loop/1` with a `door` section; `family` and `transfer`
use `zeno-fcis/loop-doors/2`. Every door runs in the loop's contained
process: its search runs in five-second search stages and each confirming
check in a two-second check stage, within the invocation's deadline; a stage
that cannot run in time is reported as inconclusive. Input documents and
manifests are bounded to 64 KiB, and the doors enumerate domains of at most
131,072 tuples. A session door prepays its document's intake against the
session's work allowance; its own search and confirming checks are bounded by
the door's budgets and stages rather than charged to the session's checker
work. A session's own admission still applies: `loop open` refuses an
original whose check exceeds the per-check work allowance
(`work-infeasible`), so the session doors are unavailable on such a core,
while `family` and `transfer` still check it.

### Learn from examples with a fixed vocabulary

```text
zeno-fcis loop learn --spec FILE
  [--original PROGRAM.zcve] [--out NEW_DIRECTORY]
  [--max-work N] [--max-tuples N] [--max-meanings N]
  [--max-cells N] [--max-conflict-subsets N]
```

This stateless command reads an ASCII `zeno-fcis/loop-learn/1` declaration
of at most 64 KiB. A native reference is optional. JSON fields
are closed and integer values are JSON integers, including Boolean wire
values 0 and 1; strings, floats, Boolean aliases and duplicate fields are
refused. Example input tuples must be distinct and within the declared
domains, and every example supplies all outputs.

```json
{
  "schema": "zeno-fcis/loop-learn/1",
  "inputs": [{"name": "x", "domain": {"kind": "Int", "min": 0, "max": 2}}],
  "outputs": [{"kind": "Int", "min": -10, "max": 10}],
  "vocabulary": {
    "constants": [0, 1], "arithmetic": ["+", "-"],
    "macros": ["min", "max", "abs"], "compare": ["<", "<=", "==", "!="],
    "boolean": ["!", "&&", "||"], "conditional": true
  },
  "max_size": 3, "misses": 0,
  "examples": [{"input": [1], "output": [2]}]
}
```

Domains are `{"kind":"Bool"}` or inclusive signed-i64
`{"kind":"Int","min":N,"max":N}`. There are 1 to 8 inputs and outputs,
at most 32 distinct constants and 64 examples, and `max_size` is 1 to 12
(default 7). Input names are distinct lower-case identifiers of at most 32
characters, excluding reserved words, output names and macro names. The
vocabulary defaults to constants `[1,2,3]` and arithmetic `["+","-"]`;
other operation lists default to empty and `conditional` to false. Listed
operations must be distinct. Only the fixed `min`, `max`, `abs` macros are
available. Multiplication, custom macro objects, opaque primitives and
nonzero `misses` are refused.

Expressions grow by size. A leaf costs one; an operation or standard macro
costs one plus its arguments. `min`, `max` and `abs` expand to checked IR
before final checking. Evaluation is eager, including unselected branches.
Raw meaning vectors retain arithmetic failures and integer values; only an
output root turns values outside its domain into output-domain failures.
Failures use tags, including at the lowest i64 output bound.

Results use `zeno-fcis/loop-doors/2`, `door: learn`, `authority: none`.
Scalar report fields use `safe-number-or-decimal-string/1`: integers from
`-9007199254740991` through `9007199254740991` are JSON numbers; larger
magnitudes are canonical decimal strings. Decode both forms as exact integers.
This covers question coordinates, learned observations, witness values, search
bounds, fills and fitted coefficients. Booleans and variant names retain their
own types. Existing decimal-string program, domain, evaluate and receipt
fields retain their formats. The [judge reference](CONTRACT_JUDGES.md#integer-values-in-json-reports)
lists the corresponding judge and display schemas.

Statuses:

| Status | Exit | Scope |
|---|---|---|
| `determined` | 0 | One meaning per output fits; the lowered candidate reproduced every example through the existing library evaluator. Uniqueness is an enumeration proposal |
| `question` | 3 | Several meanings fit; `detail.question` minimizes the largest joint bucket, then the sum of squared buckets, then the unasked input ordinal. The candidate reproduces the examples but the question remains open |
| `outside-vocabulary` | 1 | No expression in this vocabulary and size bound fits; `detail.core` is a minimum-cardinality set of example indices, ordered and tied by input ordinal. This is a proposal |
| `ambiguous` | 1 | Several meanings remain without a distinguishing unanswered input; a proposal, not a final program |
| `inconclusive` | 2 | An enumeration budget/deadline clears counts, question, core and candidate. A later capture/check stop retains the completed example-learning result and exports no qualified candidate |
| `refused` | 1 | Invalid declaration, unsupported profile, or a lowered candidate that failed exact example checking |

`counts` and `before` count scalar meanings per output, after and before
example filtering. `by_size` lists new raw integer/Boolean meanings at each
size. Question `buckets` are per output; their product defines the proposed
joint partition. Its `worst` and `squares` scores are exact decimal strings.
`representatives[].size` measures vocabulary cost, not IR instructions.
`candidate.examples_check.decided_by` is `library-evaluator`; zero examples
are explicitly `vacuous`. No equivalence receipt is claimed for this example
check. Whole-program eager failure is not a Cartesian tuple of per-output
observations; the counts remain per output.

Default caps (flags may only reduce them): 2,000,000 work units, 4,096 input
tuples, 4,096 raw meanings per kind, 1,000,000 cumulative table cells and
10,000 conflict subsets. Table cells include scratch and index copies. The
existing contained process imposes a five-second search stage and two-second
check stage. Expansion and checked split-score overflow stop inconclusive;
the conservative expanded-program cap is 256 nodes. Measured bytes/work are
reported on refusal and budget stops that return normally; an externally
terminated process cannot supply its lost scratch counters. No loop session
is opened and every outcome spends zero attempts.

For interaction, use `terminal.py loop init --learn FILE --workspace W.json`.
The initial declaration is copied into the workspace and bound by revision.
Model acts `learn ID;` and `learn-suggest QUESTION: [OUTPUT];` request analysis
or record an advisory suggestion. Only the owner uses
`terminal.py loop answer QUESTION '[OUTPUT]' --workspace W.json`; in-domain
answers outside the proposed buckets are retained too. Judge acts
`learn-question`, `learn-determined`, `learn-outside`, `learn-ambiguous` and
`learn-inconclusive` round-trip in both surfaces. Reanalysis retires older
questions; a stale question cannot confirm an example. The existing MCP
tools expose this mode and add no answer tool.

Not claimed: unbounded uniqueness, checker-certified minimality, equivalence
to a hidden function, full-domain totality, tolerance of wrong owner labels,
automatic owner answers, model quality, adoption or publication authority.
See [the loop guide](zal/LOOPS.md#learning-with-a-declared-vocabulary).

With `--original`, every fitting lowered candidate automatically enters
complete reference-table capture, then the existing native transform checker
and receipt replay against the original program. `learning_status` and
`candidate.examples_check` retain the example-learning result; only a complete
capture and a matched native replay give `qualified`. A mismatch gives `refuted`.
The successful-table capture profile supports at most 4,096 inputs; a reference
with an eager failure, or an unencodable table, remains unsupported or stopped.
Work/cell reservations and the invocation deadline carry through the phases.
`--out` exports `original.zcve`, `capture-reference.zcve`, `candidate.zcve` and
`result.json` on qualification. Both program pairs can be checked and replayed
independently with `transform check` and `transform replay`. A stopped or refuted
run exports no candidate bytes. Without a complete reference, example fit
remains explicitly unverified.

Bounded search failures include `search_limit`: the named exhausted resource
or declared search envelope, effective limits and a request for a supported,
better-shaped primitive or sketch. For example, `abs(x + y)` costs four grammar
nodes and cannot fit a three-node expression cap. The request preserves every
domain, example, law and checker cap; it makes no global impossibility claim.

## Decision examples

`tests/decision-examples.txt` lists an application's reviewed decisions, one
per line. Every application built with `new --contract` reads it with the
grammar in its `src/examples.rs`, and `contract review` compiles that same
file, so both read a file alike: the same examples, or the same error on the
same line. An error names the line and what is wrong with it.

A line that is blank, or whose first non-blank character is `#`, is skipped.
Every other line is one example:

```text
inputs | class reason post | deliveries
```

- The last `|` section holds the deliveries and the one before it the
  decision. Every earlier section holds input numbers, read in order, so the
  inputs may be split over several sections, such as `state | command |
  context`. Together they are the state fields, then the command, then the
  context, in the order the program reads them, one number each.
- The decision is the class, `accept`, `reject` or `failure`; then the
  reason, a number or `-`; then one number per state field, the successor
  state. A reject repeats the pre-state.
- The deliveries are `-`, or deliveries separated by `;`. A delivery is a
  declared channel's ID followed by one number per payload field. When the
  contract declares exactly one channel, the payload numbers alone are also a
  delivery on it. The count of numbers tells the two forms apart, so no line
  has two readings.
- A number is a decimal integer: 0 or 1 for a boolean, the value of an
  integer, the variant ID of a sum. Each lies in the declared domain of its
  input, state field or payload field. A field with no number form, such as
  text, cannot be written.

A comment takes a whole line: a `#` after the deliveries is read as a number
and refused. In dual approval, whose one channel 300 carries two payload
fields, these two lines are the same example:

```text
150 161 160 140 162 | accept - 151 161 162 | 300 162 161
150 161 160 | 140 | 162 | accept - 151 161 162 | 162 161
```

## Contract review

`contract review` is advisory. It reads an application's `project.zeno`,
`v2/policy.json` and, when present, `tests/decision-examples.txt`, binds the
generated contract to the library Authority exactly as the application does,
and writes a review packet, schema `zeno-fcis/contract-review/2`: compact
canonical JSON with a final newline, byte-identical on repeat. The review
grants no authority and changes no application file. `--out` names the
packet's file; without it the packet goes to stdout and the summary to
stderr. Nothing in the packet is a reading of a rule by the command: every
decision, of the contract and of each mutant, is the library's.

The packet records:

- The inputs: every program input position with its admitted domain, and the
  input set. A domain of at most `--max-tuples` tuples (default 2^20) is
  enumerated in full (`full-domain`). Otherwise each integer position takes a
  boundary set: its endpoints and every integer constant the rules compare
  with, add to, subtract from or assign to a value of its type, with that
  constant's two neighbours, where in domain; genesis values count as
  constants; a sum takes every variant and a boolean both values. When the
  product of the boundary sets fits the limit it is the input set
  (`boundary-product`). Otherwise the input set is probes
  (`boundary-probes`): the genesis state with the first boundary value of
  every command and context position, then each owner example, are bases,
  and each base is varied in one position, then in two positions, over the
  boundary sets, stopped at the limit. The packet names the construction
  and, for a boundary set, the values, constants and bases.
- The decision table: for each input, the library's class, reason, successor
  digest and outbox digest, or its refusal, with each distinct successor
  state, outbox and refusal written once. A refusal names its class (below)
  and, when a law refused, that law, as the library's own law diagnostics
  report it: the first law, in the contract's law order, that refused. When a
  refused input's pre-state does not satisfy a state law, its row also names
  the first such law, as `unsatisfied 500`. Rows are written out up to 2^17
  inputs; a chained SHA-256 over every row is always recorded, with the
  tallies by class and reason.
- The refusals. The state laws are the contract's laws that apply at genesis
  and to every committing decision, so that every committed state satisfies
  them: in a generated contract, the laws declared `on commit, genesis`, every
  `StateInvariant` among them, and those declared `on any, genesis` other
  than an `InitialCondition`, which applies at genesis only. For each refused
  input, the library's law evaluator runs the contract's own law programs on
  the input's pre-state as a genesis state, the state laws first; the
  pre-state satisfies the state laws when each one's verdict is satisfied.
  Refusals are counted by class: `law`, an applicable law refused; `domain`,
  a value left its declared domain; `arithmetic`, the decision program's
  checked arithmetic overflowed; `meter`, the shared meter refused at any
  stage; `input`, the inputs were not admitted; and `other`. Each count is
  split between refusals on pre-states that satisfy every state law and
  refusals on pre-states the state laws exclude, and each group of refused
  inputs with one refusal and one unsatisfied state law is listed with its
  first input.
- The examples: each line of `tests/decision-examples.txt`, read with the
  [decision-examples grammar](#decision-examples) the application compiles,
  is run through the same route and compared on class, reason, successor
  state and deliveries. A difference is a finding, and the command exits 1
  with status `disagreement`.
- The mutants: a fixed, versioned catalog
  (`zeno-fcis/contract-review-mutants/1`) applied to the rules model, in a
  fixed order: comparison flips (`<`/`<=`, `>`/`>=`, `==`/`!=`); integer
  constants moved by one, including a delivery's ordinal, channel and
  idempotency ordinal; one side of a `&&` dropped in a guard or a variable;
  two adjacent cases exchanged, the final catch-all staying last; a case and
  the next case of its class exchanging reasons; a case taking the next
  declared reason of its class; and a dropped delivery. Each mutant is
  regenerated and bound through the library as the original is. One the
  generator's own checks refuse is `refused-by-generator`; one the library's
  catalog or Authority binding refuses is `refused-by-library`. Each other
  mutant is searched for a distinguishing input, first among the owner's
  examples in file order, then through the input set in order:
  `distinguished`, with the witness written as a proposed decision example
  beside both outcomes and, for an owner example, whose outcome the owner
  shares; `equivalent-over-full-domain`, only after a fully enumerated
  domain; otherwise `not-distinguished-within-boundary-set`, which claims
  nothing more.
- The findings: owner examples the contract decides differently; owner
  examples whose outcome is a mutant's rather than the contract's, the
  signature of a wrong constant or comparison in the rules; and law
  refusals on pre-states that satisfy every state law. Such a refusal means
  that, from a state the laws allow, the rules make a decision a law
  refuses. One finding is made for each refusal and law, with the number of
  inputs and the first of them, and the case the library's evaluation of the
  decision program selects for it. A law refusal for which the library's
  diagnostics name no law, such as a malformed law frame, is named by that
  case and input alone. Refusals of other classes are counted but are not
  findings.

The human summary labels original example disagreements and law refusals,
declared-law `gaps` and scenario coverage separately. Review does not run the
scenario judge; use `contract scenario` for decision-example and mutant
coverage. A zero finding count does not establish complete requirements.

The review does not decide reachability. A pre-state that satisfies every
state law may still be unreachable from genesis: an escrow whose laws only
conserve funds allows a created escrow that already holds funds, which no
rule reaches. A state law that excludes such states, or a rule that decides
them, removes the finding. A pre-state that breaks a state law is never a
committed state, so a refusal there is reported apart and is not a finding.

Exit codes: 0 `reviewed`; 1 `disagreement` when an owner example disagrees,
otherwise `law-refusal` when a law refusal is a finding, the packet still
written in both cases, or `contract-invalid`, nothing written and the entry
at fault named; 3 when a file could not be read or the packet could not be
written; 64 for usage, including `--max-tuples 0`. The human summary counts
the refusals by class and by pre-state, and gives one line for each law
refusal finding. With `--out`, `--format json` prints a summary with schema
`zeno-fcis/cli/1`: `status`, `packet`, `packet_schema` and the packet's
`summary` object, which counts the refusals and the law refusal findings. A
review observes the contract's behaviour on chosen inputs. It is not a proof
about the rules, and a boundary set is not the domain; it does not replace
the owner's review of the rules file.

`--mutant-programs DIR` also writes, into a new directory, the decision
program of every mutant the review classifies `equivalent-over-full-domain`,
one canonical `<id>.zcve` each, as seed proposals for
`optimize --with-candidate` and `loop run --seed`. A seed is never evidence:
the review compares decisions, while output 0 of a decision program is the
selected case's index, so a decision-equivalent mutant (two disjoint cases
swapped) can still be a different program. Each seed is therefore marked with
the transform checker's own verdict against the contract's program
(`program_equivalent`), and the optimizer and the loop check it again before
using it. The JSON summary lists each seed's path, size and SHA-256 under
`mutant_programs`; an existing directory is refused (exit 3).

## Generated application command line

An application built with `new --contract` has an operational command line
of its own, in its `src/cli.rs`. Its package and binary are named after the
application and its contract version, such as `spend-approval-v1`, so the
builds of two versions of one application can share a target directory
without sharing a library build or overwriting each other's binary.

```text
APP init DATABASE [--format human|json]
APP submit DATABASE NAME=VALUE... [--format human|json]
APP decide DATABASE NAME=VALUE... [--format human|json]
APP state DATABASE [--format human|json]
APP history DATABASE [--format human|json]
APP pending DATABASE [--format human|json]
APP deliver DATABASE --to FILE [--format human|json]
APP deliver DATABASE --relay CONFIG.json [--format human|json]
APP version [--format human|json]
```

- `init` publishes the contract's genesis state into a new store and creates
  its empty submission journal, `DATABASE.submissions`. It refuses an
  existing file.
- `submit` decides one command on the store's current state and commits the
  decision unless it is a reject. The command and context fields are written
  by the names `project.zeno` gives them, as `NAME=VALUE`, or as
  `command.NAME=VALUE` and `context.NAME=VALUE`, which is required when both
  roots have a field of that name. A boolean is `true` or `false` (or 1 or
  0), an integer a decimal number, and a sum a variant's name (or ID). Every
  field must be given once. Each value is checked against its declared
  domain by the decision-examples grammar of `src/examples.rs`, so an
  unknown field, a missing one, a value of no declared form and a value
  outside its declared range are refused with the field's name, and nothing
  is written. The submission is recorded in the journal, and synchronized to
  disk, before the commit. `submit` holds an exclusive lock on the journal
  from before it reads the store's head until the commit's outcome, so
  submissions to one store run one after another; when the commit does not
  happen, for example because another program wrote the store in between,
  its journal line is removed again. A line past the store's head, left by a
  process that stopped before its commit's outcome, is removed by the next
  `submit`. A missing journal of a store at genesis, such as one left by an
  `init` that stopped after creating the store, is created empty by
  `submit`, which is exactly its content. A missing journal of a store with
  commits is refused with exit 3: the store does not hand out the
  submissions the journal would hold, so it cannot be rebuilt. Restore it
  from a backup; or create it as an empty file, after which `submit`
  continues and `history` refuses the commits the journal does not record.
  `init` never recreates a journal, since it refuses an existing store. A
  reject writes nothing.
- `decide` is a dry run of `submit`: the same Authority's decision on the
  current state, printed, with nothing committed or recorded.
- `state` prints the current state by field name, with the head: the
  contract version the store runs, the commits after genesis, the pending
  deliveries and the recorded upgrades.
- `history` prints genesis and every commit with its command, context and
  decision. It reads the submission journal and checks it against the
  store's full read-only audit: every commit position has a submission, its
  contract identity and command/context match the stored inputs, and the
  Authority decides those original inputs again. The audit supplies pre-states
  after each preceding migration or rename, and checks upgrades after the last
  commit too. Original genesis, field names, variant labels and payload names
  use their publishing version's checked schema, including when a store begins
  at a later lineage member. Different journal inputs refuse even if they
  could reach the same final state. A store with commits that `submit` did not
  record, such as one made with `--decide`, is refused, and so is a journal
  whose commit numbers do not strictly increase. `history` takes a shared
  lock on the journal, so it never reads a submission between its journal
  line and its commit. The lock is advisory: it orders the application's own
  commands, not other programs that write the journal.
- `pending` prints how many deliveries are pending and the oldest one, the
  one `deliver` sends next; it delivers nothing.
- `deliver --to FILE` sends every pending delivery, oldest first, to the file `FILE`.
  Each delivery goes through the store's typed lifecycle, `Pending` to
  `Delivered` through the library interpreter, is appended to the file as
  one JSON line, its delivery ID first, the file is synchronized to disk,
  and only then is the delivery acknowledged in the store. The file keeps
  one line per delivery ID: a delivery whose line it already holds, from a
  run whose acknowledgment did not reach the store, is not appended again,
  and a different line under a held ID is refused with the delivery left
  pending. A last line without its newline, left by an interrupted append,
  is removed before the next append. Each delivery is acknowledged before
  the next is sent, so a failure part way through keeps the deliveries
  already sent and acknowledged; its message names them. The file
  destination implements the application's `Destination` trait in
  `src/session.rs`, the extension point for other transports.
- `deliver --relay CONFIG.json` sends the same ordered entries through the
  generated application's Python relay to an HTTP receiver or a queue file.
  Choose one of `--to` and `--relay`. The configuration has schema
  `zeno-fcis/relay-destination/1`, exactly one `http` or `queue` destination,
  and optional `attempts` (1–8, default 5), `backoff` and `max_backoff`
  (0–8 seconds), and `timeout` (0.001–30 seconds, default 10). A relative
  queue path is relative to the configuration file. Python 3 is required.
  Each attempt retains the delivery's original ID and canonical payload.
  The store acknowledges it only after the worker reports success; a
  failure leaves it pending. The worker has a 120-second process deadline.
  Transport is at least once; an idempotent receiver can make retries one
  effect. The worker's success report is trusted shell input, not a proof
  of external settlement. `crash_at: "after-send"` is a test fault that
  leaves a sent entry pending and exercises this retry boundary.
- `version` prints the application's name, its contract version, its
  contract identity and the ZenoFCIS release it was generated for. The
  identity is a domain-separated SHA-256 commitment to the identity bytes of
  the library Authority bound to the current contract version.

Every command that reads a store opens it through the contract lineage, as
`--audit` and `--decide` do, so the Authority of the version the store runs
decides, and a store of another contract, or of a later version of this one,
is refused with `Identity`. `submit`, `decide`, `pending` and `deliver` need
a store at the build's own version and name `--upgrade` otherwise; `state`
and `history` read a store at an earlier version too. The forms without a
command, `NEW_DATABASE_PATH`, `--decide`, `--audit`, `--deliver`,
`--upgrade` and `--migrate`, work as before. A database path that is a
command's name must be written with a directory, such as `./state`.

With `--format json`, each command prints one JSON object, with `status`
first; an error prints `{"status":"error","error":{"code":...,"message":...}}`
and also writes the message to stderr.

| Code | Meaning |
| ---: | --- |
| 0 | completed: a submission committed (an accept or a committed failure), a decision that would commit, or a report |
| 1 | refused: an input the contract does not declare, a store of another contract or version, or another refusal of the store or the Authority; nothing was written, except that `deliver` keeps the deliveries it completed before the refusal, which the message names |
| 2 | the Authority rejected the command; nothing was written |
| 3 | the journal or the delivery destination failed, or the journal of a store with commits is missing; as for 1, `deliver` keeps the deliveries it completed first |
| 64 | the command line is malformed |

## Contract adoption and store upgrades

`contract adopt DIR --candidate C.zcve --receipt R.json --usage
preserved|new-version` makes a checked candidate the next version of an
application's contract. `DIR` holds `project.zeno` and `v2/policy.json`.
The command first regenerates the current contract and replays the receipt
against its decision program and the candidate, exactly as `transform replay`
does; only a byte-identical receipt is accepted. It then regenerates the
contract through the same pure generator as `generate contract`, with the
rules extended by one `adoptions` entry naming the candidate's SHA-256, the
receipt's SHA-256, the claimed usage and the SHA-256 of the superseded
version's policy. The generator re-derives each version's decision program
from the declarations, rules and earlier adoptions, never from a file on
disk, and replays every receipt against it; within one command a receipt it
already replayed is not enumerated again. `--usage preserved` is accepted
only when the receipt reports `usage_preserved: true`; otherwise `--usage
new-version` is required, because the usage observations sealed into each
publication change. Either way the policy bytes, and so the contract
identity, change, and a store that ran the previous version must be upgraded
(below) before the new build commits to it.

Generation also refuses:

- an adoption whose candidate is the program it replaces, and a lineage in
  which a version would repeat an earlier version's policy, and so its
  identity;
- any edit to the declarations or rules that would change a superseded
  version. Each adoption binds the superseded version's policy digest, and
  stores may run that version. Once a version is superseded, only another
  adoption changes the contract; any other change needs a new application;
- an adoption in a contract without decision-conformance law 991, and an
  adoption whose receipt's measured largest Step usage of either program,
  plus one Step for every law node, exceeds that version's Step limit, which
  could then refuse a decision. These are premises of a program successor's guarantee (below).
  Each adoption in the summary reports them as `premises`, with the Step
  usage, law Steps and limits behind them.

Before it writes anything, the command also classifies the change from the
version the adoption supersedes to the new version, with the classifier of
[`contract diff`](#contract-change-classification), and refuses any kind but
`program-successor`, naming the kind and the parts that changed. An adoption
replaces only the decision program, and the Step limit that follows its node
count, so this refusal never meets a valid adoption: it checks the generator
against the classifier.

On success the command writes, in this order, `v2/adoptions/N/program.zcve`
and `v2/adoptions/N/receipt.json`, the generated contract, and last
`v2/policy.json` with the new entry appended in the file's own layout: every
existing byte is kept, and the entry follows the last adoption, or opens a
new last key `adoptions`, one entry per line and indented with the
whitespace that starts the line of the file's first key. Compact files, whose
first key does not start a line, use two spaces for the added entry; existing
bytes and line endings are preserved, and the added entry uses the file's
line ending, `\r\n` included. This is the only command that keeps the
file's layout: `contract refresh-receipts`, when it rebinds a receipt, and
`contract evolve` write the file again in the templates' layout (below). An
interruption leaves the rules unchanged: `generate contract` then regenerates
the previous version, and running the same `contract adopt` again finishes
the adoption, accepting an adoption directory that holds only this candidate
and receipt. Any other existing directory is refused. The generated contract
consists of the current version, `src/v2_contract.rs` and `v2/policy.zcve`,
and every superseded version `k`, `src/v2_contract_vk.rs` and
`v2/policy_vk.zcve`, so `generate contract --check` checks them all. Each
generated contract states `pub const VERSION: u32` and offers
`with_lineage`, the checked catalogs of every version, oldest first. A
contract with adoptions also declares the superseded versions as modules
`v1`, `v2`, .., states `ADOPTION_RECEIPTS`, each adoption's receipt SHA-256,
and passes it to `with_lineage`'s caller with the catalogs. Any refusal,
including an unreplayable receipt, a candidate or receipt whose digest
differs from the entry, a candidate above 64 KiB, or a usage claim the
receipt does not support, exits 1 and writes nothing.

Results use schema `zeno-fcis/cli/1`: `adopted` with the adoption's ordinal,
version, digests, usage, `usage_preserved` and node counts before and after,
or `error` with `contract-invalid` (exit 1), `contract-read-failed` (exit 3)
or `adoption-write-failed` (exit 3).

Each receipt binds the transform checker's semantics version and the
library's evaluator digest, not the checker's source or the crate version, so
a new crate version or a refactor of the checker leaves recorded receipts
valid. After a deliberate change of the checker's semantics version, or of
the evaluator, recorded receipts no longer replay. `contract refresh-receipts
DIR` then checks every adoption again with the current checker, under its
receipt's limits, and requires the new receipt to record exactly what the old
one does in every field but `checker`. It writes the new receipts, the
regenerated contract and last the rules with the new digests, after
generating the result again with every new receipt replayed from scratch.
Those rules are written whole in the templates' layout, two-space indentation
and one entry per line, so a file `contract adopt` kept in its own layout
loses that layout here. It
never reuses the old verdict, and it refuses, writing nothing, when a pair is
no longer equivalent or anything else would change. Running it again
finishes an interrupted refresh. Its result is `refreshed` with the rebound
adoptions, or `current` when every receipt already was this checker's.

An application built from a contract (`new --contract`) operates on an
existing SQLite store with its whole lineage:

| Command | Effect |
| --- | --- |
| `<app> --audit DB` | Replays each history segment under the version that published it and prints the head: the contract version the store runs, commits, pending deliveries and recorded upgrades. A store at any version of the application is audited, under the versions up to its own, so a new build checks an old store before `--upgrade`. The audit opens the file read-only, in one read transaction, and saves no checkpoint: it writes nothing. |
| `<app> --upgrade DB` | Records a checked upgrade from the version the store runs to this build's version. After a full audit, the two versions must have different identities and, unless the lineage declares a data migration or a rename between them (below), equal canonical state schema bytes. When the store's shell establishes all five premises below itself, from the two versions' contracts, the record is a `program-successor`, admitted at any state; it binds the number of input tuples on which the shell compared the two decision programs and the receipt digests this build declares for the adoptions between the two versions. A generated adoption normally establishes all five. When any premise is missing, the new contract's genesis laws must admit the current state through the library's genesis evaluation, and the record is a `genesis-admission` that binds that genesis publication. For generated contracts those laws include the initial-condition law 990, so the store must then be at its declared genesis state; otherwise the refusal names the missing premise. When this build's lineage declares a behaviour change between the two versions, recorded by [`contract evolve`](#rule-changes-for-live-stores), neither route is tried: the record is a `behaviour-change`, admitted when every state law of this build's version and every inductive claim it declares hold on the store's current state, with no program comparison; the refusal names the law or claim that does not hold. When the lineage declares a data migration or a rename between two versions, recorded by [`contract evolve`](#data-migrations-and-renames), the state schemas may differ: the upgrade records one hop per such step, a `migration` admitted by the store's own forward simulation or a `rename` admitted when only names differ, each moving the head's state to the new layout, and one hop for each run of other steps between them, all at the same head in one transaction; the report describes the last hop. Every record binds both identities, the state root and the chain tip, and becomes the next chain link; the head's chain moves to it. Pending deliveries keep their IDs and commit order. The report names the kind. Any refusal writes nothing. |
| `<app> --deliver DB` | Delivers every pending outbox entry of a store at this build's version, acknowledges each and prints their IDs. A store at an earlier version must be upgraded first. |
| `<app> --migrate DB` | Converts a store created before upgrades were recorded (SQLite schema v9) to the current schema v10, after a complete audit under the version that created it. Any other schema, or a store under no version of the application, is refused and nothing is written. Opening or upgrading a v9 store without this step is refused. |
| `<app> --decide DB` | Runs the decision examples as one session and leaves the outbox pending. Without a file at `DB`, the session starts at genesis in a new database, like `<app> NEW_DB`. An existing store must run this build's version; the session continues from the store's state, taking while one remains the first example whose pre-state is that state, and records each commit and its replay under a key of its own. A store at another version is refused and left as it was; one at an earlier version must be upgraded first. It is an aid for acceptance tests and maintenance, not an operational interface, which is planned for 2.2. |

A program successor and the version it supersedes have the same reachable
states when five premises hold. The shell establishes each one from the two
versions' contracts at the upgrade, and again at every full open and audit of
the upgraded store: (1) their canonical policies differ only in the decision
program's instructions and roots and the Step limit, so the genesis literals,
laws and case table are identical; (2) both declare and require law 991 as a
decision-conformance law on every decision, which is the generator's own
test for that law, though the shell cannot see the case table and so does
not check what the law's predicate says; (3) the two decision programs are
equal on every tuple of their declared input domain, which the shell checks
by running both through the library evaluator on every tuple, at a budget no
program can exhaust, up to a cap of 100,000,000 tuples by default; (4)
neither version's Step limit binds, because each covers one Step for every
program and law node; and (5) no law observes Step usage. Under these
premises the two versions admit the same genesis states and commit the same
decisions from the same states, so an upgrade at any reachable state keeps
every property of reachable states: every law, every proved inductive claim
and the meaning of every decision example, not only the laws evaluated on the
current state. Step usage is not kept: when the programs' usage differs, the
sealed usage observations change and every sealed subject names the new
identity, so an adoption is a versioned change.

The upgrade report's `premises` lists the five premises and
`programs_equal_on_input_tuples`, the number of input tuples the shell
compared: 1,296,000 for the withdrawal queue's adoption. That comparison is
an exhaustive finite check by a tested checker, the same predicate as
`zeno-fcis transform check`, not a formal proof. Every process that upgrades
a store, or fully opens a store that holds a program-successor record,
compares the two programs once on every input tuple; within one process the
application binds its lineage once and remembers the outcome, so a second
audit compares nothing again. Two programs with identical instructions and
roots are equal without enumeration, and nothing else is skipped. For the
withdrawal queue, measured on a shared machine as the median of three runs,
one comparison took 8.0 seconds in a release build of the application and
9.5 seconds in a debug build, whose generated manifest optimises
`zeno-fcis-synthesis` while keeping overflow checks and debug assertions
on. A checkpoint open skips the comparisons before its checkpoint. The store comparison remains serial; offline parallel enumeration is available
through `transform check --offline-parallel`. No comparison runs while the store's
write lock is held: the shell establishes what it needs before its
transaction, so other connections can keep writing meanwhile. The receipt digests in the report and the
record are provenance. The shell replays no receipt; it establishes
equivalence itself. An audit requires the recorded digests to be exactly the
ones this build declares, so a build that declares other digests for the
same versions refuses the store. That refusal, and the refusal of a build
whose own contracts or comparison cap do not establish the recorded
succession, names its cause and says the store may be intact; it is not
reported as a damaged history.

Commands on an existing store open it without creating it, so a mistyped path
is refused and leaves no file.

A refusal exits 1 and prints one line on stderr: `store:`, the shell's
message saying what happened and what to do, and in parentheses the error's
`Debug` form, which starts with the variant name that scripts may match, as in
`store: upgrade refused: the store already runs this contract version, so
there is nothing to upgrade (Upgrade(SameContract))`.

The library crate exposes the same operations as typed handles.
`v2::Lineage::bind(catalogs, receipts)` binds a lineage of adoptions, with
the default comparison cap, and `Lineage::bind_with_cap` with another one;
`Lineage::bind_steps(catalogs, steps)` binds one whose `v2::Step`s may also
be behaviour changes, each with its review text and claims, data migrations
(`v2::migration::Migration`) and renames;
`Lineage::audit_read_only` audits a store at any version without writing;
`Lineage::open`
returns either a v9 store, whose only operation is `migrate`, or a v10 store
that is current or superseded. `Superseded::upgrade` consumes its handle and
returns the current one with the `UpgradeReceipt`. The pure
`v2::upgrade::decide`, `v2::upgrade::Successor::establish` and
`v2::equivalence::compare` make the decisions, `v2::behaviour` evaluates
a behaviour change's laws and claims, and `v2::migration` simulates a
migration and checks a rename; the `zeno-fcis` binary itself does not open
stores.

## Rule changes for live stores

`contract evolve DIR --to NEW` replaces the contract of the application in
`DIR` with the contract in `NEW`, a directory holding `project.zeno`,
`v2/policy.json` and optionally `tests/decision-examples.txt`, after the
owner's review, and regenerates the application's whole lineage. Without
`--migration`, a rule change takes this path as a behaviour change,
described here, and a rename as the exact rename tier; with `--migration`, a
layout change or a rule change takes it as a data migration. Both of those
are described in [Data migrations and renames](#data-migrations-and-renames).

1. The command generates both contracts and classifies the change from the
   application's current version to `NEW` with the classifier of
   [`contract diff`](#contract-change-classification). A kind no path takes
   is refused, naming the kind and the changed parts: a program successor is
   adopted with `contract adopt` instead, a layout change needs
   `--migration`, a rename takes none, and an identical or unrelated
   contract has no path. A rule change keeps the schema and the channels
   byte for byte, so a store keeps its state as it is. `NEW` may not list
   adoptions or evolutions of its own; adopt in the application afterwards.
2. The classifier's plain-language account of the change is the owner's
   review. It is written as `v2/evolutions/N/review.txt`, one line per line of
   `contract diff`'s human output, with `NEW`'s first version numbered after
   the application's versions.
3. The replaced contract is kept under `v2/evolutions/N/`: its
   `project.zeno`, its `v2/policy.json` without `evolutions`, and its
   adoptions' retained files, which move there from `v2/adoptions/`. Stores
   may run any of its versions.
4. The new `v2/policy.json` is `NEW`'s rules with an `evolutions` list: one
   entry per replaced contract, oldest first, binding the SHA-256 of that
   contract's last policy and of the review, and for a rename or a migration
   its `kind`. It is written in the templates' layout, two-space indentation
   and one entry per line, whatever the layout of either policy file.
   `NEW`'s `tests/decision-examples.txt`, when present, replaces the
   application's.

Generation, by `contract evolve` and by every later `generate contract`,
regenerates every version of every replaced contract from its kept files,
numbers the whole lineage from 1, and requires each replaced contract's last
policy to be the one its entry binds. It also computes each review again from
the two contracts and requires it to be byte for byte the kept
`review.txt` and the digest the entry binds, so neither the review nor
either contract can be edited afterwards. The current version's
`src/v2_contract.rs` then declares every earlier version as a module, lists
`ADOPTION_RECEIPTS`, every adoption of the lineage in order, and
`EVOLUTIONS`: for each behaviour change, the version it follows, the review
text, included from its file, and the inductive claims of the contract it
leads to, each compiled to a law program over the state. `with_lineage`
passes all three. A contract that never evolved is generated exactly as
before.

On success the command writes, in this order: the kept contract and the
review, the generated lineage, the examples, `project.zeno` when it changed,
and `v2/policy.json`; last it removes `v2/adoptions/`, now kept under the
evolution. Any refusal exits 1 and writes nothing. An interruption before the
rules are written leaves the application at its contract and an evolution
directory that the command refuses until it is removed. An interruption
between `project.zeno` and the rules leaves the two inconsistent; generation
then refuses them until they are restored, for instance from version control.

Results use schema `zeno-fcis/cli/1`: `evolved` with `kind`, the
classifier's kind of the change, `evolution` (`ordinal`, `directory`,
`from_version`, `version`, `path`: `behaviour-change`, `rename` or
`migration`, `superseded_policy_sha256`, `review_sha256`, `claims`,
`review_path` and, for a migration, `migration`), `review`, the review's
lines, `artifacts` and `summary`, whose `evolutions` lists every evolution
of the lineage with its `kind`; or `error` with
`contract-invalid` (exit 1), `contract-read-failed` (exit 3) or
`evolution-write-failed` (exit 3). `contract refresh-receipts` rebinds only
the current contract's receipts: a replaced contract's receipts must replay
as they are.

The store follows with the application's `--upgrade`, which records a
`behaviour-change`. The SQLite shell admits it when, on the store's current
state:

- every state law of the new version holds: each law that applies at
  genesis and to every committing decision, other than an
  `InitialCondition` law; and
- every inductive claim the new contract declares holds, each restated over
  the state as `prove` restates its post-state. All inductive claims are
  checked, whether or not `prove` ran for them.

The shell evaluates them with the library's law evaluator through the
verified core's genesis framing, over the new contract's descriptor with
every other law's predicate replaced by `true`, so that the core decodes the
state under the new contract's schema and evaluates each law and claim on
it. No decision programs are compared, so the upgrade waits on no
comparison. Genesis exactness, law 990, applies only to new stores: it is
not evaluated, and the record lists it, with every other law that applies at
genesis and is not a state law, as not evaluated. The record, magic
`ZFCISV2-BEHAVIOUR`, binds the law and claim IDs that held, the laws not
evaluated and the SHA-256 of each review between the two versions; the
upgrade row stores the review texts beside it. An audit evaluates the laws
and claims on the recorded state again and requires the stored texts to be
the reviews this build declares; a build that declares other reviews, or an
adoption, for the step refuses the store and says it may be intact.

The upgrade report adds `behaviour`: `state_laws_held`, `claims_held`,
`not_evaluated` and `reviews`, the review digests.

What the upgrade establishes, at its true strength:

- After the upgrade, every law holds on every later committed state, because
  the Authority checks the laws at every commit.
- Each inductive claim whose induction step holds, as `prove` attests it
  for the laws the claim assumes, holds from the upgrade on: it holds at the
  upgrade state, and every committed step preserves it. The upgrade itself
  checks only the upgrade state; the step is `prove`'s.
- Facts that rest only on reachability from the new contract's genesis do
  not carry over, because the history was made under the old rules. A store
  may hold a state the new contract could never reach from its own genesis;
  only the laws and claims checked on it are known to hold there.

`tools/check_contract_evolve.py` runs the study's change end to end: an
escrow store with committed history, whose 14-day window refused a dispute
14 days and one second after shipping, evolves to the 30-day window of
`crates/zeno-fcis-cli/tests/fixtures/escrow-dispute-30`, is audited
read-only by the new build, upgrades, and keeps committing: the same dispute
is accepted and the split's payouts are delivered.

## Data migrations and renames

`contract evolve DIR --to NEW --migration m.json` takes a change that
alters the state's layout, or a rule change, as a data migration: the
migration file `m.json` maps the application's current state to `NEW`'s,
and the change is admitted only by forward simulation. Without
`--migration`, a `rename` is taken as the exact rename tier and needs no
file. Everything else in [Rule changes for live
stores](#rule-changes-for-live-stores) applies: the classification first,
the review, the kept contract, the regenerated lineage, and nothing written
on a refusal.

The migration file, schema `zeno-fcis/migration/1`, gives every field of the
new state record a value from the old state, by field ID:

```json
{
  "schema": "zeno-fcis/migration/1",
  "state": {
    "120": {"from": 120},
    "121": {"from": 121},
    "124": {"default": false},
    "125": {"from": 120, "map": {"150": 0, "151": 1, "152": 2, "153": 2, "154": 2}}
  }
}
```

- `{"from": f}` carries old field `f` over, under its own name or a new
  one: a carried field or a rename. Its kind (Boolean, integer or variant)
  must not change.
- `{"default": v}` gives an added field the value `v`.
- `{"from": f, "map": {..}}` takes the value from a table over old field
  `f`'s values, which must list every one of them: one part of a split. Keys
  are decimal integers, `false` or `true`.

Values are written as the rules file writes them: `true` or `false`, an
integer in the field's range, or a variant's ID. Every new field must be
given a value. Version 1 carries every old field into at least one new field
and retains its original state-only grammar and encoding. Version 2 also allows
`"drop": [FIELD_IDS]` for old state fields that no new field uses. Unknown,
duplicate or still-used drop IDs are refused. Complete forward simulation must
preserve decisions, reasons, mapped successors and ordered deliveries; an
explicit drop alone does not admit a migration. Historical segments keep the
removed values and replay under their original contracts. Payload maps retain
every old field and stay injective. Text fields have no finite domain and cannot
be migrated.

For state expressions and payload representation changes, use schema
`zeno-fcis/migration/2`. Its optional `payloads` object maps channel IDs to
complete new payload records using the same field forms. Each changed payload
needs an explicit map. `{"expr":"old.145 >= 2"}` reads only its source
record's field 145: the library's closed finite program computes Boolean or
checked i64 results with addition, subtraction, comparisons, Boolean operators
and `choose`. Multiplication, division, text and external inputs are refused.
All source valuations must evaluate successfully into the declared target
domain, including those excluded by state laws.

Payload maps must be total and injective on the complete old payload domain,
with at most 65,536 records and 64 MiB of retained canonical mapped bytes.
Partial, lossy, trapped and over-cap maps are refused before writes. Each
future new publication must carry exactly the mapped old payload, with the
same lane, ordinal, channel, destination and idempotency. Pending deliveries
retain their original payload bytes, policy identity, entry hash and delivery
ID; after upgrade they are replayed and acknowledged under their historical
contract. This declared representation relation still requires owner review
of units and interpretation. It is not an independent proof of business
semantics. Payload shortcuts are checked against the full composed route,
including maps on otherwise unchanged payload schemas.

**Admission by forward simulation.** `contract evolve` and every later
`generate contract` compile the file against both contracts and run the
SQLite shell library's own simulation over the application's last version
and `NEW`'s first, through the
library's bound Authorities:

- It enumerates every state of the old contract's declared state domain and
  keeps those on which every state law of the old contract holds, as the
  library's law evaluator decides through the core's genesis framing. A
  store's state satisfies them however it got there: its genesis and every
  commit are checked against them, a behaviour change checks the new laws
  on the state it keeps, a rename keeps the laws, and the third item of
  this list makes a migrated state satisfy the new laws. So the kept states include
  every state a store can hold.
- For each kept state `s`: when the old contract's genesis evaluation admits
  `s`, the new contract's must admit `m(s)`. For a generated contract, whose
  law 990 admits only its declared genesis state, this says that `m` maps
  the old genesis state to the new one.
- For each kept state `s`, every state law of the new contract must hold on
  `m(s)`, observation `new-state-laws`. This holds even for a state no
  commit of the old contract leads to, for instance one a behaviour change
  kept; it is checked here, over every kept state, rather than on the
  store's head at the upgrade, and every upgrade and audit re-runs it.
- For each kept state and every command and context of the declared domain,
  the new contract's publication over `m(s)` must give the same
  observations as the old contract's over `s`: the same technical refusal,
  or the same decision class and reason; the same deliveries in order, each
  with its lane, ordinal, channel, destination and idempotency exact, and
  payload bytes exact or related by the admitted explicit representation map;
  and for a commit a successor
  state equal, byte for byte as the store would hold it, to `m` of the old
  successor. Matching successor states alone admit nothing.

The simulation runs only when the old contract's whole input domain, every
state, command and context, has at most 2^20 = 1,048,576 tuples, the cap of
[`contract review`](#contract-review). A larger domain is refused as
inconclusive: no sampling, boundary set or solver result stands in for the
enumeration, so solver evidence is not accepted for a migration. A refusal
names the observation that differs, or the field without a value in its
domain, and the input where it happened, as field values; a difference is a
behaviour change, which takes the path above without `--migration` when the
layout is unchanged.

What an admitted migration establishes, at its true strength: every state
the old contract's state laws allow maps to one the new contract's state
laws allow, and from a mapped state the new contract makes the same
decisions, sends the same deliveries and reaches the mapped successor. It covers those observations
and no others: sealed identities, certificates, the delivery IDs of later
commits and Step usage may differ. It rests on the enumeration of a finite
declared domain through the library evaluator, which the shell repeats at
every store upgrade and audit; it is not a proof about other domains.

The migration is kept as `v2/evolutions/N/migration.json`, and the rules
entry binds its SHA-256 with `"kind": "migration"` and `migration_sha256`;
generation refuses an edited file. The current version's
`src/v2_contract.rs` declares `STATE_STEPS`: each migration, field by field,
and each rename, after the version it follows; and a superseded version
whose schema a migration or rename changed keeps its own schema file,
`v2/schema_v{k}.zcve`. A lineage without a migration or rename is generated
exactly as before. The report's `evolution.migration` gives the file's
`sha256`, `path`, the old contract's `states`, the
`states_satisfying_state_laws`, the `genesis_states` among them, the
`tuples_compared` and the `observations` compared.

**Routes and shortcuts.** Migrations are built between consecutive
versions, and an upgrade across several applies each in turn. `--shortcut
FILE`, repeatable, adds a migration file with `"from_version": k` that maps
version `k` directly to `NEW`. Version `k` must be the last version of an
earlier contract this lineage evolved from, and every evolution since must
be a migration or a rename, the new one included. The shortcut is admitted
only when it agrees with the composed route of those consecutive steps: on
every state of version `k`'s declared domain on which its state laws hold,
and for every command and context, `NEW`'s publication over the shortcut's
state gives the same observations as over the route's state, and the two
states are the same. An agreeing shortcut is kept as
`v2/evolutions/N/shortcuts/from-{k}.json`, bound in the rules entry's
`shortcuts`, and checked again by every generation. A store upgrade applies
the composed route, which the shortcut equals on every state checked.

**Renames.** A `rename` changes only profile, type, field or variant names:
the new policy, with the old schema and its commitments in place of its
own, is byte for byte the old policy. `contract evolve` without
`--migration` takes it, and the rules entry records `"kind": "rename"`.

**Store upgrades.** The application's `--upgrade` records one hop per
migration or rename step:

- A `migration` record, magic `ZFCISV2-MIGRATION`: the store's shell runs
  the same forward simulation itself, from the two versions' bound
  catalogs, before it takes the write lock, and keeps the outcome for the
  lineage value. It then migrates the store's current state, checks the target
  state laws and freshly compiled target claims on that state, and moves the
  head to it. Old-schema claim programs are not reused across the migration.
  These current-state checks establish the admitted base, not a new theorem
  that the claims hold on every future state. The record binds the SHA-256 of the migration's canonical
  encoding (the shell's own, which `generate contract` compiles from
  `migration.json`), the simulation's state, admitted-state, genesis-state
  and tuple counts, the observations compared and the migrated state's
  root; the upgrade row stores the encoding and the migrated state. With
  nonempty target claims, format 2 also binds the evaluated claim programs,
  IDs, state root, checked laws, unevaluated laws and owner review. Changing
  a claim body while keeping its ID requires different admission evidence.
  Empty target claims retain the existing format-1 record bytes.
- A `rename` record, magic `ZFCISV2-RENAME`: the shell checks from the two
  catalogs that only names differ, frames the state's payload again under
  the new schema, and moves the head to it, at any state. The record binds
  the new state's root, and the row stores the state.

An audit re-derives each record: it re-runs the simulation, once per
lineage value as the program comparison is, migrates or re-frames the
recorded state again, and requires the stored encoding and state to be
exactly those. The simulation is re-run on every open of a migrated store,
not only in an explicit audit command, so that no open trusts a recorded
count, and it is kept per lineage value, as the program-successor
comparison already is. Its cost grows with the domain: about 3 seconds per
open of the spend-approval store (14,592 tuples) in a debug build of the
generated application and about 15 seconds per `contract evolve` or
`generate contract` in a debug CLI. Scaled linearly to the cap of 2^20
tuples, which no fixture approaches and which was not measured, that would
be minutes per open and per generation; a contract with a domain near the
cap pays that on every `--decide`, `--deliver` and `--audit` of a migrated
store. A build that declares another migration, or another kind of step, for
those versions refuses the store and says it may be intact; a stored state
that is not the migration's is damage. Pending deliveries keep the IDs
their commits bound, and each segment replays under its own contract.

The upgrade report adds `migration`: `sha256`, the encoding's digest,
`states`, `states_satisfying_state_laws`, `genesis_states`,
`tuples_compared`, `observations`, `state_root` and `claims_held`.

`tools/check_contract_migrate.py` runs the spend-approval migration end to
end: two version 1 stores with committed history, one with a payment
pending, are audited read-only by the migrated build and upgraded; the
pending payment is delivered with its original ID, the other store keeps
committing under version 2, and a later rename upgrades the first store at
its executed state, after which all three segments replay. An executed
spend-approval request is final, so the two properties are shown on two
stores. It also shows each refusal: a broken delivery, a changed decision, a
genesis that does not map, a disagreeing shortcut, a domain above the cap
and a layout change without a migration.

## Contract change classification

`contract diff OLD NEW` reads two contract or application directories, each
holding `project.zeno`, `v2/policy.json` and any adoptions, and generates
each one's current version exactly as `generate contract` does, replaying
every adoption receipt. It then decides exactly one kind of change from OLD
to NEW, by comparing the two canonical policies and schemas and the structure
the generator built them from, and names every changed item. It writes
nothing. It decides only the structural kind: it runs no decision, so it
never shows that a rule change preserves or changes decisions; the kind's
admission path checks that.

The kinds are tried in this fixed order, and the first whose condition holds
is the kind of the change:

| Kind | Condition | Admission path |
|---|---|---|
| `identical` | The canonical policies are byte-identical, so the contract identities are equal. | None is needed. |
| `program-successor` | NEW's canonical policy, with its decision program's instructions and roots and its Step limit replaced by OLD's, is byte for byte OLD's policy. This is premise 1 of the program-successor upgrade, the comparison the SQLite shell makes. | A `transform` equivalence receipt and the program-successor upgrade, admitted at any state. It exists today. |
| `rename` | Both schemas declare the same types, fields and variants, with the same IDs and forms; some project, type, field or variant name differs; and NEW's policy, with OLD's schema and schema commitment in place of its own, is byte for byte OLD's policy. | The rename tier, admitted at any state; it exists today: [`contract evolve`](#data-migrations-and-renames) without `--migration`. |
| `layout-change` | The state or payload layout differs, while commands, context, destination roots and channel IDs are unchanged. Shared protected and unrelated types stay unchanged. Names, laws, cases, reasons, the genesis state and the decision program may differ too. | A data migration, admitted by forward simulation over the old contract's whole declared input domain; it exists today: [`contract evolve --migration`](#data-migrations-and-renames). |
| `rule-change` | The schemas are byte-identical and so are the channels. Since neither of the first two kinds holds, the policies differ in more than the decision program and its Step limit: in laws, cases, reasons or the genesis state. | The behaviour-change upgrade, which exists today: [`contract evolve`](#rule-changes-for-live-stores) records the change with this account, and a store upgrades when every state law of the new contract and every inductive claim it declares hold on the store's state. Forward simulation also exists today, for a rule change that preserves every decision: [`contract evolve --migration`](#data-migrations-and-renames). |
| `unrelated` | Anything else. | Refused. |

The terms:

- **The state layout** is the state record, type 100, with each field's ID
  and type, and the form of every type a state field has: a boolean, an
  integer range, a text length range, or a sum's variant IDs. Names are not
  part of it. State and payload layouts may evolve together. Their changed
  leaf types must not also occur in command, context or destination roots.
- **Protected types** are command, context and destination roots and their
  field types. Changes to state or payload types shared with these roots are
  refused. Unrelated types remain unchanged.
- **The channels** are each channel's ID, destination type and payload type.

So `unrelated` covers: a channel added, removed or given another destination;
a change to the command, the context, a destination, a protected shared type
or an unused type; and names changed together
with laws, cases, reasons, the genesis state or the decision program, which no
single admission path takes. A renaming and another change can be two
versions instead. A type's form includes its range: widening an integer type
that both a state field and a command field have changes the command too, so
the change is `unrelated`.

The JSON document, schema `zeno-fcis/contract-diff/1`, depends only on the two
contracts' files, not on where they are: the same command, or the same
contracts copied elsewhere, prints the same bytes. It holds:

- `kind`, and `precedence`, the six kinds in the order above;
- `parts`, the parts of the contract that changed, in this order: `names`
  (a name of an item on both sides), `state` (the state layout), `interface`
  (every other type), `channels`, `reasons` (a declared reason or the class
  its cases give it), `genesis`, `laws` (a compiled law, declared or
  generated), `cases`, `program` (the decision program), `step-limit` and
  `limits` (the read, write, byte or effect limit);
- `old` and `new`: each version's application, its version number in its own
  lineage, the SHA-256 of its policy, schema and decision program, the
  program's node count and every limit;
- `lineage`: `old_in_new`, the version of NEW's lineage whose policy is OLD's
  current policy, with `receipts`, the receipt digests of NEW's adoptions after
  it; and `new_in_old`, the earlier version of OLD's lineage that NEW is,
  when it is one;
- `admission`: whether one is `needed`, whether the change is `refused`, and
  the `paths`, each with its `id` (`f6.1-program-successor`, `g2-rename`,
  `g2-migration`, `g2-forward-simulation` or `g14.1-behaviour-change`), its
  `feature` (an internal label for the capability), whether it
  `exists_today` (every path does), and `when` it applies. For a
  program successor, `ready` says that NEW's lineage holds OLD below its
  current version, so the receipts exist and the application's `--upgrade`
  admits a store at OLD; when NEW is instead an earlier version of OLD's
  lineage, no store goes back to it. `text` says the same in words;
- `changes`, every changed item of the contract, and `notes`, every
  difference that is not part of the contract. Each entry has an `item`, an
  `id`, a `change` (`added`, `removed`, `renamed` or `changed`) and a
  one-sentence `text`, with `old` and `new` values where they apply and, for a
  changed case, law or program, the `aspects` that changed;
- `summary`, the human output's lines.

| `item` | `id` | Changes |
|---|---|---|
| `profile` | empty | The project's name, `renamed`. |
| `type` | the type ID | `added`, `removed`, `renamed`, or `changed` in form. |
| `field` | `TYPE.FIELD` | `added`, `removed`, `renamed`, or `changed` to another type. Its text says whose field it is: the state's, the command's, the context's or a payload's. |
| `variant` | `TYPE.VARIANT` | `added`, `removed` or `renamed`; the text names the fields that have the sum, such as the command field whose variants are the command set. |
| `channel` | the channel ID | `added`, `removed`, or `changed` destination or payload type. |
| `reason` | the reason ID | `added`, `removed`, or `changed` from one class to another. |
| `genesis` | the state field ID | `changed` value. |
| `law` | the law ID | `added`, `removed`, or `changed`, with the aspects `formula`, `kind`, `scope`, or `program` when only its compiled program changed, as the generated laws 990 and 991 do when the genesis state or the cases change. |
| `case` | the old index; the new index of an added case | `added`, `removed`, or `changed`, with the aspects `when`, `class`, `reason`, `post` and `outbox`. Cases are aligned in order by their content, with every variable replaced by its definition, along a longest common subsequence; within each run of unaligned cases the i-th old case pairs with the i-th new one, and the rest are removed or added. |
| `program` | empty | `changed`, with the aspects `instructions`, `inputs` and `outputs`; `old` and `new` are the node counts. |
| `limit` | `read`, `write`, `byte`, `effect` or `step` | `changed`. |

The notes cover the application name, law, reason and channel names, reason
precedences, the `rule` notes of aligned cases, rule variables (a removed and
an added variable with one definition are reported as one renamed variable)
and claims. None of them reaches the schema or the policy.

The human output is the same account: the kind and both versions, what the
kind means, the admission line, then one line per change and per note.

Exit codes: 0 when classified, whatever the kind; 1 when a contract is
invalid, as `generate contract` would refuse it, and 3 when a file cannot be
read, both reported with schema `zeno-fcis/cli/1` and a `side`, `old` or
`new`, and nothing written; 64 for usage.

The classifier calls the shell library's `v2::upgrade::program_successor`
and `v2::migration::rename_exact` functions directly. The CLI's unit tests use
the same store library and check,
on every planted pair of
`crates/zeno-fcis-cli/tests/fixtures/contract-diff/pairs.json` and on both
adoptions, in both directions, that the shell's comparison accepts exactly
the pairs the classifier calls `identical` or `program-successor`, and that
every pair the shell's program-successor (Tier A) admission admits is a
`program-successor`. The converse does not
hold: Tier A also needs law 991 in both contracts, both Step premises and the
two decision programs' equivalence, none of which the classifier checks. That
is evidence on those pairs, not a proof for all contracts.

## Delivery relay

Two scripts in `tools/` deliver a store's pending entries to an external
system. Both use only the Python standard library. The protocol and its
strength are described below.

```sh
python3 tools/relay.py --export-command "APP --relay-export DB" \
    --ack-command "APP --relay-acknowledge DB" --http URL
python3 tools/relay.py --export-command "..." --ack-command "..." --queue FILE
python3 tools/relay_receiver.py --record LEDGER.json [--port 0]
```

`tools/relay.py` runs the export command, which prints one line of JSON per
pending delivery in commit order (`zeno-fcis/relay-export/1`), and makes one
outbound call per delivery, in order: an HTTP POST to `--http URL` with the
delivery ID as the `Idempotency-Key`, or a line appended to `--queue FILE`.
Every attempt sends the same bytes. `--timeout` (default 10 seconds) limits
the whole HTTP call, from connecting to the last byte of the answer, not each
socket read: when it passes, the relay shuts the connection down and the call
counts as timed out, even if the receiver was still answering. The relay
connects directly to the URL's host and reads no proxy settings; `--http`
must be an `http://` or `https://` URL. A call that fails or times out is
retried up to `--attempts` times (default 5), waiting
`--backoff` seconds (default 0.5) before the second attempt and doubling up
to `--max-backoff` (default 8). After a successful call it runs the
acknowledgment command with the delivery ID and the SHA-256 of the payload it
sent appended. It prints a JSON report and keeps no state of its own.

| Exit | Meaning |
| --- | --- |
| 0 | Every exported delivery was sent and acknowledged. |
| 2 | Usage error, or the export is malformed (including a payload whose SHA-256 is not its `payload_sha256`); nothing was sent. |
| 3 | A delivery stays pending: its last attempt failed or timed out, or the receiver refused it for good (a 4xx answer other than 408 and 429). Later deliveries were not sent. |
| 4 | The export command failed, or the store refused an acknowledgment. A store command still running after `--command-timeout` seconds (default 120) is killed and counts as failed; an acknowledgment cut off this way may or may not have committed, and the next run finds the delivery acknowledged or sends it again. A refusal as `AlreadyAcknowledged`, by an acknowledgment another relay made first, counts as acknowledged. |
| 75 | An injected crash, from `--crash-at after-export`, `after-send` or `after-acknowledge`, with `:N` for the Nth time; for restart tests. |

Transport is at least once: a relay stopped before an acknowledgment sends
that delivery again on its next run, with the same ID and bytes. A receiver
that honours idempotency keys sees each effect once. A queue file may hold
the same line twice. A short append is cut back to the file's previous size,
and an append to a file that does not end in a newline (a write stopped
part-way) starts with one, so a cut line does not join the next. The consumer
must keep the first line per `delivery_id` and skip any line that is not
complete JSON.

Only a 2xx answer to the POST itself counts as sent. The relay follows no
redirect: a 3xx answer is a final refusal (exit 3, the delivery stays
pending), because urllib would follow a 301, 302 or 303 with a GET that has
no body.

`tools/relay_receiver.py` is the test receiver: an HTTP server that records
each new idempotency key as one effect, answers a repeat with the same body
200 and a repeat with another body 409, and keeps its ledger in `--record`
across restarts (`GET /` returns it). `--fail-first N` answers the first N
requests 503 without recording them; `--hang-first N` makes the next N wait
`--hang-seconds` (default 5) before answering, recording the effect first
with `--hang-mode after` (the default) or nothing with `before`. It prints
`listening http://HOST:PORT` when ready. It is a test fixture, not a
production receiver.

The withdrawal-queue template's application has the two commands the relay
needs:

| Command | Effect |
| --- | --- |
| `withdrawal-queue --payouts NEW_DB` | Makes the demonstration's sixteen decisions in a new database and stops before delivery, leaving both payouts pending. |
| `withdrawal-queue --relay-export DB` | Prints the relay export of the store and writes nothing. |
| `withdrawal-queue --relay-acknowledge DB ID SHA256` | Acknowledges one delivery by its ID and the SHA-256 of its payload, both 64 lowercase hexadecimal digits. Refuses `UnknownDelivery`, `PayloadMismatch` and `AlreadyAcknowledged` by name, exiting 1, and changes no delivery. |

## Bounded completion in 1.1.0

`zeno-fcis describe synth completion` lists the exact options and filesystem
effects of `discover`, `find`, `verify` and `replay`. These commands use the
closed `zeno-fcis/completion-problem/1` JSON schema and existing finite-i64
instructions; they never execute supplied code. See
[bounded completion](BOUNDED_COMPLETION.md) for commands and evidence limits.
The `prepared-counter` template connects preparation to independently checked
nominal authorization and atomic state/outbox publication.


## Examples-first contract drafting

`contract draft` is a local authoring conversation with explicit operations.
`start` captures a plain-language intent file, fixed `project.zeno`, optional
initial [decision examples](CONTRACT_RULES.md), and supplied provenance. An
agent or owner then supplies a **complete rules file** with `propose`. There
is no hosted model, automatic prose interpretation, or automatic labeler.
Every response is JSON (`zeno-fcis/contract-draft/1`) with `authority: none`
and `hosted_model: off`. Use the returned `revision` for the next mutation.

`propose` reserves and saves an attempt before running the existing contract
generator (`generate contract`) and library binding. Invalid rules, refused
assessments, raw proposal text, provenance and the preceding revision remain in
`draft.json`. The first
bindable proposal fixes the canonical schema and program input order for later
rounds. Changing that shape requires a new session. Rules with existing adoption
or evolution histories need the existing lineage workflows, not this new-contract
workflow. `questions` returns the current review packet, including its decision table,
distinguishing inputs and proposed answers. **A proposed answer is not a label.**

Supply expected decisions through `label --examples FILE --provenance TEXT`,
bound to the current revision. Each line uses the existing shared grammar:
`inputs | class reason poststate | deliveries`. Inputs are canonical program
positions (state, command, context); Boolean/integer/sum observations are supported.
Rejects repeat the prestate; deliveries name channel and numeric payload fields.
The comparator checks class, reason, every poststate field, delivery count, channel
and payload. Destination bytes and delivery identity are not observations in this
grammar. Neither the parser nor an agent's claimed provenance authenticates an owner.
Initial and additional labels are assumed requirements, retained without overwriting
older labels, and rechecked against every later proposal. Contradictory labels need
an explicitly revised new session; this version does not retract labels.

`check` regenerates the contract, recomputes the review and exits 1 unless at least
one parsed supplied label exists, every current distinguishing input is labeled,
and no comparison or review finding fails. Newly supplied examples can change
the review's probe bases and produce more
questions; repeat `questions`, `label`, or `propose` until ready. `finalize` applies
the same fresh check at the specified revision and writes a new contract directory:
project/rules/examples, generated schema/policy/Rust, intent, complete transcript,
and `draft-finalized.json`. The report lists each compared label and difference,
question coverage, assumed provenance, and the full advisory review packet. A proposal
operation exiting 0 only means the assessment ran, not that finalization is ready.

Limits are fixed at start: 1–8 proposal rounds (default 4), 1–4096 review tuples per
assessment (default 4096), at most 1024 combined decision examples and 1024 label
submissions, 1 MiB per source/provenance and combined examples, and 16 MiB per
serialized transcript. A malformed proposal consumes a round. The final permitted
proposal may finalize if ready; an unresolved exhausted session cannot. Missing
labels, stale revisions, invalid rules, interrupted assessments and failed checks
never finalize. A process interrupted after reservation retains a consumed pending
attempt; submit another proposal if budget remains. A process crash can leave
`draft.lock` (or a sibling output publication lock); recover it only after checking
that no operation still owns it. Saved sessions are local mutable tooling data,
not tamper-proof records or authentication credentials.

Finalization writes into an exclusively owned sibling staging directory and renames
it only after all files and the receipt succeed. A write failure exposes no target
and cleans only that staging directory. Existing targets, including empty directories
and dangling links, are refused. All draft publishers serialize on a canonical-parent,
output-name lock. **Other tools must not concurrently change that output name or its
parent**: portable filesystem rename lacks a no-replace primitive; this is ordinary
cooperating-tool concurrency, not a hostile-filesystem guarantee.

After separate intent review, `zeno-fcis new APP --contract NEW_DIR --source TREE`
uses the existing scaffold. A finalized draft grants no runtime Publication,
adoption or deployment approval. The review remains bounded, advisory evidence relative to
supplied examples and declared laws; undistinguished mutants, unprobed inputs,
reachability and omitted requirements are unresolved, not proved correct.

The drafting fixtures in `crates/zeno-fcis-cli/tests/fixtures/contract-draft/` expose three
intent summaries and rewritten complete proposals. They preserve the initial
example files' recorded review provenance. Extra test labels come from a simulated
supplier evaluating the retained original template, never from the submitted
proposal. Fixture replay and generated-app tests do not establish a live owner
interaction or correct automatic translation of prose.


### Requirements-only version-space questions (2.4.0)

```sh
zeno-fcis loop question --specification FILE [--max-work N] [--max-domain N] \
  [--max-meanings N] [--max-cells N] [--max-conflict-subsets N]
```

This is an alternative to `loop question --contract DIR [--answers FILE]`.
The specification mode reads no prior core and refuses `--contract` or
`--answers` together with `--specification`. Existing contract mode is
unchanged. The bounded ASCII JSON document (64 KiB maximum, unique properties,
integral numbers) has exactly these fields:

```json
{
  "schema": "zeno-fcis/loop-version-space/1",
  "inputs": [{"name": "x", "domain": {"kind": "Int", "min": 0, "max": 3}}],
  "outputs": [{"kind": "Bool"}],
  "sketch": ["x < ?c[0..4]"],
  "requirements": [{"id": "start", "input": [0], "output": [true]}]
}
```

A law is `{"id":"law_name","law":"out0 == (x < 2)"}` and applies to
every input. An example names a complete input and output tuple. Names and
requirement IDs are unique; at most 16 independent holes, 16 outputs,
32 input/output fields combined and 64 requirements are supported.
The grammar is the loop expression grammar with finite constant/choice holes
and strict kinds. No `keep`, parameters or multiplication are supported.
An unbounded integer constant or expression hole returns inconclusive.

Reports carry `authority: none` and `attempts_spent: 0`. `question` contains
an exact proposed count, the chosen input and all output buckets with survivor
counts. `determined` contains a single representative, its full-domain law
check and replayed transform receipts. `no-completion` contains a minimum
conflict relative to this grammar, with lexical ID ties. Counts and conflicts
are labelled proposals. Any budget/deadline stop exits 2 with no count,
question or candidate; a malformed declaration or failed exact final check
is refused. A unique restricted meaning does not establish full intent.

Use `terminal.py loop init --specification FILE --workspace W.json` for a
conversation. Existing `loop answer QUESTION N` confirms an offered tuple;
`loop answer QUESTION '[values]'` confirms any complete in-domain output
(including one outside the sketch). `loop stop` retains the requirements.
Model `version-suggest` acts remain advisory. There is no MCP owner-answer path.
See [ZAL loops](zal/LOOPS.md#requirements-only-version-space-questions) for
budgets, both surfaces and explicit nonclaims.

<!-- BEGIN black-box capture reference: additive block -->

### Black-box capture (2.4.0)

```text
zeno-fcis loop capture open --spec FILE --reference FILE --session NEW_DIR
  [--reference-format python-function-v1] [--max-tuples 4096] [--rounds 32]
  [--max-examples 4096] [--node-visits 2000000]
zeno-fcis loop capture packet --session DIR
zeno-fcis loop capture candidate --session DIR
  (--candidate-json FILE | --expressions FILE)
zeno-fcis loop capture status --session DIR [--full]
```

`open` is owner-side. It accepts Python 3.11+ source defining
`reference(point) -> list[int]`, and an ABI object containing exactly `inputs`
and `outputs` in the finite-program fixture vocabulary. Six observations per
input freeze a complete successful table inside the existing contained runtime.
Crashes, timeouts, malformed labels and observed nondeterminism refuse the whole
reference. Linux x86-64, the user-unit runtime, Landlock ABI >=3 and libseccomp
are required; unsupported platforms/formats refuse without a fallback.

`packet` returns only `profile`, public `abi`, `round` and judge-selected
`examples`. It includes no owner reference source, path, digest, table or
receipt. Ordinary loop subject packets are not capture packets.

`candidate` pre-checks every selected example, then uses the existing
`transform check` plus receipt replay against the full frozen-table program.
Results are `precheck-refuted`, `candidate-refused`, `refuted`, `captured-exact`,
`inconclusive` or `refused`. `examples-equivalent` is intermediate feedback,
never acceptance. `--expressions` reads
`{"schema":"zeno-fcis/capture-expressions/1","outputs":["x0 < 3"]}`:
complete expressions over input names `x0`, `x1`, ...; holes, parameters,
`keep` and output references are refused. Counterexample variation and condition
coverage are labelled proposals. Budget stops before a checker decision exit 2 without spending a
candidate attempt; the invocation's work reservation remains charged. A
decided refutation remains `refuted`, exit 1, and spends an attempt even when
example/coverage feedback cannot complete. Its advisory sub-status describes
that stop. The check reservation uses the fixed target capacity, so a budget
threshold does not reveal the hidden encoding size. New
proposals to a captured session are refused.

`status` is private owner evidence. By default it summarizes source/table
identities, budgets, counts and progress. `--full` includes rows, compact events,
the table program, stored candidate and receipt when the report fits the 1 MiB
contained stdout cap; otherwise it refuses with `capture-full-status-output-bound`.
A captured result is checked and replayed afresh in either mode. Never give
owner status to a proposer. Sessions are mutable tooling records, not
credentials or transition authority. All refusals exit 1; search/stage budget
stops before a decision exit 2. Reference-local runtime caps are refusals;
outer supervisor exhaustion is inconclusive with unknown child accounting.
Work fields record reservations and complete observations, not actual CPU usage;
partial worker progress on a failed schedule is unknown.

The hard bounds are 4,096 tuples/examples, 64 attempts, 10,000,000 node visits per
invocation, 64 KiB input files and 2 MiB protocol/session data. The table encoding
must fit the existing checker's 256-node profile; inability to encode within it
is inconclusive. Exactness is only against the complete observed finite table;
future executable equivalence assumes a pure deterministic reference. See
[the capture guide and non-claims](zal/LOOPS.md#black-box-capture) for the
containment policy, ZAL acts and five example-only MCP tools.

<!-- END black-box capture reference -->

<!-- BEGIN composable program loops -->

## Composable program loops

These opt-in stateless commands are program proposers. Every qualified program
is decided by the existing exhaustive `transform check` with matched receipt
replay on the full declared domain. They do not spend session attempts, change
an incumbent, adopt contracts or grant publication/deployment authority.

### loop pipeline

```sh
zeno-fcis loop pipeline --original O.zcve --packet
zeno-fcis loop pipeline --original O.zcve --proposal P.json [--out NEWDIR]
```

`--packet` supplies `original_sha256` and `implementation_sha256`. The proposal
has schema `zeno-fcis/loop-pipeline/1`, those two bindings, either `program`
(finite program JSON) or `expressions` (a `zeno-fcis/loop-expressions/1`
document with vocabulary), and optional `stages`. The default sequence is
`["complete", "repair", "canonical"]`, maximum length five. Only complete
on holes, repair on checker-refuted hole-free expressions, and canonical or
resubstitute on qualified programs are enabled. Unknown stages refuse. Every
generated program is independently checked against the same original.

### loop strategy

```sh
zeno-fcis loop strategy --tasks T.json [--depth 1] [--out NEWDIR]
```

Manifest schema `zeno-fcis/loop-strategy/1` has `development` and `evaluation`
arrays; each entry is `{ "original": "relative.zcve", "proposal": P }`, where
`P` is a bound pipeline proposal. Paths stay below the manifest's directory.
Each set must contain exactly one task with disjoint original program content.
Only depth one is admitted, giving five sequences. Depths two and three or
additional tasks are refused before search because the supervisor's 62-frame
budget covers the entire invocation. All development trials
must finish within shared budgets before a winner is frozen and evaluation
starts. Rank by qualified count, node cost with twice-original failure penalty,
reserved check/replay pairs, stage count and grammar order. Evaluation never
changes the selection. These are empirical rankings and conservative pair
allowances, not optimality, generalization or total-CPU claims.


<!-- BEGIN typed modes of the existing loop strategy -->
#### Typed strategy modes

```sh
zeno-fcis loop strategy --kind split-by-cases --problem P.json --original O.zcve [--out NEWDIR]
zeno-fcis loop strategy --kind diagnose --problem D.json [--out NEWDIR]
zeno-fcis loop strategy --kind ask-when-it-matters --problem R.json [--out NEWDIR]
```

These extend the existing strategy command. `--tasks` and `--problem` are
mutually exclusive; `--kind` and `--problem` require each other. `--original`
is required only for split-by-cases and refused by the other modes. The
selection-mode `--depth` remains one; case-tree depth belongs in problem
configuration. No callback, model-defined primitive or direct owner answer
field is admitted. All problems have schema `zeno-fcis/loop-strategy-problem/1`.

| Kind | Additional problem fields | Result scope |
|---|---|---|
| `split-by-cases` | `learn`, optional `config` | Full-domain check/replay may qualify a proposed case assembly |
| `diagnose` | `circuit`, `vector`, `observations`, optional `max_faults` | Conditional minimal weak-fault explanations and a minimax probe |
| `ask-when-it-matters` | `background`, `readings`, `query` | Answers over listed finite readings and an optional owner question |

`learn` uses `zeno-fcis/learn-terms/1`: exact `original_sha256`, `status`
(`complete` or `inconclusive`), and on completion `integers` and `predicates`
as finite program JSON. Terms share the exact reference input domains and
have one full i64 or Boolean output. The consumer recomputes meanings. An
incomplete producer stops before decoding lists and cannot certify emptiness.

`circuit` contains `inputs`, `gates` (`kind`, prior `wires`) and `outputs`;
Boolean measurements are `[wire, value]` pairs. Gate kinds are `and`, `or`,
`xor`, `nand`, `nor`, `not`. Bounds are 1–8 inputs, 16 gates, and 0–2 faults
(default 2). Healthy and masked-fault explanations remain possible.

`background` and every reading alternative are single-output Boolean finite
programs with the same input domains. A reading contains unique `id`, optional
`text` and nonempty `alternatives`. `query` is `count`, `must` or `possible`;
the final reading is the query for `must`/`possible`. Equivalent masks collapse.
The conditional answer is a count or 0/1; `must` is 0 on empty premise models.
At most 16 readings, `--max-terms` total alternatives and 4,096 combinations
are admitted, also capped by `--max-terms` squared and scalar-cell limits.
Only differing answer sets justify a question. Missing intended readings
remain outside the claim. Owners confirm displayed reading/world questions
only through the existing terminal confirmation gate (`any` for holds, `none`
for does not hold); models and MCP cannot supply owner answers.

Split `config` admits `initial` 1–64 (default 8), `depth` 0–3 (2),
`committee` 1–16 (16), `alternatives` 1–8 (6), `leaf_alternatives` 0–8 (8),
`spot_checks` 1–64 (8), `queries` 0–64 (64). Sampling, ties and fallback
spot checks are deterministic. A missing cover/tree refuses as a proposal
failure, never an unrealizability result. A sampled-fit assembly may be refuted.

Pipeline, selection trials and these modes share closed typed plans. A plan
must pass structural type flow before work; its actual macro-step trace must
conform as a prefix. Plan bounds: depth 16, feedback 1–64, 2,048 type states,
512 trace steps and 100,000 structural work units per check. This covers
structure, not semantic verdicts or owner intent. Existing 22-second invocation,
5-second search, 2-second check/replay and 62-frame limits remain shared;
no per-mode budget reset or supervisor is added. All existing limit flags apply.

A full-domain checked/replayed split result exits 0 as `qualified` and extends
the export rule to write `original.zcve` and `candidate.zcve`. Advisory statuses
`question`, `proposed-diagnosis`, `proposed-answer`, `proposed-ambiguous` also
exit 0 and never export a program. Budget/time stops exit 2 as `inconclusive`,
retain charged work, spend no attempt and export no candidate or partial
answer. Refused/refuted results exit 1. Every result has `authority: none`.
The problem, implementation and plan are bound by hashes. See
[typed modes and nonclaims](zal/LOOPS.md#typed-strategy-modes) for paired acts,
owner confirmation and the supported finite scope.
<!-- END typed modes of the existing loop strategy -->

### loop rules

```sh
zeno-fcis loop rules --original O.zcve [--terms T.json | --bank B.json] [--out NEWDIR]
```

Term corpus schema `zeno-fcis/loop-terms/1` has `terms`, optionally
`vocabulary` and `parameters`. Terms are single full-output finite programs,
or `{ "kind": "Int", "expression": "x + x - x" }` (also `Bool`) lowered in
the corpus vocabulary. Bank schema is `zeno-fcis/loop-rule-bank/1`; each rule
has `lhs` and `rhs` finite term programs. Bank metadata is advisory. Rules are
requalified on the target domain, and every changed whole program is checked
against the fixed original, both with replay. An unchanged qualified result has
`improved: false` and `result_kind: unchanged-original`; its exported bytes are
the original and ZAL records a fact. Exact all-failure originals can be checked
as unchanged; this makes no successful-output or improvement claim. Rules are
concrete finite-domain proposals, not universal arithmetic identities.

### Shared bounds and output

Each file is at most 64 KiB. All three commands accept `--max-tuples`
(default 131072), `--max-gate-pairs` (1000000), `--max-search-work` (50000000),
`--max-cells` (4194304), `--max-assignments` (2000), `--max-terms` (128),
`--max-checks` (24) and `--deadline-ms` (22000), within fixed hard caps.
Search work caps each stage's existing proposer meter; repair's meter covers
top-down synthesis only. Local edits and canonicalization remain bounded by
admitted inputs and stage deadlines. A found repair always reaches check/replay.
A bounded synthesis stop is inconclusive only when no checked repair results;
a typed proposer refusal exits 1. Charged mining and synthesis work is
retained on a reported stop. Check pairs, check calls, elapsed
deadline and control frames are shared across an invocation. Exhaustion is
`inconclusive`, exit 2, with zero attempts. Refusal/refutation exits 1; I/O
failure follows the existing exit 3 convention. Packet, qualification and
completed strategy evaluation exit 0. A strategy's evaluated result may contain
unqualified task outcomes and is not program acceptance.

`--out` requires a fresh directory. It writes `result.json`, and for rules or
strategy a `bank.json` or `selection.json` when available. Only an overall
qualified pipeline/rules result exports canonical `original.zcve` and
`candidate.zcve`. Earlier successful checks remain visible in an inconclusive
report and cannot authorize candidate export. See [ZAL loops](zal/LOOPS.md)
for paired acts and each command's not-claimed list.
<!-- END composable program loops -->

<!-- BEGIN bounded knowledge operations of the existing loop strategy -->
### Scoped usage knowledge

```sh
zeno-fcis kb advise --task 'task(subject,check-program).'
zeno-fcis kb plan --have model,observations --want answer --task 'task(subject,observe). task(tool,zeno-fcis). task(version,v241). task(environment,sandbox).'
zeno-fcis kb explain 'usage_applies(L,S)' --task 'task(subject,check-program).'
zeno-fcis kb check --kb team.layer.json --kb personal.layer.json
zeno-fcis loop strategy --kb-op query --query 'strategy(S)'
```

`--kb PATH` is repeatable; `ZENO_FCIS_KB` is used when it is omitted. The public
example loads first. Configured loops consult scoped advice before work and
write proposed observations only to the final external layer's private inbox.
Exact program checkers retain acceptance authority. Explicit knowledge results
can contain private facts and should be kept private.

See [knowledge vocabulary, evidence, privacy, and bounds](KNOWLEDGE.md) for the
small fixed schema, explicit unknowns, inheritance, conflict and supersession
policy, replay recipes, explanations, and observation-driven replanning.
Read-only operations exit 0 for complete answers, 1 for integrity/refusal
failures, 2 for an inconclusive bound or no plan, and 3 for a file read failure.
No partial candidate plan is returned after a resource stop.
<!-- END bounded knowledge operations of the existing loop strategy -->

Scalar coordinates in current composition JSON results and joint tables use
`safe-number-or-decimal-string/1`; see the [integer report encoding](CONTRACT_JUDGES.md#integer-values-in-json-reports).
Fresh composition receipts use `/2` envelopes for the versioned result. Existing
receipt files are unchanged; replay requires the current exact schema and result.
Draft assessment `/2` applies the same scalar encoding to `labels_compared[].input`,
`questions[].input` and every tuple in `unlabeled_inputs`; label text and provenance
remain text. Completion result `/2` declares the same encoding for failure `input`
and `state` tuples, including optional invalid-plan states. Small coordinates keep
their numeric form. Completion source documents and canonical plan bytes retain
their existing formats; a replayed failure case still fails and supplies no plan.
