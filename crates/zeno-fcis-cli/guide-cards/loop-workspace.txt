```json
{
  "name": "loop-workspace",
  "title": "Choose a loop and drive checked proposal sessions",
  "synopsis": [
    "zeno-fcis loop open --original PROGRAM --session DIR",
    "zeno-fcis loop candidate --session DIR --candidate PROGRAM",
    "zeno-fcis loop run --session DIR --proposer local",
    "zeno-fcis loop resume --session DIR",
    "zeno-fcis loop encode --program JSON --out PROGRAM",
    "zeno-fcis loop print --program PROGRAM"
  ],
  "engine": "Session mode: durable proposal ledger, full-domain transform checking, structured feedback and replay of the retained best program.",
  "use_when": [
    "propose and revise a program",
    "drive a checked optimization loop",
    "resume a candidate workspace",
    "encode canonical program bytes",
    "print a decision program as JSON"
  ],
  "not_for": [
    "hosted execution in this build",
    "global optimality",
    "turning provenance into acceptance"
  ],
  "inputs": [
    "Canonical Bool/signed-i128 PROGRAM <= 65,536 bytes; finite-program JSON (inputs/outputs/nodes/roots) <= 1,048,576 bytes, with Int bounds/literals as canonical decimal strings. Expression documents <= 65,536 bytes; script/config/provenance <= 262,144 bytes. Open creates the session. Print reads canonical Bool/signed-i128 bytes, at most 524,288 bytes; data interchange admits at most 64 inputs, 64 outputs and 4,096 nodes."
  ],
  "outputs": [
    "zeno-fcis/transform-loop/1; read status, detail, accounting and report; session ledger, receipts, witnesses and artifacts",
    "Successful print emits raw finite-program JSON (inputs/outputs/nodes/roots), compact with sorted keys and one LF; failures use zeno-fcis/transform-loop/1. No caller files, session or receipt are changed by print."
  ],
  "exit_codes": {
    "0": "positive verdict or completed query/operation",
    "1": "invalid specification or refutation",
    "2": "blocked or inconclusive; no acceptance",
    "3": "filesystem or execution failure",
    "64": "invalid command-line usage"
  },
  "verdicts": {
    "opened": "Request admitted; submit candidate or run proposer.",
    "different": "Counterexample retained; revise candidate against all witnesses.",
    "inconclusive": "No new accepted incumbent; inspect reason and remaining work, then resume.",
    "resume-refused": "No trusted incumbent; repair integrity or reopen from original.",
    "checked-improvement": "Equivalent and smaller; selected replacement and its receipt are retained.",
    "equivalent-without-improvement:<reason>": "Equivalent but not selected; exit 0 does not imply improvement.",
    "completed": "Run driver returned; inspect attempts and stop reason, including failures and no improvement.",
    "encoded": "Program JSON admitted and encoded; no equivalence check or receipt.",
    "resumed": "Ledger/request validated and retained evidence replayed; replay reservation stays charged.",
    "duplicate-of-original / duplicate-of-attempt:<N>": "Canonical/JSON duplicates exit 1; expression/sketch duplicates independently confirmed by check/replay exit 0. Both consume an attempt.",
    "print success": "Exit 0 emits only canonical program JSON, directly reusable by encode; malformed/unsupported/oversized bytes exit 1, I/O 3, containment 2, usage 64."
  },
  "limits": {
    "attempts_cap": 8,
    "checks_cap": 8,
    "model_calls_cap": 4,
    "deadline_ms_cap": 20000,
    "worker_ms_cap": 25000,
    "search_stage_cpu_ms": 5000,
    "check_stage_cpu_ms": 2000,
    "memory_mib": 64,
    "minimum_check_work": 1000000,
    "max_check_work": 268435456,
    "session_work_hard_cap": 4329447424,
    "transcript_bytes": 524288,
    "storage_bytes": 2097152,
    "program_json_bytes": 1048576,
    "script_config_provenance_bytes": 262144,
    "print_program_bytes": 524288
  },
  "refusals": {
    "work-policy-changed": "Open a fresh session with this checker; do not rewrite request identity.",
    "work-infeasible": "Choose a smaller declared domain or supported checker budget.",
    "output-exists": "Use a new output file for encode."
  },
  "cost": {
    "measured_units": "unknown",
    "example_wall_ms": "unknown",
    "telemetry": "PENDING: populate from source-bound invocation telemetry"
  },
  "steps": [
    {
      "command": "zeno-fcis loop open --original crates/zeno-fcis-cli/tests/fixtures/i128-door-programs/bounded-counter-c2.original.zcve --session \"$WORK/session\"",
      "exit": 0,
      "expected": {
        "status": "opened"
      },
      "contains": []
    }
  ],
  "example": {
    "command": "zeno-fcis loop resume --session \"$WORK/session\"",
    "exit": 0,
    "expected": {},
    "contains": [
      "zeno-fcis/transform-loop/1"
    ]
  },
  "sources": {
    "synopsis": "docs/CLI_REFERENCE.md#loop-commands; crates/zeno-fcis-cli/src/loop_command.rs",
    "engine": "docs/CLI_REFERENCE.md#loop-commands; crates/zeno-fcis-cli/src/loop_command.rs",
    "inputs": "docs/CLI_REFERENCE.md#loop-commands; crates/zeno-fcis-cli/src/loop_command.rs",
    "outputs": "docs/CLI_REFERENCE.md#loop-commands; crates/zeno-fcis-cli/src/loop_command.rs",
    "exit_codes": "docs/CLI_REFERENCE.md#loop-commands; crates/zeno-fcis-cli/src/loop_command.rs",
    "verdicts": "docs/CLI_REFERENCE.md#loop-commands; crates/zeno-fcis-cli/src/loop_command.rs",
    "limits": "docs/CLI_REFERENCE.md#loop-commands; crates/zeno-fcis-cli/src/loop_command.rs",
    "refusals": "docs/CLI_REFERENCE.md#loop-commands; crates/zeno-fcis-cli/src/loop_command.rs"
  },
  "commands": [
    "loop",
    "loop open",
    "loop candidate",
    "loop run",
    "loop resume",
    "loop encode",
    "loop print"
  ],
  "pitfalls": "Read the operation-specific status, not only the exit. Encode/resume do not submit proposals; run can complete without improvement; meaning can return unequal keys; teach-back can report wrong rules. No report grants runtime or publication authority."
}
```

