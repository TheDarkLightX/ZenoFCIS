# Symbolic per-case checks

`contract review` and `transform check` enumerate every input tuple of the
declared domain, and stop at their caps: 2^20 tuples for the review and
100,000,000 for the transform checker. An application with money amounts or
timestamps has a far larger domain; the study escrow below has more than
2^128 tuples. On such a domain these checks are inconclusive
(`domain-too-large`).

Two commands ask the pinned CVC5 and Z3 instead, one SMT query per rule case:

- `zeno-fcis contract check-symbolic` checks that every committing case keeps
  every declared state law, and an owner's strengthening invariant.
- `zeno-fcis transform check --symbolic` checks that a candidate decision
  program gives the same results as the original.

Their results keep the formal-tools evidence classes
([ADR 0003](adr/0003-epistemic-status.md), [formal tools](FORMAL_TOOLS_RC3.md)):

| Result | Evidence | Exit | What it rests on |
| --- | --- | ---: | --- |
| A counterexample | Checked | 1 | A solver's model, replayed through the library: the decision program evaluator, the law evaluator and the bound Authority. The replay alone decides; the solver's word does not. |
| `holds` from enumeration | Proved | 0 | Every tuple of the domain ran through the bound Authority and the library law evaluator. |
| `holds` from the solvers | Attested | 2 | CVC5 answered `unsat` with proof steps, which nothing checks. Z3's `unsat` corroborates it and never stands alone. A solver-only "holds" is attested by CVC5, corroborated by Z3, not proved, so it exits 2 like a CVC5 `unsat` from `prove`. |
| `inconclusive` | none | 2 | Anything else: `unknown`, a timeout, a crash, an `unsat` without proof steps, a model that does not replay, solvers that disagree without a replayed model, a construct the encoding does not translate, or a planted control that was not refuted. |

A symbolic result never replaces enumeration where enumeration fits. Where
`check-symbolic`'s domain fits `--max-tuples`, it enumerates the domain as
well, enumeration decides, and a disagreement between the two routes is
inconclusive. `transform check --symbolic` refuses a domain that fits
`--max-input-tuples`, which the exhaustive check then decides.

## Solvers

