# Generated withdrawal queue

This local application keeps one vault with two withdrawal lanes, A and B.
Each lane's owner asks for a withdrawal, and a keeper's tick pays at most one
lane per tick. The tick's context carries an alarm, raised by an operator or
an oracle, and an honored alarm pauses payouts. The property users care about
is that an alarm can delay a withdrawal but never trap it: whatever sequence
of requests and alarms arrives, every requested withdrawal is paid. That is a
property of every possible future, which the per-step laws of `project.zeno`
cannot state. The template shows four patterns:
- a *synthesized controller step*: `zeno-fcis synth` selected the tick's
  decision from a sketch of a fair policy by checking every hole assignment
  against the tick rules on all 384 inputs, and replays it in Rust, Python,
  and JavaScript;
- a *certified controller*: the table that step defines, with the vault's
  contract, was checked by OrbitSynthesis for every input sequence, and
  `tests/controller.rs` re-checks it with an independent implementation of
  the same decision procedure;
- a *refinement law*: the law checker requires each concrete tick to make
  the certified table's move and the contract's transition, so the finite
  model's property applies to the running application;
- reserved funds, a conservation law, and *inductive claims*: a request
  reserves its amount inside the balance, a payout moves exactly that amount
  into a payout request through the outbox, and two claims for `zeno-fcis
  prove` state that the action laws alone keep the vault solvent and the
  pause in range, for every integer. CVC5 attests each induction step, and
  `tests/claims.rs` checks the rest of the argument.

`src/program.rs` decides deposits and requests by hand-written rules and
ticks by the synthesized step in `synthesized/transition.rs`. The law checker
in `src/laws.rs` evaluates the formulas in `project.zeno` against every
decision and refuses any decision that breaks them, including a tick that
does not follow the certified table in `src/controller.rs`.

`project.zeno` owns record fields, command variants, reason order, channel
types, relational formulas, and claims. `build.rs` supplies explicit scalar
bounds and catalog meanings, then checks and generates the schema and project
bindings. `profile.rs` binds the exact source, including the synthesis
problem and its selected step, the controller's contract, its pin, the
certified strategy, and the model that wrote them, and the runtime-only law
manifest.

## The rules

The vault has eight fields:

| Field | ID | Meaning | Range |
| --- | --- | --- | --- |
| `balance` | 120 | units held, including reserved ones | 0 through 4 |
| `lane_a` | 121 | lane A: `Empty` (180), `Arrived` (181), or `Pending` (182) | |
| `amount_a` | 122 | lane A's requested amount; 0 when empty | 0 through 4 |
| `lane_b` | 123 | lane B, as for lane A | |
| `amount_b` | 124 | lane B's requested amount | 0 through 4 |
| `pause` | 125 | ticks that stay paused after the current one | 0 through 2 |
| `must_serve` | 126 | alarms are ignored until a payout | |
| `priority` | 127 | the lane paid first when both are due: `A` (170) or `B` (171) | |

The vault starts empty: no balance, both lanes empty, no pause, lane A first.
An `Arrived` lane holds a request that no tick has seen yet; a `Pending` lane
holds one a tick has presented to the controller and not paid. A lane is
*due* at a tick when it is arrived or pending.

Each command has an `action` (field 130), a `lane` (131), and an `amount`
(132, from 1 to 2). Its context carries the `caller` (140) and the `alarm`
(141).

| Action | ID | Sent by | Effect when accepted |
| --- | --- | --- | --- |
| `Deposit` | 160 | `Operator` (190) | adds `amount` to `balance`; ignores `lane` |
| `RequestWithdrawal` | 161 | the lane's owner: `OwnerA` (191) or `OwnerB` (192) | records `amount` in the lane, which becomes `Arrived` |
| `Tick` | 162 | `Keeper` (193) | presents the arrived lanes and follows the controller; ignores `lane` and `amount` |

The rules apply in this order, and the first that applies decides:

1. A command from any other caller is rejected with `wrong_caller` (200).
2. `RequestWithdrawal` on a lane that is not empty is rejected with
   `lane_occupied` (201). One request per lane at a time is how this
   application realizes the model's coalescing of arrivals.
3. `RequestWithdrawal` whose `amount` exceeds the unreserved balance, the
   balance minus both lanes' amounts, is rejected with
   `insufficient_balance` (202). A recorded request is covered from the
   moment it is recorded, so a payout never lacks funds.
4. `Deposit` that would take the balance above 4 is rejected with
   `over_capacity` (203).
