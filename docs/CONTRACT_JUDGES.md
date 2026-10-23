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
advisory packet with `authority: none` and changes no application file.
`zeno-fcis contract JUDGE --help` lists each judge's options, and the
sections below describe its report. The
[contract review](CLI_REFERENCE.md#contract-review) section of the CLI
reference gives the review packet's fields.

## Integer values in JSON reports

Scalar integer fields in current door and judge reports use
`safe-number-or-decimal-string/1`, named by `integer_encoding`:

- Values from `-9007199254740991` to `9007199254740991` are JSON numbers.
- Other signed 128-bit values are canonical decimal strings, with a minus sign
  only for negative values and no leading zeros. New numeric fields encode zero as `0`.
- Boolean fields remain booleans; variant names remain names. Expression text,
  program/domain documents and existing string-valued arrays (including
  scenario posts, ordinary review packets, evaluate results and receipts)
  retain their own formats, even for small values.

The current schemas are `zeno-fcis/loop-doors/2`,
`zeno-fcis/black-box-capture/2`, `zeno-fcis/loop-version-space/2`,
`zeno-fcis/contract-profile/2`, `zeno-fcis/contract-lemma/2`,
`zeno-fcis/contract-transition-lemma/2`, `zeno-fcis/contract-probe/2`,
`zeno-fcis/contract-scenario/2`, and the gap-judge packet
`zeno-fcis/contract-review/4`. The ordinary string-valued review packet remains
`zeno-fcis/contract-review/2`. Public loop workspace views use
`zeno-fcis/zal-loop-view/2`. Synthesis result, artifact and vector reports use
`/2`; draft assessments name `zeno-fcis/contract-draft-assessment/2`.

For example, `5` is still a JSON number, while `2^100` is the string
`"1267650600228229401496703205376"`. Python readers decode numeric coordinates
with exact `int` arithmetic. JavaScript readers can display the string directly
or use `BigInt(value)` for an integer field. This preserves existing small-value
meaning and every signed 128-bit endpoint without floating-point rounding.
These reports remain advisory; the encoding supplies no decision authority.

## A contract to judge

The judges enumerate the whole declared domain, so they suit contracts with
small field ranges, such as the [core components](../core-components/README.md).
A bounded counter of capacity 2 has 12 inputs: a value from 0 to 2, an action,
`Inc` (150) or `Dec` (151), and whether the operator is authorized. From a
source checkout, use the ready [counter example](../examples/guide-counter/project.zeno),
which carries no family-certificate qualification. Run the commands below from
the repository root; no instantiation is required. Family installation still
requires fresh source-bound evidence as the core-components README describes.

Its laws say that the value stays within 0 and 2 (law 500), that an accept is
authorized and in range (law 501), and that an accepted `Inc` adds one and an
accepted `Dec` subtracts one (law 502).

## Is the starting state admitted?

A profile, state lemma, reachable-state probe or run-property judgment starts
only after the bound runtime Authority admits the declared genesis. Refusal
reports `no-admissible-starting-state`, the first refusing law when available,
and zero admissible starting and reachable states. It cannot become an
inductive result over an empty run. `new --contract` applies the same gate
before writing application files; ordinary compilation remains available for
diagnosing a refused start. Symbolic induction checks admission before asking
a solver. Table and collection checks use their respective runtime Authority.

A scenario starting at declared genesis uses that admission too. A stated
pre-state uses the state-invariant checks instead: initial-condition law 990
constrains startup, rather than every later state. Existing decision laws,
resource limits and rejection order remain independent checks.

## Does every case decide?

Ordinary `contract review` and `review --gaps` include `case_coverage` for
scalar policies, including the typed numeric profile. When `status` is
`checked`, `scope` is `full-domain` and `tuples` is the complete declared
input count. Each `cases` entry records its zero-based `case` index, optional
`rule` note, rendered `when`, `guard_true` count, `decided` count and
`law_consistent` count. `decided` counts first-match selections before the
Authority admits or refuses the decision. `law_consistent` counts those
selections whose pre-state satisfies every state law. Neither is a claim
about reachability or eventual progress through a history.

A `case-never-decides` finding has `decided: 0` and one of two `cause` values:
`guard never true`, when the guard holds on no declared tuple; or
`covered by earlier cases`, when every matching tuple selects an earlier
case. `covered_by` names each earlier case that actually selects such a tuple,
its `when`, and one `witness` with the first tuple's `ordinal` and `input`.
Witness `input` coordinates are decimal strings in program order, with
Boolean values as 0/1 and variants by ID, including full-width integers.
Several earlier cases can cover one case together. A mere overlap is not a
dead-case finding. A `case-only-on-excluded-pre-states` finding is weaker:
`decided` is positive but `law_consistent` is zero. Its `first` witness names
`unsatisfied_state_law`. Every finding includes the exact human `text`.

For example, inserting `status == 151 -> Reject 200` immediately after the
dual-approval rule `status != 150` reports:

```text
dead case: cases[1] `status == 151` decides no tuple: covered by earlier cases; cases[0] `status != 150` at 151 160 160 140 160
```

The inserted guard holds on 128 tuples but decides none. The shipped
dual-approval policy has no case-coverage findings. Both new finding kinds
contribute to `summary.case_findings` and `summary.findings`, use status
`case-coverage` when no example or law-refusal finding takes precedence, and
exit 1 like existing findings. No new exit code or packet schema is introduced:
these fields extend the ordinary `/2` and gap `/4` packets additively.

Above the enumeration cap, the symbolic packet says
`case_coverage.status: "not checked"`, with a `reason` and `cases: null`.
Boundary reviews also make no case-coverage claim. Undefined guard evaluation
likewise leaves coverage `not checked`, with its first refused tuple; it is
never interpreted as a false guard. Runtime refusals remain separately
reported. This coverage applies to scalar case tables; the separate
`tables/1` review format does not provide this analysis.

**Background and lineage.** First-match rule subsumption, overlapping rules
and missing rules are established decision-table analysis questions. A DMN
decision-table analysis citation could not be confirmed offline, so no
unverified bibliographic citation is asserted here. The scoped integration
reports the existing library's generated guard and selection graph over a
fully enumerated domain; **prior-art search pending**. It grants no runtime,
resource or publication authority.

## Do the laws pin the contract down?

```text
zeno-fcis contract review examples/guide-counter --gaps --out review.json
```

The base summary gains one line:

```text
gaps: 0 of 12 tuples determined by the declared laws (0 of 12 on valid pre-states); mutants admitted 9, caught 28; inactive laws []; reject-everything core admitted where the contract decides: true
```

For each input the judge proposes every decision a contract of this schema
could make: a reject with each declared rejection reason, an accept with each
state as successor, and a committed failure with each declared failure reason
and state. A reason belongs to the one class its cases use, as the
[contract language reference](CONTRACT_RULES.md#cases) requires; a failure-only
reason is never proposed as a reject. The declared laws admit several of them on every input. For
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

Reason dispositions also constrain the alternatives. In the
[dual-approval example](../examples/dual-approval/project.zeno), reason 202
belongs to committed failures and cannot label a reject; `on reject:202`
remains invalid. These [proposed laws](../examples/dual-approval/proposed-laws.txt)
pin rejection eligibility and exclude accepting or failing on a rejecting
input:

```text
zeno-fcis contract review examples/dual-approval --gaps --gaps-laws examples/dual-approval/proposed-laws.txt --out review.json --format json
```

The original laws determine 0 of 384 inputs. With these proposals, 300 inputs
have one admissible decision: 256 closed-request rejects, 32 unauthorized
rejects and 12 repeated-officer rejects. On valid pre-states the count is 220
of 304. The remaining committing decisions can still vary; the proposals do
not establish complete specification or change the contract.

A proposed law can be checked before it goes into `project.zeno`. This one, in
`proposed.zeno`, says that the counter rejects only an unauthorized change or
one at a bound:

```text
law 9000 rejects_only_when_needed on reject = context.102.130 == 0 || (command.101.120 == 150 && pre.100.110 == 2) || (command.101.120 == 151 && pre.100.110 == 0);
```

```text
zeno-fcis contract review examples/guide-counter --gaps --gaps-laws proposed.zeno --out review.json
```

The judge first requires that the proposal holds on the contract's own
decision wherever it applies: it applies on the 8 rejects and holds on each.
Then it repeats every measure with it. The JSON summary retains the original
counts and adds `summary.gaps.with_proposals`; the terminal prints a second
`gaps with proposals: 4 of 12 tuples determined` line with 3 mutants admitted. Determined inputs rise from 0 to 4, the
admitted mutants fall from 9 to 3, and `m0043` is now caught. The reject
reasons stay free: no declared or proposed law says which reason a reject
gives. A proposal that fails on the contract's decision is a false alarm,
reported with the first input, and the command exits 1. Adding a law stays the
owner's edit.

## Which states matter?

```text
zeno-fcis contract profile examples/guide-counter
```

reports, for the counter, 3 declared states, all 3 satisfying the state law,
all 3 reachable from genesis and 3 minimal: no input sequence tells two of them
apart only by its successors. In a retry budget of capacity 1, 4 states are
reachable and 3 are minimal. Two of them always lead to the same decisions,
whatever input follows. That is a design question for a person: an application
persists and exposes its state, so the judge proposes no change.

## Does an invariant hold?

```text
zeno-fcis contract lemma examples/guide-counter --lemma 'post.100.110 <= 1'
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

`zeno-fcis contract scenario examples/guide-counter` replays each run through the contract
and reports the first step whose decision, or written pre-state, differs. When
every run passes, it replays them through every mutant and through the
reject-everything core too. `count-up` expects a commit, so it kills that
core: one concrete run rules out the core the declared laws allowed. When a run
fails on the contract, no kill is credited, since a mutant could fail it for
the same reason.

Unsigned `u128` subjects use the same full-width numeric Authority and
numeric decision-example grammar as contract review. Write unsigned values as
canonical decimal text, including values above the signed range. Numeric
scenarios check accept, reject and committed-failure decisions, channel/payload
deliveries, step continuity and genesis admission. The signed grammar also
supports the identity suffixes and technical-refusal observations described
below; those extensions are not part of the numeric example grammar.

`contract probe count` uses the existing numeric space on unsigned subjects.
It counts the original Authority decisions, with separate false, undefined and
refused totals; the condition is an observation and never an admission law.
A domain above `--max-tuples` remains inconclusive. Neither command narrows an
unsigned value through a signed integer.

## Can the seller be paid twice?

A seller wants to know that a repeated release cannot pay them again, and that
no payout follows a refund. The escrow example declares these questions in
`v2/run-properties.json`:

```json
{
  "schema": "zeno-fcis/run-properties/1",
  "properties": [
    {"kind": "at_most_once", "name": "seller_paid_at_most_once", "channel": 300},
    {"kind": "never_after", "name": "no_payout_after_refund", "a": 301, "b": 300},
    {"kind": "predicate", "name": "one_payout_per_decision", "formula": "outbox.300 <= 1"}
  ]
}
```

```text
zeno-fcis contract runs examples/apps/escrow-deal
```

All three receive `inductive`. This means proved inductively over reachable
states of this contract paired with its observer, exhaustive over the declared
finite domains. It holds for runs of any length in that finite model. It does
not claim a property over unbounded domains, other parameter choices, shell
replay keys, stale revisions or external payment completion. A delivery here
is an admitted outbox obligation; a destination's payment and retry behavior
needs its own assurance.

The first built-in form counts deliveries on the named channel over the whole
run, saturating at two so the observer remains bounded. Optional `key_field`
counts separately for every value in that payload field's declared finite
domain, including multiple matching deliveries in one decision. It currently
requires a numeric, Boolean or variant key domain of at most 64 values.
`never_after` excludes a B delivery on a decision strictly later than an A
delivery. A and B in the same decision do not violate that form; use a
predicate to exclude that too. Neither form depends on delivery ordinals to
count payments.

Use an ordinary state lemma when the contract already retains the required
fact. An observer is for history the contract does not store. For example:

```json
{
  "schema": "zeno-fcis/run-properties/1",
  "observer": {
    "saw_refusal": {"min": 0, "max": 1, "initial": 0,
      "update": "choose(verdict.reject || verdict.refused, 1, before.saw_refusal)"}
  },
  "properties": [
    {"kind": "predicate", "name": "refusals_recorded",
     "formula": "verdict.reject -> observer.saw_refusal == 1"}
  ]
}
```

Updates run simultaneously on every decision, including rejects, committed
failures and technical refusals. They read `before.NAME`, the verdict and the
complete deliveries; predicates read both `before.NAME` and the updated
`observer.NAME`, plus `pre.`, actual `post.`, and delivery observations.
Verdict flags are `verdict.accept`, `verdict.reject`, `verdict.failure`, and
`verdict.refused`. At genesis all flags are false, deliveries are empty,
`pre.` and `post.` both mean genesis, and before/observer values are the declared
initial values. Command and context are absent at genesis. The predicate must
be defined and true there too.

Observer fields have explicit inclusive integer bounds. Undeclared names,
wrong types, duplicate JSON keys and unknown fields are refused. Expressions
are limited to 4096 bytes and the existing bounded expression admission. Arithmetic
uses the library's existing eager checked scalar interpreter. Undefined
expressions and updates outside observer bounds block an inductive result.
The checker admits at most 32 declared observer fields and 64 properties.
`--max-tuples` bounds both the contract domain and each full declared product
of contract and observer domains; exceeding either is inconclusive (exit 2).
An unbounded observer counter must be replaced with a bounded sufficient fact,
such as zero/one/two-or-more.

The checker observes the actual library Authority, pairs its finite states
with observer values, and reuses the state lemma judge's initiation and
reachable consecution check. Exhausting every outgoing input from every
reachable pair establishes a closed finite invariant. A failed predicate is
`false` (exit 1); its breadth-first parents give a shortest run from genesis
as scenario lines, with exact delivery ordinals. A failed genesis obligation
has a zero-step trace. Undefined results and incomplete enumeration exit 2
and claim nothing. The packet schema is `zeno-fcis/contract-run-properties/1`;
it binds the project, policy and property file by SHA-256 and grants no
runtime, resource or publication authority.

## Reading deliveries in a question

Transition lemmas (`lemma --transition`) and condition probes can read
`outbox.count` for all deliveries, `outbox.CHANNEL` for a channel's count and
`outbox.CHANNEL.FIELD` for the checked sum of that payload field over every
matching delivery. For example, `outbox.300 <= 1` and
`outbox.300.140 == post.100.112 - pre.100.112` test the seller's deliveries.
The legacy payload-qualified shorthand is supported only when exactly one
channel uses that payload type; shared payload types require the channel ID.
State-only lemmas continue to read state, since a state alone has no deliveries.

`delivery.INDEX.exists`, `.channel`, `.ordinal`, `.idempotency` and
`.field.FIELD` inspect an individual delivery at its zero-based list index,
up to the policy's declared maximum delivery count. An absent delivery or
field reads zero, with `exists` false; use the flag when absence matters.
Only numeric, Boolean and variant payload fields have a number form.
An unrepresentable value or overflowing field sum is undefined when read.

A scenario delivery may be written `CHANNEL@ORDINAL:IDEMPOTENCY PAYLOAD...`.
For example, `300@0:0 2` requires channel 300, delivery ordinal 0,
idempotency ordinal 0 and payload 2. Each optional pair is compared exactly;
the older `300 2` continues to compare only channel and payload. A mismatch
fails the scenario. Scenarios therefore observe deliveries, including their
identity ordinals. Technical refusals use `inputs | refused CLASS STATE | -`,
where CLASS is the reported refusal class and STATE repeats the pre-state.
These are diagnostic scenario observations and cannot commit a decision.

The scenario judge reports the earliest failing step, with `ordinal differs`
or `idempotency ordinal differs` when a pinned identity value changes. The
suffix is optional for each delivery, so older examples retain their meaning.
Background: Glenford J. Myers, *The Art of Software Testing*, Wiley (1979),
describes checking observed results against expected outcomes; Leslie Lamport, *Time, Clocks, and the Ordering of Events in a
Distributed System*, Communications of the ACM 21(7), 1978, supplies background
on ordered event identities. These comparisons concern declared delivery
metadata, with no claim about receiver execution; prior-art search pending.


## Asking a question

`contract probe` answers one exact question between edits:

```text
zeno-fcis contract probe explain examples/guide-counter --input '2 150 1'
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
  decision. A determined input means the declared laws admit exactly one full
  outcome equal to the Authority's successfully executed decision: class,
  reason, successor and complete ordered deliveries. This does not establish
  the owner's intent. The question door uses the same definition, with any
  explicit answer requirements included in its stated basis. Its law-only
  count matches the gap judge on the exported laws. For supported channels,
  `determination.uncovered_components` names `deliveries` that those laws
  cannot constrain; complete outbox answers are compared separately.
  Delivery-observing laws retain the complete-universe scope guard.
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
- each case's guard, first-match and state-law-consistent selection counts,
  and its dead or excluded-only classification;
- each lemma's verdict, initiation, state counts, per-state digest,
  counterexample to induction and trace;
- each scenario's verdict, and whether one kills the reject-everything core;
- the probes `decisions` and `depends`, and the case `explain` names for one
  input.

It does not recompute mutant classifications, the measures repeated with
proposed laws, or the probes `count`, `implies` and `reachable`; unit and
command tests cover those. Both sides must also equal the recorded known
answers in `tools/test_data/contract_judges/known-answers.json`. On all 77
complete instances the declared laws refuse the reject-everything core and
one generated scenario kills it. The failure and refusal variants retain
partial laws that admit it; their scenarios also kill it. Coverage witnesses
are checked by unit and native controls rather than this reference. The same
builder wrote the judges and the reference, so a
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
## Background

Edmund M. Clarke, E. Allen Emerson and A. Prasad Sistla, “Automatic
Verification of Finite-State Concurrent Systems Using Temporal Logic
Specifications,” *ACM Transactions on Programming Languages and Systems*
8(2), 1986, is background for exhaustive finite-state verification. The
starting-state and transition checks here are implementation-specific
acceptance conditions within that broader tradition.

## What is new here

This release shares runtime genesis admission across the finite judges and
application generation, pairs contract decisions with bounded run
observers, and shares typed decision alternatives with the question loop. The comparative assessment of this combination is **prior-art
search pending**.

## Scope

The judgments cover the declared finite domains and explicit resource limits.
They remain advisory, and provide no shell execution, publication or external
settlement authority. The tests and independent references qualify the named
subjects and implementation, with the limits described above.