Both commands read a tools manifest (`--tools FILE`, format
`zeno-fcis/tools/2`, as in the [CLI reference](CLI_REFERENCE.md#tools-manifest))
that names CVC5 1.3.3 and, to corroborate it, Z3 4.16.0. Each configured
solver is admitted once per command through the formal-tools adapter: the
file must hash to the manifest's SHA-256, a private copy of exactly those
bytes must report the pinned version, and every query runs that copy with
the adapter's fixed arguments, cleared environment, timeout, output bound
and process-group containment. Each query runs on both solvers at once.
After `sat` the solver is asked again for its model, after CVC5's `unsat` for
its proof; two runs of one query that disagree give no answer.

The pinned binaries are the ones inside the archives that
`release/formal-tools-linux-x86_64.sha256` lists:

```text
cvc5 1.3.3   e8d7870d57ab55e81619d2373b043da05ea1d37ca393931bdb5d8b9788cd64c4
z3 4.16.0    e583c4186a45e72411fa2cb2048401eed03f0f8e5f24694676a8f6271a50b765
```

`--queries DIR` creates `DIR` and keeps each query's SMT-LIB text
(`LABEL.smt2`) with each solver's final output (`LABEL.cvc5.out`,
`LABEL.z3.out`). Reports name each query by label and SHA-256.

## `contract check-symbolic`

```text
zeno-fcis contract check-symbolic [<contract-dir>] [--strengthening FILE] [--tools FILE]
    [--max-tuples N] [--out REPORT.json] [--queries DIR] [--format human|json]
```

It reads `project.zeno` and `v2/policy.json`, builds the contract and binds
it to the library Authority as `generate contract` does, and checks the
decision program the rules compile to. An adopted program is exhaustively
equivalent to it by its receipt, so the result carries over to it.

**Targets.** The declared state laws: the laws that apply at genesis and to
every committing decision, except `InitialCondition` laws (the same set
`contract review` uses). Each strengthening clause is a target too. Laws that
read the command, the context or the decision are not targets; the Authority
enforces them at run time. The report lists them under `laws.not_targets`.

**Queries.** For each committing case (Accept or CommittedFailure) and each
target, one query asks for an input tuple where all of these hold:

- every value lies in its declared domain;
- every state law and every strengthening clause holds on the pre-state;
- the decision program selects the case, and the Authority would build and
  validate its decision: ingress, program execution with its checked
  arithmetic, output decoding, the successor's field domains, and the
  delivery schemas;
- the target fails on the successor, by a false value or an undefined one
  (an overflow or a division by zero).

The Authority's resource meter is not encoded, so a query can only admit
more tuples than the Authority does; a model the meter would refuse fails
replay. `unsat` therefore means that the case never needs a state-law
refusal from a lawful pre-state and keeps every clause. With genesis
satisfying every clause, which the library law evaluator checks, the clauses
then hold on every state the application can reach.

**Replay.** A model counts only when the library confirms it: the pre-state
satisfies every target (law evaluator), the program selects the case
(program evaluator), the successor breaks the target (law evaluator), and the
bound Authority refuses the decision by a law, or, for a clause it does not
enforce, commits exactly that successor.

**Planted controls.** Every run also asks queries that a working solver and
encoding must answer `sat`, each replayed the same way:

- *premise*: some lawful pre-state exists (genesis is one). If this is not
  refuted, the whole run is inconclusive.
- *reachability*, per case: a lawful pre-state commits the case. An `unsat`
  reports the case as `vacuous`; its targets then hold trivially.
- *domain*, per target: some state of the domain breaks it. An `unsat`
  reports the target as `implied-by-domain`.

An inconclusive control makes the run inconclusive.

**Exit codes.** 0 when enumeration proves every target on every case; 1 for a
replayed counterexample; 2 when the solvers attest every target, or when
anything is inconclusive; 1 for a contract or strengthening file it cannot
read as one; 2 when a configured solver fails admission; 3 when a file
cannot be read or written.

### Strengthening file

A strengthening states extra state formulas. The check assumes them on every
pre-state, beside the state laws, and checks them on every committing case's
successor and on genesis. They are not enforced at run time: a strengthening
is an argument, sound only because the check shows that genesis satisfies it
and every committing case preserves it.

```json
{
  "schema": "zeno-fcis/strengthening/1",
  "invariants": [
    {"name": "created_pays_nothing",
     "formula": "post.100.110 == 150 -> post.100.113 == 0 && post.100.114 == 0"}
  ]
}
```

- `invariants` holds at most 64 clauses. No other keys are allowed.
- `name` is a lowercase identifier of at most 64 bytes (`a-z`, `0-9`, `_`,
  starting with a letter), unique in the file.
- `formula` is at most 4,096 bytes in the rules' expression language
  (`->`, `||`, `&&`, comparisons, `+`, `-`, `*`, prefix `!` and `-`,
  `choose(c, a, b)`, `div_floor(a, b)`, `div_ceil(a, b)`, integers, `true`,
  `false`). It reads the state only, as a state law does, through
  `post.ROOT.FIELD`. It compiles with the law compiler into a law program.

### Report

With `--out`, the report is written to that file and a summary is printed
(human lines, or one JSON object with `--format json`). Without it, the
report goes to stdout and the summary lines to stderr. The report is
canonical JSON with schema `zeno-fcis/symbolic-check/1`:

| Field | Meaning |
| --- | --- |
| `status`, `evidence` | `holds` with `proved` or `attested`; `refuted` with `checked`; `inconclusive` with `none`. |
| `authority` | Always `none`: a report grants nothing. |
| `application`, `identity`, `sources` | The template name, the bound Authority's identity, and the SHA-256 of `project.zeno`, `v2/policy.json` and the strengthening file. |
| `domain` | Every input position with its domain, the domain's size (`null` above 2^128), the enumeration cap, and the `route`: `exhaustive-and-symbolic` or `symbolic`. |
| `laws` | The state laws checked (`targets`) and the other laws (`not_targets`). |
| `strengthening`, `genesis` | Each clause, and each target's verdict on the genesis state. |
| `premise` | The premise control. |
| `cases` | Per committing case: its rule text, class, reachability control (`reached`) and one entry per target with `status`, `evidence`, `vacuous`, the `symbolic` query result, the `exhaustive` result when enumeration ran, and a replayed `counterexample`. |
| `domain_controls` | Per target: `substantive`, `implied-by-domain` or `inconclusive`. |
| `summary`, `causes` | Counts of results, vacuous cases and solver disagreements, and every reason the run is inconclusive. |
| `solvers` | Each admitted solver's version and binary SHA-256. |

A query result has `status` `unsat`, `sat` or `inconclusive`, its evidence,
each solver's answer with its wall-clock milliseconds, whether CVC5 printed
proof steps and whether Z3 corroborated, and for `sat` the solver whose model
replayed and what the library made of it (`replayed`). A model that replays
while the other solver answered `unsat` is still a checked counterexample;
the disagreement is reported with it and counted.

## `transform check --symbolic`

```text
zeno-fcis transform check --original FILE --candidate FILE --symbolic --tools FILE
    [--step-limit N] [--max-input-tuples N] [--symbolic-receipt OUT] [--queries DIR]
```

It admits both programs as `transform check` does (the same refusals for
unreadable programs and different input or output domains) and then requires
a domain above `--max-input-tuples`. The Step limit must be at least each
program's node count, so that it never binds; otherwise the result is
inconclusive (`budget-boundary`).

The domain is split by what the original does. One piece holds the inputs on
which it returns outputs with a given first output; for a contract's
decision program the first output is the decision code, so there is one
piece per case (one piece in all when the first output has more than 256
values). Two more pieces hold the inputs on which it fails with `Arithmetic`
and with `OutputDomain`. Each piece is one query for an input on which the
candidate gives another result: another case, other outputs, or another
failure. Every overlapping pair of an original case and a candidate case is
such an input unless the two agree, so every overlapping pair is covered.
The pieces cover the domain: every input traps, leaves an output domain, or
returns outputs with one first output.

Programs are encoded as the library evaluator runs them: every node is
evaluated, including unused nodes and unselected arms, addition and
subtraction are checked in `i64`, and outputs must lie in their domains. A
counterexample is replayed through the library's `execute_v2` on both
programs and stands only if their results differ. A planted control, the
candidate with its first output plus one, must be refuted and replayed in
every run.

A contract's decision table is fixed, so equal outputs give the same
decision, reason, successor and deliveries.

| Status | Exit | Meaning |
| --- | ---: | --- |
| `attested-equivalent` | 2 | Every piece's query was answered `unsat` by CVC5 with proof steps, which nothing checks, and the planted control was refuted. Attested, not proved. |
| `counterexample` | 1 | A replayed input on which the results differ (Checked). |
| `inconclusive` | 2 | A piece or the control was not decided, or the Step limit binds. |
| `refused` | 1 | The exhaustive checker's refusals, and `exhaustive-check-fits` when the domain fits the cap. |
| `solvers-blocked` | 2 | The manifest does not load, configures no CVC5, or a solver fails admission. |

`--symbolic` requires `--tools`, and `--receipt` cannot be combined with it.
`--symbolic-receipt OUT` creates a new file for an `attested-equivalent`
result, with schema `zeno-fcis/transform-symbolic-receipt/1`. It records both
programs' digests, sizes and node counts, the domain, each piece's query
digest and whether Z3 corroborated it, the control, both solvers' identities
and the checker identity (`zeno-fcis/transform-symbolic/1` and the
evaluator digest). It states `"evidence": "attested"`.

**A symbolic receipt is not an exhaustive receipt.** `transform replay`,
`contract adopt`, `contract refresh-receipts` and contract generation read
only `zeno-fcis/transform-receipt/1` and refuse it as unreadable, so a
candidate checked only symbolically cannot be adopted.

## Encoding

Both commands translate exactly what the library runs, never rule text: the
bound Authority's descriptor (its scalar decision program, output types,
decision table, channel schemas and law programs), or two admitted programs.
SMT-LIB's `Int` is unbounded, and every bound is stated explicitly.

