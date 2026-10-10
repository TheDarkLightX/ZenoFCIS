# `zeno-fcis` CLI reference

This guide is included in the `2.3.0-rc.1` candidate. Its retained V1
protocol guidance does not define the normal V2 authority route; see
[the candidate release notes](RELEASE_NOTES.md).

The `zeno-fcis-cli` package in `1.1.0` publishes the `zeno-fcis` binary.
It pins `clap = 4.6.1` without environment parsing or color output.

```text
zeno-fcis describe [COMMAND...]
zeno-fcis new <dir> --template minimal|mini-determinator|durable-counter|prepared-counter|account-lockout|order-fulfillment|inventory-reservation|compliance-gateway|withdrawal-queue|agent-treasury-guard [--source TREE]
zeno-fcis new <dir> --contract <contract-dir> [--source TREE]
zeno-fcis check [project.zeno] [--format human|json] [--require-substantive] [--require-resolved-paths]
zeno-fcis generate [project.zeno] --out <dir> [--check] [--format human|json]
zeno-fcis generate contract [<app-dir>] [--check] [--format human|json]
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
zeno-fcis optimize --program FILE [--strategy FILE] [--profile functional-bool-v1|checked-i64-v1] [--with-candidate FILE]... [--candidate-out OUT] [--receipt OUT] [--max-input-tuples N]
zeno-fcis contract draft start --session DIR --intent FILE --project FILE --provenance TEXT [--examples FILE] [--rounds 4] [--max-tuples 4096]
zeno-fcis contract draft propose --session DIR --revision SHA256 --rules FILE --provenance TEXT
zeno-fcis contract draft questions|check --session DIR
zeno-fcis contract draft label --session DIR --revision SHA256 --examples FILE --provenance TEXT
zeno-fcis contract draft finalize --session DIR --revision SHA256 --out NEW_DIR
zeno-fcis contract review [<app-dir>] [--out PACKET.json] [--max-tuples N] [--format human|json]
zeno-fcis contract export-program [<contract-dir>] --out FILE [--format human|json]
zeno-fcis contract adopt [<app-dir>] --candidate FILE --receipt FILE --usage preserved|new-version [--format human|json]
zeno-fcis contract refresh-receipts [<app-dir>] [--format human|json]
zeno-fcis contract diff <old-dir> <new-dir> [--format human|json]
zeno-fcis contract evolve [<app-dir>] --to <contract-dir> [--migration FILE [--shortcut FILE]...] [--format human|json]
zeno-fcis contract check-symbolic [<contract-dir>] [--strengthening FILE] [--tools FILE] [--max-tuples N] [--out REPORT.json] [--queries DIR] [--format human|json]
```

`new` refuses a nonempty target. `check` parses and elaborates in one command.
It also classifies every law and claim by
[substance](CLAIM_SUBSTANCE.md). Human output warns on stderr about formulas
that cannot constrain any transition. JSON output adds a `substance` object,
and `--require-substantive` exits 1 with `status: "vacuous"` when any are
found. It also reports law and claim paths that name no declared type or
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
and `v2/policy_vN.zcve`. With `--check` it writes nothing, names each drifted
file and exits 1. An invalid contract exits 1 and writes nothing. The
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
- copies the tree's `Cargo.lock`, so the external crates are the versions
  the tree pins; the application's first build adds the application itself;
- copies the tree's `rust-toolchain.toml` when it has one.

Without a tree, `new` exits 2 and names `--source`; with a tree that cannot
bind the application, it exits 1. Either way it writes no file. A build of
the CLI with the environment variable `ZENO_FCIS_BUILD_TREE` set records that
path as its tree instead, or, when it is empty, none, so that the binary
holds no build directory; such a binary always needs `--source`. The release
binaries are built that way (`tools/rc_package.py build`). With an installed
release binary, extract the release's `source/zeno-fcis-<version>-source.tar.gz`
and pass `--source <extracted source tree>`:

