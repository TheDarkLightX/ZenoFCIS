# ZAL loops: proposing programs to the bounded loop

The bounded `zeno-fcis loop` judges complete candidate programs against an
admitted original. The **doors** described here let a proposer, a person or
a model, say more than a whole program: one expression per output, a program
with holes, or one program for a whole family of core components. The doors
work out candidates and explanations. **The model proposes; ZenoFCIS's
checkers decide acceptance**: every program reported as accepted, completed
or "checked repair" has passed `transform check` on every declared input
tuple, and its receipt has been replayed byte for byte. Regions, repair
sites, "unrealizable" and "not found" are the doors' own findings, which no
checker decides.

People and models write their proposals in ZAL's loop profile,
`zal/fcis-loop/1`, in either of two surfaces (symbolic or controlled
English), and work in one shared workspace. Only the owner answers questions,
through the terminal.

Laws, invariants, scenarios and probes are routed to the contract judges;
teach-back rules, questions to the owner and architectures go to three more
doors over the contract's whole declared domain ("Requirements" below).

## The doors

| Door | You submit | What comes back | What it means |
|---|---|---|---|
| Expression | One expression per output (`loop candidate --expressions`) | Accepted, or a counterexample with the failing region, ranked repair sites and possibly a checked repair | The session's own check decides; the repair is labelled "checked repair" only after `transform check` accepts it and its receipt replays |
| Sketch | Expressions with holes (`loop sketch`) | `completed`, `unrealizable`, `not-found`, `inconclusive` or `refused` | `completed` passed `transform check` with replay; the session checks it or recognizes a byte duplicate; `unrealizable` is the doors' claim, with a witness input or a listed, exhausted, complete candidate space; `not-found` claims nothing |
| Family | One program over family parameters (`loop family`) | A verdict per instance, the smallest failing instance, and fitted forms `a * $P + b` checked again | Holds on the checked instances only; only a checker counterexample makes an instance `refuted`, and an unrealizable sketch is labelled `unrealizable` |
| Transfer | A program written for one instance (`loop transfer`) | Per target instance: carried over directly, by opening constants near a changed parameter, or by repair | Each result is checked on its instance |
| Meaning | A complete program (`loop meaning`) | Its meaning key and whether it means what the original means | Equal keys exactly when `transform check` finds the programs equivalent, within its limits |

Each expression or sketch spends one session attempt, like a JSON
candidate. A duplicate that passes the door's confirming check and replay
completes with exit 0 and `selection: duplicate`; it still spends its attempt.
The door's confirming `transform check` and replay run first; if
they do not complete in the invocation's time, the result is `inconclusive`,
the attempt is not spent and the incumbent is unchanged. Family and transfer runs keep no session. A loop session refuses an
original whose check would need more work than its per-check allowance
(`work-infeasible`). Where that happens depends on the program's size as
well as its input tuples: with the default allowance, the core components'
programs reach it above roughly 6,000 tuples, and the smallest programs near
18,000. On such a core only the family and transfer doors are available,
and a workspace cannot be opened on it.

### The expression language

It is the contract policy-expression grammar: fields by name, declared
variant names (`action == Inc`), `true`, `false`, `!`, `&&`, `||`, `->`,
`==`, `!=`, `<`, `<=`, `>`, `>=`, `+`, `-`, and `c ? a : b` at the lowest
level. Boolean fields used as integers lower as `field ? 1 : 0`, including
vote sums. The doors add:

| Form | Meaning |
|---|---|
| `?c` | some constant |
| `?c[lo..hi]` | some constant from `lo` to `hi` |
| `?e` | some expression, found by synthesis |
| `?{a \| b}` | exactly one of the listed expressions |
| `$C` | a family parameter, bound per instance |
| `keep` | the whole output, unchanged from the original program |

A document (`zeno-fcis/loop-expressions/1`) lists one expression per output
and the vocabulary that gives the field names in program order, the variant
names and the number of cases. It is ASCII, at most 64 KiB and at most 256
expressions; an unknown name is refused. An expression is at most 8,192
bytes and 512 nodes; nesting (parentheses, `!`, unary minus, conditionals,
`->` chains) is bounded before the parser descends, and a tree deeper than
128 is refused, in the loop doors and in the workspace's parsers alike.

### Why a proposal is wrong, and how to fix it

When the check finds a counterexample, the expression door also reports:

- **the region**: how many tuples are wrong, and either a formula that holds
  exactly on them (the doors evaluate it on every tuple before showing it)
  or the field values they all share. `output_regions` describes each wrong
  output separately, alongside the union;
- **the sites**: the positions in your own expression where some value would
  make the output right everywhere. Constants and edits precede synthesized
  rewrites; replacement size and edited size precede whole-program size.
  By the doors' own inverse reasoning, no other position can fix
  the output; no checker confirms this. A site's replacement is an exact constant, a
  one-step edit (a negated or flipped condition, swapped branches, an
  off-by-one, the other operator) or a synthesized term. The root of an
  output is a site for a constant or a one-step edit;
- **the repair**: the best site's fix for each wrong output, or a short
  greedy sequence of fixes when faults sit in several branches. Greedy
  edits run before synthesized rewrites.

### Holes

