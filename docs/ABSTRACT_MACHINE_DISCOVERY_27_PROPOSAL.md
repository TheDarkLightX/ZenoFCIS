# Abstract-machine discovery for ZenoFCIS 2.7

**Status: draft proposal.** This document proposes a staged research and
implementation path. It adds no runtime instruction, supported API, checker,
compiler backend, or assurance claim. The accompanying arithmetic search example
is executable evidence with the narrow scope described below.

## 1. Start with the abstraction library

ZenoFCIS should explore new machines by first discovering **verified macros in
the abstraction library**. A macro gives a useful operation a name, signature,
explicit assumptions, and a finite expansion into existing canonical semantics.
It can improve authoring and reuse without making a generated instruction set
the source of truth. Only measured benefits should justify a subsequent small
VM or hardware backend.

The proposed sequence is:

1. Discover and independently check reusable macros over a fixed semantic
   profile.
2. Co-design a small VM, its lowering, and its verifier around useful workloads.
3. Consider hardware when software measurements identify a worthwhile target.

An LLM may propose operations, expansions, invariants, and counterexamples.
Symbolic synthesis may fill holes or search a grammar. Neither supplies
acceptance authority. A small, versioned checker must decide claims against
canonical semantics that the proposer cannot change.

This separates four activities often called “machine synthesis”: searching for
programs over an existing ISA; learning an existing instruction's semantics;
choosing or inventing instructions; and implementing those instructions in RTL.
Evidence for one activity does not establish the others. A one-instruction
interface is a candidate design, not an objective by itself: operands, addressing
modes, function units, microcode, and validation may contain most of its
complexity.

## 2. Existing foundations and their limits