5. `Deposit` and `RequestWithdrawal` are accepted as the table says.
6. `Tick` is accepted, and the vault responds as follows:
   - if `pause` is above 0, nothing is paid and `pause` decreases by one;
     when it reaches 0 with a lane due, `must_serve` becomes true. An alarm
     during a pause changes nothing;
   - otherwise, if the alarm is raised and `must_serve` is false, the alarm is
     honored: nothing is paid and `pause` becomes 2, so the alarm's own tick
     and the next two pay nothing;
   - otherwise the controller pays: the priority lane when both lanes are
     due, the due lane when one is, nothing when none is. `must_serve`
     becomes false, whether or not a lane is paid. After a payout, the paid
     lane becomes empty with amount 0, `balance` decreases by the paid
     amount, one payout request for that lane and amount is queued, and
     priority moves to the other lane.

   In every case an arrived lane that was not paid becomes pending.

A rejection changes nothing and queues nothing. Payout requests go to
`settlement` on channel 300 with the paid lane (field 150) and amount (151).
The paid amount leaves the balance in the same decision that queues the
request; the request tells the settlement rail to pay it out.

## The synthesized step and the certified controller

`controller/model.py` states rule 6 twice, from one set of Python functions:
as a synthesis problem for ZenoFCIS and as a contract for OrbitSynthesis.

`synthesis.json` is the synthesis problem. Its inputs are the controller's
memory (each lane pending or not, `pause`, `must_serve`, the priority lane)
and what a tick brings (each lane arriving or not, the alarm): 384 inputs.
Its outputs are the decision (wait, pay A, pay B) and the next memory. Its
contract is rule 6 as a relation over inputs and outputs. Its sketch is the
fair policy with five holes: whether an alarm is honored during must-serve,
which lane is paid when both are due, how long an honored alarm pauses
payouts, and where priority goes after paying each lane. Each hole lists
wrong alternatives first: deferring to alarms in must-serve, a fixed lane,
pauses of one or three ticks, priority kept by the paid lane. `zeno-fcis
synth run` evaluated 67 of the 72 assignments before finding the one that
satisfies the contract on every input, and rejected the first assignment on
the input where nothing is due and the alarm is raised. The selected step,
its complete vectors, and the emitted Rust are checked in under
`synthesized/`; `zeno-fcis synth verify` replays the emitted Rust, Python,
and JavaScript on all 384 inputs, and `tools/check_synthesis.py` compares
the vectors with a separate Python restatement of rule 6.

`controller/contract.json` is the vault's part of rule 6 in OrbitSynthesis's
contract format: 24 plant states (each lane pending or not, `pause` 0 to 2,
`must_serve`), 8 inputs (which lanes arrive, times the alarm), and 3 outputs.
Paying during a pause, paying on an honored alarm, and paying a lane that is
not due are forbidden transitions. Its two recurrence goals are "lane A is
not pending" and "lane B is not pending". `controller/strategy.json` is the
Mealy table that the synthesized step defines, read from
`synthesized/vectors.json`: 48 memory states, the plant state the controller
tracks times the priority lane. It is pinned to the contract by
`controller/contract.sha256`.

OrbitSynthesis's checker decides `G safe AND GF goal_A AND GF goal_B` for
every input sequence: the environment may choose any input at every tick,
forever, with no fairness assumption. It explores the reachable product of
plant and memory states, rejects a forbidden transition, and for each goal
rejects a reachable cycle that avoids it. On the shipped files it prints
`ACCEPT` and reports 26 reachable product states, 208 transitions, and 2
goals. On `rejected/fixed-priority.strategy.json`, which always favors lane
A, it prints a cycle in which lane A arrives and is paid on every tick while
lane B stays pending: goal 1 is starved. On
`rejected/alarm-deferential.strategy.json`, which waits on every alarm
including in must-serve, it prints a cycle in which the alarm stays raised
and lane A stays pending: goal 0 is starved. Both exit with status 1. Those
two tables are also the first two hole alternatives that the synthesis
rejected: the per-step contract already states the policy, and the
whole-run check shows that the policy is enough.

What the accepted verdict establishes, and what it does not:
- The liveness claim holds for the finite model: under every sequence of
  arrivals and alarms, each lane is paid infinitely often when it keeps
  asking, so no alarm schedule can freeze payouts. It is a checked property
  of `contract.json` and `strategy.json`, not of the running application by
  itself.
