---
name: zenofcis-machine-code
description: Search, synthesize or check x86-64 and RV64I word kernels through ZenoFCIS machine-byte proposal sessions, counterexamples and independently replayed artifacts. Profile 2 adds CF/ZF/SF/OF flags, carries, comparisons, selects, up to six inputs and two results. Use for LLM-guided machine-code loops, multi-word and compare-and-select kernels, and embedded kernel emission.
---

# Checked machine-code search

Work from the selected ZenoFCIS checkout with Python 3.11 or newer. Read its
`docs/MACHINE_CODE_SYNTHESIS_27.md` and current
`python3 -B -m tools.machine_code.run --help` before selecting a profile.
This tool complements the finite decision-program loops. The explicit
`zal/word-kernel/1` route connects one modular-word problem; arbitrary ZAL
application lowering and runtime adoption retain separate obligations.

## Choose the contract

Start from exact input/output meaning. Profile1 has two unsigned64 inputs and
one modular64 result, arbitrary entry scratch state, a declared instruction
palette and a16-body-instruction ceiling. It supports x86 SysV register kernels
and RV64I register kernels. Checked overflow, wider arithmetic, effects, memory
and other ABIs require an explicit supported profile; do not narrow the task to
make it fit. Inspect `test-data/machine-code/` for the closed JSON format.

For a 64-bit add, subtract or multiply whose meaning is checked, saturating or
signed, write a `zeno-fcis/machine-contract/1` contract first (fields `schema,
target, operation, signedness, bits, meaning, inputs, grammar, max_instructions,
objective`). `python3 -B -m tools.machine_code.run contract lower --contract
C.json --problem-out P.json` exits 0 with a profile-2 problem, or exits 1 with a
named missing bridge (`effectful-overflow`, `multiword`, `high-product`) or an
unsupported reason. A checked contract lowers to two results: the value modulo
2^64, then a 0/1 status that is 1 on overflow; read the status first. Open the
session on P.json. `contract check --contract C.json --code K.bin` checks bytes
against the lowered problem (exit 0 equivalent, 1 counterexample or refused, 2
inconclusive). Never hand-write a one-result modular problem for such a task.

Profile 2 (`zeno-fcis/machine-problem/2`, targets `x86_64-sysv-register/2` and
`rv64i-register/2`) is registered separately for carries, comparisons and
selects. It declares one to six named u64 `inputs` (x86 `rdi, rsi, rdx, rcx,
r8, r9`; RV64I `a0`..`a5`) and one or two `outputs` (x86 `rax, rdx`; RV64I
`a0, a1`). Expressions add `ashr`, `ult`, `ule`, `slt`, `sle`, `eq` (each 0/1)
and `["ite", c, a, b]` (a when c is not 0). x86 adds ADC, SBB, CMP, TEST, NEG,
SAR, SETcc, MOVZX and CMOVcc; RV64I adds SLT, SRA, SLTI, SLTIU, XORI, ORI, ANDI
and SRAI. Only CF/ZF/SF/OF are modelled: parity conditions refuse, and reading a
flag left undefined (ZF/SF after IMUL, OF after a shift by 2 or more) refuses
with `ReadsUndefinedFlag`. Entry flags are arbitrary and quantified. SETcc
writes only the low byte, so zero the register before the flag-setting
instruction or MOVZX afterwards. Examples: `add128-x86.json`, `umin-x86.json`,
`add128-rv64i.json`. Counterexamples give named inputs, entry registers, entry
flags and expected/actual result lists. The same `open/packet/candidate/search/
resume/emit/replay` commands apply; `open` selects the checker from the schema.
The packet `diagnosis` covers profile 2 (flags in the trace, per-result
expected/actual, `first_missing_subterm` with its result index; comparisons
often live only in flags). `run bench` times profile-2 x86 artifacts against
GCC's code for the same outputs and refuses RV64I artifacts.
ZAL sources reach profile 2 with a `profile zal/word-kernel/2;` line (English
`ZAL word kernel profile 2.`): `inputs a_lo:u64,...;`, `result 1 ... modulo 2^64;`
and `result 2 ...` (English `Return first/second ...`). Comparisons are written
`(a is below b (unsigned))`, `(a is at most b (signed))`, `(a equals b)` and
`(if c then a else b)`. `what-if` takes `--input NAME=VALUE` per input. The
examples are `test-data/zal-word-kernel/add128-x86*.zal` and `umin-x86*.zal`.

## Connect human ZAL requirements

For human wording, use the selected checkout's closed word-kernel profile:

```sh
python3 -B integrations/zal/run.py word-kernel read --source SOURCE.zal
python3 -B integrations/zal/run.py word-kernel show --source SOURCE.zal --revision REVISION
python3 -B integrations/zal/run.py word-kernel question --source SOURCE.zal --revision REVISION --text 'What happens on overflow?'
python3 -B integrations/zal/run.py word-kernel what-if --source SOURCE.zal --revision REVISION --x 18446744073709551615 --y 1
python3 -B integrations/zal/run.py word-kernel open --source SOURCE.zal --revision REVISION --session NEW_SESSION
```