| Supported | Semantics followed |
| --- | --- |
| Program instructions `Input`, `Int`, `Bool`, `Add`, `Sub`, `Eq`, `Lt`, `And`, `Not`, `Select` | Eager: every node is defined. `Add` and `Sub` trap outside `i64`. Booleans are 0 and 1 on the wire. |
| Input leaves Bool, I128, U128 within `i128`, Sum and Enum | The ingress codes, with a variant's code mapped to its ID. |
| Decision construction and delivery schemas | Branch selection by code, the reason and class rules, successor field domains (pre and post), delivery conditions, channel lookup, payload order and field domains, ASCII text. |
| Law instructions `Literal`, `Observe` of `post` and `initial` state fields, `Add`, `Sub`, `Mul`, `Div` (floor and ceiling), `ToI128`, `Eq`, `Lt`, `And`, `Not`, `Select` | Eager. Integer results must lie in `i128`; a division needs a nonzero divisor; `div_floor` and `div_ceil` round as defined, with negative operands. A law holds only when every node is defined and its root is true. |

Anything else makes the check inconclusive with reason `unsupported: ...`;
nothing is approximated. That covers other program instructions or domains,
guarded observations (`ObserveWhen`), observations of the pre-state, the
command, the context or the decision in a target, exact division, U128 law
arithmetic, and constants or bounds outside `i128`.