- The response bound is 8 ticks: counting the tick that first presents a
  lane, it is paid on or before the eighth tick, whatever the other lane and
  the alarm do. A recorded request is presented at the next tick, so it is
  paid within 8 ticks of being recorded. `tests/controller.rs` computes the
  bound over the product graph, and `tests/conformance.rs` checks it on the
  running application.
- The refinement law (503, with its Rust part) connects each concrete tick to
  that model: the tick's output and next memory are the certified table's,
  and the plant part of the state follows the contract's transition. With
  every tick refined, a sequence of concrete ticks is a run of the model, so
  the model's property applies to it. This is checked tick by tick at run
  time and over the reachable grid in tests; it is not a machine-checked
  proof.
- The cost of "delayed, never frozen" is a deliberate policy: when the pause
  ends with a lane due, the vault enters must-serve and ignores alarms until
  it has paid one lane. During a sustained alarm the vault therefore pays at
  most one withdrawal every four ticks: three paused ticks, then one
  must-serve tick. A single alarm delays a payout by at most three ticks.

## What is checked

Every decision is checked at run time, before it can be published:
- law 500: the balance is at most 4 and covers both lanes' amounts, an
  empty lane holds no amount and an occupied one at least one unit, and
  must-serve is set only outside a pause while a lane is pending;
- law 501: money moves only as the command says: a deposit adds its amount,
  a request reserves its amount inside the balance, and a tick removes from
  the balance exactly the amounts that leave the lanes, an amount leaving a
  lane exactly when the tick empties it, at most one lane per tick;
- law 502: each command comes from its caller and meets its rule: a deposit
  adds at least one unit within the capacity, a request reserves at least
  one unit of unreserved cover on an empty lane and records the arrival, and
  deposits and requests leave the controller's part of the state alone;
- law 503: a tick follows the vault's rules as far as a formula can state
  them: no payout during a pause, the pause counts down, an honored alarm
  pays nothing and starts the pause, every arrived lane is presented, an
  empty lane stays empty, and must-serve starts exactly when the last paused
  tick ends with a lane due;
- in Rust, because formulas cannot see them: the rejection reason, the exact
  payout request, the absence of effects, the genesis, and the refinement:
  for a tick, the contract's transition table must allow the output the
  decision made and give the plant state it left, and the certified table's
  row for the tracked memory must select that output and lead to the memory
  it left. The checker reads neither the synthesized step nor, for the
  contract check, any strategy row, so a step or a table that the contract
  forbids is refused.

Law 500 is also claimed by induction, for `zeno-fcis prove`, together with
the pause's range:

```text
claim 600 vault_stays_solvent all inductive assume [501, 502, 503] = pre.100.120 <= 4 && pre.100.120 >= pre.100.122 + pre.100.124 && (pre.100.121 == 180 -> pre.100.122 == 0) && (pre.100.121 != 180 -> pre.100.122 >= 1) && (pre.100.123 == 180 -> pre.100.124 == 0) && (pre.100.123 != 180 -> pre.100.124 >= 1) && (pre.100.126 == 1 -> pre.100.125 == 0 && (pre.100.121 == 182 || pre.100.123 == 182));
claim 601 pause_stays_in_range all inductive assume [502, 503] = pre.100.125 >= 0 && pre.100.125 <= 2;
```

The authority refuses every decision that breaks a law enforced on it, and
this application never commits a failure, because the law checker refuses
every one (law 508). So laws 501 to 503 bound every committing decision, and
each claim assumes them on every commit. An induction step asks whether any
transition the assumed laws admit can take a vault that satisfies the
invariant to one that does not. It is checked for every integer, not only
the schema's domain: the step assumes only that each enumerated field holds
one of its declared variants, as admission guarantees. Claim 600 is law
500's formula over the vault before a decision, so its step says that laws
501 to 503 alone keep every committed vault solvent and within capacity.
Claim 601 says that laws 502 and 503 keep the pause within the controller's
range, which the refinement check's plant index needs. CVC5 answers `unsat`
for both steps, which is attested, not independently checked.

Stating claim 600 changed the laws. The draft's formulas admitted four
transitions that the rules above forbid, and CVC5 found each in turn: a
deposit taking the balance from 4 to 5; a deposit of −1, an amount the
schema refuses but the step cannot see; a tick from a pause outside the
schema's range that set must-serve; and a tick that emptied a lane while
keeping its amount. The Rust part of the law checker refused all four at run
time, but a claim can assume only formulas. Laws 502, 503, and 501 now state
the deposit's capacity, positive amounts, when must-serve is set, and that
an amount leaves a lane exactly when the lane is emptied. Claim 601 was
attested on the first run.

