# `zeno-fcis` CLI reference

The `zeno-fcis-cli` package in `1.1.0` publishes the `zeno-fcis` binary.
It pins `clap = 4.6.1` without environment parsing or color output.

```text
zeno-fcis describe [COMMAND...]
zeno-fcis new <dir> --template minimal|mini-determinator|durable-counter|prepared-counter|account-lockout|order-fulfillment|inventory-reservation|compliance-gateway|withdrawal-queue|agent-treasury-guard
zeno-fcis new <dir> --contract <contract-dir>
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
zeno-fcis transform replay --receipt FILE --original FILE --candidate FILE [--max-input-tuples N]
zeno-fcis optimize --program FILE [--strategy FILE] [--candidate-out OUT] [--receipt OUT] [--max-input-tuples N]
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
application's `project.zeno` and `v2/policy.json` and replaces
`v2/schema.zcve`, `src/v2_contract.rs` and `v2/policy.zcve`; the policy bytes
come from the library's encoder, and the library's catalog binding must accept
them first. With `--check` it writes nothing, names each drifted file and
exits 1. An invalid contract exits 1 and writes nothing. The generator's own
checks name the file and entry at fault. Among them: an accept has no reason,
and a reject or committed failure has one; constants lie within their
declared ranges; the variants of a sum the decision program reads have
consecutive IDs; a law that applies at genesis reads only `post` fields; and
no declared law shares an ID with a law generation adds (990, 991, and the
reject and committed-failure laws). A contract that passes these checks but
that the library's catalog binding refuses is reported as that refusal, with
no entry named.
`new --contract` builds an application from a directory holding
`project.zeno`, `v2/policy.json` and, optionally,
`tests/decision-examples.txt`. `graph` and `explain` are derived
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
and the checker identity: crate version, SHA-256 of
`crates/zeno-fcis-cli/src/transform.rs`, and the library's evaluator digest.
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
new dependency. Each class carries its scalar kind, interval bounds and, when
the declared domain has at most 64 input tuples (six Boolean inputs), its
exact signature: the value on every tuple in the checker's enumeration order,
with poison at the tuples where an earlier `Add` or `Sub` has already trapped.
Two classes merge only when their signatures are identical, poison included.
Beyond 64 tuples the annotations are conservative: rules merge only classes
that no possible trap can poison, while congruence and commutativity merge
the same computation on the same operands. Every `Add` or `Sub` that may
overflow on a tuple whose operands are defined is pinned: no rule removes,
merges away or folds it, and extraction emits it whether or not an output
uses it. Arithmetic is otherwise rewritten only by constant folding, and a
pinned instruction is never a constant.

A strategy is a small versioned JSON document in a closed grammar; unknown or
duplicate keys, wrong types and out-of-range values are refused, and no
user-supplied code runs. Phase names come from a closed set:

| Phase | What it does |
| --- | --- |
| `boolean` | Algebraic rules over `And`, `Not`, `Eq`, `Lt` and the Or form `Select(a, a, b)`: involution, idempotence, identity, annihilation, complement, absorption, De Morgan's Or recognition, Or commutation, factoring both ways, mux recognition and sharing-directed associativity. |
| `select` | `Select` simplification for both kinds: equal arms, constant or negated conditions, Boolean identities, and the condition's known value inside its arms. |
| `fold` | Constant folding from exact signatures or interval bounds, for never-poisoned classes only. |
| `share` | Common-subexpression sharing modulo commutativity of `And` and `Eq`; structural sharing of identical instructions is inherent to the e-graph. |
| `semantic-merge` | Merges classes with identical exact signatures, then adds single instructions over existing classes whose signature an existing class already has (Not, And, Eq, Lt and Select), so extraction can use them. Skipped, and reported as such, above 64 tuples. |

```json
{
  "schema": "zeno-fcis/optimize-strategy/1",
  "phases": [{"phase": "boolean", "rounds": 3}, {"phase": "select", "rounds": 2}],
  "limits": {"max_enodes": 20000, "max_classes": 10000,
             "max_rewrites_per_round": 10000, "max_extraction_rounds": 8},
  "extractor": "dag-greedy"
}
```

| Key | Meaning and range |
| --- | --- |
| `schema` | Required; exactly `zeno-fcis/optimize-strategy/1`. |
| `phases` | Required; 1 to 16 objects, each `{"phase": NAME, "rounds": 1..=8}` with `NAME` from the table above. A round collects every match against the current graph, applies the rewrites and rebuilds; a phase stops early when a round changes nothing. |
| `limits` | Optional object; each entry is optional and defaults to the value shown, with range 1 to 100000: `max_enodes` (e-nodes ever created, retired duplicates included), `max_classes` (live classes), `max_rewrites_per_round` (rewrites proposed or completions attempted in one round), `max_extraction_rounds` (improvement rounds of `dag-greedy`). A phase that reaches a limit stops and names it. |
| `extractor` | Optional; `dag-greedy` (default) or `tree`. |

Without `--strategy` the fixed default strategy, version 1, runs: `fold` 1,
`share` 1, `boolean` 3, `select` 3, `semantic-merge` 2, `boolean` 2, `select`
2, `fold` 1, with the limits above and `dag-greedy`. After every phase a
candidate is extracted, re-encoded canonically and judged, so an early
phase's candidate survives a later phase that finds nothing better.

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

Each extracted candidate is screened before judging: it must be componentwise
no larger than the original in instructions and bytes, strictly smaller in at
least one, not byte-identical to the original or an earlier candidate, and not
worse than the incumbent on (instructions, bytes). Then `transform check` runs
it over the full domain at the full Step budget with the default declared Step
limit of 256, which never binds. The best accepted candidate wins by
(instructions, bytes, largest Step usage, canonical bytes). Step usage is
reported from the receipt and never compared for equivalence.

Results use schema `zeno-fcis/optimize-result/1`. Every result has a `detail`
object; once the program and strategy are read, the report also carries
`original_sha256`, `strategy_sha256` and the checker `limits`. A searched
result's `detail` reports the original's size, the domain size and whether
signatures were exact, the strategy, the termination bounds (`search`: phases
requested and run, per-phase rounds requested and run, saturation, the limit
hit if any, rewrites, nodes added, merges accepted and refused, e-node and
class counts), every candidate with its verdict (`accepted`,
`accepted-not-better`, `counterexample`, `inconclusive`, `refused`,
`not-smaller`, `duplicate`, `not-better-than-incumbent`, `unextractable`), and
`best` with the candidate's digest, sizes, largest Step usage and its complete
transform receipt.

| Status | Exit | Meaning |
| --- | ---: | --- |
| `improved` | 0 | An accepted candidate is smaller than the original. `--candidate-out` and `--receipt` were created when given. |
| `no-checked-improvement` | 2 | The search ended within its bounds without an accepted smaller candidate; the original stands. Nothing is written. |
| `inconclusive` | 2 | The domain exceeds `--max-input-tuples` (default 100,000,000); no candidate could be judged, so nothing was searched. |
| `refused` | 1 | The original failed the library importer, exceeds 64 KiB, or has an empty input domain. |
| `invalid-strategy` | 1 | The strategy is not a document of the closed grammar, or exceeds 64 KiB. |
| `output-exists` | 1 | A `--candidate-out` or `--receipt` path already exists, as a file or a link; nothing was searched. |
| `io-error` | 3 | A file could not be read, is not a regular file, or an output could not be created. |

The written receipt is the checker's `zeno-fcis/transform-receipt/1` for the
original and the written candidate; `transform replay` reproduces it byte for
byte. The same program and strategy give byte-identical output. On the
recorded withdrawal-queue artifacts the default strategy reaches the 7-node
Boolean kernel (from 16) and a 46-node retained controller (from 69; the
recorded hand candidate has 60), each accepted with a receipt, and the
106-node current decision graph reaches 100 nodes with every one of its
1,296,000 tuples checked; on the sixteen Boolean benchmark seeds it matches or
beats every recorded candidate and leaves the minimal originals unchanged.
These are results of a bounded search,
not minimality claims: signatures are exact only up to 64 tuples, larger
domains get algebraic rules under conservative trap bounds, and an accepted
candidate changes Step usage, so adopting one into an application remains a
separate reviewed step that this command does not perform.

## Bounded completion in 1.1.0

`zeno-fcis describe synth completion` lists the exact options and filesystem
effects of `discover`, `find`, `verify` and `replay`. These commands use the
closed `zeno-fcis/completion-problem/1` JSON schema and existing finite-i64
instructions; they never execute supplied code. See
[bounded completion](BOUNDED_COMPLETION.md) for commands and evidence limits.
The `prepared-counter` template connects preparation to independently checked
nominal authorization and atomic state/outbox publication.