```sh
tar -xzf zeno-fcis-1.1.0-source.tar.gz
zeno-fcis new my-app --contract my-contract --source zeno-fcis-1.1.0
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
  ZenoDEX are shaped after its `SwapIntent`.

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
| CVC5 returns UNSAT with proof-shaped output | 2 | 2 | The proposal and output are retained. The current path does not independently check the proof. |
| Z3 returns UNSAT | 2 | 2 | The result remains blocked because the current path has no Z3 proof checker. |
| CVC5 or Z3 returns SAT and the built-in evaluator replays the model | 1 | 0 | A normalized counterexample is retained. |
| `--backend lean` | 2 | 2 | The current public exporter refuses this unsupported mode. The historical exporter is test-only. |
| Tool is missing, reports `unknown`, or supplies unsupported evidence | 2 | 2 | The requested result remains blocked. |
| Tool crashes, times out, exceeds a bound, or encounters a filesystem failure | 3 | 3 | The bounded execution failed. |

The retained `KernelChecked` evidence type does not provide a public Lean
exporter. Restoring a checked symbolic route is explicit planned work; see the
[V2.3 roadmap](V2_3_PLAN.md). The existing solver path remains attested unless
the required independent proof checking and translation obligations are met.

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

`transform`, `optimize` and `contract review` enumerate input domains, and a
release build of the CLI (`cargo build --release -p zeno-fcis-cli`) does
that several times faster than a debug build. In the app study of
2026-10-05, three reviews took 29 s, 64 s and 168 s with a debug build and
4.3 s, 8.5 s and 36 s with a release build, 4.7 to 7.5 times faster, and
their packets were byte-identical. Prefer a release build for these
commands; their results do not depend on the build.

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

Results use schema `zeno-fcis/transform-result/1`. Every result has a `detail`
object. Once both programs are read, `check` also reports the `limits` it used
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
`zeno-fcis/transform-check/1`, and the library's evaluator digest. The
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
[published 100-case corpus](benchmarks/published-corpus/README.md)
(calibration only) 508 instructions; under `functional-bool-v1` every Boolean
seed and corpus output passes the profile's gate with the same totals.
[`docs/benchmarks/measure_optimizer.py`](benchmarks/measure_optimizer.py)
reproduces these figures and replays every receipt. In a release build the
portfolio takes about 1 second on the controller, 14 seconds on the decision
graph (mostly the checker's enumeration of 1,296,000 tuples) and under 0.5
seconds on 87 of the 100 corpus cases (at most 1.5 seconds); a debug build is
several times slower.

These are results of a bounded search, not minimality claims: tables are exact
only for supports of at most 2^16 tuples and within the table budget, larger
supports get algebraic rules under conservative trap bounds, the cut table
covers three inputs, and an accepted candidate changes Step usage, so adopting
one into an application remains a separate reviewed step that this command
does not perform.

## Bounded optimization loop

`zeno-fcis loop` runs the bounded adaptive loop specified in
[docs/neurosymbolic-loop](neurosymbolic-loop/README.md): a proposer suggests
complete candidate programs and `transform check` judges every one against the
admitted original on its whole declared input domain. The loop keeps an
incumbent that is either the original, with no receipt, or a checked
replacement built from a genuine equivalence; a model, a user, a receipt file
or a "passed" flag cannot build one. Proposals are data, never code.

| Command | Effect |
| --- | --- |
| `loop open --original P --session DIR [--profile functional-bool-v1\|checked-i64-v1] [--attempts N] [--checks N] [--model-calls N] [--check-work N] [--session-work N] [--deadline-ms N]` | Admits the original under the profile, freezes the canonical request (`zeno-fcis/transform-request/1`) and its `request_id`, and creates the session directory. Limits above the compiled ceilings (8 attempts, 8 checks, 4 model calls, 1,000,000 work units per check, 8,000,000 per session, 20,000 ms) are refused; lower limits win. Hosted providers, source disclosure and spending are fixed disabled. |
| `loop candidate --session DIR (--candidate C \| --candidate-json J) [--provenance FILE]` | Spends one attempt on a supplied candidate (canonical bytes, or the fixture vocabulary `{inputs, outputs, nodes, roots}` encoded canonically first) and prints typed feedback: a replayed counterexample, an admission refusal, an incomplete check, or an equivalence with its actual cost and selection reason. |
| `loop run --session DIR --proposer local\|fake\|hosted [--script S] [--hosted-config H]` | Drives attempts until the proposer is exhausted or a limit stops the session. `local` is a deterministic rewriter; `fake` replays a script (`zeno-fcis/fake-provider-script/1`) and is metered as a model; `hosted` is the provider adapter, which is disabled in this build and answers "unavailable" without any network, credential or spending path. |
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
session-wide and persisted. Check workers run on a supervised thread with a
two-second deadline and panic capture; memory caps are not installed by this
shell, the checker's allocation being bounded by the admitted artifact limits.

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
outside the optimizer's table. In a
debug build the default strategy finishes within 2 seconds on every measured
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
`v2/policy.json` with the new entry appended. It renders the whole rules file
again in the templates' layout, two-space indentation and one entry per line,
keeping its keys in order, so the file's own spacing and line breaks are not
kept. An
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
generating the result again with every new receipt replayed from scratch. It
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
withdrawal queue, measured on a shared machine, one comparison took about 3
seconds in a release build of the application and about 4 seconds in a
debug build, whose generated manifest optimises `zeno-fcis-synthesis` while
keeping overflow checks and debug assertions on. A checkpoint open skips the
comparisons before its checkpoint. Enumeration runs on one thread; parallel
enumeration is planned, not built. No comparison runs while the store's
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
   its `kind`. `NEW`'s `tests/decision-examples.txt`, when present, replaces
   the application's.

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
given a value, every old field must be carried into at least one new field,
since a migration renames, adds and splits fields and drops none, and keys
other than these are refused. Text fields have no finite domain and cannot
be migrated. Deliveries are compared without a mapping: a layout change keeps
every payload and destination type, and a change of either is classified
`unrelated` and refused before any migration is read.

**Admission by forward simulation.** `contract evolve` and every later
`generate contract` compile the file against both contracts and run the
SQLite shell's own simulation, compiled into the CLI from the shell's
source, over the application's last version and `NEW`'s first, through the
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
  with its lane, ordinal, channel, destination, payload and idempotency
  value as the library's candidate holds them; and for a commit a successor
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
| `program-successor` | NEW's canonical policy, with its decision program's instructions and roots and its Step limit replaced by OLD's, is byte for byte OLD's policy. This is premise 1 of the F6.1 upgrade, the comparison the SQLite shell makes. | An F3 equivalence receipt and the F6.1 program-successor upgrade, admitted at any state. It exists today. |
| `rename` | Both schemas declare the same types, fields and variants, with the same IDs and forms; some project, type, field or variant name differs; and NEW's policy, with OLD's schema and schema commitment in place of its own, is byte for byte OLD's policy. | G2's rename tier, admitted at any state; it exists today: [`contract evolve`](#data-migrations-and-renames) without `--migration`. |
| `layout-change` | The state layout differs, while every other type and every channel is unchanged. Names, laws, cases, reasons, the genesis state and the decision program may differ too. | A G2 data migration, admitted by forward simulation over the old contract's whole declared input domain; it exists today: [`contract evolve --migration`](#data-migrations-and-renames). |
| `rule-change` | The schemas are byte-identical and so are the channels. Since neither of the first two kinds holds, the policies differ in more than the decision program and its Step limit: in laws, cases, reasons or the genesis state. | G14.1's behaviour-change upgrade, which exists today: [`contract evolve`](#rule-changes-for-live-stores) records the change with this account, and a store upgrades when every state law of the new contract and every inductive claim it declares hold on the store's state. G2's forward simulation also exists today, for a rule change that preserves every decision: [`contract evolve --migration`](#data-migrations-and-renames). |
| `unrelated` | Anything else. | Refused. |

The terms:

- **The state layout** is the state record, type 100, with each field's ID
  and type, and the form of every type a state field has: a boolean, an
  integer range, a text length range, or a sum's variant IDs. Names are not
  part of it. A type a state field has counts as the state's only when no
  command, context, payload or destination field has it and it is no channel's
  destination or payload, on either side.
- **Every other type** is the command and context roots, payloads and
  destinations, the types their fields have, a type the state shares with
  them, and any type nothing has.
- **The channels** are each channel's ID, destination type and payload type.

So `unrelated` covers: a channel added, removed or given another destination
or payload type; a change to the command, the context, a payload, a
destination, a shared type or an unused type; and names changed together
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
  plan `feature`, whether it `exists_today` (every path does), and `when`
  it applies. For a
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

The classifier's `program-successor` comparison mirrors the shell's
`v2::upgrade::program_successor` line for line. The CLI's unit tests compile
the shell's upgrade and equivalence modules from their own source and check,
on every planted pair of
`crates/zeno-fcis-cli/tests/fixtures/contract-diff/pairs.json` and on both
adoptions, in both directions, that the shell's comparison accepts exactly
the pairs the classifier calls `identical` or `program-successor`, and that
every pair the shell's F6.1 Tier A admission admits is a `program-successor`. The converse does not
hold: Tier A also needs law 991 in both contracts, both Step premises and the
two decision programs' equivalence, none of which the classifier checks. That
is evidence on those pairs, not a proof for all contracts.

## Delivery relay

Two scripts in `tools/` deliver a store's pending entries to an external
system. Both use only the Python standard library. The protocol and its
strength are described in `docs/V2_SQLITE_STAGE.md`, "Relay protocol".

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

`propose` reserves and saves an attempt before running the existing F1 generator
and library binding. Invalid rules, refused assessments, raw proposal text,
provenance and the preceding revision remain in `draft.json`. The first
bindable proposal fixes the canonical schema and program input order for later
rounds. Changing that shape requires a new session. Rules with existing adoption
or evolution histories need the existing lineage workflows, not this new-contract
workflow. `questions` returns the current F2 packet, including its decision table,
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

`check` recomputes F1/F2 and exits 1 unless at least one parsed supplied label exists,
every current distinguishing input is labeled, and no comparison or F2 finding
fails. Newly supplied examples can change F2's probe bases and produce more
questions; repeat `questions`, `label`, or `propose` until ready. `finalize` applies
the same fresh check at the specified revision and writes a new contract directory:
project/rules/examples, generated schema/policy/Rust, intent, complete transcript,
and `draft-finalized.json`. The report lists each compared label and difference,
question coverage, assumed provenance, and the full advisory F2 packet. A proposal
operation exiting 0 only means the assessment ran, not that finalization is ready.

Limits are fixed at start: 1–8 proposal rounds (default 4), 1–4096 F2 tuples per
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
adoption or deployment approval. F2 remains bounded, advisory evidence relative to
supplied examples and declared laws; undistinguished mutants, unprobed inputs,
reachability and omitted requirements are unresolved, not proved correct.

The G7 fixtures in `crates/zeno-fcis-cli/tests/fixtures/contract-draft/` expose three
intent summaries and rewritten complete proposals. They preserve the initial
example files' recorded review provenance. Extra test labels come from a simulated
supplier evaluating the retained original template, never from the submitted
proposal. Fixture replay and generated-app tests do not establish a live owner
interaction or correct automatic translation of prose.