# Choose a loop and drive checked proposal sessions

## Use when

propose and revise a program; drive a checked optimization loop; resume a candidate workspace; encode canonical program bytes.

## Not for

hosted execution in this build; global optimality; turning provenance into acceptance.

## Inputs

Canonical Bool/signed-i128 PROGRAM <= 65,536 bytes; finite-program JSON (inputs/outputs/nodes/roots) <= 1,048,576 bytes, with Int bounds/literals as canonical decimal strings. Expression documents <= 65,536 bytes; script/config/provenance <= 262,144 bytes. Open creates the session.

Engine: Session mode: durable proposal ledger, full-domain transform checking, structured feedback and replay of the retained best program.

Synopsis: ["zeno-fcis loop open --original PROGRAM --session DIR", "zeno-fcis loop candidate --session DIR --candidate PROGRAM", "zeno-fcis loop run --session DIR --proposer local", "zeno-fcis loop resume --session DIR", "zeno-fcis loop encode --program JSON --out PROGRAM"]

Set WORK to a fresh writable directory outside the source tree. Run commands from the repository root with zeno-fcis on PATH.

### Which loop to use

| Task | Subcommand | Checker that decides |
| --- | --- | --- |
| Admit a reference and freeze proposal budgets | `loop open` | Program/profile/request admission; no equivalence verdict |
| Check one supplied program or expression | `loop candidate` | Session transform checker on every declared input; expressions also require check/replay |
| Drive local, fake, or disabled hosted proposals | `loop run` | Same session checker for every submitted candidate |
| Recover retained evidence | `loop resume` | Request/ledger validation and reserved checker replay |
| Convert finite-program JSON to canonical bytes | `loop encode` | Library program admission and codec; no equivalence verdict |
| Inspect canonical bytes as program JSON | `loop print` | Existing library decoder and admission; no equivalence verdict |
| Compare a candidate with a complete paged arithmetic reference | `loop capture whole` | Transform checker and receipt replay on every complete lookup piece |
| Freeze an executable observation table | `loop capture open` | Repeated observations plus table/evaluator consistency; no candidate acceptance |
| Obtain public capture examples | `loop capture packet` | Session-binding validation; no program verdict |
| Submit a capture proposal | `loop capture candidate` | Exposed-example precheck, then full-table transform check and replay |
| Inspect private capture evidence | `loop capture status` | Session validation and fresh check/replay of retained success |
| Complete, repair or simplify a proposal | `loop pipeline` | Transform check and matching replay against the fixed original |
| Select stage sequences on development, then evaluate | `loop strategy --tasks` | Pipeline check/replay per task; ranking selects a sequence, not program acceptance |
| Assemble by cases, diagnose faults, or ask about readings | `loop strategy --kind` | Split: capture plus original-bound check/replay; other modes: advisory finite inference |
| Query or plan from structural usage facts | `loop strategy --kb-op` | Bounded knowledge/type inference; no program acceptance |
| Discover or reuse concrete finite-domain rewrites | `loop rules` | Target-domain rule check/replay and whole-program check/replay |
| Learn expressions from complete examples | `loop learn` | Library evaluator checks examples; with `--original`, capture plus original-bound check/replay |
| Fill finite holes in an existing proposal session | `loop sketch` | Confirming check/replay, then the session transform checker |
| Check a parameterized program or fit finite hole values | `loop family` | Transform check/replay on each listed instance only |
| Try direct use, sketching, then repair on new instances | `loop transfer` | Transform check/replay on each listed target only |
| Compute finite observation hashes | `loop meaning` | Library evaluator over the full domain; no independent transform verdict |
| Evaluate one expression at one input | `loop evaluate` | Library evaluator at that point; no equivalence verdict |
| Ask where contract decisions remain open | `loop question --contract` | Door admissibility computation and gaps judge where applicable; answers remain advisory |
| Distinguish finite requirement/sketch completions | `loop question --specification` | Enumeration proposes; singleton needs law evaluator plus transform check/replay |
| Check restated rules against the core | `loop teach-back` | Door evaluator over declared inputs and reachable states; wrong rules still give a completed report |
| Evaluate component interfaces/tables | `loop decompose` | Door simulation; `--compose` additionally uses the composition checker and obligation check |