Copy the read result's revision; use `--syntax english` for controlled English.
Review both forms and source maps, including declared wraparound. Then use the
same `word-kernel packet/candidate/search/emit/replay` commands with that exact
revision and outside-source output directories. Search uses source-declared
per-invocation budgets; impose an outer resource budget separately. The wrapper
re-lowers source before replay and keeps the original independent checker.
Its artifacts nest the six original machine files without modifying them.

Use named inputs and every entry scratch assignment in counterexample feedback
to discuss the requirement; never infer acceptance or auto-edit meaning.
MAX+1 evaluates to 0 for this profile. Checked overflow, wider integers,
effects/memory and distributed protocols are unsupported; more inputs, two
results and comparisons need `zal/word-kernel/2` (above).
ZAL remains a broad abstraction language; other profiles stay unchanged.

Body data-memory operations refuse, but x86 RET reads the caller's stack and
changes stack/control state under ABI assumptions. The word-result model does
not establish these effects, flags/CET, faults or timing; RV return assumes a
valid ra/link. Do not describe the kernel as never touching memory or as a
complete ABI proof. The portable examples and focused pure/full-SMT gates
are in `test-data/zal-word-kernel/` and `tools/test_zal_word_kernel.py`.
Run `python3 -B -m unittest tools.test_zal_word_kernel.Pure` for pure controls;
running the test file directly also requires the real installed Z3 checker.

## Propose, check and revise

Keep session/output directories outside frozen source. Open a fresh session:

```sh
python3 -B -m tools.machine_code.run open --problem PROBLEM.json --session NEW_SESSION
python3 -B -m tools.machine_code.run packet --session SESSION
```

Use the packet as the LLM input. Supply actual lowercase `code_hex` bytes in a
`zeno-fcis/machine-proposal/1` packet with bounded descriptive provenance, then:

```sh
python3 -B -m tools.machine_code.run candidate --session SESSION --proposal PROPOSAL.json
python3 -B -m tools.machine_code.run packet --session SESSION
```

For a counterexample, read the packet's advisory `diagnosis`: the register trace after each instruction, the
specification subterms that already appear, and `first_missing_subterm`. Repair that step first. Prefer the
`generic_failure` view to the solver view, because solver counterexamples often use degenerate values such as
zero. The diagnosis never decides acceptance.

Read the verdict, not merely the exit code: counterexample/refused exits1;
inconclusive exits2; malformed/binding/I/O refusal exits65. Preserve failed
proposals and actual witnesses. Correct the candidate using the inputs and
scratch assignments; sample agreement never replaces the full-word check.
Do not alter an existing session's problem or checker bindings.

For local search, choose and record practical limits:

```sh
python3 -B -m tools.machine_code.run search --session SESSION \
  --max-candidates 10000 --max-checks 64 --deadline-ms 5000 --solver-ms 500
python3 -B -m tools.machine_code.run resume --session SESSION
python3 -B -m tools.machine_code.run emit --session SESSION --out NEW_ARTIFACT
python3 -B -m tools.machine_code.run replay --artifact ARTIFACT
```

Search exhaustion does not establish no implementation exists. A stored
incumbent must recheck; missing replay budget remains inconclusive. Emission
produces machine bytes, linkable ELF and matching assembler data, not deployment
authority. Recheck artifacts before any separately authorized target execution.

## Reuse checked kernels

Before searching, look in `test-data/machine-code/kernels/` for a checked
kernel with the same problem. Recheck the whole library, and add a replayed
artifact under a new name (an existing entry is never replaced):

```sh
python3 -B -m tools.machine_code.run library check --dir test-data/machine-code/kernels
python3 -B -m tools.machine_code.run library add --artifact ARTIFACT \
  --dir test-data/machine-code/kernels --name NAME --summary "One line." --route search
```

`check` exits 0 with `checked` only if every entry is `equivalent` at its
recorded cost now; `failed` exits 1 and names each failing entry; refusals exit
65. An entry carries no stored verdict, so cite the fresh check, not the file.

## Judge usefulness

Compare byte/body-instruction cost, setup/search/check/replay costs, and actual
target execution separately. The native test entry point is
`python3 -B tools/test_machine_code_native.py`, and
`python3 -B tools/test_machine_code_profile2_native.py` for profile 2;
missing platform/tool support is not a pass. x86 instruction and flag models
are compared with the executing CPU; RV64I is compared under QEMU only in the
workflow's `rv64i-execution` job, and a local skip is no RV64I evidence. Preserve the default compiler baseline and match the declared
hardening/ABI profile before comparing sizes. Smaller code is not necessarily
faster. Record regressions and equal results as well as improvements.

Retain exact problem, proposals, events, artifacts, commands, checker/solver
identities, limits and failures in the task's configured evidence ledger.
State full-word SMT/model evidence, finite native tests and remaining
ISA/ABI/application/platform obligations separately. Never claim global
optimality or complete embedded-app qualification from these kernel loops.
