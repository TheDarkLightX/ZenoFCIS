# Contract judges

A contract's case table decides; its laws state what every decision must
satisfy. Four questions remain after the contract compiles and its decision
examples pass:

- Do the declared laws pin the decisions down, or would they admit a quite
  different decision table?
- Which states can a store actually reach, and does the contract keep
  information its decisions never show?
- Does a candidate invariant hold on every reachable state?
- Does a run written from intent go the way the owner expects?

`zeno-fcis contract review --gaps`, `contract profile`, `contract lemma`,
`contract scenario` and `contract probe` answer them exactly over the
contract's declared domain. Each binds the contract to the library Authority as
an application binds it, so every decision they report is the library's, and
every law verdict comes from the library's law evaluator. Each writes an
advisory packet with `authority: none` and changes no application file. The
[CLI reference](CLI_REFERENCE.md#contract-judges) gives every option, field and
exit code.

## A contract to judge

The judges enumerate the whole declared domain, so they suit contracts with
small field ranges, such as the [core components](../core-components/README.md).
A bounded counter of capacity 2 has 12 inputs: a value from 0 to 2, an action,
`Inc` (150) or `Dec` (151), and whether the operator is authorized. From a
pristine source checkout, as the core-components README describes:

```text
python3 tools/instantiate_core.py bounded-counter counter --param C=2
```

Its laws say that the value stays within 0 and 2 (law 500), that an accept is
authorized and in range (law 501), and that an accepted `Inc` adds one and an
accepted `Dec` subtracts one (law 502).

## Do the laws pin the contract down?

```text
zeno-fcis contract review counter --gaps --out review.json
```

The summary gains one line:

```text
gaps: 0 of 12 tuples determined by the declared laws (0 of 12 on valid pre-states); mutants admitted 9, caught 28; inactive laws []; reject-everything core admitted where the contract decides: true
```

For each input the judge proposes every decision a contract of this schema
could make: a reject with each declared reason, an accept with each state as
successor, and a committed failure with each reason and state when the
contract has one. The declared laws admit several of them on every input. For
an authorized `Inc` at 0 the contract accepts and moves to 1, but the laws also
admit a reject with any of the three reasons: nothing in them says when the
counter must accept, or which reason a reject gives.

The same gap shows in the mutants. Of 48 mutants of the extended catalog, 9
decide differently somewhere and are never refused by the Authority where the
contract decides. `m0043` turns the accepting `Inc` case into a reject: the
declared laws admit a counter that refuses every authorized increment. The
packet gives a witness for each, an input with both decisions. The
reject-everything core, a contract of the same declarations whose only firing
case rejects, is admitted on every input for the same reason.

A proposed law can be checked before it goes into `project.zeno`. This one, in
`proposed.zeno`, says that the counter rejects only an unauthorized change or
one at a bound:

```text
law 9000 rejects_only_when_needed on reject = context.102.130 == 0 || (command.101.120 == 150 && pre.100.110 == 2) || (command.101.120 == 151 && pre.100.110 == 0);
```

```text
zeno-fcis contract review counter --gaps --gaps-laws proposed.zeno --out review.json
```

The judge first requires that the proposal holds on the contract's own
decision wherever it applies: it applies on the 8 rejects and holds on each.
Then it repeats every measure with it. Determined inputs rise from 0 to 4, the
admitted mutants fall from 9 to 3, and `m0043` is now caught. The reject
reasons stay free: no declared or proposed law says which reason a reject
gives. A proposal that fails on the contract's decision is a false alarm,
reported with the first input, and the command exits 1. Adding a law stays the
owner's edit.

## Which states matter?

```text
zeno-fcis contract profile counter
```

reports, for the counter, 3 declared states, all 3 satisfying the state law,
all 3 reachable from genesis and 3 minimal: no input sequence tells two of them
apart only by its successors. In a retry budget of capacity 1, 4 states are
reachable and 3 are minimal. Two of them always lead to the same decisions,
whatever input follows. That is a design question for a person: an application
persists and exposes its state, so the judge proposes no change.

## Does an invariant hold?

```text
zeno-fcis contract lemma counter --lemma 'post.100.110 <= 1'
```

A lemma is written as a state law. The judge answers by induction over the
declared domain: genesis satisfies it, but from state 1 an authorized `Inc`
commits state 2, which it excludes, and state 1 is reachable. The verdict is
`false`, with the shortest trace from genesis, two `Inc` steps. A lemma that
every reachable state satisfies, but that an unreachable state it admits
breaks, is `not-inductive`: it is true here and must be strengthened before
induction proves it. An `inductive` lemma holds on every reachable state of the
declared domain.

## Does a run go as intended?

A scenario file groups decision-example lines into runs under `# scenario
NAME` comments:

```text
# scenario count-up
0 | 150 | 1 | accept - 1 | -
1 | 150 | 1 | accept - 2 | -
2 | 150 | 1 | reject 201 2 | -
# scenario unauthorized
0 | 151 | 0 | reject 200 0 | -
```

`zeno-fcis contract scenario counter` replays each run through the contract
and reports the first step whose decision, or written pre-state, differs. When
every run passes, it replays them through every mutant and through the
reject-everything core too. `count-up` expects a commit, so it kills that
core: one concrete run rules out the core the declared laws allowed. When a run
fails on the contract, no kill is credited, since a mutant could fail it for
the same reason.

## Asking a question

`contract probe` answers one exact question between edits:

```text
zeno-fcis contract probe explain counter --input '2 150 1'
```

names case 1, `(action == 150) && (value == 2)`, as the case that decides, case
0 `!authorized` as tried first and found false, and the decision as the example
line `2 150 1 | reject 201 2 | -`. Other probes find a shortest path to a state,
count the inputs where a condition holds, test an implication, list the
distinct decisions, and find two inputs that differ in one position and are
decided differently.

## What the judges establish, and what they do not

- Every answer is exact over the declared domain and nothing beyond it. A
  judge that enumerates the domain, every one except `scenario` and `probe
  explain`, is `inconclusive` over `--max-tuples`, and `review --gaps` also
  over `--max-work`: it claims nothing and exits 2.
- The gap judge reads only the laws `project.zeno` declares; the framework laws
  generation adds are left out, since law 991 admits only the case table's own
  decision. A determined input means the declared laws admit one decision
  there, not that it is the decision the owner intends. The judge proposes
  decisions without deliveries, so a contract that delivers is out of its
  scope.
- An admitted mutant is a witness that the laws allow a different decision
  table. A caught mutant is one the Authority refuses on at least one input
  where it decides differently from the contract; its other differences may
  still be admitted, and the refusal may come from a value leaving its declared
  domain rather than from a law. The catalog is finite, and catching a mutant
  says nothing about changes outside it. A mutant that survives every scenario
  is not shown equivalent.
- An inductive lemma holds on the reachable states of this declared domain. It
  is not a proof for other parameter values or wider ranges, and it is checked
  by enumeration, not by a solver or proof assistant; `prove` handles inductive
  claims with a solver.
- Minimal states are counted by decisions only: class and reason, or refusal.
  The judges do not propose a hardened or smaller contract.
- A scenario says what one run does. Passing scenarios do not show that the
  contract is right elsewhere, and kills are credited only when every scenario
  passes on the contract.

## How the judges are checked

`tools/check_contract_judges.py` regenerates the 77 declared instances of the
ten core-component families, adds four bounded-counter variants with a
committed failure, a law the case table breaks and a law that determines some
decisions, and runs every judge on each through a built CLI. A separately
written reference, `tools/contract_judges_reference.py`, reads the same two
files by the public rules in this documentation, decides every input and
evaluates every declared law itself. It recomputes part of what the judges
report, and the check compares exactly that part:

- the profile's state counts, memory bits, state laws, robustness counts and
  the digests of its per-input decision table and per-state table;
- the gap judge's admissible counts and freedom on both domains, law
  activation, its per-input digest and the reject-everything verdict;
- each lemma's verdict, initiation, state counts, per-state digest,
  counterexample to induction and trace;
- each scenario's verdict, and whether one kills the reject-everything core;
- the probes `decisions` and `depends`, and the case `explain` names for one
  input.

It does not recompute mutant classifications, the measures repeated with
proposed laws, or the probes `count`, `implies` and `reachable`; unit and
command tests cover those. Both sides must also equal the recorded known
answers in `tools/test_data/contract_judges/known-answers.json`. On all 77
instances the declared laws admit the reject-everything core and one generated
scenario kills it. The same builder wrote the judges and the reference, so a
mistake they share would not show as a disagreement.

Fault controls show that the comparisons can fail. Each compared field,
perturbed in the CLI's answers, is reported by the comparison; each recorded
field, perturbed, is reported by the recorded-answer check; contracts with a
single input decided differently, at the first, last and a seeded random
position, disagree with the original's answers when run through the CLI; and a
fault on a scenario's path, a false proposed law, malformed proposals and
every limit get their own verdicts. A self-test disables the comparisons and
requires the controls, and the whole check, to fail. The acceptance scenarios
`specification-gaps` and `core-profile` run these checks. Their pass is
evidence about these subjects and that build, not a proof about every
contract.