Successful `loop print` emits the program object alone. All reports have `authority: none`. A session is a persistent directory holding
a frozen request and charged attempt/check/replay ledger. An **attempt** is one
proposal submitted to its driver, including malformed or duplicate proposals;
intake or an unfinished preliminary stage may return before submission.
**Typed feedback** means structured JSON checker outcomes, witnesses and reasons.
The **incumbent** is the retained best checked program, initially the original.
**Finite-program JSON** (historically “fixture vocabulary”) has exactly
`inputs`, `outputs`, `nodes`, `roots`; Int bounds/literals are canonical decimal
strings. A **bound proposal** names the exact reference and implementation
hashes. **Guarded stages** run only when their input conditions hold.
**No session** means no ordinary session directory or attempt ledger is created;
explicitly requested evidence files can still be written. Capture has its own
private session schema and protocol, distinct from ordinary proposal sessions.

Use `zeno-fcis loop COMMAND --help` for accepted flags and defaults. Help exits
0; malformed command-line usage exits 64. Operation exits are not interchangeable:
`run` can exit 0 without improvement, `meaning` can exit 0 with different keys,
`teach-back` exits 1 for wrong/refused rules and 2 for vacuous/incomplete rules.
`strategy --tasks` can exit 0 with unqualified evaluation tasks; an open
`learn` question exits 2.
Containment may return inconclusive, exit 2, before application work starts.
`rules` can qualify an unchanged original with no rewrite checks: inspect
`baseline_check`, `checks`, `improved` and `claim`. Unsupported rule-mining
arithmetic does not prevent the independent baseline check/replay.
Question's five `--max-*` flags apply to `--specification`; contract mode
accepts but ignores them and uses its own limits. Contract determination includes reason and
delivery constraints, so it need not match the gaps judge on exported laws;
a completed report can also contain disagreeing answers.
Neither a proposer, a hash, a role label nor a session file grants publication
authority. Capture owner/proposer roles are conventions, not authenticated users.

## Steps

```sh
zeno-fcis loop open --original crates/zeno-fcis-cli/tests/fixtures/i128-door-programs/bounded-counter-c2.original.zcve --session "$WORK/session"
```

Expected: exit 0; {"status": "opened"}.

## Verdicts

Question disagreements and wrong teach-back rules exit 1. Requirements-only questions and an unresolved `loop learn` question exit 2. Session lifecycle reports remain completed operations; inspect their individual attempt outcomes.

Encode/print data interchange permits the existing extended scalar shape:
64 inputs, 64 outputs and 4,096 nodes; print reads at most 524,288 bytes.
Small graphs retain their existing profiles and bytes. Candidate/checker
limits remain unchanged. Historical i64 profiles are refused, not reinterpreted.
`SignedMulDivFloor` uses `["SignedMulDivFloor", a, b, d]` in program JSON.

`loop print --program PROGRAM` emits exactly `inputs`, `outputs`, `nodes`,
`roots` on exit 0: compact JSON, sorted object keys and one trailing newline.
Int bounds/literals are decimal strings; references are integer indices. Feed
this JSON to `loop encode --program` for byte-identical canonical program bytes.
Malformed/unsupported/oversized bytes exit 1, containment 2, I/O 3 and usage 64;
failures use `zeno-fcis/transform-loop/1` with status and reason. Print changes
no caller files and creates no session or receipt.

`opened`: Request admitted; submit candidate or run proposer.

`different`: Counterexample retained; revise candidate against all witnesses.

`inconclusive`: No new accepted incumbent; inspect reason and remaining work, then resume.

`resume-refused`: No trusted incumbent; repair integrity or reopen from original.

`checked-improvement`: Equivalent and smaller; selected replacement and its receipt are retained.

`equivalent-without-improvement:<reason>`: Equivalent but not selected; exit 0 does not imply improvement.