The steps say something about this application only together with
`tests/claims.rs`, which checks that:
- claim 600 restates law 500 exactly, over the vault before a decision;
- the law checker's own observer, `laws::state_observations`, reads every
  field each invariant reads, and on each of the 13,500 vaults the schema
  admits each invariant has a definite value, the one the words above give
  it;
- each invariant holds on the exact genesis vault that a new shell stores;
- the law manifest enforces laws 501, 502, and 503 on every committing
  decision, and refuses the rejection law or an undefined law as a step's
  assumption.

Two more laws have no formula, so `profile.rs` registers them instead of
`project.zeno` carrying always-true or always-false placeholders:
- 508: the application never commits a failure, and the law checker refuses
  one outright;
- 509: a rejection publishes nothing, by construction. The law framework
  records it as satisfied for every rejection.

As a result, this passes:

```sh
zeno-fcis check project.zeno --require-substantive --require-resolved-paths
```

The tests check the running application:
- `tests/controller.rs` re-checks the shipped files: the contract's pin and
  the strategies' pins match its bytes; the Rust tables equal the JSON; the
  contract table equals a Rust restatement of rule 6 on all 576 entries and
  the strategy equals a restatement of the controller on all 384 rows; the
  synthesized step equals the certified table on all 384 inputs and refuses
  inputs outside its domain; an independent implementation of
  OrbitSynthesis's decision procedure accepts the shipped strategy with 26
  reachable product states and 208 transitions and rejects both strategies
  under `rejected/` with a counterexample cycle; a wrong pin and a forbidden
  row are rejected; and the response bound over the product graph is 8
  ticks.
- `tests/conformance.rs` explores the application from genesis under
  deposits of each amount, requests of each lane and amount, and ticks with
  and without the alarm, each from its caller: 420 reachable vaults and
  3,360 decisions, each run through admission, the authority, the program,
  the law checker, the committed patch, and the outbox, and compared with a
  reference model of these rules written in words, without the step or the
  tables. It also checks 140 decisions with wrong callers and ignored
  fields, that from every reachable vault a lane that is arrived is paid
  within 8 ticks and a pending one within 7 under every path in the grid,
  24 examples in `tests/decision-examples.txt`, the schema bounds, and that
  only the empty vault is a valid genesis.
- `tests/claims.rs` connects the induction steps to the application, as
  described above.
- `tests/laws.rs` gives the law checker decisions a faulty program could
  make, such as a deposit above the capacity, a payout during a pause, a
  payout to a lane that is not due, the wrong lane when both are due, an
  alarm honored in must-serve, a payout of the wrong amount, or a request
  that is not covered, and requires it to refuse each one; it also shows
  which law refuses each kind of faulty decision.
- `tests/lifecycle.rs` runs the demonstration, a sustained alarm with
  repeated requests, rejections that leave the vault and outbox unchanged,
  and a pause that survives a database reopen.
- `tests/determinism.rs` decides 120 inputs (five vaults, six commands, every
  caller) eight times each through `execute_probed`, then again in three
  child processes with a changed environment, and requires every decision
  digest to match.

The reference model and the examples were written from these rules by the
program's author. They catch binding, adapter, and table errors, not a
misreading shared by both. The examples file's header records who wrote the
examples and whether the project's owner has reviewed them.

```sh
zeno-fcis purity src/program.rs src/laws.rs src/controller.rs synthesized/transition.rs
```

All of these are detectors: agreement shows that these runs matched and that
no error-level rule fired, not that the application is correct for every
deployment. The controller check covers every path of the finite model; the
synthesis check covers every input of the step; the conformance grid covers
every decision reachable under its commands, with amounts of 1 and 2 and a
balance of at most 4; the induction steps cover every integer, but rest on
the solver's unchecked `unsat`.

## How ZenoFCIS built and checked this

Every step below was run with `zeno-fcis` 1.1.0 built from this checkout
with Rust 1.97.1, Python 3.12.3, Node.js 22.23.1, OrbitSynthesis at commit
`0ab3648`, and the pinned solvers CVC5 1.3.3 and Z3 4.16.0. Lean 4.30.0 is
pinned in the same tools manifest, but there is no Lean export for inductive
claims yet, so it was not run. Commands run from this application's
directory unless noted.

1. Generation. `build.rs` lowers `project.zeno` with the reviewed scalar
   bounds and generates the typed schema and project bindings into Cargo's
   output directory at every build; nothing under `src/` restates a shape.

