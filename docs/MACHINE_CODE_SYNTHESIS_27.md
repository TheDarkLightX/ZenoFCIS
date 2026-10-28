# Experimental machine-code synthesis loops

The machine-code tool searches actual instruction sequences and accepts raw
machine-byte proposals from an LLM or another proposer. A separate byte decoder
and SMT checker establish the declared word-function property. Source emission
and native compilation are separate stages.

This is an experimental 2.7 tool entry point, not a registered replacement for
the Rust CLI's finite synthesis targets or a qualified complete-app compiler.
ZAL retains its broader abstraction meaning. The explicit `zal/word-kernel/1`
profile below connects controlled human wording to this one modular-word
problem format. Other ZAL profiles and full application lowering retain their
own ABI, rejection, effects and metering obligations.

## Registered profiles

| Target | Calling contract | Admitted computation |
| --- | --- | --- |
| `x86_64-sysv-register/1` | `rdi=x`, `rsi=y`, return `rax`; other admitted caller-saved registers initially arbitrary | 64-bit MOV, ADD, SUB, AND, OR, XOR, low IMUL, immediate MOV/shifts, displacement-free LEA, NOP, final RET |
| `rv64i-register/1` | `a0=x`, `a1=y`, return `a0`; `t0/t1` initially arbitrary, `zero=0` | RV64I ADD, SUB, AND, OR, XOR, SLTU, ADDI, SLLI, SRLI, exact NOP and final JALR return |

Each problem declares its exact instruction palette and at most 16 body
instructions. A candidate must consume completely, finish with the single
registered return, and contain at most 256 bytes. Other control flow, memory
operations, system calls, privileged operations, alternate registers and
unregistered encodings refuse. A return byte inside an immediate is data.

The specification quantifies over **all 2^128 pairs of unsigned 64-bit inputs**
and all entry scratch-register assignments. Its arithmetic is modulo 2^64. Checked
overflow, U256/I256 values and multi-result error ABIs require their own explicit
profile; narrowing those tasks to this profile would change their requirements.

The decoder and its concrete/SMT semantics do not import the proposal encoder or
search implementation. Z3, the handwritten instruction model, Python and the
host remain trusted premises. An SMT result here is not a kernel proof of the
complete ISA or physical processor.

## Use the loop

From the selected repository root, use Python 3.11 or newer with the qualified checker
dependency. The Linux x86-64 wheel is pinned by hash in
`tools/machine_code_requirements.txt`; other tool/platform combinations need
their own qualification. Existing local installations are source/binary-bound
in each session, not silently treated as the hash-pinned wheel.

The closed problem schema is shown in `test-data/machine-code/`. Expressions
are JSON arrays with these constructors:

| Meaning | Expression |
| --- | --- |
| Input | `["input", "x"]` or `["input", "y"]` |
| Unsigned word literal | `["const", "195"]`; canonical decimal string in 0 through 2^64-1 |
| Bitwise complement | `["not", expression]` |
| Binary word operation | `[operator, left, right]`, where operator is `add`, `sub`, `mul`, `and`, `or` or `xor` |
| Shift | `["shl", expression, count]` or `["lshr", expression, count]`; integer count in 0 through 63 |

Expressions have at most 64 nodes and depth 16. The profile has no comparison
or conditional constructor. A different mathematical requirement needs either
an equivalent expression in this vocabulary or an explicitly extended profile.

```sh
python3 -B -m tools.machine_code.run --help
machine_work=$(mktemp -d)
python3 -B -m tools.machine_code.run open \
  --problem test-data/machine-code/add64-x86.json \
  --session "$machine_work/session"
python3 -B -m tools.machine_code.run packet --session "$machine_work/session"
```

The packet gives an LLM the complete problem, palette, source identity and
retained feedback. It supplies a data proposal, for example:

```json
{"schema":"zeno-fcis/machine-proposal/1","code_hex":"488d0437c3","provenance":{"kind":"llm","name":"selected proposer","note":"proposal for the declared modular sum"}}
```

Save the proposal outside the frozen source tree, then run:

```sh
python3 -B -m tools.machine_code.run candidate \
  --session "$machine_work/session" --proposal "$machine_work/proposal.json"
python3 -B -m tools.machine_code.run packet --session "$machine_work/session"
python3 -B -m tools.machine_code.run resume --session "$machine_work/session"
```

The accepted sum bytes decode as `lea rax,[rdi+rsi]` followed by `ret`.
A wrong candidate returns actual input/scratch values and expected/actual
results. Revise from that feedback. Descriptive model provenance is not
authenticated origin or approval.

The built-in proposer can also search the declared finite instruction product:

