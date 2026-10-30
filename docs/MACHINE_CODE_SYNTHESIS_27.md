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

## What a checked kernel guarantees

> Under the stated machine model and environmental assumptions, these exact bytes satisfy these specified
> properties for every admitted input and initial state.

Every qualification in that sentence matters.

**The compiler is out of the trusted path for the checked bytes, but these remain trusted:**

- the byte decoder;
- the instruction semantics, checked against the executing CPU on sampled states but not proved;
- the SMT checker and Z3;
- loading and linking;
- the calling convention;
- the hardware.

**Outside the check:**

- missing or wrong requirements: a modular sum that returns 0 for `MAX + 1` is correct under its contract, and
  wrong for a contract that must refuse overflow;
- integration mistakes;
- timing and side channels;
- unmodelled processor behaviour.

**Proved separately or not at all:** correctness, code size and measured speed. Optimality would need a defined
candidate space, a cost model, and evidence that no cheaper admissible candidate exists.

**Counterexamples:** retained counterexamples reject a repeated failure automatically. They do not make a
proposer learn, and they do not make search terminate.

**Certification:** formal evidence can support assurance objectives, for example under DO-333 alongside
DO-178C. It does not replace them.

## Registered profiles

Profile 1 is listed here. [Profile 2](#profile-2-flags-carries-and-comparisons)
is registered separately and adds flags, comparisons, selects, up to six inputs
and two results; profile-1 problems, sessions, receipts and checks are unchanged.
[Profile 3](#profile-3-memory-regions) adds declared memory regions on x86-64 and RV64I.

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
search implementation. Z3, the authored instruction model, Python and the
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

Expressions have at most 64 nodes and depth 16. Profile 1 has no comparison
or conditional constructor. A different mathematical requirement needs either
an equivalent expression in this vocabulary or an explicitly extended profile,
such as profile 2.

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
results. Revise from that feedback.

Each counterexample in the packet also carries an advisory `diagnosis`. It is computed with the concrete model, and it
covers both the solver's counterexample and the first of 64 deterministic pseudo-random inputs on which the
candidate also fails. Solver counterexamples often use degenerate values such as zero, which make value matching
ambiguous. The diagnosis holds:

- every admitted register after each body instruction;
- every specification subterm with its value, and the earliest point and registers at which that value appears;
- `first_missing_subterm`, the earliest compound subterm the candidate never computes.

For example, for `(xor (mul x 3) (lshr y 1))`, a candidate that shifts left instead of right shows
`(mul x 3)` in `rax` after instruction 1 and reports `(lshr y 1)` as never computed. The diagnosis has no authority:
a value can appear by coincidence, and only the independent checker decides. Descriptive model provenance is not
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

### Exact pruning and split search

Search skips candidates that cannot be needed, using each grammar instruction's
footprint. A footprint is what the instruction reads, may write and always
overwrites. It is derived from the profile checker's own symbolic semantics, and
read conservatively. Four classes are skipped:

- **Duplicates.** An instruction whose final state another palette entry matches
  exactly (proved by Z3, with the same declared flag reads), when that entry has
  a shorter or byte-smaller encoding. An example is RV64I `add` with swapped
  sources.
- **Reorderings.** Two neighbouring instructions that are independent, meaning
  neither writes what the other reads or writes. Swapping them changes neither
  the results nor the cost. A neighbouring pair is never proposed in
  byte-descending order, so at least the byte-least ordering of each class is
  proposed; other orderings may be proposed too.
- **Lost inputs.** Once an input the specification depends on is held by no
  register or flag, no continuation can be equivalent. Dependence is decided
  semantically, by one Z3 query per input, so `(xor x x)` does not require `x`.
- **Dead instructions.** An instruction whose every write is overwritten before
  being read, or is never read and is not a result. Deleting it from an admitted
  candidate gives an admitted, strictly cheaper sequence with the same results,
  which the length-ordered search has already proposed. A refused candidate can
  become admitted by the deletion, for example a dead conditional move that
  reads an undefined flag. Refused candidates are never accepted, so skipping
  them loses nothing.

These four rules are semantic: every skipped admitted candidate is either never
equivalent (a lost input) or has a kept, admitted candidate with no greater cost
and the same results. Separately, the cost bound stops the search once no longer
sequence can reach the incumbent's cost; it preserves the optimum, not one
representative of every behaviour.

A search that exhausts its grammar therefore reports the same optimum as before,
provided no solver answer was unknown. Search results report `duplicate_pruned`, `order_pruned`,
`lost_pruned`, `dead_pruned` and `bound_skipped`, and `--no-prune` restores the
full enumeration. Measured on the
test problems (checker decisions unchanged):

| Problem | Proposed without pruning | Proposed with pruning |
| --- | --- | --- |
| `add64-x86` | 156 | 29 |
| `add128-x86` | 1,555 | 228 |
| `minmax-x86` | 100,000 (budget exhausted) | 438 (grammar exhausted) |
| library `signed-saturating-add-x86` | 100,000 (budget exhausted, no candidate) | 23,897 of 960,800 (grammar exhausted, 31-byte kernel) |
| memory `copy2-sum-x86` | 3,906 | 366 |
| memory `copy2-sum-rv64i` | 3,906 | 296 |

For [profile 3](#profile-3-memory-regions), every region word is a footprint
location named `<region>[<index>]`, and a load or store also reads its base
register, because admission needs that register to still hold the region base.
Each access also writes its own observed location `admitted:<form>`. Deleting an
access can turn a refused candidate, one that accesses through a redefined
base, into an admitted one, so an access is dead only when an identical access
follows it. The results compared at the end include every read-write region
word. Without the `admitted:` locations, the exactness test loses the cheapest
refused candidate of `copy2-sum-rv64i`.

Admission also tracks which bases an earlier instruction wrote, and the register
values do not show that history: `or rdi, rdi` leaves `rdi` unchanged, but a
later access through `rdi` is then refused. For each base, the footprint asks
the profile-3 admission rule whether an instruction marks that base written. If
it does, the instruction writes a `based:<base>` location, which every access
through that base reads. Such an instruction is therefore never reordered past
the access, deleted before it, or merged with a duplicate that leaves no mark.

An independent review found the counterexamples that motivated this. Without
the marks, pruning lost the only equivalent kernel of a three-instruction
problem, and the byte-least kernel of another. Both problems are now tests.

Search and `split-search` start their concrete filters from the session's
earlier counterexamples, as well as the seed states. Both stop at the deadline
even while pruning is still skipping candidates.

`split-search` handles two-result problems. It searches for each result alone,
then joins two parts when their footprints show that the second part neither
reads what the first may write nor overwrites the first part's result. A join
that agrees on the filter states goes to the checker like any proposal.

The joined kernel need not be optimal, and the result says so (`"split": true` and
a `minimality` note). For example, the two parts of `minmax`
each need their own comparison. Recorded as the incumbent, it bounds a later
`search`:

```sh
python3 -B -m tools.machine_code.run split-search --session "$machine_work/session"
python3 -B -m tools.machine_code.run search --session "$machine_work/session"
```

The footprints and joins only steer the search. The checker still decides every
candidate, and `global_optimality` stays false: an exhausted grammar says
nothing about instructions outside it.

The tests compare, on sixteen small problems up to three instructions, the
cheapest kept candidate with the cheapest candidate overall, for each objective.
They do this per behaviour class, where a class is the results (and, for memory
kernels, every read-write region word) on 24 fixed entry states. The problems
include shifts, `imul`'s undefined flags, conditional moves, partial `setcc`
writes and the six memory kernels.

The tests also check that the checker refutes every candidate skipped for a lost
input, through each problem's full length. Planted defects fail these tests:
- an unobserved result;
- omitted flag reads or writes;
- an ignored write-write conflict;
- empty shift footprints;
- unproved duplicates;
- a syntactic "required";
- copies not counted as holding an input;
- a store's region-word write omitted, which fails all six memory problems.

An independent review also checked every admitted form's footprint symbolically
(59,322 profile-1 and profile-2 forms) and checked dead and order preservation
with full-word Z3 queries on 1,245 sequences. These checks are bounded evidence,
not a general proof.

These are the semantic pruning and decomposition ideas of McSynth and McSynth++
([Srinivasan 2017](https://research.cs.wisc.edu/wpis/papers/tr1844.pdf);
[Srinivasan, Sharma and Reps 2016](https://research.cs.wisc.edu/wpis/abstracts/oopsla16-synthesis.abs.html)),
restricted to the skips that lose no candidate. The lost-input rule is the
McSynth++ bits-lost pruner with a semantic notion of "required", which is what
keeps it exact. The reordering rule is the partial-order reduction of
independent actions, keeping at least one representative per class (Godefroid,
*Partial-Order Methods for the Verification of Concurrent Systems*, LNCS 1032,
1996). The duplicate rule is equivalence reduction over single instructions
(Smith and Albarghouthi, VMCAI 2019).

### No cheaper program in the palette

An exhausted search shows that no cheaper candidate exists in the grammar, but
only after enumerating it. `realize` asks the solver the question directly: does
any admitted program with a cost below a bound exist?

```sh
python3 -B -m tools.machine_code.run realize --session "$machine_work/session"
python3 -B -m tools.machine_code.run realize --problem test-data/machine-code/umin-x86.json \
  --below 11 --out "$machine_work/realize.json"
python3 -B -m tools.machine_code.run realize --problem test-data/machine-code/umin-x86.json \
  --replay "$machine_work/realize.json"
```

It asks one question per length k whose cheapest possible cost is below the
bound:

- **Query.** Each of the k slots picks a form through a bounded selector that
  ranges over exactly the problem's `grammar`, the list the enumerator walks.
  Each form's effect comes from the profile checker's own SMT semantics. On
  profile 2, a read of an undefined flag is excluded as a constraint, just as the
  checker refuses it. With the `bytes` objective, the total size is also bounded.
- **Solving.** The query must give the specified results on a set of example
  states, starting with the search's filter states. A solution is decoded to
  bytes and goes to the checker. A counterexample becomes a new example, and the
  question is asked again.
- **Answers.** Unsat means that no selector choice fits even the examples. It is
  recorded as "no admitted program of length k with `bytes` cost below N exists
  under the stated model and palette", with the hash of the query and the
  examples that produced it. A solution the checker accepts is recorded with its
  bytes, and the run stops there. A solver `unknown`, a spent budget or an
  inconclusive check is `inconclusive`, never unrealizable.

`--replay` re-establishes the record's whole claim, not only its queries.

- **Lengths:** it recomputes which lengths the problem, objective and bound make
  eligible, which the bound excludes and which an empty palette leaves without
  any candidate. Every eligible length must appear once, in order. A realizable
  record may stop at its realizable length.
- **Status:** the record's status must follow from its entries, and every unsat
  entry must carry the exact statement for its length and bound.
- **Queries:** each query is rebuilt from its examples. It must have the recorded
  hash and stay unsat.
- **Bytes:** a realizable entry's bytes must recheck equivalent, at that length
  and below the bound.

A record whose bound was raised afterwards therefore fails, because a length the
new bound makes eligible is missing. So do records with missing, repeated or
relabelled lengths, a changed status, statement or objective, or another
mode's entries.

A bound of 0 is admitted: no length can cost less, so every length is excluded
and the record is unrealizable without a query. This is what `--session` asks
when the best candidate is an empty body. With an empty palette, every positive
length has no candidate at all, and the record lists it under
`excluded_by_empty_palette`. `--session` asks below the session's best checked
candidate after rechecking it. When every length is unsat, that candidate is the
least-cost program within the palette and the length bound, under the registered
model. `global_optimality` stays false: the answer says nothing about
instructions outside the palette, longer programs or other instruction sets.

Profile 3 is supported too. Region words are quantified state next to the
registers and flags, and loads and stores step through the profile-3 checker's
own semantics. Results, every read-write region word and every read-only region
word, which must stay unchanged, are compared on each example. The checker's
order-dependent admission rules are constraints of the query: a slot may not
address a region through a base register an earlier slot wrote, and on x86 may
not read a flag an earlier slot left undefined.

An unsat is only as exact as the claim that the query ranges over the
enumerator's candidate space. `tools/test_machine_code_realize.py` therefore
checks it against exhaustive exact enumeration, in both directions. It covers
every test, library and memory-kernel problem with at most 20,000 candidate
sequences (23 problems on all three profiles), plus six built for the test.
- one whose only short program uses the palette's last form;
- one with no program at all;
- one with region-base writes;
- one where IMUL leaves ZF undefined before a CMOVE.
- one whose empty body is already equivalent;
- one with an empty palette.

For each, the enumerator must exhaust its grammar. `realize` below its least cost
must prove every length unsat. `realize` below the least cost plus one must
return checked bytes of exactly that cost. With no program, `realize` without a
bound must prove every length unsat, apart from those an empty palette leaves
without candidates. All 29 agree.

A second control checks the admission constraints directly. With no examples
and no bound, the set of selector sequences the query admits must equal the set
the checker's own admission accepts. It enumerates both on palettes with flag
reads, undefined flags and region-base writes, at lengths 0 to 3.

Five planted defects are registered with the checker controls for this part:
- a selector range that drops the last form;
- an ignored cost bound;
- a dropped region-base rule;
- admitted undefined-flag reads;
- a replay that skips the eligible-length check.

Each is detected by its intended control under `python3 -B` and `python3 -O -B`.

`realize` exits 0 for `unrealizable`, `realizable` and `replayed`, 1 for a failed
replay, 2 for `inconclusive`, and 65 for refused input or an existing `--out`.
The method is component-based synthesis by counterexample-guided refinement
([Gulwani, Jha, Tiwari and Venkatesan, PLDI 2011](https://www.microsoft.com/en-us/research/publication/synthesis-loop-free-programs/)),
used here as unrealizability evidence for a finite palette, as complete
super-optimizers do ([Lu and Bodík, OOPSLA 2025](https://lsrcz.github.io/files/OOPSLA25.pdf)).

#### Partitioned subspaces

`--groups G` splits the palette into G contiguous groups of forms. Each slot
picks one group, so a length k has G^k subspaces. Together they cover every
candidate exactly once. Each subspace has its own query: its slots range only
over their group's forms. `--workers W` (at most 4) decides the subspaces in W
processes, each with its own Z3 context, and all work shares one deadline.

```sh
python3 -B -m tools.machine_code.run realize --problem test-data/machine-code/umin-x86.json \
  --below 11 --groups 2 --workers 4 --out "$machine_work/realize.json"
```

- **Combining:** a length is unsat only when every one of its subspaces is unsat.
  A subspace that finds bytes the checker accepts makes the length realizable. A
  subspace that runs out of time or rounds makes the length `inconclusive`.
- **Record:** each subspace keeps its group ranges, its query hash and its
  examples. `--replay` checks, without the code that made the partition, that
  each slot's ranges are contiguous and cover the palette. It also checks that
  the subspaces are exactly their product. Then it rebuilds and re-solves every
  subspace query.
- **Unchanged default:** without `--groups`, the query is byte-for-byte the one
  above, and older records still replay.

`--split-ms P` makes the decomposition adaptive and hierarchical. A subspace
whose query does not answer within P ms is halved on its widest slot. Its two
children replace it and inherit the counterexamples it found. Replay's cover
check therefore does not assume a grid. It requires every subspace to lie in the
palette, no two to overlap, and their sizes to add up to the whole space.

The cross-check also runs in partitioned mode, with 2 groups and 4 workers, on
the same 29 problems, and all agree. With a 1 ms probe, which splits nearly
everything, it is cross-checked and replayed on three problems. Two more planted
defects are detected under both modes: a partition that omits its last group,
and an undecided subspace counted as unsat.

Measured wall times are below. They come from paired runs, one mode after
another, on an AMD Ryzen Threadripper 1950X shared with other work (load average
5 to 13). The 60 s limit applies to each query, and the run deadline was 900 s
for the 32-form problems and 1,800 s for the others. Each answer is the same in
every mode where it was decided. The decoys are admitted forms over the
problem's own registers. 32 forms is the profile limit.

| Problem (forms, bound) | Single query | 2 groups, 4 workers | 3 groups, 4 workers | Adaptive, 4 workers |
| --- | --- | --- | --- | --- |
| `minmax-x86` (8, below 5 instructions) | 1.4 to 1.6 s | 1.7 s | 3.4 s | 1.7 s (probe 2 s or 5 s, no split) |
| library signed saturating add (7, below 31 bytes) | 10.0 to 12.5 s | 33.5 s | 197.3 s | 12.4 s (probe 2 s), 10.6 s (5 s) |
| the same with 10 decoys (17) | 83.3 to 101.2 s | 251.5 s | not completed | 677.2 s (probe 2 s, 581 splits), 296.7 s (5 s, 99 splits) |
| `minmax-x86` with 24 decoys (32) | 7.8 s | 7.4 s | not run | 9.0 s (probe 10 s, no split) |
| signed saturating add with 25 decoys (32) | lengths 0 to 4 unsat; 5 to 7 time out (277.7 s) | lengths 0 to 5 unsat; 6 and 7 inconclusive at the deadline | not run | lengths 0 to 5 unsat; 6 and 7 inconclusive at the deadline |

At these palette sizes, splitting does not make a decided question faster.

- **Costs:** every subspace pays for building its own query, and repeats the
  counterexample rounds from the filter states. A split also discards the work
  its parent's query had done.
- **Gain:** on the hardest problem, splitting decides one more length, because
  each subspace gets its own query time limit. It does this by spending more
  solver time in total, not by being faster.
- **Default:** the single query stays the default. Use `--groups` or
  `--split-ms` when a length times out.

HieraSynth's gains come from instruction sets of hundreds of instructions. Our
problems are capped at 32 forms.

Exit 0 means a completed positive operation. Counterexamples/refused candidate
verdicts exit 1; inconclusive/no-checked-candidate exits 2; malformed input,
binding drift and I/O refusal exit 65. Always inspect the JSON status. The tool
does not call a hosted LLM or require model credentials: coding agents consume
packets and supply proposals through their existing tool loop.

### A second solver for "equivalent"

`second-solver` rechecks every entry of a library directory. It then gives the
query of each `equivalent` verdict to cvc5, the 1.3.3 release that the
formal-tools workflow pins by hash. The verdict counts as confirmed only when
cvc5 also answers unsat. A different answer, a timeout or another cvc5 version
reports `inconclusive`.

```sh
python3 -B -m tools.machine_code.run second-solver --dir test-data/machine-code/kernels \
  --cvc5 "$ZENO_FCIS_CVC5"
```

Both solvers read the same SMT-LIB text, Z3's rendering of the checker's query.
The query is recorded by briefly replacing Z3's solver class. One lock serializes
every such recording, so overlapping calls in one process each send their own
query; a control interleaves two calls to show this.
This narrows the trust placed in one solver. It does not check the instruction
models or the encoding. On 2026-10-30 every kernel in `kernels/` (13) and
`memory-kernels/` (6) was confirmed. `tools/test_machine_code_second_solver.py`
runs only when `ZENO_FCIS_CVC5` names that cvc5. A planted disagreement, where
the query loses its assertions, is reported `inconclusive`.

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

## Checked kernel library

A developer who needs a branch-free 128-bit add, a saturating add or an
absolute value should not have to rerun a search or trust old output. The
library in `test-data/machine-code/kernels/` keeps each checked kernel as one
entry: the problem it was checked against, the exact bytes, the cost and how the
bytes were found (`proposal` or `search`). It holds 12 kernels across both
profiles and both targets, such as `add128-x86`, `add128-rv64i`, `sub128-x86`,
`umin-x86`, `signed-saturating-add-x86` and `abs-x86`.

```text
python3 -B -m tools.machine_code.run library check --dir test-data/machine-code/kernels
python3 -B -m tools.machine_code.run library add --artifact ARTIFACT --dir test-data/machine-code/kernels \
    --name NAME --summary "One line." --route proposal
```

`check` rechecks every entry now with the independent checker for its profile,
requires the recorded cost, and rebuilds the ELF object and assembly from the
bytes. It reports `checked` (exit 0) only when every entry is `equivalent`;
otherwise `failed` (exit 1), with `counterexample`, `cost-differs` or `refused`
on each failing entry. An empty library, a stray file, a misnamed entry or an
unknown field fails. `add` imports only an emitted artifact that replays here,
and never replaces an existing entry (exit 65).

Entries store no verdict or receipt. A receipt binds the checker identity (the
checker sources, Python and Z3 hashes), so it would go stale after any checker
edit or on another host. A fresh recheck does not. The library check gives the
same ISA-model guarantee as a session check, for each entry on the current
checker. On its own it does not establish native execution, timing, global
optimality or fitness for a particular use. CI runs
`tools/test_machine_code_library.py`, which also checks that a wrong kernel, a
wrong cost and malformed entries fail.

```text
python3 -B -m tools.machine_code.run library check --native --dir test-data/machine-code/kernels
python3 -B -m tools.machine_code.run library bench --dir test-data/machine-code/kernels --out NEW_DIR \
    [--iterations 1000000] [--repeats 21]
```

`check --native` also executes each x86 entry. It needs a Linux x86-64 host
with gcc.

- **Method:** each entry's ELF object is rebuilt from its bytes and linked with a
  generated C runner.
- **Comparison:** every result word is compared with the problem's integer
  reference, `model.evaluate` for profile 1 and the profile-2 checker's
  expression evaluator for profile 2. Profile-3 entries also compare every
  region word; see [native memory runs](#native-memory-runs).
- **Vectors:** every combination of eight edge values per argument when there
  are at most 4,096 of them, otherwise 4,096 sampled edge vectors. Then 4,096
  deterministic pseudo-random vectors.
- **RV64I:** each RV64I entry reports `native: not-run`, reason `needs RV64I
  execution`.
- **Unsupported host:** a host that cannot run x86 code natively reports
  `inconclusive` (exit 2), never a pass.
- **Failure:** a mismatch or a runner error fails the library (exit 1), and
  reports the first mismatching arguments with their expected and actual
  results. This happens even when the model check passed.
- **Result:** on an AMD Ryzen Threadripper 1950X, the 10 x86 entries in the
  repository agreed on 49,608 vectors. The 2 RV64I entries were not run.

Native agreement covers the vectors run on that processor only.

`bench` rechecks every entry first. It then times each x86 entry that
rechecked, using the paired harness of `run bench`, against GCC's `-O2` code
for the same expressions. It writes `bench.json` and one table, `bench.md`. The
table records the CPU, the load average at the start and at the end, the
compiler, the iterations and the repeats, and states *This machine and load
only; not a portable speedup claim.* An entry that does not recheck is not
timed, and the result is `failed`. RV64I rows read `not-run: needs RV64I
execution`.

`tools/test_machine_code_library_native.py` covers both commands. It checks
that a planted wrong entry fails natively, including a wrong second result that
a stubbed model check accepts. It also checks that an unsupported host is
inconclusive and that the table keeps its scope.

## Qualification and costs

```sh
python3 -B tools/test_machine_code_verifier.py
python3 -O -B tools/test_machine_code_verifier.py
python3 -B tools/test_machine_code_loop.py
python3 -O -B tools/test_machine_code_loop.py
python3 -B tools/test_machine_code_native.py
```

The instruction model itself is checked against the executing CPU, which serves as the oracle, following the
differential method for testing CPU emulators (Martignoni et al., ISSTA 2009):

```sh
python3 -B tools/test_machine_code_conformance.py
```

It runs on Linux x86-64 with GNU `gcc`, `as`, `objcopy` and `nm`.

- **What it enumerates:** every admitted x86 form. That is the seven register ALU operations over all register
  pairs, SHL/SHR with every count, MOVABS with edge immediates, LEA over every base, index and scale, and NOP.
  That is 4,699 forms.
- **Encoding and decoding:** each encoding is compared with GNU `as`, and each form must round-trip through the
  independent decoder.
- **Execution:** each form runs from full register states, 16 by default, set with
  `ZENO_MACHINE_CONFORMANCE_STATES`. The destination must equal the concrete model on every state and the SMT
  model on a subset, and every other admitted register must be unchanged.
- **Controls:** two planted model defects, a wrong LEA scale and an arithmetic right shift, must be detected.

A 64-state run on an AMD Ryzen Threadripper 1950X compared 300,736 executions with the concrete model and
28,194 with the SMT model, with no disagreement. Agreement covers the sampled states on the executing processor.
It is not a proof of the ISA or of other processors. Hand-written x86 semantics are known to be error-prone:
stratified synthesis found errors in 58 manually specified instruction variants (Heule et al., PLDI 2016).
The RV64I model has not yet been compared with any RISC-V execution environment.

The separate native gate requires a Linux x86-64 host with GNU `gcc`, `as`,
`objcopy`, `readelf` and `nm`. It checks x86 bytes against GNU assembler and actual linked
execution on boundary and deterministic pseudorandom inputs. GNU readelf independently identifies
the RV64I object; it does not execute RISC-V code. A supported RISC-V execution
environment and embedded application integration remain separate obligations.

The objective counts raw kernel bytes or body instructions, excluding the final
return from the instruction count. ELF overhead, synthesis/checking/setup cost,
memory, compilation and measured execution cost are separate quantities.
No global optimality or runtime speedup follows from a smaller candidate.

Measure time with the paired benchmark, never infer it from size:

```sh
python3 -B -m tools.machine_code.run bench --artifact ARTIFACT --out NEW_DIR [--iterations N] [--repeats R]
```

- **Replay first:** a kernel is timed only after its artifact replays. A changed artifact is refused before
  any timing.
- **Baseline:** generated from the problem's expression as C and compiled with `gcc -O2`.
- **Harness:** both functions go through the same function-pointer harness, in a latency mode (dependent calls)
  and a throughput mode (independent inputs). Repeats alternate kernel and baseline, with RDTSC between LFENCEs.
- **Report:** median ticks per call and the kernel/baseline ratios, with the CPU, load and compiler.

Example: the searched 64-bit sum (`lea rax,[rdi+rsi]; ret`, 5 bytes) against GCC's `endbr64; lea; ret`
(9 bytes), on an AMD Ryzen Threadripper 1950X under load, measured latency ratio 0.999 and throughput ratio
0.97. At this size the call dominates, and the 4 saved bytes buy no measurable time. Results hold for that
machine, load and call ABI only. They are not a portable speedup claim.

### Calling-convention qualification

`tools/machine_qualification/abi.py` checks that a kernel is a correct leaf
function. It covers obligations A-01 to A-07 of the qualification
design, for the profiles `linux-x86_64-sysv-u64-leaf/1` and
`linux-rv64-lp64-u64-leaf/1`:

| Obligation | Check | Evidence |
| --- | --- | --- |
| A-01 decoding | the whole callable decodes, ends in the exact return (`c3`, or `jalr x0, 0(ra)`), with at most 16 body instructions | closed decoder |
| A-02 result | the word query is unsat | solver, relative to the generated model |
| A-03 frame | every register a body instruction may write is caller-saved (flags are scratch) | footprints from the checker's own semantics |
| A-04 return | the return transition gives `RIP = [S]`, `RSP = S + 8` (x86) or `pc = ra & ~1` with `ra`, `sp` unchanged (RV64) | solver over the return rule, given A-03 |
| A-05 termination | straight-line, with no admitted operation that faults in the model | admitted operation set |
| A-06 coverage | the recorded query is exactly the unrestricted mismatch formula, rebuilt from the problem and the bytes and compared by Z3; an entry layout chosen independently of the inputs | recorded query |
| A-07 model | the evidence kind: GNU `as` and CPU conformance (x86); GNU `as` and QEMU in CI (RV64I) | stated, not proved |

These checks are weaker than the design's full obligations, and the gate says
so. A-03 covers the modelled general registers and flags, not the extended-state
frame. A-04 proves the return rule, not a composed permission, canonical-address
and protection model. A-05 excludes faulting operations by name rather than
encoding fetch and return faults. A-07 states its evidence kind without binding
an ISA edition.

The native harness (x86 only) calls each kernel through a trampoline generated
at run time.

- **Call site:** 16-byte aligned, so the kernel enters with `RSP + 8` a multiple
  of 16.
- **Before the call:** distinct canaries go into `rbx`, `rbp` and `r12` to
  `r15`, and into the 16 words below the return slot. MXCSR and the x87 control
  word are recorded.
- **At the true continuation:** the first instruction sets a flag. The return
  slot is then compared with that continuation's address, so a kernel that
  returns anywhere else is reported.
- **Immediately after the return:** the harness captures the results, those
  registers, the stack pointer, the canaries, DF, MXCSR and the x87 control
  word.
- **Refusals:** a run of zero vectors is refused. A library directory with no
  applicable entry reports `not-applicable`, never `qualified`.

Fixtures outside every palette are each reported as their fault. These cover
the five that return normally, listed below, plus three from the third review:
- a rewritten return address;
- a changed MXCSR rounding mode;
- an alignment probe, which shows the kernel's entry alignment.

The five that return normally:
- a callee-saved write;
- a stack write;
- DF left set;
- a wrong stack delta;
- a wrong result.

```sh
python3 -B -m tools.machine_qualification.abi --dir test-data/machine-code/kernels
```

On 2026-10-30 all 13 library kernels met A-01 to A-07. The 11 x86 kernels
conformed natively on 512 vectors each; RV64I kernels are not run here.

**Memory kernels.** Profile-3 kernels use the profiles
`linux-x86_64-sysv-u64-region-leaf/1` and `linux-rv64-lp64-u64-region-leaf/1`.
Region bases arrive in the argument registers the problem names. Three
obligations gain a memory part:

- **A-03 frame:** the footprints' memory writes must be read-write region
  words only, and profile-3 admission must pass. Nothing else in memory is
  written: every store is an aligned word inside a read-write region.
- **A-04 return:** each store is placed at its region word's layout address,
  and the return slot must still hold `R`.
- **A-06 coverage:** region `k` starts at `0x100000000000 + k * 0x10000`. The
  regions are 64-byte aligned, disjoint and at least 1 MiB from the entry
  stack window, chosen before and independently of any input.

A-05 admits loads and stores only when admission passed, and A-07 states that
the x86 memory forms have native library runs but no per-form conformance, and
that RV64I `ld` and `sd` have no differential evidence yet.

The native harness passes each region's address in its base register. Each
region is a static, 64-byte-aligned array with 8 canary words before and after
it. After the return it checks everything the register harness checks. It also
checks the read-write words against the integer reference, that the read-only
words are unchanged, and that every out-of-region canary is unchanged. Three
more planted fixtures outside every palette return normally. Each is reported
as exactly its fault:
- a store one word past a region;
- a store into a read-only region;
- a callee-saved write.

A dropped store is reported as a region-word fault.

```sh
python3 -B -m tools.machine_qualification.abi --dir test-data/machine-code/memory-kernels
```

On 2026-10-30 all six memory kernels met A-01 to A-07, and the three x86
kernels conformed natively on 512 vectors each. RV64I is not run here.

The CET profile `linux-x86_64-sysv-u64-leaf-cet/1` prefixes the legacy fragment
with `ENDBR64`. The fragment's obligations are checked, and the prefixed
callable runs natively. Active IBT and shadow-stack modes are reported
unsupported: this processor lacks `ibt` and `user_shstk`, and no active-mode
harness exists yet. LP64D, which needs a different object ABI and a D-capable
environment, is not claimed; the emitted RV64 object keeps `e_flags = 0`.

### Analysing supplied paired measurements

`tools/machine_qualification/performance.py` implements the qualification design's frozen paired-ratio analysis
over measurements that someone else supplies. It runs, times and measures nothing, and it makes no measurement
claim of its own.

```sh
python3 -B -m tools.machine_qualification.performance --plan PLAN.json --observations ROWS.json
```

- **Plan:** a `zeno-fcis/machine-performance-plan/1` plan is frozen at 20 sessions of 6 candidate/baseline pairs,
  3 AB and 3 BA each. It records the seed and the resulting order schedule, one call count for both arms, the
  number m of preregistered comparisons, alpha 1/20 and the margin 1/50.
- **Rows:** the `zeno-fcis/machine-performance-observations/1` rows bind the plan's hash. A changed plan
  therefore needs new measurements.
- **Refused input:** duplicate keys, unknown fields, non-integer numbers and non-canonical decimal strings are
  refused (exit 65).
- **Invalid measurements:** each reason is listed, and nothing is classified (exit 1). The reasons are a reused
  observation or session identity, a missing or repeated pair, a zero duration, unequal call counts, an order that
  differs from the schedule, wrong outputs and a changed artifact.
- **Analysis:** uses `fractions.Fraction` only.
  - Each pair's ratio is candidate over baseline elapsed nanoseconds.
  - A session's summary is the mean of its 3rd and 4th sorted ratios.
  - The point estimate, the *median paired elapsed-time ratio*, is the mean of the 10th and 11th sorted summaries.
  - The interval runs from the k-th to the (21-k)-th summary. k is the largest rank whose two-sided binomial tail
    is at most alpha/m: 6 for one comparison (coverage 1005176/1048576), and 4 for 7 or 14.
- **Classification:** with delta = 1/50, the result is `improved` when the upper endpoint is below 49/50,
  `practically_equivalent` when the interval lies in [49/50, 51/50], `regressed` when the lower endpoint is above
  51/50, and `inconclusive` otherwise or when no rank is admissible.
- **Presentation:** decimal strings round lower endpoints down and upper endpoints up. They never decide.

The coverage statement needs independent identically distributed representative sessions, which an analysis
cannot establish. The report is not mean execution time, application savings, instruction latency or a portable
speedup claim. It also does not establish the measurement's truthful acquisition. The test
`tools/test_machine_qualification_performance.py` uses fabricated rows only.

Keep the default GCC baseline and the explicitly matching register-kernel
baseline. Some distributions add ENDBR64 by default; its four bytes are a
control-flow-hardening difference, not a synthesis improvement. Profile 1 does
not model CET. A deployment using CET needs a separately qualified entry/ABI
profile. Retain equal or worse results, including missed compact instructions.

Apply the [five-part feasibility standard](RESEARCH_FEASIBILITY.md) before
promoting a route. These experiments do not complete native-checker refinement,
installed isolation, full-width DeFi, arbitrary embedded apps or release gates.

### Planted-defect controls for the checkers

`tools/machine_qualification/controls.py` registers named planted defects in the two independent checkers, after
mutation analysis (DeMillo, Lipton and Sayward, IEEE Computer 1978):

```sh
python3 -B -m tools.machine_qualification.controls list
python3 -B -m tools.machine_qualification.controls run [--defect NAME]... [--out REPORT.json]
```

- **Defects:** each one is a single source edit to `verify.py`, `verify2.py` or `verify3.py`. Examples are a 32-bit immediate,
  unextended RISC-V immediates, scratch registers or entry flags fixed instead of quantified, a dropped symbolic
  instruction, five-bit shift counts, wrong CF, ZF, SF and OF rules, a writable x0, an unknown solver answer read
  as equivalence, and a counterexample reported without its concrete replay. The fourteen profile-3 defects
  include a byte displacement used as a word index, an RV64I store offset cut to five bits, an admitted 32-bit
  access or load into x0, an admitted store to a read-only region, a misaligned or
  out-of-region access, a redefined base, a store dropped from the symbolic run, and a region word, base pointer
  or scratch register fixed instead of quantified. Each defect names the control expected to expose it.
- **Specimens:** each edit is applied to its own copy of the checker package, the control modules and the kernel
  library. The specimen's checker identity is computed from its own sources, and the run checks that the control
  imported that copy.
- **Outcomes:** `detected` (the intended control failed on a test assertion), `survived` (it passed),
  `inconclusive` (a crash, skip or timeout, an assertion raised inside the checker, or an identity or
  session-binding refusal) or `invalid-specimen` (the edit does not apply once, the copy does not compile or
  import, or the control is missing). Only `detected` counts as a detection.
- **Positive controls:** the three verifier suites, every kernel in `test-data/machine-code/kernels` and
  `test-data/machine-code/memory-kernels`, and every intended control must pass on an unmodified specimen first. Each control runs under `python3 -B` and `python3 -O -B`.
- **Exit:** 0 when every defect is detected by each of its controls in both modes, 1 when a positive control fails
  or a defect survives, and 2 otherwise.

A defect may also name a second control that must detect it, run and reported in the same way.

At 0dd0dce1 the first 29 defects were detected in both modes. Three of them, a concrete SRA that masks its count to
five bits, a concrete SF read from bit 62 and a symbolic LE that ignores ZF, passed the profile-2 verifier suite and
the kernel library then, so only the hardware conformance runs and the RV64I model comparison detected them. Unit
tests in `tools/test_machine_code_profile2_verifier.py` now check both models without native execution: an SRA by
32 to 63 of a negative value, SF on results whose bits 63 and 62 differ, and LE on equal values. They are the intended
controls of those three defects and of their mirror defects in the other model, with conformance kept as the second
control. A detected defect shows that one control distinguishes one edit. It does not show that the checker is
complete. `tools/test_machine_qualification_controls.py` checks the outcome classes on small real specimens.

Seven defects cover the immediate forms. The profile-2 verifier tests are their intended controls, and five of them
also have a hardware or decoder-scan conformance test as a second control:

- an 8-bit, a 32-bit or a RAX short-form immediate zero-extended in the decoder;
- a negative immediate zero-extended in the symbolic model;
- a concrete CMP carry computed against the unwrapped negative immediate;
- an accepted `81 /n id` whose value fits 8 bits;
- an accepted `81 /n id` on RAX.

### Search pruning: exactness, planted defects and measurement

```sh
python3 -B -m tools.machine_qualification.pruning_exactness run [--case NAME]... [--out REPORT.json]
python3 -B -m tools.machine_qualification.pruning_defects run [--defect NAME]... [--jobs N] [--out REPORT.json]
python3 -B -m tools.machine_qualification.search_bench run --out REPORT.json [--no-prune] [--repeats N]
python3 -B -m tools.machine_qualification.search_bench compare BEFORE.json AFTER.json
```

- **Exactness:** for twelve small grammars with specifications over profiles 1 and 2 on both targets, and for the
  six profile-3 memory kernels over their own grammars, every sequence is enumerated with the footprint pruning off
  and on. A twin of a removed sequence is a kept, admitted sequence of no greater cost, in bytes and in
  instructions, and of no greater search rank (objective cost, other cost, then the code's hex), whose observed
  results Z3 proves equal from every entry register, flag and region word: the result registers and, on profile 3,
  every read-write region word. With the lost-input rule turned off, every sequence
  removed as dead, out of order or a duplicate needs a twin. With the full pruning, every sequence removed because a
  required input is lost must be refuted by the independent checker against the specification; any other removal
  needs a twin or, when its only twins lose an input, the same refutation. A removed sequence the checker refuses
  statically (an undefined flag read, or a memory access through a redefined base) needs neither. Any other removal
  is an exactness failure.
- **Planted defects:** thirteen named edits to `footprint.py` run in isolated copies under `-B` and `-O -B`:
  omitted x86 flag writes, a conditional move treated as a full overwrite, ignored write-write and write-after-read
  conflicts in reordering, an unobserved result register on each profile, duplicates accepted without the Z3 proof,
  every argument register taken as required, an input taken as required because the specification mentions it, an
  input copied to another register forgotten, and the byte tie-break reversed in the duplicate and in the order rule.
  Each must leave a failing removal. The two tie-break edits keep equal-cost code that ranks later on the hex
  tie-break; only the rank condition on twins detects them. The syntactic edit is caught
  only by the two cases that specify (x ^ x) ^ y, where x occurs but is irrelevant. The eleventh, a conditional move
  marked as overwriting its destination while still reading it, is a registered equivalent mutant: the dead rule
  consults reads before overwrites. It counts as an invalid specimen only when it survives with removal decisions
  identical to the unmodified pruning on every case. Crashes and timeouts never count as detections.
- **Measurement:** the benchmark searches every problem in `test-data/machine-code/`, its kernel library and its
  memory kernels at fixed budgets (100,000 candidates, 128 checks, 60 s, 1 s solver timeout) and records every
  counter the search returns, wall times, the objective, the found cost and the stopping budget. `compare` ranks
  each found cost by the problem's objective first, as the search does, and fails when a cost is worse or lost, the
  objective changed, or two different costs cannot be ranked because no objective was recorded.

At 50ada8ee (scheme `exact-footprint/2`), the pruning was exact on all eighteen cases. With the lost-input rule off,
8,874 removals each had a proved twin and 1,328 were refused (profile-3 loads through a redefined base). With the
full pruning, 8,286 removals had a proved twin, all 604 lost-input removals were refuted by the checker, and 67
removals whose only twins lose an input were refuted too. The lost-input rule never fires on profile 3: its required
inputs are not derived there, which only prunes less. Ten planted defects were detected in both modes and the
equivalent mutant was confirmed in both; the syntactic edit was caught only by the (x ^ x) ^ y cases, and the
ignored write-after-read conflict also on the memory kernels. Over the 18 problems measured at dc410214, three
repeats each, candidates fell from 107,123 to 24,886, solver checks from 27 to 24, and the summed median time from
27.5 s to 9.1 s on one loaded machine. The lost-input rule skipped 485 sequences on the x86 128-bit add and subtract
problems; no library grammar has a duplicate entry. All found costs were identical except signed saturating add. It
exhausted its grammar and found its 31-byte, 7-instruction library kernel, where the unpruned search stopped at the
candidate budget with none. The newer `minmax-x86` problem finds its 18-byte, 5-instruction kernel after 438
proposals and 3 solver checks; unpruned, it reaches the same cost but stops at the 100,000-candidate budget. On the
six memory kernels, candidates fell from 8,574 to 822 and solver checks from 12 to 7, with every recorded cost
found. With `--no-prune`, every counter and cost equalled the baseline. The exactness result holds for these
grammars, lengths and specifications under the registered ISA models.

## Profile 2: flags, carries and comparisons

Profile 2 is a second, separately registered pair of targets for multi-word
arithmetic and decision kernels: 128-bit add and subtract with carry, carry-out,
unsigned and signed minimum and maximum, saturating add, absolute value and
compare-and-select guards. Profile 1 keeps its own validator, checker
(`tools/machine_code/verify.py`) and tests, and they are unchanged.

| Target | Calling contract | Admitted computation |
| --- | --- | --- |
| `x86_64-sysv-register/2` | Inputs in `rdi`, `rsi`, `rdx`, `rcx`, `r8`, `r9`, in declared order; results in `rax`, then `rdx`. Every other admitted register and CF, ZF, SF and OF initially arbitrary | Every profile-1 x86 form; 64-bit register ADC, SBB, CMP and TEST; NEG; SAR by an immediate; SETcc on the low byte of an admitted register; MOVZX of a low byte into a 64-bit register; 64-bit register CMOVcc; ADD, SUB, AND, OR, XOR and CMP with a sign-extended 8-bit or 32-bit immediate |
| `rv64i-register/2` | Inputs in `a0` to `a5`, in declared order; results in `a0`, then `a1`. `t0` to `t2` and every `a` register without an input initially arbitrary; `zero=0` | Every profile-1 RV64I form, plus SLT, SRA, SLTI, SLTIU, XORI, ORI, ANDI and SRAI, over `zero`, `t0` to `t2` and `a0` to `a7` |

The admitted x86 registers are the nine of profile 1: `rax`, `rcx`, `rdx`, `rsi`,
`rdi` and `r8` to `r11`. SETcc and CMOVcc take the conditions `o`, `no`, `b`,
`ae`, `e`, `ne`, `be`, `a`, `s`, `ns`, `l`, `ge`, `le` and `g`, as in `setb` and
`cmovl`. The parity conditions refuse. A two-result kernel returns like the C
declaration `struct { uint64_t lo, hi; } f(...)` or `unsigned __int128 f(...)`:
in `rax:rdx` on x86 and `a0:a1` on RV64I. The bounds of profile 1 still apply:
at most 16 body instructions and 256 bytes, one final return, no other control
flow, memory, system or privileged operations.

### Immediate operands

`["add", "rax", -5]` adds a constant: an integer as the second operand of
`add`, `sub`, `and`, `or`, `xor` or `cmp` is a signed immediate from -2^31 to
2^31-1. The instruction uses it sign-extended to 64 bits, so `-5` means
2^64-5 and `["and", "rax", -256]` clears the low byte. Flags follow the register
forms of the same instructions. ADC, SBB, TEST and MOV have no immediate form in
the profile.

Each form has exactly one admitted encoding, the one GNU `as` emits:

| Immediate | Encoding |
| --- | --- |
| -128 to 127 | `REX.W 83 /n ib` (4 bytes) |
| Otherwise, on `rax` | the short form `REX.W 05`, `0D`, `25`, `2D`, `35` or `3D` with `id` (6 bytes) |
| Otherwise | `REX.W 81 /n id` (7 bytes) |

The decoder refuses other encodings of the same instruction:

- `Imm32WhereImm8Fits`: a 32-bit immediate that fits in 8 bits;
- `AccumulatorFormRequired`: `81 /n id` on `rax`;
- `UnsupportedImmediateGroup`: `/2` (ADC) or `/3` (SBB);
- `UnusedRexBits`: a REX bit with no register to select;
- `MemoryOperandRefused`: a memory operand.

Profile 3 reuses these profile-2 instruction models, so it admits the same forms.

### Flags

CF, ZF, SF and OF are modelled exactly. The table gives each instruction's
effect on them.

| Instructions | CF, ZF, SF, OF after the instruction |
| --- | --- |
| ADD, ADC, SUB, SBB, CMP, NEG, including immediate forms | All four set from the operation |
| AND, OR, XOR, TEST | CF and OF cleared; ZF and SF set from the result |
| IMUL | CF and OF set on signed overflow; ZF and SF undefined |
| SHL, SHR, SAR by 0 | Unchanged |
| SHL, SHR, SAR by 1 | All four set |
| SHL, SHR, SAR by 2 to 63 | CF, ZF and SF set; OF undefined |
| MOV, MOVABS, LEA, MOVZX, SETcc, CMOVcc, NOP | Unchanged |

PF, AF and the other flags are not modelled. No admitted instruction reads
them. A candidate that reads a flag an earlier instruction left undefined is
refused with `ReadsUndefinedFlag` before any solver call. Entry flags are
arbitrary but defined, so reading one is admitted and quantified.

SETcc writes only the low byte of its register; the other 56 bits are kept. To
produce a 0 or 1 word, clear the register before the flag-setting instruction,
or apply MOVZX afterwards. The checker models the partial write, so a missing
zero-extension is a counterexample. MOVZX always reads the low byte of a 64-bit
register name, so `["movzx", "rdx", "rdi"]` assembles as `movzx rdx, dil`.

### Problem format

The schema is `zeno-fcis/machine-problem/2`. It has exactly the fields
`schema`, `target`, `inputs`, `outputs`, `grammar`, `max_instructions` and
`objective`.

- `inputs`: zero to six distinct names, each matching `[a-z][a-z0-9_]{0,15}`.
  They are assigned to the input registers in order. A problem with no inputs
  is a constant problem, for example one whose every input was fixed by
  [specialization](#exact-specialization); then every argument register is
  quantified scratch.
- `outputs`: one or two expressions, assigned to the result registers in order.
  Each has at most 64 nodes and depth 16.
- `grammar`, `max_instructions` and `objective`: as in profile 1. Forms use
  64-bit register names; for example, `["setb", "rdx"]` writes `dl`.

Expressions use the profile-1 constructors, with `["input", NAME]` naming a
declared input, plus these constructors.

| Meaning | Expression |
| --- | --- |
| Arithmetic shift right | `["ashr", expression, count]`; integer count 0 to 63 |
| Unsigned less than, less or equal | `["ult", left, right]`, `["ule", left, right]`; 1 when true, else 0 |
| Signed less than, less or equal | `["slt", left, right]`, `["sle", left, right]`; two's complement; 1 or 0 |
| Equality | `["eq", left, right]`; 1 or 0 |
| Select | `["ite", condition, then, else]`; `then` when `condition` is not 0, else `else` |

Greater-than comparisons swap the operands. Examples are
[`add128-x86.json`](../test-data/machine-code/add128-x86.json),
[`umin-x86.json`](../test-data/machine-code/umin-x86.json) and
[`add128-rv64i.json`](../test-data/machine-code/add128-rv64i.json).

### Checking and the loop

`tools/machine_code/verify2.py` decodes the bytes, checks the palette, bound and
flag reads, and asks Z3 for an assignment where any result differs. The query
quantifies all 2^(64n) values of the n inputs, every other admitted register
and, on x86, the four entry flags. A counterexample names each input, each
initial register, the initial flags on x86, and the lists of expected and
actual results. It is replayed on the concrete model before it is reported.

The commands are those of profile 1: `open`, `packet`, `candidate`, `search`,
`resume`, `emit` and `replay`. `open` selects the checker from the schema. Search
filters candidates on eight fixed concrete states before any solver call. One
of them has every free register at 2^64-1 and every entry flag set. A candidate
that reads an undefined flag counts as a concrete refutation.

```sh
machine_work=$(mktemp -d)
python3 -B -m tools.machine_code.run open \
  --problem test-data/machine-code/umin-x86.json --session "$machine_work/session"
python3 -B -m tools.machine_code.run search --session "$machine_work/session" --deadline-ms 20000
python3 -B -m tools.machine_code.run emit --session "$machine_work/session" --out "$machine_work/artifact"
python3 -B -m tools.machine_code.run replay --artifact "$machine_work/artifact"
```

This search proposes 259 candidates. It selects `cmp rdi, rsi; mov rax, rsi;
cmovb rax, rdi`, which is 11 bytes. The artifact files and replay checks are those
of profile 1.

The packet's advisory `diagnosis` covers profile 2 as well. It traces every
admitted register, and on x86 CF, ZF, SF and OF, with null for a flag left
undefined. It matches the subterms of every result. Each view lists the expected
and actual value of every result register, and `first_missing_subterm` names
its result index. Subterms are matched against registers only, but a comparison
usually lives in a flag. A comparison reported as never computed may therefore
be held in a flag the trace shows. For example, the 128-bit add that uses ADD
instead of ADC reports `(ult (add a_lo b_lo) a_lo)` as missing from result 1,
and the trace shows CF set after the low-word ADD.

### Trusted base

The checker trusts:

- `verify2.py` for the decoder, the concrete and SMT instruction models, and the
  flag rules;
- `profile2.py` for problem validation;
- the instruction record shared with `verify.py`;
- Z3, Python and the host.

The encoder in `targets.py`, the search, the advisory diagnosis, the timing
harness and every proposer, including an LLM, are not trusted. The encoder,
search and proposers supply data only; the diagnosis and harness only report.

### Evidence

```sh
python3 -B tools/test_machine_code_profile2_verifier.py
python3 -O -B tools/test_machine_code_profile2_verifier.py
python3 -B tools/test_machine_code_profile2_loop.py
python3 -O -B tools/test_machine_code_profile2_loop.py
python3 -B tools/test_machine_code_profile2_native.py
python3 -B tools/test_machine_code_profile2_conformance.py
python3 -B tools/test_machine_code_rv64i_conformance.py
```

- **Literal-byte controls:** the verifier test uses GNU-assembled x86 bytes and
  field-packed RV64I words, not the encoder. Its controls include a lost carry,
  swapped conditions, the partial SETcc write, quantified entry flags, refused
  undefined-flag reads, refused parity, byte-register and width variants, and
  solver-unknown and replay-mismatch mocks.
- **x86 conformance:** the conformance test enumerates every admitted x86 form,
  21,313 forms in all. Of these, 14,364 are immediate forms: every 8-bit
  immediate and ten edge 32-bit immediates for each of the six operations and
  nine registers. Each encoding is compared with GNU `as` and round-trips
  through the decoder. Each form then runs on the CPU from full register states
  with CF, ZF, SF, OF, PF and AF loaded by POPFQ. All nine registers and the
  four modelled flags, read back by PUSHFQ, must equal the concrete model on every
  state, and the SMT model on two states. The SMT comparison skips 8-bit
  immediates outside a sample of ten values, which leaves 8,029 forms. Flags the
  model leaves undefined are not compared. Six planted model defects must be
  detected:
  - SBB carry without borrow;
  - ADD overflow equal to carry;
  - SETcc writing the whole register;
  - CMOVLE ignoring ZF;
  - an SHL carry taken from the wrong bit;
  - a negative immediate zero-extended.
- **Decoder language:** a scan of 3,438,592 short byte bodies checks that every
  accepted body is an enumerated form in its unique canonical encoding. The scan
  covers:
  - every two-byte body, and every three-byte body under the profile's prefixes;
  - every four-byte `REX.W 83` body;
  - every REX prefix and ModRM byte of `81 /n id`, and the RAX short forms, with
    sampled 32-bit immediates. These include 8-bit values, which must be refused.
- **Immediate forms on this CPU:** the default 16-state run passes on an AMD
  Ryzen Threadripper 1950X. It makes 341,008 hardware comparisons with the
  concrete model over all 21,313 forms, and every GNU `as` encoding matches the
  encoder.
- **Measured run:** a 64-state run on an AMD Ryzen Threadripper 1950X compared
  444,736 executions with the concrete model and 13,898 with the SMT model,
  with no disagreement.
- **Native execution:** eight kernels go through `open`, a proposal or search,
  `emit` and `replay`. Each emitted `kernel.o` is linked with a C runner whose
  expected values use 128-bit arithmetic or GCC overflow builtins. The runner
  checks every combination of eight edge values per input, then 4,096
  pseudorandom input vectors.

| Kernel | Route | Kernel bytes | GCC -O2 bytes, no CET |
| --- | --- | --- | --- |
| 128-bit add, two results | proposal | 13 | 18 |
| 128-bit subtract, two results | proposal | 13 | 18 |
| Add with carry-out, two results | proposal | 13 | 12 |
| Unsigned minimum | search | 11 | 11 |
| Signed maximum | proposal | 11 | 11 |
| Saturating unsigned add | proposal | 13 | 14 |
| Saturating signed add | proposal | 31 | 37 |
| Absolute value | proposal | 11 | 11 |

The byte counts include the return and record size only. GCC's carry-out
version is smaller than the proposal.

`run bench` also times profile-2 x86 artifacts. The baseline is GCC's
compilation of the problem's own output expressions. Each function takes one
argument per input, and two results come back as the two-word structure. Two
runs were made with the default 1,000,000 iterations and 21 repeats, on the
same AMD Ryzen Threadripper 1950X under a load average of about 8, with GCC
13.3. They gave these kernel/baseline tick ratios:

- 128-bit add: latency 0.79 to 0.83, throughput 0.82 to 0.83;
- 128-bit subtract: latency 0.77 to 0.83, throughput 0.77 to 0.86;
- saturating signed add: latency 0.72 to 0.73, throughput 0.45 to 0.52;
- the other five kernels: between 0.92 and 1.08, so no difference is claimed.

These are paired measurements on that machine, load and call ABI only. They
are not a portable speedup claim. RV64I artifacts are refused, because they
cannot be executed on an x86 host.

- **RV64I execution:** `tools/test_machine_code_rv64i_conformance.py` runs in the
  `rv64i-execution` job of the machine-code workflow, which installs `qemu-user`
  and the `riscv64-linux-gnu` toolchain. It enumerates 21,166 RV64I forms of
  profile 2. Encodings are compared with GNU `as -march=rv64i`. Each form runs
  under QEMU user mode against the profile-2 concrete model on every state and
  its SMT model on one. Forms inside profile 1 are also compared with the
  profile-1 concrete model. Four planted defects must be detected. The job sets
  `ZENO_MACHINE_REQUIRE_RV64I=1`, so missing tools fail it. Elsewhere the test is
  skipped with the reason. The development host has neither QEMU nor the
  cross toolchain, so **no RV64I execution evidence exists until that job
  passes**. QEMU agreement is evidence about the emulator, not RISC-V hardware.
  The conformance test's local agreement of the RV64I concrete and SMT models
  is not execution evidence.

### Non-claims

- Only CF, ZF, SF and OF are modelled. Profile 2 makes no claim about PF, AF,
  DF, the other RFLAGS bits, or the flags a kernel leaves on return.
- There is no CET or ENDBR64 entry, memory operation, branch, fault or timing
  model.
- Agreement on sampled states is not a proof of the ISA or of other processors.
- A selected kernel is not claimed globally optimal, and no speedup is claimed
  without paired timing on the target.
- The ZAL `zal/word-kernel/1` route lowers to profile 1, and `zal/word-kernel/2` lowers to profile 2.

### Arithmetic contracts

**Journey.** A developer needs `x + y` that must never wrap silently. They write
a contract that names the arithmetic meaning, lower it to a profile-2 problem,
and propose or search against that problem. A kernel that is only correct
modulo 2^64 fails it with a counterexample.

A `zeno-fcis/machine-contract/1` contract has exactly the fields `schema`,
`target`, `operation` (`add`, `sub` or `mul`), `signedness` (`unsigned` or
`signed`), `bits` (64, 128 or 256), `meaning` (`modular`, `checked`,
`saturating` or `trapping`), `inputs` (two names), and the problem's `grammar`,
`max_instructions` and `objective`. With `r` the result modulo 2^64:

| Meaning | Lowered results |
| --- | --- |
| `modular` | One: `r`. Signedness does not change these bits. |
| `checked` | Two: `r`, then a status word, 1 when the mathematical result is outside the declared unsigned or signed 64-bit range and 0 otherwise. Read the status first; `r` is the mathematical result only when the status is 0. |
| `saturating` | One: the mathematical result clamped to the declared range. |

The status word is `(ult r x)` for unsigned add, `(ult x y)` for unsigned
subtract, `(slt (and (xor x r) (xor y r)) 0)` for signed add and
`(slt (and (xor x y) (xor x r)) 0)` for signed subtract. A checked contract
therefore never lowers to a one-result modular problem.

Routing is a finite decision, applied in this order:

1. an unknown meaning, operation, signedness or width, or a target outside
   profile 2, is unsupported, with the exact reason;
2. `trapping` names the missing bridge `effectful-overflow`, because profile-2
   kernels are pure register functions;
3. 128 and 256 bits name `multiword`;
4. checked or saturating `mul` names `high-product`, because profile 2 has only
   the low 64 bits of a product.

A refusal carries no problem. `problem_sha256` digests the lowered problem, as
for any session. `contract_problem_sha256` digests the contract's digest
together with that problem digest. Modular signed and unsigned addition share
a problem but not this identity. The lowering record has no authority; only the
checker decides whether bytes meet the lowered problem.

```sh
machine_work=$(mktemp -d)
python3 -B -m tools.machine_code.run contract lower --contract CONTRACT.json \
  --problem-out "$machine_work/problem.json"
python3 -B -m tools.machine_code.run open --problem "$machine_work/problem.json" \
  --session "$machine_work/session"
python3 -B -m tools.machine_code.run contract check --contract CONTRACT.json --code kernel.bin
```

`contract lower` exits 0 when lowered and 1 when refused. `contract check` exits
0 when equivalent, 1 on a counterexample or a refused contract, and 2 when
inconclusive. Both exit 65 on malformed input or an existing output file.

`tools/test_machine_code_contracts.py` checks:

- (2^64 - 1) + 1 gives 0 when modular, 0 with status 1 when checked, and
  2^64 - 1 when saturating; as signed words it does not overflow;
- all 720 routing cases against an independent table;
- each of the 14 supported lowerings equals exact arithmetic on wider bit
  vectors for all 2^128 input pairs, under Z3 and the checker's expression
  semantics;
- a kernel with the correct wrapped sum and a constant zero status word passes
  the modular contract and fails the checked and saturating ones;
- three lowerings reproduce the library problems `add-carry-out-x86`,
  `saturating-add-x86` and `signed-saturating-add-x86` exactly, and every
  library problem keeps its stored form.

The layer covers 64-bit add, subtract and multiply only. Division, shifts,
negation, wide and effectful contracts are refused or absent.

## Profile 3: memory regions

**Journey.** A developer needs a kernel that swaps two words in memory, which no
register-only profile can state. They declare the region its pointer argument
names, its size and its access, and state the final contents they expect. The
search or a proposer supplies bytes, and the profile-3 checker accepts them or
returns the inputs, pointer values, entry registers, flags and region words on
which a result or a final region word differs. Search prunes memory kernels
with the same exact footprint rules as the other profiles.

Profile 3 is a third, separately registered checker for straight-line x86-64
and RV64I kernels that load and store 64-bit words. Profiles 1 and 2 are
unchanged.

| Target | Calling contract | Admitted computation |
| --- | --- | --- |
| `x86_64-sysv-memory/3` | Each region's base pointer in the argument register it names; word inputs in the other argument registers, in order; results in `rax`, then `rdx` | Every profile-2 x86 form, plus `mov r64, [base]`, `mov r64, [base + disp8]`, `mov [base], r64` and `mov [base + disp8], r64` |
| `rv64i-memory/3` | The same, with argument registers `a0` to `a5` and results in `a0`, then `a1` | Every profile-2 RV64I form, plus `ld rd, disp(base)` and `sd rs2, disp(base)`; `sd` may store `zero` |

**Problem format.** The schema is `zeno-fcis/machine-problem/3`, with exactly
the fields `schema`, `target`, `inputs`, `regions`, `outputs`, `memory`,
`grammar`, `max_instructions` and `objective`.

- `regions`: one to four regions, each with exactly `name`, `base` (a distinct
  argument register), `words` (1 to 16) and `access` (`read-only` or
  `read-write`).
- `inputs`: zero to six word inputs; with the regions, at most six arguments.
- `outputs`: zero to two result expressions.
- `memory`: one expression per word for exactly the read-write regions: their
  expected final contents.
- Expressions are those of profile 2, plus `["initial", REGION, INDEX]`, the
  word at that index of the region at entry.
- Memory forms in `grammar` are `["load", DST, BASE, DISP]` and
  `["store", BASE, DISP, SRC]`, with DISP in bytes.

A problem needs a result or a read-write region. For example, the swap states
`"memory": {"p": [["initial", "p", 1], ["initial", "p", 0]]}` for a two-word
read-write region `p` based at `rdi`.

**Admission.** These rules apply before any solver call, to the decoded bytes
and to every palette form. Each refusal names its reason.

| Rule | Refusal |
| --- | --- |
| x86: a memory operand is `[base]` (ModRM mod 0) or `[base + disp8]` with a nonzero disp8 (mod 1) | `UnsupportedAddressing`, `NonCanonicalDisplacement` |
| RV64I: a load or store is 64-bit (`funct3` 3), and a load does not write `zero` | `UnsupportedMemoryWidth`, `ZeroWriteHintRefused` |
| The base is a region's base register | `MemoryBaseNotARegion` |
| No earlier instruction wrote that register | `RedefinedRegionBase` |
| The displacement is a multiple of 8 | `MisalignedAccess` |
| The displacement addresses a word inside the region | `DisplacementOutsideRegion` |
| A store targets a read-write region | `StoreToReadOnlyRegion` |

On x86, SIB, RIP-relative and 32-bit displacements refuse, as do every other
memory operand and partial-width access. On RV64I, every other load and store
width refuses. Writing a base register after its last access
is admitted.

**Checking.** `tools/machine_code/verify3.py` runs the decoded body over the
whole state: each region is a list of word variables, a load reads a word and a
store replaces it. Non-memory instructions use the profile-2 models unchanged.
Z3 then looks for an assignment where any result or any read-write region word
differs from the problem, or a read-only word changed. The query quantifies
every word input, every base pointer value, every initial region word, every
other admitted register and, on x86, the four entry flags. A counterexample names all of
them, with the expected and actual results and region words, and it is replayed
on the concrete model before it is reported. `open`, `search`, `candidate`,
`emit`, `replay` and `library` select this checker from the schema; their
concrete filter states also cover region contents.

**Assumptions.** These are stated in every result's scope and are not checked:

- the regions are pairwise disjoint and disjoint from the stack;
- each region base is 8-byte aligned, so an admitted access is aligned;
- nothing else writes the regions during the call.

**Evidence.**

```sh
python3 -B tools/test_machine_code_profile3_verifier.py
python3 -O -B tools/test_machine_code_profile3_verifier.py
python3 -B tools/test_machine_code_profile3_loop.py
python3 -O -B tools/test_machine_code_profile3_loop.py
python3 -B -m tools.machine_code.run library check --dir test-data/machine-code/memory-kernels
python3 -B tools/test_machine_code_profile3_native.py
python3 -O -B tools/test_machine_code_profile3_native.py
python3 -B -m tools.machine_code.run library check --native --dir test-data/machine-code/memory-kernels
```

- The verifier suite uses x86 bytes checked against GNU objdump and RV64I words
  packed from the I- and S-type layouts, anchored to two known encodings. It covers
  every admission refusal, a wrong store order and a dropped store, each refuted
  with a replayed counterexample, and quantified region words, base pointers,
  scratch registers and flags. Solver-unknown and replay-mismatch mocks are
  never equivalent.
- [`test-data/machine-code/memory-kernels`](../test-data/machine-code/memory-kernels)
  holds three kernels for each target, found by `search`:
  - a two-word swap, 15 bytes on x86-64 and 20 on RV64I;
  - a load, add and store, 10 and 16 bytes;
  - a two-word copy from a read-only region that also returns the sum, 18 and
    24 bytes.
  The loop test finds each again, then emits, replays and adds it
  byte-identically.
- The fourteen profile-3 planted defects are detected under both interpreter
  modes.

<a id="native-memory-runs"></a>**Native memory runs.** `library check --native`
runs each x86 memory entry on this processor. It never decides equivalence.

- A kernel runs only if its bytes pass profile-3 admission. Otherwise the
  entry reports `not-run` with `memory access not admitted: REASON`.
- Each region is its own 64-byte-aligned static array, so the regions are
  disjoint, aligned and apart from the stack. The runner passes each region's
  address in its base register and the word inputs in the other argument
  registers.
- Per state the runner loads the initial words, calls the kernel, and writes
  back the results and every region word. The states are the six search filter
  states, 1,024 states of edge values and 4,096 pseudo-random states.
- Results and read-write words are compared with the problem's integer
  reference. Read-only words must come back unchanged.
- RV64I entries report `needs RV64I execution`, and `bench` does not time
  memory kernels.

On an AMD Ryzen Threadripper 1950X, the three x86 memory entries agreed on
5,126 states each. A swap with a dropped store and a copy that returns the
wrong sum, both accepted by a stubbed model check, fail natively. A store to a
read-only region is never run.

**Counterexample diagnosis.** For profile 3 the packet's advisory `diagnosis`
adds a memory view. It lists every region word at entry and after each
instruction, the words each instruction wrote, and the expected and actual
final value of every read-write word. `first_differing_region_word` names the
first read-write word, in region and index order, whose final value differs,
with the instruction that last wrote it. Subterms are matched against registers
and region words.

**Non-claims.**

- Native agreement covers the listed states on that processor only. Faults,
  invalid buffers and overlapping regions are never run. RV64I memory kernels
  are not executed. Otherwise this is checked against the stated model only.
- The instruction models were generated, not derived from vendor
  specifications.
- Aliasing between regions, faults, caches, atomicity and timing are not
  modelled.
- The disjointness and alignment assumptions are the caller's obligation.

### Exact specialization

**Journey.** A developer has a checked 128-bit adder, but every caller passes
zero for both high words. They fix those two inputs. The tool writes a smaller
problem over the two low words, a Z3 proof that it agrees with the original
problem at those values, and a rewritten kernel that the checker accepts
against the smaller problem.

```sh
machine_work=$(mktemp -d)
python3 -B -m tools.machine_code.run specialize \
  --kernel test-data/machine-code/kernels/add128-x86.json \
  --fix a_hi=0 --fix b_hi=0 --out "$machine_work/low-words"
python3 -B -m tools.machine_code.run specialize-replay --dir "$machine_work/low-words"
```

`--problem P` specializes a profile-2 problem alone. `--problem P --code K.bin`
or `--kernel ENTRY.json` also specializes a kernel; that kernel must first check
equivalent to the original problem. Repeat `--fix NAME=VALUE` with a declared
input and a canonical unsigned decimal word.

**Specialized problem.** The remaining inputs keep their original order and are
renumbered from the first argument register. Each output is the original output
with the fixed inputs replaced by constants. It is then simplified by constant
folding and by these identities: `x+0`, `x-0`, `x|0`, `x^0`, `x*1`,
`x&(2^64-1)`, `x*0`, `x&0`, a select with a constant condition, and a shift by 0.
Without a kernel, the grammar and bound stay as in the original problem. With a
kernel, the rewritten kernel's forms are added to the grammar (refused above 32
forms), and the bound becomes the larger of the original bound and the new
length (refused above 16). Fixing every input gives a constant problem.

**Proof.** Z3 checks a query asserting that some specialized output differs from
the original output with the fixed values, over every assignment of the
remaining 64-bit inputs. Nothing is written unless the answer is unsat. The
query is stored as `proof.smt2`, and the record binds its SHA-256. Replay
recomputes the specialization and reruns the stored query.

**Kernel rewrite (advisory).** The specializer works on the kernel's decoded
instructions, in the spirit of WiPEr, the machine-code partial evaluator in
V. Srinivasan's dissertation *Synthesis of Machine Code: Algorithms and
Applications* (University of Wisconsin–Madison, 2017, TR1844):

1. It sets the fixed inputs as constants and propagates values, both registers
   and CF/ZF/SF/OF, through the instructions in their original order.
2. It folds every instruction whose reads are all known, and every instruction
   that known operands turn into a copy or a constant (for example `x+0` or
   `x&0`). An x86 instruction is folded this way only when every flag read
   later then has a known value. For example, `x+0` clears CF and OF.
   `ADC`/`SBB` with a known clear CF become `ADD`/`SUB`. A `CMOVcc` with a known
   condition becomes a copy or is dropped. Logic operations record that they
   clear CF and OF.
3. It keeps, to a fixpoint, the instruction that set each flag a kept
   instruction reads.
4. The kept instructions run in order. Each operand comes from whichever
   register holds its value, so renumbered arguments need no prologue of moves.
   A value is computed in the register where a later instruction or a result
   needs it. On x86, a commutative operation may swap its operands to do this.
   On x86, a constant source operand that fits a sign-extended 32-bit
   immediate becomes the immediate of `add`, `sub`, `and`, `or`, `xor` or
   `cmp`. Any other missing constant is set just before it is needed: by
   `MOV r64, imm64` on x86, or by an `ADDI`/`SLLI` chain on RV64I.
5. Backward register and flag liveness removes dead instructions. On x86, a
   zero constant becomes a hoisted `XOR r, r` where no flag is live.

The independent profile-2 checker must then find the bytes equivalent to the
specialized problem. It quantifies every remaining input, scratch register and
entry flag. Otherwise nothing is written. The rewrite has no authority of its
own.

`--out` must be a new directory; nothing is overwritten. It receives:

- `original.json`, `problem.json` and `proof.smt2`;
- `original.bin` and `kernel.bin`, when a kernel was given;
- `specialization.json`, which records:
  - the original and specialized problem hashes;
  - the fixed values, the remaining inputs and the argument map;
  - the proof answer, solver and query hash;
  - for a kernel, both code hashes and costs.

`specialize` and `specialize-replay` exit with these codes:

| Exit | Meaning |
| --- | --- |
| 0 | specialized or replayed |
| 1 | counterexample: a failed proof or a rejected kernel; also a refused original kernel |
| 2 | inconclusive |
| 65 | malformed input, or the output directory already exists |

`tools/test_machine_code_specialize.py` checks these behaviours:

- a wrong simplification makes the proof fail, and nothing is written;
- four library kernels on both targets specialize; the specialized kernel
  rechecks against its specialized problem; the record names both problem and
  both code hashes; replay passes; and concrete runs agree with the original
  kernel at the fixed values;
- fixing all four inputs of `add128-x86` gives a constant problem and a kernel
  made only of constant loads;
- a planted off-by-one constant fold is rejected by the checker with a concrete
  counterexample, and nothing is written;
- malformed fixes, profile-1 problems, an existing output directory, an
  unchecked original kernel and a changed record are refused;
- every profile-2 library kernel specializes over every nonempty subset of its
  inputs, with one deterministic draw of boundary or random words each. Each
  rewrite either checks equivalent or is refused for its size before checking;
  none may produce a counterexample.

At this checkpoint that last test covers 9 kernels and 61 cases:

- the checker accepts 60 rewrites;
- one RV64I case needs more than 16 instructions for its constants and is
  refused before checking;
- all 8 fully fixed cases have fewer instructions than the original kernel;
- of the 52 partly fixed cases, 8 have fewer instructions, 14 the same number
  and 30 more.

On x86, a constant that fits a sign-extended 32-bit immediate now goes directly
into `add`, `sub`, `and`, `or`, `xor` or `cmp`. A commutative operation whose
destination holds the constant swaps its operands first. Other constants still
cost a 10-byte load, and ADC, SBB and CMOVcc still need their constant in a
register, so partly fixed rewrites are often longer. Finding smaller kernels for
a specialized problem is a search question; `search` can start from the
specialized problem.

**Scope.** Profile-2 problems and straight-line, register-only kernels under the
registered ISA model. The proof covers output equality only. Kernel equivalence
comes from the checker, not from the rewrite. Not claimed: native conformance,
timing, a smaller or optimal rewritten kernel, or other targets.

### Per-result slices

**Journey.** A reviewer wants to know which instructions of a two-result kernel
compute each result. They slice it. Each result gets the instructions that can
reach it, checked as a kernel of its own.

```sh
python3 -B -m tools.machine_code.run slice \
  --kernel test-data/machine-code/kernels/add128-x86.json --out "$machine_work/slices"
```

The kernel must first check equivalent to its problem. For each result, the
backward slice keeps every instruction whose writes reach that result register
at the return through later reads. It uses the exact footprints from
`footprint.py`, which come from the checker's own symbolic semantics: reads and
writes over-approximate, and overwrites under-approximate. So a partial or
conditional write (`SETcc`, `CMOVcc`) keeps the earlier value of its destination
in the slice. A checked kernel's instructions are all grammar entries, so each
has a derived footprint; the command refuses with `FootprintUnavailable`
otherwise. The slice keeps the original instruction order. For the second
result, a copy into the first result register is appended.

Each slice becomes a one-result problem with that output alone. Its grammar is
the original grammar plus the slice's forms, and its bound is large enough for
the slice. The independent profile-2 checker must find every slice equivalent
to its own output, over every input, scratch register and entry flag. Otherwise
nothing is written; the slicer has no authority of its own.

The output directory receives:

- `original.json` and `original.bin`;
- `resultN.json` and `resultN.bin` for each result;
- `slices.json`, with:
  - the original problem and code hashes;
  - for each slice, its instruction indices, problem and code hashes, and its
    checked cost.

`slice` exits with these codes:

| Exit | Meaning |
| --- | --- |
| 0 | sliced |
| 1 | a slice was rejected, or the kernel did not check (refused) |
| 2 | inconclusive |
| 65 | malformed input, or the output directory already exists |

`tools/test_machine_code_slice.py` checks these behaviours:

- every profile-2 library kernel slices into checked one-result kernels;
- `add128-x86`'s low word needs only its `MOV` and `ADD`, and its high word
  needs all four instructions;
- a planted footprint defect that drops `ADC`'s carry read loses the low `ADD`
  from the high word's slice, and the checker rejects that slice with a
  concrete counterexample, so nothing is written;
- the slicer uses the shared footprints, including `ADC`'s read of CF, and
  refuses when a footprint cannot be derived;
- an unchecked kernel and a profile-1 problem are refused.

This follows McSlice, the machine-code slicer in the same dissertation. Here it
covers straight-line register-only kernels, where the instruction footprints
fully describe the dependences. Slices are checked, not trusted. They are not
claimed minimal: a slice keeps every instruction its footprints reach, even
when the value would not change the result.

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
effects, extra inputs, multiple results and distributed protocols refuse; the
[profile-2 subsection](#zal-word-kernel-profile-2) covers more inputs, two
results, comparisons and selects.

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

### ZAL word-kernel profile 2

**Journey.** A developer asks for a 128-bit add as two 64-bit words. They write
the requirement in ZAL as one sentence per result, read it back in plain English,
try an example with `what-if`, and open a session. A proposer or the built-in
search supplies bytes. The profile-2 checker accepts them or returns named input
values, entry registers and flags on which a result differs. That counterexample
is phrased as a question about the requirement.

```text
profile zal/word-kernel/2;
kernel add128;
target x86_64-sysv-register/2;
inputs a_lo:u64,a_hi:u64,b_lo:u64,b_hi:u64;
result 1 (add a_lo b_lo) modulo 2^64;
result 2 (add (add a_hi b_hi) (ult (add a_lo b_lo) a_lo)) modulo 2^64;
palette [["mov","rax","rdi"],["add","rax","rdx"],["mov","rdx","rsi"],["adc","rdx","rcx"]];
max-body 4;
objective bytes;
limits candidates=2000 checks=16 deadline-ms=20000 solver-ms=1000;
```

The English form reads:

```text
Inputs a_lo, a_hi, b_lo and b_hi are unsigned 64-bit words.
Return first (a_lo plus b_lo) with wraparound modulo 2^64.
Return second ((a_hi plus b_hi) plus ((a_lo plus b_lo) is below a_lo (unsigned))) with wraparound modulo 2^64.
```

[`umin-x86.zal`](../test-data/zal-word-kernel/umin-x86.zal) states
`(ite (ule x y) x y)`, in English
`(if (x is at most y (unsigned)) then x else y)`. Its session `search` finds
the same 11-byte kernel as the library entry. Each example also has an
`.english.zal` form.

**Formal meaning.** A `profile zal/word-kernel/2;` or `ZAL word kernel profile
2.` line selects this profile. The declarations follow in a fixed order:

- `kernel` and `target`, which must be a profile-2 target;
- `inputs`, with 1 to 6 distinct names in argument order;
- one or two results, written `result 1` and `result 2` when there are two;
- `palette`, `max-body`, `objective` and `limits`, as in profile 1.

Lowering produces a `zeno-fcis/machine-problem/2` problem. That problem is
validated by `profile2.validate_problem`, `verify2._palette` and
`session._grammar`. The two examples lower to exactly the library problems
`add128-x86` and `umin-x86`. Expression meanings are those of the machine
profile 2; the ZAL layer only parses and renders.

| Symbolic | English |
| --- | --- |
| `(ashr e 3)` | `(e shifted right arithmetically by 3)` |
| `(ult a b)`, `(ule a b)` | `(a is below b (unsigned))`, `(a is at most b (unsigned))` |
| `(slt a b)`, `(sle a b)` | `(a is below b (signed))`, `(a is at most b (signed))` |
| `(eq a b)` | `(a equals b)` |
| `(ite c a b)` | `(if c then a else b)`, which gives `a` when `c` is not 0 |

Comparisons give 1 or 0. Expression words cannot be input names. `render`
converts between the syntaxes exactly; English and symbolic sources share one
revision. Every expression node has a source span with an id such as
`result/2/expression/2`. The binding also hashes `word_kernel2.py`.

**Commands.** These differ from profile 1:

- `what-if` takes one `--input NAME=VALUE` per declared input. The `--x`/`--y`
  options stay for profile 1.
- The controlled question lists every input, for example
  `What if a_lo is 1, a_hi is 0, b_lo is 2 and b_hi is 0?`.
- `What can be clobbered?` also names the result registers and the quantified
  entry flags.

Every other command is unchanged. Refusals name the declaration and the reason.
They cover:

- an unknown or duplicate input;
- more than 6 inputs or 2 results;
- an expression as a shift count, such as a comparison result;
- a target that disagrees with the profile line.

A `profile zal/word-kernel/1` line keeps the profile-1 parser and its refusals.

**Scope.** Profile 2 is still a closed straight-line word profile:

- no memory, effects, checked overflow, U256/I256, loops or branches;
- only CF, ZF, SF and OF are modelled.

The machine-profile non-claims above apply. The source binding does not
authenticate intent.

```sh
python3 -B tools/test_zal_word_kernel2.py
python3 -O -B tools/test_zal_word_kernel2.py
```

These checks cover:

- the syntax round trips, with library-equal lowering for both examples;
- every refusal above;
- a real profile-2 search for `umin`, and a candidate for `add128`, each emitted
  and replayed;
- a planted wrong expression that yields a source-level counterexample;
- a rehashed change of meaning, which replay refuses.

## Sources and agent skill

The instruction contracts follow the
[Intel ISA manuals](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html)
and the ratified [RV32I](https://docs.riscv.org/reference/isa/v20260120/unpriv/rv32.html)/
[RV64I](https://docs.riscv.org/reference/isa/v20260120/unpriv/rv64.html) specifications.
[STOKE](https://theory.stanford.edu/~aiken/publications/papers/asplos13.pdf)
provides a research precedent for instruction search; the present tool does not
reuse its implementation or claim its optimization results.

### Research lineage

The 2.7 search and checking techniques adapt published work. None of that work's
implementation is reused, and none of its measured results is claimed here.

| Technique here | Source |
| --- | --- |
| Footprint pruning, split search, lost-input pruning | McSynth and McSynth++ ([Srinivasan 2017](https://research.cs.wisc.edu/wpis/papers/tr1844.pdf); [Srinivasan, Sharma and Reps, OOPSLA 2016](https://research.cs.wisc.edu/wpis/abstracts/oopsla16-synthesis.abs.html)), restricted to exact skips |
| Reordering and duplicate pruning | partial-order reduction (Godefroid, LNCS 1032, 1996); equivalence reduction (Smith and Albarghouthi, VMCAI 2019) |
| Specialization | WiPEr ([Srinivasan and Reps, OOPSLA 2015](https://research.cs.wisc.edu/wpis/abstracts/oopsla15.abs.html)) |
| Per-result slices | McSlice (Srinivasan and Reps, OOPSLA 2016) |
| Solver unrealizability (`realize`) | loop-free synthesis by exists-forall queries (Gulwani et al., PLDI 2011); complete space partitions ([Lu and Bodik, OOPSLA 2025](https://lsrcz.github.io/files/OOPSLA25.pdf)) |
| Admission rules as part of correctness (profile 3) | capability-aware superoptimization ([CapOpt, OOPSLA 2026](https://2026.splashcon.org/details/oopsla-2026/107/CapOpt-Capability-aware-Superoptimization-for-Secure-and-Provably-Faster-Code), abstract); K2 (SIGCOMM 2021) |
| Memory as word variables with stated assumptions | effect variables in [Arrival (OOPSLA 2025)](https://www.contrib.andrew.cmu.edu/~bparno/papers/arrival.pdf); STOKE's stated stack assumption |
| Assembler cross-check of every admitted encoding | Arrival's encoding-template validation |
| Verifier in the loop for proposals | LPO ([Xu et al., ASPLOS 2026](https://arxiv.org/abs/2508.16125)) |

**The instruction models.** The models used here were generated, then checked
against GNU `as` and against this CPU (x86) on sampled states. They were not
derived from vendor specifications. Arrival and Islaris check against
authoritative machine-readable semantics instead, such as Arm's ASL and the Sail
RISC-V model; for ZenoFCIS that check is future work.

The reusable [machine-code skill](../skills/zenofcis-machine-code/SKILL.md)
records route selection, feedback, replay and evidence boundaries.