2. The model. `python3 -B controller/model.py problem`, run inside
   `controller/`, wrote `synthesis.json`, `controller/contract.json`, its
   pin, and the two rejected strategies. It printed the contract's SHA-256
   and, for each rejected strategy, `response bound None`: a cycle that
   never pays a due lane.

3. Synthesis. `zeno-fcis synth run synthesis.json --out synthesized` selected
   the step: status `selected`, `assignments_evaluated` 67,
   `input_coverage` 384, and the first counterexample at input
   `[0,0,0,0,0,0,0,1]`: with nothing due and the alarm raised, the first
   assignment's one-tick pause answers `[0,0,0,1,0,0]` where the rules
   require a pause of two. Then, with `--check`, it reported `current`.

4. The table. `python3 -B controller/model.py tables`, inside `controller/`,
   read `synthesized/vectors.json`, checked that the step's table equals the
   fair policy on all 384 vectors, and wrote `controller/strategy.json` and
   `src/controller.rs`. It reported 26 reachable product states, 208 edges,
   and a response bound of 8 ticks.

5. Replay in every target.
   `zeno-fcis synth verify synthesis.json --out synthesized --receipt
   rust-conformance.json` reported `passed` with `inputs_checked` 384 under
   `rustc 1.97.1`; the same with `--target python --out python-step` after
   `synth run --target python`, and with `--target javascript`, reported
   the same certificate `ef31358b...` and the same output hash under
   Python 3.12.3 and Node.js v22.23.1.

6. Certification. From the OrbitSynthesis checkout, with `CONTROLLER` set to
   this application's `controller` directory:

   ```sh
   python3 scripts/check_controller.py $CONTROLLER/contract.json $CONTROLLER/strategy.json --contract-pin $CONTROLLER/contract.sha256
   ```

   printed `ACCEPT: the controller satisfies the finite contract for every
   input sequence.` and `Checked 26 reachable product states, 208
   transitions, 2 recurring-progress goals.` The same command on each file
   under `controller/rejected/` printed `REJECT [recurrence_counterexample]`
   with a prefix and a repeating cycle, naming goal 1 for the fixed priority
   and goal 0 for the deferential alarm, and exited with status 1.

7. Checks. `zeno-fcis check project.zeno --require-substantive
   --require-resolved-paths` reported `claims=2` and exit 0, and
   `zeno-fcis purity src/program.rs src/laws.rs src/controller.rs
   synthesized/transition.rs` reported `clean (0 errors, 0 warnings) in 4
   files`.