The inspected 2.7 integration baseline is
[`b7d6fb31258146130f0adfdde4348b88282433cf`](https://github.com/TheDarkLightX/ZenoFCIS/tree/b7d6fb31258146130f0adfdde4348b88282433cf).
At that baseline, the
[finite IR](../crates/zeno-fcis-synthesis/src/finite/ir.rs) uses profile-specific,
checked `i128` semantics, including arithmetic-extension profiles. Its
[scalar evaluator](../crates/zeno-fcis-synthesis/src/finite/evaluation/mod.rs)
evaluates nodes eagerly in order, including both arms of `Select`. Intermediate
overflow and output-domain failures matter. Fused arithmetic operations can
have different widening and narrowing behavior from a superficially equivalent
sequence of smaller operations.

The existing [program-search crate](../crates/zeno-fcis-program-search/src/lib.rs)
already separates untrusted proposals from `zeno-fcis transform check` and
`transform replay`; proposing a candidate does not authorize adoption. This is
the appropriate boundary for another proposer. This document does not claim
that those commands currently accept macro packages or arbitrary machines.

[ZAL's progression ladder](zal/ladder/README.md) offers multiple readings of a
canonical formula under a declared theory. Its
[finite refinement relation](zal/REFINEMENT.md) compares declared behavior,
including decisions and rejection reasons; it explicitly does not establish
equal resource usage. The proposed executable macro entry format is an
extension to develop, not an existing capability of the ladder. Interpreter
proofs likewise remain tied to their exact subjects and assumptions, as described
in [finite interpreter assurance](FINITE_INTERPRETER_ASSURANCE.md).

## 3. A bounded candidate language

The first candidate grammar should be deliberately small:

```text
Macro   := typed parameters, precondition, ordered Body, output roots, frame
Body    := a bounded sequence of typed canonical nodes or earlier macros
Node    := an operation admitted by the selected canonical finite profile
Operand := a parameter, admitted constant, or earlier result
```

This is an illustrative proposal, not a parser specification. A concrete first
implementation must fix node, reference, constant, expansion-depth, and package
size bounds. References between macros form an acyclic graph. Expansion is a
total function on admitted packages, with a checked bound on its output size;
invalid packages produce a typed refusal. Recursive macros, unbounded loops,
dynamic allocation, host calls, and new effect permissions are outside phase 1.
Expansion must not require executing generated code.

Each abstraction-library entry should bind its semantic profile, parameter and
result domains, preconditions, canonical expansion, scratch requirements,
declared writes, cost bound, and replayable evidence. Its symbolic and readable
views must refer to this same meaning. Provenance identifies the exact source,
expander, checker, dependency versions, assumptions, and claim scope. A familiar
name such as `add` must never hide a new overflow rule.

A precondition is an obligation at every use. The checker must establish it from
the source domain and context, or the expansion must include an explicitly
specified guard. Narrowing the accepted input domain merely to make a rewrite
pass is not refinement of the original operation. Useful successful witnesses
and coverage of the intended source domain are required alongside refusal
tests; “always refuse” must not pass as a useful implementation.

## 4. What acceptance must mean

Let `S` be a pinned canonical profile, `E` the total macro expander, `P` an
admitted source program, `x` an admitted input and initial state, and `b` an
admitted logical budget. For a representation macro, define its meaning through
its canonical expansion. Any implementation that executes that representation
must satisfy:

```text
for every P, x, b in the declared domain:
  Observe(RunMacro(P, x, b)) = Observe(RunS(E(P), x, b))
```

`Observe` includes successful values and successor state; decision class and
rejection reason where applicable; technical refusal and the first failing
operation; ordered effects if the profile has them; and logical usage on both
success and refusal. Malformed program bytes also require a separate admission
and decoding equivalence obligation. The relation must not quietly quantify
only over successful runs: the declared input envelope includes values that
must refuse, not only values on which the source computation succeeds.

Representation macros retain canonical logical charging. A shorter encoding
cannot turn ten expanded operations into one unit of gas. Physical work and
memory require separate implementation bounds; logical gas is not elapsed time.
A backend may execute more efficiently while preserving the specified logical
charges and budget-boundary behavior.

Replacing an existing canonical program `P` with another expansion `Q` is a
stronger optimization claim, not automatic macro acceptance. The proposed
strict mode requires the same observation equation for `P` and `Q` at every
admitted budget. If an optimization deliberately changes logical usage or
budget-related refusals, it needs a separately versioned contract and an explicit
refinement policy. Existing functional transformation evidence must not be
relabelled as meter equivalence.

Framing is part of the relation: inputs and unrelated state remain unchanged;
private scratch has a declared initial condition and lifetime; and failed
execution exposes no partial successful result. Register-machine experiments
must establish required scratch restoration. A real implementation must specify
cleanup on failure as well as success and account for its cost.

For example, the mathematical-integer addition sequence
`T = 0 - X; Y = Y - T; T = T - T`, with `T = 0` initially, preserves the intended
result and restores scratch. Under checked `i128`, it refuses immediately for
`X = i128::MIN, Y = 1`, although `Y + X` is representable. Similarly,
`Y = 0 - Y; Y = X - Y` fails first at `Y = i128::MIN, X = 1`. These are required
negative cases for a proposed checker. An admitted domain restriction can make
a narrower theorem valid, but cannot silently remove source inputs. Likewise,
eliminating a failing unused `Select` arm changes today's eager semantics.

## 5. Search broadly; accept independently

Use counterexample-guided inductive synthesis (CEGIS) as an inner loop:
generate a candidate, reject malformed candidates, check its obligations, and
return a concrete counterexample to the proposer. Small finite domains can be
exhaustively checked; symbolic proofs must state their widths, domains,
assumptions, and trusted components. A solver timeout or `unknown` is
inconclusive, never acceptance. A bounded result is labelled with its bound.

Enumeration, Rosette sketches, stochastic search, and an LLM can share this
interface. Candidate semantics must be data in the admitted grammar, not
arbitrary Python or generated checker code. A proposer's self-authored
interpreter alone is not independent acceptance evidence; ground the oracle in
separately reviewed canonical semantics.

Equality saturation can share equivalent expressions and extract alternatives
under different costs. Every admitted rewrite needs a checked, profile-specific
side condition. Ordinary algebraic equivalence is insufficient for checked
overflow, eager failure order, or logical charging. E-graph membership alone is
not an acceptance certificate. Replay must validate the chosen derivation or
the final candidate independently.

An optional MAP-Elites outer search could retain diverse candidates by code
size, scratch demand, or instruction family. This is a heuristic for allocating
search effort, with no proof or acceptance authority. It offers no completeness
or global-optimality guarantee. Feed discovered counterexamples back into the
search set, retain failed candidates, and track which constraint excluded each
one. Do not let the generator modify the reference semantics or checker after
seeing a failure.

## 6. Included experiment: exact affine microcode search

The accompanying
[standard-library Python experiment](../examples/abstract-machine-search/machine_microcode_search.py)
explores five fixed one-operation arithmetic machines. It is a small example of
symbolic program search plus comparison between supplied machine alternatives;
it does not discover an unrestricted ISA.

There are three registers: read-only `X`, writable `Y`, and writable scratch `T`.
Initial state is `(x, y, 0)` and every successful target must leave `X` unchanged
and restore `T` to zero. A source operand may name any register; a destination
may name `Y` or `T`. The five operation definitions are addition, subtraction,
reverse subtraction, copy, and negative copy. Operand reads precede the write,
including when source and destination alias.

The search represents register values exactly as affine coefficients of `x`
and `y` over mathematical integers. Breadth-first search merges identical
symbolic states and explores sequences of length at most five. Its results are:

| Machine's operation on destination `B` | `Y = Y + X` | `Y = Y - X` | `Y = X` |
| --- | ---: | ---: | ---: |
| `B = B + A` | 1 | None through 5 | None through 5 |
| `B = B - A` | 3 | 1 | 4 |
| `B = A - B` | 2 | 2 | 2 |
| `B = A` | None through 5 | None through 5 | 1 |
| `B = -A` | None through 5 | None through 5 | 2 |

Numbers are minimum lengths in this precise mathematical-integer grammar,
with these scratch and frame requirements. “None through 5” makes no claim
about longer programs. Reverse subtraction has a smaller sum of lengths for
these three equally weighted tasks, but does not dominate subtraction: its
subtraction takes two operations instead of one. This is not a runtime or
hardware-performance comparison.

The nine discovered programs are replayed by a separate concrete interpreter
on all 256 input pairs of four-bit words: **2,304 program/input checks per run**.
Two negative controls expose omitted scratch restoration and an omitted
scratch-zero precondition. The exact replay command, from the repository root,
is:

```sh
python3 examples/abstract-machine-search/check_replay.py
```

The [replay wrapper](../examples/abstract-machine-search/check_replay.py) runs the
original script normally and with Python `-O`, comparing semantic results
exactly against [expected results](../examples/abstract-machine-search/expected.json)
after excluding elapsed time. Explicit failures remain active under `-O`.

The affine identities also hold for modular arithmetic because addition,
subtraction, and copying respect reduction modulo the word modulus. The
**minimal-length claim does not transfer to finite widths**, where additional
modular identities may exist. Neither claim establishes checked-overflow
equivalence. The experiment does not model actual SUBLEQ branches, instruction
fetch, memory, effects, native execution, or RTL. It uses a trusted Python
implementation and emits no proof-assistant certificate. There is no
universality or production-isolation claim.

## 7. Extending discovery to an actual machine

After useful macros exist, a second grammar can parameterize a machine's word
width, bounded register file, memory regions, instruction fields, and transition
rules. Start with small families: a register VM, a stack VM, subtraction with
branching, or transport-triggered operations. Explicitly select checked or
modular arithmetic and signedness. Search may vary instruction subsets,
semantics within a bounded transition grammar, encodings, and compiler rules;
each change creates a new candidate subject.

A compiler correctness obligation then relates canonical state to machine
state. Establish the initial-state relation for every admitted source input and
its compiled initialization. Under explicit environment and nondeterminism
assumptions, quantify over **every permitted target execution**: all observable
trace prefixes, terminal outcomes, refusal prefixes, and logical usage must
correspond to canonical execution. Existence of one matching target run is
insufficient. Each source step's implementation must finish within a proved
bound or produce the corresponding refusal; stuttering needs a decreasing rank
or independently checked termination bound.

Separately establish coverage of the intended source-program and input domain,
with successful representative workloads. Rejecting difficult source programs
cannot silently narrow that claim. Correctness of selected instruction snippets
does not establish whole-program control flow, decoding, calling conventions,
or scratch ownership.

Isolation is a separate obligation quantified over **all admitted guest
programs**, including hostile ones, not merely compiler outputs. The decoder,
memory bounds, resource accounting, and host-call boundary must constrain those
programs. If host calls are introduced later, the profile must specify capability
checks, ordered request/response observations, and failure behavior. A small ISA
does not by itself isolate native execution or external effects.

Finite-width, finite-memory exploration cannot establish an unbounded
universality claim. Such a claim would require an explicit unbounded model and
a simulation theorem. ZenoFCIS's practical objective is a useful, checkable
implementation for declared workloads, so universality is not an acceptance
gate.

## 8. Measurements and phased acceptance

Keep training/search workloads separate from sealed evaluation workloads.
Retain counterexamples used for refinement as regression inputs; they are no
longer held-out evidence. Publish them only after source and privacy review.
Final correctness checking still covers the entire claimed domain. Sealed
workloads test performance generalization, not semantic truth. Record workload
provenance, deterministic seeds where relevant, search budgets, hardware, tools,
and all failed or inconclusive attempts.

Report the Pareto frontier rather than collapsing everything into instruction
count. Measure expanded nodes, encoded bytes, scratch and peak memory,
interpreter and checker size, logical usage, execution latency, and checker
replay cost. Include total discovery cost: candidate generation, LLM calls,
failed synthesis, solver time, independent checking, and repeated attempts.
State both cold and amortized costs over a declared reuse count. Hardware adds
area, timing, energy, memory-system cost, and compiler effort. No single-instruction
design receives a discount for operations hidden in operand decoding.

| Phase | Deliverable | Acceptance gate |
| --- | --- | --- |
| 0: reproducible example | The included affine search and expected results | Exact normal/`-O` replay; both negative controls detected; limitations retained |
| 1: abstraction library | Bounded macro schema, expander, independent checker, useful entries | Total expansion; profile/domain/frame obligations; checked-overflow and budget-boundary counterexamples; reproducible evidence; successful intended-domain witnesses |
| 2: VM co-design | One explicit VM profile, compiler, executable model, refinement evidence | Whole-program and refusal refinement; all-guest isolation obligation; sealed-workload benefit against the canonical implementation; complete cost accounting |
| 3: hardware | Optional RTL and implementation evidence | Software-to-RTL relation, reset and control-flow checks, measured gain after synthesis under stated assumptions |

Before each phase, declare workloads, baselines, cost limits, and the improvement
required to justify proceeding. Phase 1 can succeed through clearer authoring
and reusable checked abstractions without claiming faster execution. Phase 2
must justify its additional semantics, compiler, and verification burden. If a
conventional small VM wins, keep it. A custom machine or hardware project is
not the default outcome.

Every accepted artifact needs replay instructions and a claim manifest binding
the canonical semantics, candidate, expansion or compiler, checker, proof or
exhaustive evidence, resource bounds, and explicit trusted assumptions. Changed
subjects require renewed evidence. These are proposed acceptance requirements;
the included experiment supplies only phase 0 evidence.

## 9. Primary literature and practical comparators

These sources motivate parts of the design; none proves this proposal:

- [Rosette guide](https://docs.racket-lang.org/rosette-guide/ch_getting-started.html):
  solver-aided execution, synthesis, and verification. Machine discovery still
  needs a declared semantics grammar or an outer design search.
- [æSIP, ISCA 2026](https://jerrygao.me/assets/papers/aesip.pdf): combines several
  rewrite proposers, including an LLM, with verification and hardware subset
  selection for RV32IM. Straight-line rewrites receive SMT checks; branch
  rewrites rely on simulation. It does not invent arbitrary new ISAs.
- [Isaria, ASPLOS 2024](https://jamesbornholt.com/papers/isaria-asplos24.pdf):
  generates compiler rules from supplied ISA semantics and a cost model. This
  supports retargetable compilation, not independent discovery of the ISA.
- [AGON, version 2](https://arxiv.org/html/2412.20954v2): uses LLM-generated
  structured instruction semantics in a processor-generation flow. Its described
  testing and simulation do not establish universal semantic equivalence.
- [Formal Verification of SUBLEQ Microcode Implementing the RV32I ISA](https://ics.jku.at/files/2022FDL_FormalVerificationSUBLEQMicrocode.pdf):
  Rosette analysis found nine buggy instruction implementations despite passing
  compliance tests. Its per-procedure and width limitations should not be read
  as complete verification of a full 32-bit processor.
- [OpenASIP](https://openasip.org/) and its
  [transport-triggered architecture overview](https://openasip.org/tta.html):
  practical compiler, simulation, and hardware co-design tools. They do not
  automatically discharge this proposal's refinement obligations.
- [MAXQ architecture](https://www.analog.com/en/resources/technical-articles/introduction-to-the-maxq-architecture.html):
  a historical practical single-`MOVE` interface whose operand encodings trigger
  arithmetic and control operations. Count the underlying machinery.
- [CertrBPF, CAV 2022](https://link.springer.com/chapter/10.1007/978-3-031-13188-2_15):
  a mechanized refinement from a Coq VM specification to C with isolation
  properties. It is a practical comparator for a small verified VM without
  requiring a one-instruction ISA.