`completed`: Run driver returned; inspect attempts and stop reason, including failures and no improvement.

`encoded`: Program JSON admitted and encoded; no equivalence check or receipt.

`resumed`: Ledger/request validated and retained evidence replayed; replay reservation stays charged.

`duplicate-of-original / duplicate-of-attempt:<N>`: Canonical/JSON duplicates exit 1; expression/sketch duplicates independently confirmed by check/replay exit 0. Both consume an attempt.

Exit codes: {"0": "positive verdict or completed query/operation", "1": "invalid specification or refutation", "2": "blocked or inconclusive; no acceptance", "3": "filesystem or execution failure", "64": "invalid command-line usage"}

Outputs: ["zeno-fcis/transform-loop/1; read status, detail, accounting and report; session ledger, receipts, witnesses and artifacts"]

## Cost

{"example_wall_ms": "unknown", "measured_units": "unknown", "telemetry": "PENDING: populate from source-bound invocation telemetry"}

Limits: {"attempts_cap": 8, "check_stage_cpu_ms": 2000, "checks_cap": 8, "deadline_ms_cap": 20000, "max_check_work": 268435456, "memory_mib": 64, "minimum_check_work": 1000000, "model_calls_cap": 4, "program_json_bytes": 1048576, "script_config_provenance_bytes": 262144, "search_stage_cpu_ms": 5000, "session_work_hard_cap": 4329447424, "storage_bytes": 2097152, "transcript_bytes": 524288, "worker_ms_cap": 25000}

## Coordinate

Run Commons `advise` for the task first. Compute the exact subject digest, take an exclusive lease `loop:<subject digest>`, and respect existing leases. After checking, add a result record with the command, subject, verdict, scope and retained evidence; release the lease. Telemetry and advice grant no checker authority.

## Pitfalls

Read the operation-specific status, not only the exit. Encode/resume do not submit proposals; run can complete without improvement; meaning can return unequal keys; teach-back can report wrong rules. No report grants runtime or publication authority.

Refusals and remedies: {"output-exists": "Use a new output file for encode.", "work-infeasible": "Choose a smaller declared domain or supported checker budget.", "work-policy-changed": "Open a fresh session with this checker; do not rewrite request identity."}

## Worked example

```sh
zeno-fcis loop resume --session "$WORK/session"
```

Expected: exit 0; ["zeno-fcis/transform-loop/1"].

## See also

Background for program interchange: Tim Bray, "The JavaScript Object Notation
(JSON) Data Interchange Format", RFC 8259, IETF, 2017. Print reuses the existing
decoder and adds no new decoding method; its scope is representation inspection.

Background: Armando Solar-Lezama, Liviu Tancau, Rastislav Bodik, Sanjit Seshia and
Vijay Saraswat, “Combinatorial Sketching for Finite Programs”, ASPLOS, 2006
(counterexample-guided synthesis); Dana Angluin, “Learning Regular Sets from
Queries and Counterexamples”, Information and Computation, 1987 (query and
counterexample learning); Ross Tate, Michael Stepp, Zachary Tatlock and Sorin
Lerner, “Equality Saturation: A New Approach to Optimization”, POPL, 2009
(equality-based optimization). These sources describe background methods, not
proofs of this implementation. This reference adds no new algorithm or priority
claim. Scope: full-domain checking is limited to each command's declared finite
domains, admitted profile and resource limits; enumeration, ranking and
suggested questions retain their stated advisory status.

Sources: {"engine": "docs/CLI_REFERENCE.md#loop-commands; crates/zeno-fcis-cli/src/loop_command.rs", "exit_codes": "docs/CLI_REFERENCE.md#loop-commands; crates/zeno-fcis-cli/src/loop_command.rs", "inputs": "docs/CLI_REFERENCE.md#loop-commands; crates/zeno-fcis-cli/src/loop_command.rs", "limits": "docs/CLI_REFERENCE.md#loop-commands; crates/zeno-fcis-cli/src/loop_command.rs", "outputs": "docs/CLI_REFERENCE.md#loop-commands; crates/zeno-fcis-cli/src/loop_command.rs", "refusals": "docs/CLI_REFERENCE.md#loop-commands; crates/zeno-fcis-cli/src/loop_command.rs", "synopsis": "docs/CLI_REFERENCE.md#loop-commands; crates/zeno-fcis-cli/src/loop_command.rs", "verdicts": "docs/CLI_REFERENCE.md#loop-commands; crates/zeno-fcis-cli/src/loop_command.rs"}

Run `zeno-fcis guide find "propose and revise a program"` for related cards.

Background: IEEE and The Open Group, *POSIX.1-2024*, Shell Command Language, 2024,
defines shell exit status conventions. See the [shared exit contract](../CLI_REFERENCE.md#exit-codes)
for this interface; the packet defines the finite scope of each verdict.