8. Proofs. The solvers are not part of the default gate. They need a tools
   manifest naming the installed binaries by path, version, and SHA-256, and
   Lean's distribution root and tree hash; the library's
   `docs/FORMAL_TOOLS_RC3.md` gives its schema. `prove` keeps every run's
   exact input and output under `.zeno-fcis/evidence/` next to the project,
   so these runs were made on a copy of this directory.

   ```text
   $ zeno-fcis doctor --tools zeno-fcis.tools.json
   cvc5 1.3.3 e8d7870d57ab55e81619d2373b043da05ea1d37ca393931bdb5d8b9788cd64c4
   z3 4.16.0 e583c4186a45e72411fa2cb2048401eed03f0f8e5f24694676a8f6271a50b765
   lean 4.30.0 3e0d0d3d801675359f2d4cf9815bfdb417b20b92fdd9d48b3b14c95bbae28bbf
   ```

   CVC5 answered `unsat` for both induction steps. ZenoFCIS classifies that
   as a proposal and exits with code 2, because the proof output is not
   checked independently: each step is attested, not kernel-checked. The
   scope line says what else the application must check.

   ```text
   $ zeno-fcis prove project.zeno --claim 600 --backend cvc5 --tools zeno-fcis.tools.json
   cvc5 claim 600: UNSAT proposal retained; proof output was not independently checked
   cvc5 claim 600 assumes laws [501, 502, 503] on every commit, [] on accepts, [] on committed failures
   cvc5 claim 600 scope: every transition that satisfies the assumed laws preserves the invariant; it holds on every committed state only when the application also observes every value the invariant reads, checks the invariant on its exact genesis state, and enforces each assumed law on the decisions the claim assumes it on
   (exit 2)
   $ zeno-fcis prove project.zeno --claim 601 --backend cvc5 --tools zeno-fcis.tools.json
   cvc5 claim 601: UNSAT proposal retained; proof output was not independently checked
   cvc5 claim 601 assumes laws [502, 503] on every commit, [] on accepts, [] on committed failures
   cvc5 claim 601 scope: every transition that satisfies the assumed laws preserves the invariant; it holds on every committed state only when the application also observes every value the invariant reads, checks the invariant on its exact genesis state, and enforces each assumed law on the decisions the claim assumes it on
   (exit 2)
   ```

   Before the laws were completed, the same command on claim 600 printed
   `cvc5 claim 600: replayed counterexample retained`, with the scope line
   `a transition that satisfies the assumed laws breaks the invariant; its
   starting state may be unreachable, so strengthen the invariant or assume
   more laws` and exit code 1, once for each of the four transitions listed
   under [What is checked](#what-is-checked); each is retained as
   `counterexample.json`.

   Z3 also answered `unsat` for both steps. RC3 has no Z3 proof checker, so
   each run is recorded as blocked evidence, with exit code 2. There is no
   Lean export for inductive claims yet, so `--backend lean` selects
   nothing.

   ```text
   $ zeno-fcis prove project.zeno --claim 600 --backend z3 --tools zeno-fcis.tools.json
   z3 claim 600 blocked: UnsupportedEvidence
   z3 claim 600 assumes laws [501, 502, 503] on every commit, [] on accepts, [] on committed failures
   (exit 2)
   $ zeno-fcis prove project.zeno --claim 601 --backend z3 --tools zeno-fcis.tools.json
   z3 claim 601 blocked: UnsupportedEvidence
   z3 claim 601 assumes laws [502, 503] on every commit, [] on accepts, [] on committed failures
   (exit 2)
   $ zeno-fcis prove project.zeno --claim 600 --backend lean --tools zeno-fcis.tools.json
   claim 600 does not select compatible lean
   no compatible claim/backend pair was selected
   (exit 2)
   ```

   The steps alone say nothing about this application. `tests/claims.rs`
   checks the base case, the observer, and the law manifest, which the
   library's guide to inductive claims requires.

9. Draw the project. `zeno-fcis graph project.zeno --format mermaid` prints
   one node, `c400[vault]`: the project has a single component, so the
   diagram adds nothing to this README.

## Run this development candidate

This template depends on the current ZenoFCIS checkout. From that checkout,
run:

```sh
python3 tools/check_generated_application.py
```

That gate creates a fresh application using the CLI, checks its synthesis in
Rust, Python, and JavaScript, patches its dependencies to the exact checkout,
checks dependency versions against the workspace lock, then compiles and
runs its tests and demonstration as an isolated package. When
`ORBIT_SYNTHESIS_ROOT` names an OrbitSynthesis checkout, it also runs that
checker on the shipped contract and strategies and requires the verdicts
above; otherwise it prints that the check was not run. The solvers are not
part of the gate; step 8 above lists their commands. With these development
dependencies available in a standalone checkout:

```sh
cargo +1.97.1 test --locked
cargo +1.97.1 run --locked -- new-vault.sqlite
```

The SQLite shell is the `sqlite` feature, on by default. Without it,
`cargo +1.97.1 build --no-default-features` builds the core alone: the
generated bindings, the program, the law checker, the profile, the delivery
adapter, and `authority()`, with no database; the gate checks that it also
compiles for `wasm32-unknown-unknown`. `create`, `invoke`, `journey`, and the demonstration
binary need the feature.

The demonstration requires a new database path. It deposits four units,
meets every rejection reason, and requests a withdrawal on each lane. The
keeper then ticks eight times with the alarm raised on most of them: the
alarm pauses payouts, the pause ends, must-serve pays lane A on the fourth
tick, a second alarm pauses again, and must-serve pays lane B on the eighth
tick, at the bound. It then interrupts payout delivery, reopens the
database, finishes delivery, and checks that the balance equals the deposits
minus the payouts. It prints a JSON summary.

The destination keeps an in-memory idempotency ledger, which survives the
database reopen only within one process. A real destination must persist its
delivery IDs and entry hashes. The context's `caller` and `alarm`, and the
principal, are trusted tutorial inputs, not remote authentication: a
deployment must authenticate each caller and take the alarm from a source it
trusts before admission. The property checked here says what the vault does
with an alarm, not whether the alarm was right. Source hashes identify
reviewed example policy, not certified binaries or release evidence. Changing
the source, including the synthesis and controller files, changes policy
identity and requires a new database unless a separately reviewed migration
is implemented.