Each output's holes are solved on their own. A single constant hole has a
complete answer over signed 64-bit constants: a value, a single-input
witness, or exhaustion of the shared constant's constraints. Different
inputs can require incompatible values while each input permits a value
on its own. Such a result reports `complete-space-exhausted` with its scope
and, where available, `conflicting_inputs`; it supplies no false single-input
witness. A choice hole is decided by trying each option. A constant hole
compared with an expression without holes gets a complete candidate range
(that expression's values, one wider on each side). Additive offsets around
the hole shift that range, including `e + (?c - k)` and `(?c - k) + e`;
intermediate overflow limits remain part of the derived range.

With several holes, the search runs over what the holes mean, not over
their values:

- a constant or choice hole that sits **alone on its route** is derived
  exactly by itself. Alone means every test above it and every operand
  beside it is free of holes; holes in the other branch of a `c ? a : b`
  serve other inputs and do not count;
- the joint pivot, initial candidate spaces and reading order are frozen
  before that solving starts. The remaining holes are visited in that
  order, with the first value of each equal parent-vector class retained;
- only terminal assignments or disjoint pruned subtrees spend the assignment
  budget. Internal visits and class-value work are reported separately.
  Partial class construction stops inconclusively before an empty-space claim.
  The assignment-prefix argument is scoped to constant/choice holes; it is
  not a time or total-work guarantee.

No distinct program is skipped, so a complete candidate set stays complete.
A constant hole with no range and no comparison is still enumerated from a
heuristic candidate list (small numbers, the program's constants, domain
bounds, frequent offsets) that is not complete, so a sketch can be
completable without the doors finding it. A candidate whose `+` or `-`
would leave the signed 64-bit range on some input is skipped: the program
would trap there.

### Families

A family manifest (`zeno-fcis/loop-family-manifest/1`) lists instance
programs with their parameter values; the shell writes it from instances it
generates with `tools/instantiate_core.py` and `contract export-program`. A
manifest entry may be marked `"fit": false`: its holes are solved and
checked, but it is not used to fit the forms, so the fitted program's check
on it is a check on an instance it was not fitted on.

## The workspace and the terminal

```sh
python3 integrations/zal/terminal.py loop init --contract DIR --workspace W.json
python3 integrations/zal/terminal.py loop init --family bounded-counter --workspace W.json
python3 integrations/zal/terminal.py loop show --workspace W.json            # latest events
python3 integrations/zal/terminal.py loop show --workspace W.json --packet   # the subject card
python3 integrations/zal/terminal.py loop next --workspace W.json           # next contract question
python3 integrations/zal/terminal.py loop answer q1 2 --workspace W.json     # or any / none
python3 integrations/zal/terminal.py loop say "rule t1: where value == 0 -> reject at_zero;" --workspace W.json
```

The workspace binds its subject by revision: the SHA-256 of the contract
text, its policy and the exported program's digest. At initialization it
copies the contract's `project.zeno` and `v2/policy.json` into its own
directory, read-only; the revision, the original program and every routed
judge read that copy, never the source directory, so a later edit of the
source changes nothing in this workspace (a new workspace sees it). A copy
whose bytes no longer match the hashes recorded at initialization is
refused before any judge reads it. Remembered invariant and rule verdicts
are keyed by the revision and the policy's hash as well as by their
meaning. It keeps a hash-chained
transcript in which every event carries both surfaces, verified on every
load, and a view file `W.json.view.json` for displays. Its incumbent is the
loop session's checked incumbent, which the session re-checks and replays on
every door command; the workspace's copy is for display.

### Memory by meaning

The workspace remembers a refuted program by what it means, not only by its
text. `zeno-fcis loop meaning` keys a complete program by its observation on
every declared input, as the library's evaluator gives it to
`transform check`: every output, or the exact failure. Two programs get the
same key exactly when `transform check` finds them equivalent, within its
limits, on the declared finite domain.

A program that means what an earlier refuted program meant is answered from
memory with a new judge statement, and spends no attempt:

```
same p2 as p1: 36 inputs, event 1, refuted;
Same p2: it means exactly what p1 means on all 36 inputs, judged refuted at event 1.
```

A correct program is never answered from memory: every correct program means
what the original means, and each is judged for its size. Laws are not keyed:
what a law admits relates every input to every possible decision, too much
to tabulate for each submission.

## Requirements: laws, invariants, scenarios, probes, rules and questions

These acts describe what the core should do rather than propose a program.
The workspace writes each in the form a contract judge reads (field names
become numbered paths such as `pre.100.110`; a Boolean field reads as
`== 1`) and replies with what the judge found. The translation keeps the
loop language's meaning: in a truth position a value is true when it is 1,
`!x` on a number is `x == 0`, and a Boolean field in a sum is 0 or 1, as in
the contract's own laws. A form whose meaning has no exact law text
(comparing a truth value with a number, `||` or `->` on a number, a
conditional whose branches mix a truth value and a number) is refused,
never rewritten.

| Act | Judged by | The reply |
|---|---|---|
| `law ID on SCOPE: F;` | `contract review --gaps` with the law proposed | where it applies and how many surviving mutants it rules out, or a false alarm: an input where the core decides against it. It stays a proposal for the owner's review |
| `invariant ID: F;` | `contract lemma` | inductive (checked on every declared state), false (with the reachable step that leaves it, or the failing genesis), or not inductive (with the unreachable step that leaves it) |
| `scenario` and `step` | `contract scenario` | passed, or the first step that differs; how many mutants it kills, and whether the reject-everything core fails it |
| `probe ID: explain / reachable / count / implies / depends ...;` | `contract probe` | the probe's exact answer. A probe over program outputs (`out0`) or `values`/`distinguish` has no contract probe |
| `probe ID: freedom;` | `loop question` | how many decisions the requirements determine, and a question for the owner |
| `rule ID: where C -> D;` | `loop teach-back` | exact, holds (with the inputs it misses), wrong (with its exceptions), or vacuous |
| `component`, `case`, `decompose` | `loop decompose` | required behaviour, interface counterexamples, case tables, blame |

The `loop question` door and session commands require Linux and a reachable
systemd user manager with the configured containment facilities. Check
`systemctl --user show --property=Version` if startup refuses. A missing user
manager is a `containment-unavailable:systemd-user-manager` prerequisite
failure. Cleanup uncertainty is reported separately and does not by itself
establish that a writer leaked. These commands retain their containment
requirements; see [installation prerequisites](../INSTALLATION.md).

The review summary labels original example disagreements and law refusals
separately from `gaps`, which measures freedom left by declared laws. Run
`contract scenario` to measure decision examples and mutant kills.
`findings 0` alone establishes neither complete requirements nor scenario
coverage. Structurally refused mutants remain separate from semantic kills.

### The invariant feedback cycle

An invariant's reply is feedback for the next proposal, recorded with its
round in the transcript. A false or non-inductive invariant comes with its
counterexample to induction: the state it holds on, the input, the core's
decision there and the successor outside it, and, for a false one, the path
from genesis. A false invariant at genesis comes with the genesis state. An
inductive one comes with its scope: every declared state, checked
exhaustively. The proposer revises the invariant and submits it again. This
holds over the declared finite domain only, and "inductive" is about this
contract's decisions, within its bounds.

### Questions for the owner

`loop question` groups the inputs where the declared laws, and the owner's
answers so far, still admit more than one decision, by the case of the core
that fires there, and asks about the largest group. The question names its
inputs with a formula checked on every input to hold on exactly them, an
example, and the decisions still admitted, the core's own marked.

The owner answers through the terminal. The answer becomes laws over exactly
the asked inputs, and the gaps judge checks them against the core. Two cases
are reported, not refused: an answer that differs from the core is a
**disagreement**, and the core is never changed to match it. An answer that
leaves some input with no admissible decision, given the earlier answers, is
refused, and the question and the recorded requirements stay as they were.

The contract law language cannot name a reject reason. When the owner picks
a reason, the reason is recorded beside the laws and only the question door
applies it; the gaps judge does not see it. Stats and workspace facts report
`determined_by_laws` beside `determined_with_reasons`. The legacy `determined`
and `determined_rate` fields retain their meaning with recorded reasons.
Conflicts include an input and the requirements responsible; proposed-law
false alarms include the owning answer, scope and formula beside the law ID.

Repeated freedom probes reuse the open question's ID and emit one `ask`.
A changed question supersedes the old one, whose answer is then refused.
The owner's `loop next` starts this flow without a model round trip.

A teach-back rule that the requirements do not yet force becomes a question
too: whether the core must decide that way where the rule holds.

### Teach-back

`loop teach-back` judges a rule restating how the core decides on every
input of the declared domain, and on the states reachable from genesis:
**exact** (it applies exactly where the core decides so), **holds** (it
also misses some inputs, listed as a region), **wrong** (exceptions, grouped
by what the core does instead, each with a region and an example) or
**vacuous**. A set of exact rules that covers every input without overlap is
a complete summary of the core.

### Decomposition

`loop decompose` holds an architecture (components in merge order, each
owning state fields, reading fields and deciding reasons) against the core:

1. **Required behaviour**: the component that owns an input's reason must
   make the core's decision there; every component before it must accept and
   give the core's successor for the fields it owns.
2. **Realizability**: two inputs that agree on everything a component reads
   but need different behaviour from it are an interface counterexample. The
   door suggests the fewest further reads that tell every such pair apart,
   and says when a read belongs to a later component (the architecture then
   needs a reorder). Reasons no component decides, and failures that must
   change a field a later component owns, are reported.
3. **Case tables**: a component without cases gets the core's case table
   projected onto it, or a lookup table. A component with cases is blamed,
   with a region and an example, where it differs from its required
   behaviour. A committed successor outside its field's domain is marked
   refused, and counts as a difference.

Steps 1 to 3 are the door's own evaluation (`"decided_by":
"door-simulation"`): the required behaviour comes from the core's Authority,
but the components' tables are evaluated by the door's expression evaluator,
not by any component Authority or checker, so `completed`,
`meets-requirements` and `refuted` are proposals. An interface
counterexample is a witness anyone can recheck. A structure problem or an
interface counterexample makes the verdict `unrealizable`, whatever tables
are given. An explicit read of a later component's field reports
`reads-later` with the field and a suggested component order. A cycle has
no suggested order. This finding concerns the submitted order; a suggested
reorder remains a proposal.

4. **The composed check** (`--compose`): the components, each with its case
   table, become a composed-domain composition
   (`zeno-fcis/checked-composition/3`): one generated contract per component,
   and a manifest that binds each component's inputs to the request and to
   earlier components' state. The composition checker's own joint step runs
   on every input of the core's declared domain and is compared with the
   core's decision; the composition's obligation check runs too. Every
   submitted guard is kept as written; where a component's last guard is not
   `true`, a catch-all reject with a reason of its own is appended, so an
   input no submitted case decides differs from the core. The reported
   verdict is the weaker of steps 1 to 3 and the composed check: a step-3
   blame is never reported as `accepted`. `--export DIR` writes the manifest
   and the contracts to a new directory (an existing one is refused), for
   `zeno-fcis contract check-composition`, whatever the verdict: a refuted
   composition is exported too. The generated contracts carry only their fields'
   bounds as a state law and `true` invariants and assumptions, so `proved`
   says the composition is well formed and its bindings stay in their
   domains; the comparison is what says it behaves as the core. The export
   also contains `verdict.json` with the composed status, difference count,
   first difference, contract-source digest and manifest digest. A refuted
   comparison remains `refuted` there even if the bounds obligations pass.

This no-outbox composition profile refuses source cores that declare channels
or contain deliveries, with `compose-core-deliveries-unsupported`, before any
export. Its comparison covers class, reason and successor only; the source
restriction prevents delivery-stripped components from being accepted.

Steps 1 to 3 alone do not claim that the components composed behave as the
core; only step 4 is a checked verdict (`"decided_by":
"composition-checker"`), for the core's declared finite domain only.

### Memory by meaning for invariants and rules

An invariant is remembered by the declared states it admits (the lemma
judge's own table digest), and a teach-back rule by where it applies, the
decision it claims and, when it gives one, its successor on every input
(teach-back compares the successor outside the rule's region too, for the
inputs it misses), so equal keys give equal verdicts. The same
invariant or rule in other words is answered from memory with its earlier
verdict (`same i2 as i1: 9 states, event 3, inductive;`).

## MCP tools

`integrations/zal/mcp.py` offers, for the workspace named by
`ZAL_LOOP_WORKSPACE`: `zal_loop_read`, `zal_loop_packet`, `zal_loop_submit`,
`zal_loop_translate` and `zal_loop_legend`. There is no answer or say tool,
and a model document that answers is refused. A tool permission granted by a
harness is not the owner's agreement, and nothing here adopts a program or a
requirement: adoption stays `zeno-fcis contract adopt`.

## What is not claimed

- No result is claimed to be the smallest program; "checked" means equal to
  the original on every declared tuple, nothing more.
- Family results cover the checked instances only, never all parameter
  values or sizes.
- A meaning key covers the declared finite domain only; it says nothing about
  inputs outside it.
- A synthesis "not found" claims nothing about whether a completion exists.
  Synthesis has a fixed work budget per door run, so on larger domains it
  finds less; constants and one-step edits are tried first and do not use it.
- Nothing is claimed about the quality of any model that uses the doors.
- Channels separate authority by convention between clients of one user;
  they are not authentication.
- Every requirement result (questions, teach-back, decompositions, invariants)
  holds over the contract's declared finite domain only.
- Size, in practice: each door stage has a fixed 5-second deadline, so
  teach-back and a question with answers can stop inconclusive on declared
  domains above roughly 10,000 state × input tuples, and the admissibility
  table is refused (inconclusive) above 4,000,000 law checks. An inconclusive
  result claims nothing. The judge controls apply these two doors to domains
  of at most 8,192 tuples and report larger instances as outside that
  envelope.
- A teach-back rule is `required` only where every decision the confirmed
  requirements admit is the rule's; where none is, it is reported
  `forbidden` with an admitted decision as the witness. The copy check on a
  workspace's contract is a hash comparison before each judge call, not a
  lock against a writer inside the workspace's own directory.
- An answer's reason, which no law can state, binds only the question door.
  The gaps judge does not see reason requirements: its counts and its
  false-alarm checks cover the answers' laws only.
- A decomposition that passes steps 1 to 3 is not claimed to compose to the
  core; the composed check claims it on the core's declared domain only.
- Steps 1 to 3 of a decomposition are the door's own evaluation of the given
  tables; no component Authority or checker confirms them.


## Requirements-only version-space questions

`zeno-fcis loop question --specification FILE` takes finite input/output
bounds, a sketch, universal laws over the inputs and `outN`, and complete
examples. It reads no existing core. The schema is
`zeno-fcis/loop-version-space/1` (see the CLI reference).

A meaning is the complete input-major output table. Duplicate expressions
and assignments count once. Eligible completions must be total under eager
signed-i128 evaluation and stay within every output bound. Finite constant
ranges and choice holes are exhaustive; a Boolean `?c` ranges over 0 and 1.
An unbounded integer `?c` or `?e` is inconclusive. Multiplication, parameters,
`keep`, implicit core names and mixed kinds are refused.

The Rust door proposes the exact number of meanings satisfying the
requirements. With several meanings, it asks at the input minimizing the
largest answer bucket, then the sum of squared bucket sizes, then input
ordinal. Every answer is a complete output tuple with its survivor count.
These counts, partitions and minimum conflicts are **proposals**, not
acceptance decisions. A minimum conflict is relative to this sketch and its
bounds, with lexical requirement-ID ties; an empty grammar has an empty
conflict.

With one meaning, the CLI lowers its representative and checks each law
and example on every input using the existing library law evaluator.
Candidate-bound Boolean obligations include output totality and bounds and
pass the existing `transform check` against true; each receipt replays.
Only then is the status `determined` and the candidate `checked`. The report
includes each obligation and reference in canonical hex and its receipt,
so a separate `transform replay` can inspect the same evidence. This grants
no adoption or publication authority.
Measured law calls, steps and reads are retained on success and on a later
refusal or budget stop, separately from the door's conservative work charges.

```sh
python3 integrations/zal/terminal.py loop init --specification specification.json --workspace W.json
python3 integrations/zal/terminal.py loop show --workspace W.json --packet
python3 integrations/zal/terminal.py loop answer QUESTION 1 --workspace W.json
python3 integrations/zal/terminal.py loop answer QUESTION '[0,1]' --workspace W.json
python3 integrations/zal/terminal.py loop stop --workspace W.json
```

The workspace copies the initial declaration into its owned state and
recomputes analysis before an answer. The existing owner terminal channel
alone can confirm a complete output tuple or stop; a model can request
`versions ID;` and submit `version-suggest QUESTION: [outputs];`. Suggestions
are labelled advisory and do not change requirements. Stale IDs and
out-of-domain tuples refuse. An in-domain answer outside the offered buckets
becomes an example, and may exclude this grammar. Owner stop preserves every
confirmed requirement. Existing MCP tools support read/submit/translate;
there is no MCP answer or stop tool. Channel conventions are cooperative,
not authenticated identity.

Both ZAL surfaces contain typed requests, count proposals, full-tuple
questions with buckets, suggested answers, owner answers, checked
singletons, relative conflicts and stop statements. For example:

```text
version-question q1 at [0]: [0] -> 2 | [1] -> 2;
Version question q1 at [0]: answer [0] leaves 2 meanings or answer [1] leaves 2 meanings.
```

All phases share limits for work, input tuples, retained meanings, cumulative
table cells and conflict subsets. Defaults are 2,000,000 work units, 4,096
inputs, 4,096 meanings, 1,000,000 cells and 10,000 subsets; hard caps also
apply. A stop in filtering, partitioning, conflict minimization or final
checking is `inconclusive` (exit 2), with count/question/candidate null and
no attempt spent. A representation or final-check refusal exposes no exact
result either. The contained CLI deadlines remain additional limits.

Not claimed by this mode:

- A unique meaning establishes neither the owner's full intent nor behavior
  outside the sketch and declared finite input/output domains.
- No unrestricted synthesis, global conversation optimum, logarithmic query
  bound, model quality or runtime improvement is claimed.
- Counts and relative conflicts are not checker-certified cardinality proofs.
- Law-frame checking is lower-level evidence; it is not an adopted contract,
  state-transition authority, deployment or release qualification.

<!-- BEGIN black-box capture guide: additive block -->

## Black-box capture

The owner provides executable reference source and a finite input/output ABI.
`loop capture open` freezes a complete successful reference table before any
examples are exposed. A proposer receives the public ABI and judge-selected
examples, submits a complete finite program, and receives more examples when
the existing checker refutes it. `captured-exact` requires `transform check`
against a program encoding the whole frozen table, followed immediately by
byte-for-byte receipt replay. `examples-equivalent` is an intermediate filter
over selected examples and grants no acceptance.

The initial reference format is `python-function-v1`: Python 3.11+ source
defining `reference(point)`, receiving an integer tuple and returning a list
of exact integers. Boolean results use integer 0/1; Python Booleans, floats,
wrong arities and out-of-domain results are refused. All state, request and
authorization inputs must be explicit. Example:

```python
def reference(point):
    return [int(point[0] < 3)]
```

Its spec uses the existing finite-program domain vocabulary:

```json
{"inputs":[{"kind":"Int","min":"0","max":"7"}],"outputs":[{"kind":"Bool"}]}
```

```sh
zeno-fcis loop capture open --spec spec.json --reference reference.py --session capture
zeno-fcis loop capture packet --session capture
zeno-fcis loop capture candidate --session capture --candidate-json proposal.json
zeno-fcis loop capture status --session capture
zeno-fcis loop capture status --session capture --full
```

`open` requires a new directory. It observes each point six times: twice in
each of three fresh workers, ordered forward, reverse and forward. A crash,
malformed result, timeout or observed disagreement refuses the entire reference
and publishes no session. Formats other than `python-function-v1` are refused.
The finite observations cannot prove arbitrary code deterministic. Exactness
is against the **complete frozen observed table**; equivalence to future
executions assumes the reference is a pure deterministic function of every
declared input.

Reference acquisition runs inside the existing `checker_process` transient
user unit, retaining its 64 MiB group memory cap, zero swap, 64-task cap,
caller CPU/priority restrictions and whole-invocation/stage deadlines. Before
compiling owner code, the interpreter adapter installs no-new-privileges,
non-dumpability, Landlock ABI >=3 and a seccomp allowlist. Only a precreated
fresh `scratch.txt` is writable. Outside file content, network, process
creation, exec and cross-process access are denied. Each worker additionally
has a two-second CPU cap, 48 MiB address-space cap, 32 descriptors, a 64 KiB
scratch-file cap and a three-second wall deadline. Kernel/Python/libseccomp
integrity is trusted. Linux x86-64 and the contained runtime are required;
macOS and missing facilities return typed refusals, with no fallback.

Default bounds are 4,096 tuples/examples, 32 decided candidate attempts and
2,000,000 reserved node visits per candidate invocation, including full
comparison and replay. The full check reserves the fixed 256-node target
capacity, rather than disclosing the hidden table encoding size. Files are at
most 64 KiB; worker messages and private
session data at most 2 MiB. The table encoding must fit the existing
profile-1 checker's 256 nodes; a larger required encoding is inconclusive,
even when the table has fewer than 4,096 rows. Canonical limits and the ABI
are preserved. Invocation and lifetime reservations bound retries as well as
completed attempts. A budget stop before a checker decision returns `inconclusive`,
exit 2, without a candidate attempt; prepaid work remains recorded. Once the
checker refutes a candidate, that decision is durably charged before optional
feedback work. Reference-local CPU/wall/output/memory limits are reference
failures: they refuse with exit 1. Outer stage/invocation exhaustion before a
decision is inconclusive with exit 2. An interrupted supervisor reports unknown
attempt accounting; the private session retains any recorded decision.
Acquisition reports scheduled observations and complete valid replies separately;
internal worker work and a failed schedule's partial progress are unknown. These
reservations are bounds, not measurements of actual CPU or evaluator work.

Every candidate is pre-checked against every selected example. A repeat that
fails one is `precheck-refuted`, with no full comparison. Full checker
refutations permit the door to propose a failing input farthest in Manhattan
distance from the last added example, with the first ordinal breaking ties.
It adds that example before covering both reachable outcomes of every
candidate `Select` condition. Unreachable outcomes are explicit; eager traps
before a condition produce no outcome. These choices and coverage reports
are proposals, not extra judges, path coverage or MC/DC. After exact refutation,
example or coverage exhaustion is advisory feedback
(`feedback` or `coverage.status`), while the result remains `refuted`, exit 1,
and spends the attempt. A captured session refuses new proposals;
`status` rechecks and replays its stored candidate. It summarizes private
metadata by default; `--full` requests the table/evidence only if the response
fits the contained runtime output cap. Events retain newly selected ordinals
and coverage counts, rather than repeated packets. Interrupted invocations can
leave scratch directories as well as a session lock; inspect ownership before
recovery.

### Capture through ZAL and MCP

The owner provisions a separate workspace:

```sh
python3 integrations/zal/terminal.py capture init --workspace capture.json \
  --spec spec.json --reference reference.py --session capture
python3 integrations/zal/terminal.py capture show --workspace capture.json
```

Set `ZAL_CAPTURE_WORKSPACE` for the MCP server. `zal_capture_read`,
`zal_capture_packet`, `zal_capture_submit`, `zal_capture_translate` and
`zal_capture_legend` use `zal/fcis-loop/1`. There is no provisioning, private
status, answer or say tool. Existing owner confirmation gates are unchanged.
The packet contains exactly `profile`, `abi`, `round` and `examples`, with
no source/path/hash, full table, original program, receipt or private events.
The workspace supplies revision-bound headers in both surfaces. Input names
are `x0`, `x1`, ...; the model supplies one complete expression per output,
with no `keep`, output references, parameters or holes:

```text
capture p1 out0 = x0 < 3;
examples-equivalent p1: 0 examples;
capture-example p1: input (0), output (1);
captured-exact p1: 8 tuples, replay matched;
```

```text
Capture p1 computes out0 as x0 is less than 3.
Examples equivalent p1: agrees with all 0 selected examples.
Capture example p1: input (0), output (1).
Captured exact p1: equal on all 8 declared inputs, with receipt replay matched.
```

Only model capture proposals are routed from MCP. Model-authored owner answers
and judge capture claims are refused. The workspace stores public transcripts
under the existing advisory lock; the Rust session uses an exclusive invocation
lock. An interruption can leave `capture.lock`; recover it only after checking
that no operation owns it. Session files are local preparation/evidence data.

### Capture: what is not claimed

- `open` does not prove determinism or equivalence to future executions, and
  supports no native binaries, installable dependencies or arbitrary formats.
- `packet` and the MCP endpoints provide an example-only API. They do not
  authenticate clients or sandbox a separately launched coding agent with host
  filesystem access. Keep the owner reference and private session outside that
  agent's accessible files; do not send an ordinary contract/family packet.
- `candidate` never treats fitting examples, coverage, an exhausted round budget
  or a reference crash as exact capture. No guarantee of convergence, minimum
  program size, model quality, unbounded behavior or undeclared I/O/state follows.
- `status` and saved receipts grant no adoption, publication, deployment or
  release authority. Source/table hashes detect accidental drift under a trusted
  host; the local session is not an authenticated or tamper-proof ledger.

<!-- END black-box capture guide -->

<!-- BEGIN composable program loops -->
## Composable program loops

These stateless commands compose program-search stages. The original program
and its full input/output domains stay fixed. They are distinct from the
architecture command `loop decompose --compose`. All results carry
`authority: none`; none replaces a session incumbent or adopts a contract.

### Pipeline

`loop pipeline --original O.zcve --packet` reports the original's SHA-256,
the compiled checker/operator source identity and the fixed grammar. Copy
those bindings into a `zeno-fcis/loop-pipeline/1` proposal, containing either
`program` (the existing finite program JSON) or `expressions` (an existing
`zeno-fcis/loop-expressions/1` document with vocabulary). Run:

```sh
zeno-fcis loop pipeline --original O.zcve --proposal P.json --out new-result
```

The proposal's `stages` array defaults to `complete`, `repair`, `canonical`.
At most five stages are allowed. Each stage has a fixed guard:

| Stage | Required input | Proposal |
|---|---|---|
| `complete` | Expressions with holes | Completion from the existing sketch solver |
| `repair` | Hole-free expressions refuted by the checker | Repair from the existing repair door |
| `canonical` | A checker-qualified program | Existing hash-consing and dead-node pruning |
| `resubstitute` | A checker-qualified program | Two bounded resubstitution rounds from program search |

Each emitted program independently crosses `transform check` against the
same original, with its receipt replayed. `qualified` requires equivalent
and matched replay; eager arithmetic and output-domain failures are compared
as well as values. A found repair is checked even if a bounded synthesis solve
returned no plan; that advisory stop matters only when no checked repair results.
A typed proposer refusal exits 1; an exhausted budget exits 2.
A wrong optimization can be rejected while an earlier
qualified program is retained. A later budget stop makes the whole run
`inconclusive` even when earlier qualification evidence remains in the report.

Pipeline not claimed:

- Completion or repair success from a proposer is not qualification.
- A missing completion is not an impossibility result.
- A target with only failing original observations is not qualified by this
  discovery command. Existing expression completion/repair requires an
  original that succeeds everywhere; program proposals can preserve mixed
  successful and failing observations.
- No minimum-program, owner-intent, session acceptance or deployment claim.

### Strategy discovery

`loop strategy --tasks T.json [--depth 1] --out new-result` consumes a
`zeno-fcis/loop-strategy/1` manifest with nonempty `development` and
`evaluation` arrays. Each task contains a relative `original` file and a
bound pipeline `proposal`. Duplicate original content, including a renamed
development original in evaluation, is refused. Runtime depth is restricted
to one, with exactly one development task and one evaluation task. Additional
tasks and depths two or three are refused before search: the supervisor's
62 control frames cover the entire invocation, so those grids cannot complete.

The admitted grammar has five sequences: gate-only, complete, repair, canonical
and resubstitute. Rank development outcomes by qualified-task count,
then final nodes (an unqualified task costs twice its original node count),
then reserved checker evaluation pairs, sequence length and grammar order.
The selected stages, development bindings, limits and implementation identity
are frozen and hashed before evaluation. Evaluation never changes the winner.

All five trials and evaluation share the invocation deadline, checker allowance
and control-frame budget. Even within the admitted two-task shape, expensive
stages or reduced limits can stop as inconclusive.
A development budget stop emits no selection. An evaluation budget stop
retains the frozen selection and reports incomplete evaluation.

Strategy not claimed:

- Reserved pairs include a complete check and receipt replay allowance;
  they are not executed CPU time or total synthesis work.
- Selection is an empirical result for these development tasks and this
  fixed grammar, not a globally optimal strategy.
- Evaluation tasks are held out from selection only; no unseen-family or
  generalization guarantee is claimed.
- The grammar cannot supply a checker, shell command, callback or owner answer.

### Concrete rewrite discovery

`loop rules --original O.zcve [--terms T.json | --bank B.json] --out new-result`
extracts live terms from the program and optionally includes supplied terms,
or reuses a supplied `zeno-fcis/loop-rule-bank/1` bank. A term corpus uses
`zeno-fcis/loop-terms/1`: its `terms` are either finite program JSON with a
single full-range typed output, or `{kind, expression}` objects with the
corpus's `vocabulary` and optional `parameters`.

The proposer groups exact complete value tables by output kind and excludes
any term that fails. It proposes decreasing concrete replacements. The bank's
classes, costs and metadata carry no authority. Every rule is rebound and
checked/replayed on the target's actual input domains; only freshly qualified
rules are considered. Every changed whole program is checked/replayed against
the fixed target original. This catches removal of dead eager traps and losses
of shared subexpressions that a term comparison alone would miss.

Rules not claimed:

- These are concrete terms, not universally quantified arithmetic laws or
  patterns with metavariables.
- A source-domain receipt or bank cannot authorize target-domain reuse.
- A cheaper term does not ensure a smaller whole program; actual candidate
  cost is tested before its independent whole-program check.
- No minimal rule basis, minimum program or equality-saturation claim.
- `qualified` means a replayed equivalence result. An unchanged result has
  `improved: false` and `result_kind: unchanged-original`; it does not claim a
  learned improvement, and ZAL records a fact instead of an accepted rewrite.
- Rules can preserve an original that fails on every input. That receipt covers
  exact failure semantics only, without a successful value-observation claim.

### Budgets, exports and ZAL

Documents and program files are bounded to 64 KiB. Defaults cap tuples at
131,072, retained proposer cells at 4,194,304, metered search work at
50,000,000, sketch assignments at 2,000, terms at 128, checker calls at 24
and reserved check/replay pairs at 1,000,000. Search work caps the existing
meters per stage: sketch enumeration, optimizer work, concrete mining, and
repair's top-down solves. It does not count local repair edits or all CPU work.
Repair retains charged synthesis work. A found repair goes to check/replay;
a bounded solve without a plan is inconclusive only when no checked repair
results. Mining also retains charged work on a stop.
Local repair and canonicalization are bounded by admitted inputs and the
stage deadline. A whole invocation has 22
seconds inside the existing 25-second containment; stages retain the existing
5-second search and 2-second check limits and 62-frame allowance. Flags may
lower these bounds. An exhausted bound returns exit 2, never a successful
emptiness or impossibility claim, and spends zero session attempts.

An output directory must be new. It receives `result.json`, optionally
`bank.json` or `selection.json`. Only an overall `qualified` pipeline or rule
result exports `original.zcve` and `candidate.zcve`. Failed runs can retain
earlier evidence, but do not export a candidate. Source identities describe
embedded compiled source under a stable-build assumption, not authenticated
binary or malicious-host attestation.

The existing `zal_loop_submit` accepts these additional acts in
`zal/fcis-loop/1`; both surfaces parse to the same typed values:

| Symbolic | Controlled English |
|---|---|
| `pipeline p out0 = keep;` | `Pipeline p proposes out0 as keep.` |
| `stages p: complete, repair, canonical;` | `Stages of p: complete, then repair, then canonical.` |
| `stages p: gate-only;` | `Stages of p: gate only.` |
| `strategy s out0 = keep;` | `Strategy s proposes out0 as keep.` |
| `explore s: depth 1;` | `Explore s to depth 1.` |
| `mine r out1 = value + value - value;` | `Mine r proposes out1 as value plus value minus value.` |

Multiple output acts with the same ID form one pipeline or strategy proposal.
Multiple `mine` acts collect concrete terms. The shell writes documents and
calls Rust; it computes no verdict, term table or ranking. Pipeline and mining
use the workspace's bound original. Strategy acts require a family workspace
with exactly two distinct instances: one development and one evaluation task.
Entries marked `fit: false` are evaluation-only;
with none marked, the final listed instance is reserved before trials begin.
The workspace records results and leaves its incumbent and requirements
unchanged. There is still no answer or say MCP tool. Permission to submit a
proposal is not owner agreement.
<!-- END composable program loops -->


<!-- BEGIN typed strategy modes (one existing strategy surface) -->
## Typed strategy modes

`loop pipeline` and every development/evaluation trial of `loop strategy`
now declare a plan before search. One closed registry supplies the operation
names and artifact signatures. Sequence, tagged alternatives and bounded
feedback are type-checked before execution, then the executed macro-step trace
must be a prefix of that declared plan. A refused or stopped run may end early.
Stopped cores retain an owned invoked-step prefix and charged work, without
partial result tables. Invoked steps do not imply a successful operation.
These checks cover structure and possible artifact flow. Runtime guards and
existing semantic checkers still decide whether a program qualifies. A plan
whose goal is reachable does not mean that a run reached it.

The same `loop strategy` command also supports three named modes. `--tasks`
and `--problem` select mutually exclusive modes; there is no second strategy
command or arbitrary executable primitive registry:

```sh
zeno-fcis loop strategy --kind split-by-cases --problem P.json --original O.zcve
zeno-fcis loop strategy --kind diagnose --problem D.json
zeno-fcis loop strategy --kind ask-when-it-matters --problem R.json
```

Every problem has schema `zeno-fcis/loop-strategy-problem/1`. Unknown fields,
operations and direct owner-answer fields are refused. Problem, compiled
implementation and declared plan hashes bind each completed report. A model
may suggest finite readings or program terms using the existing fixed IR; it
cannot define primitives, supply a checker or confirm a reading.

### Split by cases

`learn` is a narrow `zeno-fcis/learn-terms/1` corpus: `original_sha256`,
`status`, `integers` and `predicates`. The library producer
`learn::strategy_terms_v1` returns the complete declared bounded term space as
owned finite programs; callers serialize these programs for the strategy CLI.
`loop learn` reports example-learning results, rather than this corpus.
Integer terms use a single full
signed-64-bit output; predicates use a single Boolean output. Both share the
exact reference input domains. Serialized meaning tables are not accepted.
The consumer re-evaluates every term with the library evaluator. A producer
status of `inconclusive` propagates before decoding any partial lists.
`complete` describes the declared producer search, not all possible programs.

The proposer greedily covers queried examples with small terms, learns a case
tree, and considers alternative predicates and leaf terms. It asks the fixed
reference where the committee most evenly disagrees. When the committee
agrees, spot checks first visit untested predicate minority sides inside each
current leaf region, then remaining input ordinals. Initial sampling and ties
are deterministic. Every emitted assembly crosses the existing full-domain
`transform check` and matched receipt replay against the same immutable
original. Queried ordinals record training/validation membership, not physical
library evaluation calls; the fixed reference table is computed in preparation.
This extends the program export rule to qualified split results;
sample fit or region coverage alone never exports a candidate.

Split not claimed:

- The finite corpus is not a complete language or minimum-program proof.
- An uncovered sample or missing case tree is a proposer refusal, not
  unrealizability. There is no automatic retry or external oracle service.
- Agreement, queries and region coverage are proposals; a sampled-fit assembly
  can still be refuted by the full-domain checker.
- No black-box behavioral capture, owner intent, optimal sample efficiency,
  universal rule or performance advantage is claimed.

### Diagnose

A problem supplies a `circuit` with Boolean `inputs`, operand-first `gates`
(`kind`, prior `wires`) and measured `outputs`; a primary-input `vector`; and
`observations` as `[wire, value]` pairs. Fixed gate kinds are `and`, `or`,
`xor`, `nand`, `nor` and `not`. The proposer enumerates inclusion-minimal
weak-fault explanations: each faulty gate may produce either Boolean value,
and every permitted wire vector is retained. The healthy empty explanation is
included when it fits. It proposes an unmeasured internal wire minimizing the
largest prediction bucket, then the sum of squared bucket sizes, then ordinal.
An explanation allowing both readings contributes to both buckets.

Diagnose not claimed:

- These are conditional explanations of supplied measurements, not
  authenticated sensors or actual fault identification.
- Masked faults, inconsistent measurements, multiple explanations and an empty
  explanation set remain possible; a singleton does not establish the real fault.
- No arbitrary circuit language, repair or accepted program is produced.

### Ask when it matters

A `background` Boolean program bounds worlds. Each `readings` item has a unique
`id`, optional display `text`, and finite Boolean-program `alternatives`.
`query` is `count`, `must` or `possible`; for the last two, the final item is
also the query reading. Equivalent masks under background collapse to the
first spelling. The proposer computes answers for every surviving reading
combination, then asks only when readings change an answer and a statement
has differing answer sets. It chooses the strongest such dependence and the
first separating world. Without such dependence it may report an ambiguous
proposal. `must` is false when the premises have no models.

Reading not claimed:

- Answer stability is conditional on the supplied finite readings and
  background; it is not completeness of interpretations or owner intent.
- A missing intended reading is not discovered by the strategy.
- Counts, Boolean answers, questions and diagnoses do not qualify programs,
  confirm requirements or grant adoption authority.
- No model call, natural-language interpretation judge or owner-answer CLI
  flag is provided.

Only the existing owner confirmation channel may answer a reading question.
Through the terminal owner gate, `answer Q: any;` means the displayed statement
holds at the displayed world; `answer Q: none;` means it does not. The English
surface uses the corresponding existing answer forms. A fresh problem,
question and implementation binding must match before the owned reading list
is filtered. Confirmed evidence persists if subsequent computation stops,
while the answer remains inconclusive. Each fresh command invocation has its
own checked trace. Cross-turn owner evidence is recorded separately. Model
answers are refused and there is no MCP answer tool.

### Bounds and paired acts

The existing file, stage, invocation and checker limits apply. Plans have depth
at most 16, feedback bounds 1–64, at most 2,048 type states, traces at most 512
steps and at most 100,000 structural work units per check. Split configuration
is optional: `initial` (default 8, 1–64), `depth` (2, 0–3), `committee`
(16, 1–16), `alternatives` (6, 1–8), `leaf_alternatives` (8, 0–8),
`spot_checks` (8, 1–64) and `queries` (64, 0–64). Additional explaining leaves
are bounded separately from the base committee. Diagnoses admit 1–8 inputs,
at most 16 gates and `max_faults` 0–2 (default 2). Readings admit at most 16
items, 128 total alternatives by default, and at most 4,096 combinations,
further limited by `--max-terms` squared. Tuple/world, scalar scratch-cell and
metered search bounds are enforced before retaining tables; reservations are
conservative, not CPU measurements. A stop returns inconclusive, exit 2,
zero attempts, no partial count/diagnosis/answer, and no candidate export.
Question and completed advisory-proposal results exit 0 with `authority: none`.

Both `zal/fcis-loop/1` surfaces use the same mode and canonical JSON problem:

| Symbolic | Controlled English |
|---|---|
| `strategy-run s diagnose: JSON;` | `Strategy s runs diagnose using JSON.` |
| `strategy-run s split-by-cases: JSON;` | `Strategy s runs split-by-cases using JSON.` |
| `strategy-run s ask-when-it-matters: JSON;` | `Strategy s runs ask-when-it-matters using JSON.` |

Replace `JSON` with the complete problem object. The shell transports problems
and records Rust reports; it computes no meaning table, ranking or diagnosis.
Only a replayed full-domain split program receives an existing accepted reply.
Other modes produce conditional facts/questions and leave the incumbent and
confirmed contract requirements unchanged. Prefix hooks claim only these
strategy/program acts so other loop families can add their own hooks.
<!-- END typed strategy modes -->

<!-- BEGIN bounded public-schema strategy knowledge -->
## Strategy knowledge engine

The native `kb advise`, `kb plan`, `kb replan`, `kb explain`, `kb check` and
`kb query` commands share a bounded stratified Datalog engine and the fixed
public strategy catalog. [Knowledge semantics and configuration](../KNOWLEDGE.md)
describes structured scopes, evidence recipes, exclusions, layers, provenance,
limits and automatic consultation for configured local loops. Exact program
checkers and receipt replay still decide acceptance.

Both `zal/fcis-loop/1` surfaces operate on the compiled public example:

| Symbolic | Controlled English |
|---|---|
| `strategy-kb k: JSON;` | `StrategyKB k uses JSON.` |

`JSON` is `{ "operation": "check" }`,
`{ "operation": "query", "query": "strategy(S)" }`, or
`{ "operation": "plan", "have": ["model"], "want": ["answer"] }`.
Optional `max_steps`, `max_states` and `max_work` lower the bounds.
These acts transport requests and record Rust reports as facts. They cannot
select external knowledge paths, accept programs or answer owner questions.
Ambient private KB configuration is suppressed for these public-example acts.
MCP also disables automatic KB use on ordinary loop commands; explicit
`kb_advise`, `kb_plan` and `kb_explain` are read-only private query surfaces.

User knowledge, recipe results and inbox proposals stay outside the publishing
tree. The packaging include manifest pins the single neutral example. The
filename/content guards refuse other `.kb` files; general secret scans do not
prove absence of all confidential information.
<!-- END bounded public-schema strategy knowledge -->

## Learning with a declared vocabulary

`zeno-fcis loop learn --spec FILE` proposes expressions from examples,
without reading an existing core or reference executable. The owner declares
the finite domains, constants, checked operators and standard macros before
the search. The door builds typed expressions bottom-up by size and retains
the smallest vocabulary-cost representative of each raw scalar meaning.
Output-domain failures are identified only at the root, so expressions that
compute different integers outside an output's range can still participate
in later arithmetic. Eager arithmetic failures remain distinct tagged
observations; neither failure tag consumes an integer value.

The door's counts, uniqueness, conflicts and question ranking are proposals
relative to that vocabulary, the size bound and the finite domain. After
lowering, the existing library evaluator checks the proposed program against
**every provided or confirmed example**. This example check decides exactly
what was reproduced. Its result does not establish that the proposed
function matches a hidden target on unanswered inputs.

Supply `--original PROGRAM.zcve` to finish automatically: the fitting candidate
enters complete native-reference capture, then the existing exact checker and
receipt replay against the original. Example fit remains a separate result;
only both matched checks give `qualified`. `--out NEW_DIRECTORY` retains the
original, capture table program and qualified candidate for independent replay.
The capture profile requires successful reference outputs throughout its
declared domain and at most 4,096 tuples. Limits and the invocation deadline
remain in force through capture and checking. Split-by-cases uses the same
capture and checker path after assembly.

An exhausted search names its resource or bounded vocabulary in `search_limit`
and requests a better-shaped lowerable primitive or sketch. The request never
changes the examples, domains, laws or checker limits. Opaque primitives remain
unsupported in the native vocabulary.

| Reply | Meaning |
|---|---|
| `determined` | One scalar meaning per output fits. The final program reproduced every example; uniqueness remains a vocabulary proposal |
| `question` | Several fit. Ask the unconfirmed input with the smallest largest answer bucket, then sum of squared buckets, then input ordinal |
| `outside-vocabulary` | None fits. A minimum-cardinality subset of examples excludes this bounded vocabulary; a proposal, not a checker verdict |
| `ambiguous` | Several remain and no unconfirmed input distinguishes them; no completed learning claim |
| `inconclusive` | Enumeration, filtering, conflict minimization, lowering or checking stopped on a budget; no count, core, question or final program follows from a prefix |

Counts and buckets are per output. A question's joint partition is their
product; it is not a count of distinct whole-program failure observations.
Vocabulary size counts each input/constant once and each operation or
standard macro once plus its arguments. It is not emitted instruction count.
The only macros are the fixed `min`, `max`, `abs`; the only arithmetic is
checked `+` and `-`. There is no act for a model to add a primitive or change
the declared vocabulary. All examples are mandatory: nonzero `misses` is
refused, even when an owner believes a label is noisy.

```sh
python3 integrations/zal/terminal.py loop init --learn learn.json --workspace W.json
python3 integrations/zal/terminal.py loop show --workspace W.json --packet
python3 integrations/zal/terminal.py loop answer q1 '[2]' --workspace W.json
```

The workspace copies the initial declaration and binds its revision, so
editing the original file changes nothing. Models submit `learn next;` or
`learn-suggest q1: [2];`; suggestions never enter the examples. Only the
owner confirms a complete output tuple through the existing terminal answer
gate. An in-domain answer outside every proposed bucket is retained, and
can make the next result outside-vocabulary. A later budget stop preserves
the answer. Reanalysis uses fresh question IDs and retires stale questions.
There is no MCP answer tool, and no model can submit a learn judge reply in
any workspace.

Both `zal/fcis-loop/1` surfaces carry the same typed acts:

| Symbolic | Controlled English |
|---|---|
| `learn next;` | `Learn next.` |
| `learn-suggest q1: [2];` | `Learn suggestion q1: [2] (advisory).` |
| `learn-answer q1: [2];` | `Learn answer q1: [2].` |
| `learn-question q1: [1] remaining [3];` | `Learn question q1: when [1], [3] meanings remain (proposal).` |
| `learn-outside next: [0,2];` | `Learn outside next: examples [0,2] exclude this bounded vocabulary (proposal).` |
| `learn-ambiguous next: [3];` | `Learn ambiguous next: [3] meanings remain (proposal).` |
| `learn-inconclusive next;` | `Learn inconclusive next: no completed enumeration.` |

`learn-determined ID: SHA on N examples;` renders as `Learn determined ID:
program SHA reproduces N examples; uniqueness is a vocabulary proposal.`
The exact example-check detail and candidate program are attached to that
judge event. Zero examples are explicitly reported as a vacuous check.
Boolean tuple values use wire integers 0 and 1; a JSON Boolean, string or
float in a tuple is refused.

The command is stateless and spends no session attempt. Its default caps are
4,096 domain tuples, 4,096 raw meanings per kind, 2,000,000 work units,
1,000,000 cumulative table cells and 10,000 conflict subsets; caller flags
can reduce them. Search and final checking use the existing contained stages
and their deadlines. A cut-short minimum conflict is inconclusive rather
than an irreducible subset mislabeled as a smallest one. See
[the CLI declaration format](../CLI_REFERENCE.md#learn-from-examples-with-a-fixed-vocabulary).

### What learn does not claim

- No unique hidden function or equivalence to a black box follows from a
  vocabulary singleton or agreement with the examples.
- No smallest IR program, unbounded optimum or checker-certified vocabulary
  minimality is claimed; size and uniqueness are enumeration proposals.
- No automatic oracle, opaque/model-supplied primitive, multiplication,
  vocabulary expansion or noise-based acceptance is provided.
- No full-domain totality is required of the proposal; eager failures on
  unanswered inputs remain part of its scalar meaning.
- No source exploratory numbers, model-quality gain, adoption, publication
  or application authority is claimed. Every result carries `authority: none`.
