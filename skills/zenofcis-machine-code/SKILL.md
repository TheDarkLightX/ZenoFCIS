---
name: zenofcis-machine-code
description: Search, synthesize or check x86-64 and RV64I word kernels through ZenoFCIS machine-byte proposal sessions, counterexamples and independently replayed artifacts. Use for LLM-guided machine-code loops and embedded kernel emission.
---

# Checked machine-code search

Work from the selected ZenoFCIS checkout with Python 3.11 or newer. Read its
`docs/MACHINE_CODE_SYNTHESIS_27.md` and current
`python3 -B -m tools.machine_code.run --help` before selecting a profile.
This tool complements the finite decision-program loops; it has no arbitrary
ZAL-to-native application bridge or runtime adoption command.

## Choose the contract

Start from exact input/output meaning. Profile1 has two unsigned64 inputs and
one modular64 result, arbitrary entry scratch state, a declared instruction
palette and a16-body-instruction ceiling. It supports x86 SysV register kernels
and RV64I register kernels. Checked overflow, wider arithmetic, effects, memory
and other ABIs require an explicit supported profile; do not narrow the task to
make it fit. Inspect `test-data/machine-code/` for the closed JSON format.

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

## Judge usefulness

Compare byte/body-instruction cost, setup/search/check/replay costs, and actual
target execution separately. The native test entry point is
`python3 -B tools/test_machine_code_native.py`; missing platform/tool support is
not a pass. Preserve the default compiler baseline and match the declared
hardening/ABI profile before comparing sizes. Smaller code is not necessarily
faster. Record regressions and equal results as well as improvements.

Retain exact problem, proposals, events, artifacts, commands, checker/solver
identities, limits and failures in the task's configured evidence ledger.
State full-word SMT/model evidence, finite native tests and remaining
ISA/ABI/application/platform obligations separately. Never claim global
optimality or complete embedded-app qualification from these kernel loops.