```sh
python3 -B -m tools.machine_code.run search --session "$machine_work/session" \
  --max-candidates 10000 --max-checks 64 --deadline-ms 5000 --solver-ms 500
python3 -B -m tools.machine_code.run emit \
  --session "$machine_work/session" --out "$machine_work/artifact"
python3 -B -m tools.machine_code.run replay --artifact "$machine_work/artifact"
```

Concrete counterexamples filter proposals; surviving candidates still require
the full-word SMT check. Search uses one shared candidate/check/deadline budget.
A selected stored incumbent must recheck within that budget; otherwise the
result is inconclusive. An in-memory result from the current invocation can be
reused on its unchanged candidate. Budget exhaustion is not unrealizability.

Exit 0 means a completed positive operation. Counterexamples/refused candidate
verdicts exit 1; inconclusive/no-checked-candidate exits 2; malformed input,
binding drift and I/O refusal exit 65. Always inspect the JSON status. The tool
does not call a hosted LLM or require model credentials: coding agents consume
packets and supply proposals through their existing tool loop.

## Artifacts and independent replay

Emission first rechecks the selected incumbent and writes:

- `kernel.bin`: actual target instruction bytes;
- `kernel.o`: relocatable little-endian ELF64, machine ID 62 or 243, function `zeno_kernel`;
- `kernel.S`: exact assembler byte directives for the same function;
- the problem, receipt and closed artifact manifest.

Replay checks file identities, current checker/solver inputs, strict schema and
integer types, exact reconstructed object and assembly correspondence, and
the full-word property again. Rehashing altered data alone cannot establish
semantic equivalence or change the registered assurance statement.

Session events retain failed proposals, witnesses and a hash chain. Partial
writes, unexpected files, source drift or an existing lock refuse. Storage and
the host are trusted; this is not authenticated publication or anti-rollback.
Use a new session after a deliberate specification or checker change.

## Qualification and costs

```sh
python3 -B tools/test_machine_code_verifier.py
python3 -O -B tools/test_machine_code_verifier.py
python3 -B tools/test_machine_code_loop.py
python3 -O -B tools/test_machine_code_loop.py
python3 -B tools/test_machine_code_native.py
```

The separate native gate requires a Linux x86-64 host with GNU `gcc`, `as`,
`objcopy`, `readelf` and `nm`. It checks x86 bytes against GNU assembler and actual linked
execution on boundary and deterministic pseudorandom inputs. GNU readelf independently identifies
the RV64I object; it does not execute RISC-V code. A supported RISC-V execution
environment and embedded application integration remain separate obligations.

The objective counts raw kernel bytes or body instructions, excluding the final
return from the instruction count. ELF overhead, synthesis/checking/setup cost,
memory, compilation and measured execution cost are separate quantities.
No global optimality or runtime speedup follows from a smaller candidate.

Keep the default GCC baseline and the explicitly matching register-kernel
baseline. Some distributions add ENDBR64 by default; its four bytes are a
control-flow-hardening difference, not a synthesis improvement. Profile 1 does
not model CET. A deployment using CET needs a separately qualified entry/ABI
profile. Retain equal or worse results, including missed compact instructions.

Apply the [five-part feasibility standard](RESEARCH_FEASIBILITY.md) before
promoting a route. These experiments do not complete native-checker refinement,
installed isolation, full-width DeFi, arbitrary embedded apps or release gates.

## ZAL human-to-machine word-kernel connection

Use `integrations/zal/run.py word-kernel` for this closed profile. It leaves
existing FSM, typed, effects, lens and Haskell-backed ZAL meanings and the
browser workspace unchanged. The symbolic and controlled-English examples in
[`test-data/zal-word-kernel/`](../test-data/zal-word-kernel/) mean the same thing:

```text
profile zal/word-kernel/1;
kernel add64;
target x86_64-sysv-register/1;
inputs x:u64,y:u64;
result (add x y) modulo 2^64;
palette [["mov","rax","rdi"],["add","rax","rsi"],["lea","rax","rdi","rsi",1],["xor","rax","rsi"]];
max-body 3;
objective bytes;
limits candidates=1000 checks=16 deadline-ms=1000 solver-ms=200;
```

Its English result is `Return (x plus y) with wraparound modulo 2^64.` English
also states the target, unsigned inputs, palette, objective and every budget.
Expressions admit `x`, `y`, canonical decimal words, fully parenthesized `not`,
`add/sub/mul`, `and/or/xor`, and `shl/lshr` by a literal 0..63. English uses
`complement`, `plus/minus/times`, `bitwise and/or/xor`, and `shifted left by` /
`shifted right logically by`. There are exactly nine declarations, at most 64
expression nodes and depth 16. Checked overflow, U256/I256, memory operations,
effects, extra inputs, multiple results and distributed protocols refuse.

First read the source and copy its returned revision into later calls:

```sh
python3 -B integrations/zal/run.py word-kernel read --source test-data/zal-word-kernel/add64-x86.zal
python3 -B integrations/zal/run.py word-kernel show --source SOURCE.zal --revision REVISION
python3 -B integrations/zal/run.py word-kernel render --source SOURCE.zal --revision REVISION --to english
python3 -B integrations/zal/run.py word-kernel question --source SOURCE.zal --revision REVISION --text 'What happens on overflow?'
python3 -B integrations/zal/run.py word-kernel what-if --source SOURCE.zal --revision REVISION --x 18446744073709551615 --y 1
python3 -B integrations/zal/run.py word-kernel lower --source SOURCE.zal --revision REVISION
```

Use `--syntax english` for English sources. `show` returns both canonical forms
and the exact lowered problem. Other questions are `What can be clobbered?`,
`Does it access memory?`, `What are the limits?`, `How is a candidate checked?`,
and `What if x is 18446744073709551615 and y is 1?`. That example returns 0 under
the specification; it does not execute candidate bytes.

Use fresh ordinary directories outside source for the connected loop:

```sh
python3 -B integrations/zal/run.py word-kernel open --source SOURCE.zal --revision REVISION --session NEW_SESSION
python3 -B integrations/zal/run.py word-kernel packet --session SESSION --revision REVISION
python3 -B integrations/zal/run.py word-kernel candidate --session SESSION --revision REVISION --proposal PROPOSAL.json
python3 -B integrations/zal/run.py word-kernel search --session SESSION --revision REVISION
python3 -B integrations/zal/run.py word-kernel emit --session SESSION --revision REVISION --out NEW_ARTIFACT
python3 -B integrations/zal/run.py word-kernel replay --artifact ARTIFACT --revision REVISION
```

Proposals use the existing `zeno-fcis/machine-proposal/1` schema. For this
example, `4889f84831f0c3` computes XOR and should yield a counterexample;
`488d0437c3` proposes the sum through LEA. The unchanged independent decoder
and SMT checker decide. Feedback names x, y, every entry scratch assignment,
the required and reported actual results. The human question grants no
acceptance and never edits the requirement. LLMs supply proposal data only.

Lowering binds raw ASCII source, semantic revision, exact expression spans,
workflow limits and bridge/machine sources. English and symbolic forms share a
semantic revision but have distinct raw source hashes. Search limits apply per
invocation; candidate/emit/replay use the declared solver timeout. Existing
session event/byte ceilings apply. Impose a separate outer workflow budget.
Emission retains the original six-file artifact inside `machine/`; its wrapper
adds `source.zal`, `binding.json` and a strict manifest. Replay re-parses and
re-lowers source, compares the nested machine problem, then calls the original
artifact checker again. Rehashing changed human meaning cannot make unchanged
bytes satisfy it. Trusted local storage/loader assumptions remain; hashes do
not authenticate owner intent.

Body data-memory operands refuse. The x86 return **reads the caller's return
address from the stack** and changes stack/control state under ABI assumptions.
Those effects, flags, CET, instruction fetch, faults and timing are outside
this word-result model. RV return assumes a valid ra/link environment. This
does not establish that a kernel never accesses memory or prove a complete
ABI. Emission, compilation, target execution and deployment remain separate.

The explicit observation contract is the returned u64 word under this ISA and
register model. Equal words do not establish whole-machine stuttering, timing
or cost equivalence. `What is observed?` explains that boundary. Failed,
counterexample and inconclusive recorded proposals remain in the session even
when the incumbent is unchanged. Preserve that history beside emitted artifacts;
the selected artifact is not the full search history. Deadlines remain
inconclusive, and instruction caps, byte/body counts and search/check charges
remain separate. Straight-line modeled steps are bounded; temporal `next` /
`within` requirements and general external progress are unsupported.

```sh
python3 -B -m unittest tools.test_zal_word_kernel.Pure
python3 -O -B -m unittest tools.test_zal_word_kernel.Pure
python3 -B tools/test_zal_word_kernel.py
```

The first two gates are pure parser/binding/transport controls with explicitly
supplied checker fixtures. The full third gate invokes the real installed
Z3 checker and identities; missing solver support fails it. These gates are
not native execution, application qualification or release claims.

## Sources and agent skill

The instruction contracts follow the
[Intel ISA manuals](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html)
and the ratified [RV32I](https://docs.riscv.org/reference/isa/v20260120/unpriv/rv32.html)/
[RV64I](https://docs.riscv.org/reference/isa/v20260120/unpriv/rv64.html) specifications.
[STOKE](https://theory.stanford.edu/~aiken/publications/papers/asplos13.pdf)
provides a research precedent for instruction search; the present tool does not
reuse its implementation or claim its optimization results.

The reusable [machine-code skill](../skills/zenofcis-machine-code/SKILL.md)
records route selection, feedback, replay and evidence boundaries.