Tests compare the encoding with the library at sampled points: the program
encoding with `execute_v2` (traps, output domains, eager evaluation); the
contract encoding with the bound Authority, the program evaluator and the law
evaluator on 3,000 sampled tuples for each template and both study
contracts; and law arithmetic (`div_floor`, `div_ceil` with zero and negative
divisors, `i128` overflow, `choose`, negation) with the law evaluator.

## Limits and trusted base

- An `unsat` rests on CVC5 and on this encoding. CVC5's proof is not
  checked, and no Lean certificate is produced. The sampled tests check the
  encoding at points, not everywhere.
- A refutation rests on the library evaluators only.
- The check covers the declared state laws and clauses on committing cases.
  It says nothing about laws over the command, context or decision, which
  the Authority enforces, or about liveness.
- A vacuous case or an `implied-by-domain` target is reported, not hidden.

## The app study's escrow

On the escrow of the 2026-10-05 app study
(`crates/zeno-fcis-cli/tests/fixtures/escrow`, more than 2^128 tuples), with
the pinned solvers:

- Law 500 (funds are conserved) with the strengthening above: all 27 targets
  (9 cases, laws 500 and 501 and the clause) hold, attested by CVC5 and
  corroborated by Z3, at about 40–230 ms per CVC5 query; every control was
  refuted with a replayed model.
- Law 500 alone is refuted on the Fund case with a replayed counterexample:
  a Created state that already paid the buyer 1 with 1 funded, which the
  buyer funds with 1. The Authority refuses the decision by law 500. Law 500
  is not inductive on its own.

## Running the solver tests

The tests in `crates/zeno-fcis-cli/tests/symbolic_cli.rs` named
`pinned_symbolic_*` are ignored by default, like the repository's other
pinned-solver tests. They need `ZENO_FCIS_CVC5` and `ZENO_FCIS_Z3` set to the
pinned binaries, and fail if either hashes differently:

```sh
ZENO_FCIS_CVC5=/path/to/cvc5 ZENO_FCIS_Z3=/path/to/z3 \
  cargo +1.97.1 test -p zeno-fcis-cli --locked --test symbolic_cli pinned_symbolic_ -- --ignored
```

The ATDD scenario `symbolic-per-case-checks` always runs the solver-free
tests (the encodings against the library, the reference solver, and
stand-in solvers for a planted disagreement and `unknown`). It runs the
pinned tests when both variables are set; otherwise it prints

```text
atdd: SKIP pinned symbolic solver tests: ZENO_FCIS_CVC5 and ZENO_FCIS_Z3 are not both set (docs/SYMBOLIC_CHECKS.md)
```

and passes. The `formal-tools` workflow sets both variables and runs them.
