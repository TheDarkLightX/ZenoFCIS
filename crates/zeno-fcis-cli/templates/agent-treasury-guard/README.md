# Generated agent treasury guard

This local application guards a treasury that an AI agent trades for. The
agent, an LLM or another model, proposes swaps. Its proposal is only a
command: the decision here is the guard, and it decides what the treasury
commits. Accepted swaps leave through the outbox as requests shaped after
ZenoDEX's `SwapIntent`, and ZenoDEX's settlements and failures come back as
commands. It shows six patterns:
- an untrusted proposer: whatever the agent proposes, the treasury never
  commits more than its daily budget, never drops below its reserve, never
  swaps outside the slippage bound against the oracle price, and never has
  more than one swap outstanding;
- inductive claims for `zeno-fcis prove`: law 500 states the reserve, the
  non-negative base balance, the daily budget, and the pending swap's
  bookkeeping as one invariant, and CVC5 attests that the laws claim 600
  assumes preserve it across every transition they admit, for every integer.
  Neither the slippage bound nor the limit of one outstanding swap is part
  of that invariant: law 507 checks the slippage bound on each accepted
  proposal, and under law 503 a proposal is accepted only when no swap is
  outstanding;
- a synthesized decision: a small hand-written adapter computes the guard's
  facts with checked arithmetic, and a core that `zeno-fcis synth` selected
  and checked on every fact tuple decides which rule applies first;
- the model's identity as an input: the context names the model the agent
  runs, and only an approved model may propose;
- time and the oracle price as inputs: every request carries `now`, the
  price, and the price's own time in its context, and the decision reads no
  clock and no feed;
- numbered external requests, as in the order-fulfillment template: each
  request carries the tick it was proposed at, and an answer about an earlier
  swap is rejected as stale.

`project.zeno` owns record fields, command variants, reason order, channel
types, relational law formulas, and the claims that `zeno-fcis prove` checks.
`build.rs` supplies explicit scalar bounds and catalog meanings, then checks
and generates the schema and project bindings. `profile.rs` binds the exact
source, including the synthesis files, and the runtime-only law manifest.
The adapter in `src/program.rs` and the law checker in `src/laws.rs` are
written by hand; the law checker evaluates the formulas against every
decision the adapter makes, and refuses any decision that breaks them.

## The rules

The domain is scaled so that the tests can cross every rule input. A day is
4 ticks, and time runs from tick 0 through tick 11: three days. Balances are
bounded at 20. A deployment would use seconds, a day of 86,400 of them, and
its own units; the rules would not change.

The treasury has seven fields:

| Field | ID | Meaning | Range |
| --- | --- | --- | --- |
| `quote` | 130 | the quote asset it holds | 0 through 20 |
| `base` | 131 | the base asset it holds | 0 through 20 |
| `spent_today` | 132 | quote value committed on the day of `last_seen` | 0 through 4 |
| `last_seen` | 133 | the tick of the last committed decision | 0 through 11 |
| `pending` | 134 | 170 `NoSwap`, 171 `PendingBuy`, or 172 `PendingSell` | |
| `pending_amount` | 135 | the amount the outstanding swap sells; 0 with `NoSwap` | 0 through 3 |
| `pending_min_out` | 136 | the least the outstanding swap must deliver; 0 with `NoSwap` | 0 through 3 |

It starts with 6 quote, 1 base, nothing spent, at tick 0, with no swap. The
constants are a daily budget of 4 quote, a per-trade cap of 3 quote, a reserve
of 2 quote, and a slippage bound of one quarter: a swap must deliver at least
three quarters of its value at the oracle price.

Every command has an `action` (field 140) and carries `direction` (141: 173
`BuyBase`, 174 `SellBase`), `amount` (142, from 1 through 3), `min_out` (143,
from 0 through 3), `intent` (144, a tick), and `amount_out` (145, from 0
through 3). An action ignores the fields it does not use; the constructors in
`src/lib.rs` set them to their smallest admitted values. The context carries
the `caller` (150: 178 `Agent` or 179 `Dex`), `now` (151, a tick), the oracle
`price` (152, 1 or 2 quote per base), `price_time` (153, the tick the price
was observed at), and the `model` the agent runs (154: 180 `TreasuryAgentV2`,
181 `TreasuryAgentV1`, or 182 `UnlistedModel`). Only `TreasuryAgentV2` is
approved.

| Action | ID | Sent by | Effect when accepted |
| --- | --- | --- | --- |
| `ProposeSwap` | 175 | `Agent` | debits `amount` from the sold asset, adds the swap's value to `spent_today`, records the swap as pending, and queues one request |
| `SwapSettled` | 176 | `Dex` | credits `amount_out` to the bought asset and clears the swap |
| `SwapFailed` | 177 | `Dex` | commits failure `swap_failed`: refunds the held `pending_amount` to the sold asset and clears the swap; the budget is not restored |

A swap's *value* is its `amount` for a buy (the amount of quote it sells) and
its `amount` times the oracle price for a sell (the amount of base it sells,
valued in quote). The *least minimum* a swap may carry keeps three quarters of
the value the oracle promises for the bought asset, rounded up so that
rounding never favors the agent: `div_ceil(amount * 3, 4 * price)` base for a
buy, and `div_ceil(amount * price * 3, 4)` quote for a sell.

The rules apply in this order, and the first that applies decides:

1. An action from any other caller is rejected with `wrong_caller` (200).
2. A command whose `now` is not later than `last_seen` is rejected with
   `clock_not_advanced` (201). Every commit takes its own tick, so an intent's
   number, the tick it was proposed at, is unique.
3. A `ProposeSwap` under a model other than `TreasuryAgentV2` is rejected with
   `unapproved_model` (202).
4. A `ProposeSwap` whose `price_time` is later than `now` or more than 1 tick
   before it is rejected with `stale_price` (203).
5. A `ProposeSwap` while a swap is pending is rejected with `swap_outstanding`
   (204).
6. A `SwapSettled` or `SwapFailed` while no swap is pending is rejected with
   `no_swap_outstanding` (205).
7. A `SwapSettled` or `SwapFailed` whose `intent` is not `last_seen`, the tick
   of the outstanding proposal, is rejected with `stale_callback` (206). A
   late answer about an earlier swap cannot settle the current one.
8. A `SwapSettled` whose `amount_out` is less than `pending_min_out` is
   rejected with `short_settlement` (207).
9. A `ProposeSwap` whose value exceeds 3 is rejected with `over_trade_cap`
   (208).
10. A `ProposeSwap` whose value, added to the day's `spent_today`, exceeds 4
    is rejected with `over_daily_budget` (209). If `now` falls on a later day
    than `last_seen`, the day's `spent_today` is 0.
11. A `ProposeSwap` whose `min_out` is below the least minimum is rejected
    with `slippage_too_wide` (210).
12. A buy that would leave less than 2 quote, or a sell of more base than the
    treasury holds, is rejected with `below_reserve` (211).
13. Otherwise the command is accepted, or commits failure `swap_failed`
    (212), as the table says. `last_seen` becomes `now`. If `now` falls on a
    later day than the previous `last_seen`, `spent_today` restarts from 0,
    whatever the command; an accepted proposal then adds its value.

A rejection changes nothing and queues nothing. Requests go to `zenodex` on
channel 300. A request carries `intent_number` (160, the proposal's tick),
`asset_in` and `asset_out` (161 and 162: 183 `Quote` or 184 `Base`),
`amount_in` (163), `min_amount_out` (164), and `deadline` (165, the proposal's
tick plus 2).

## The adapter and the synthesized core

`src/program.rs` reads the treasury and computes eleven facts with checked
arithmetic, each a yes or no: the caller is the action's sender, the clock
advanced, the model is approved, the price is fresh, a swap is pending, the
answer names the outstanding intent, the settlement delivers the minimum, the
value is within the cap, the value fits the day's budget, the minimum keeps
three quarters of the oracle value, and the sold asset stays above its floor.
The core in `synthesized/transition.rs` takes the action and those facts and
returns one code: 0 to accept, 1 through 12 for the rule of that rank, or 13
to commit the failure. The adapter then applies the effects the table above
states, again with checked arithmetic.

`synthesis.json` has two parts:
- a contract, which states rules 1 through 13 as a relation over the action,
  the facts, and the code;
- a sketch: the same chain of checks with three holes, one for each pair of
  checks whose order decides only which reason a proposal that breaks both
  is refused with: model before price, cap before budget, and slippage
  before reserve.

`zeno-fcis synth run` evaluated the 8 hole assignments on all 6,144 tuples of
an action and eleven facts and selected the first that satisfies the contract
on every one: the README's order for all three pairs. The selected program,
its complete input and output vectors, and the emitted Rust are checked in
under `synthesized/`. That selection is exhaustive verification against the
contract, for the program as the library's interpreter runs it; `synth
verify` replays the emitted Rust, Python, and JavaScript on every tuple. It
is only as right as the contract: `tools/check_synthesis.py` in the library
compares the vectors with a separate restatement of the rules, and
`tests/conformance.rs` compares the emitted Rust with another one.

## The request to ZenoDEX

The payload is shaped after the `SwapIntent` of
[ZenoDEX](https://github.com/TheDarkLightX/ZenoDEX): an exact-in swap names
`asset_in`, `asset_out`, `amount_in`, `min_amount_out`, and a `deadline`.
Those names are mirrored here. It is not ZenoDEX's wire encoding, and
`intent_number` is this guard's number, not ZenoDEX's `intent_id`, which is a
hash. The adapter that delivers a request derives the intent ID, the pool, the
sender key, and the signature, submits the intent, and reports the outcome
back as a `SwapSettled` or `SwapFailed` command naming `intent_number`. A
request that ZenoDEX never fills expires at its deadline, which the adapter
reports as a failure.

The guard decides what leaves the treasury, not what the market returns. A
settlement credits exactly what ZenoDEX reports, and the guard checks only
that it is at least `min_amount_out`.

## From an agent's proposal to a command

The agent never calls this decision. It produces a proposal, for example as
structured output from a model:

```json
{"direction": "SellBase", "amount": 1, "min_out": 2}
```

The shell turns it into `propose(SellBase, 1, 2)` and supplies the context
itself: the caller it authenticated, its own clock, the oracle price and the
price's time from its own feed, and the model identity from its own record of
which model produced the proposal. The agent cannot choose any of those. The
demonstration in `src/lib.rs` scripts the agent: a fixed list of proposals,
good and bad, with the outcome each must have. It calls no model and uses no
network.

## What is checked

Every decision is checked at run time, before it can be published:
- law 500: the quote balance keeps the reserve, the base balance never goes
  negative, `spent_today` stays between 0 and the budget, and the swap
  amounts are held only while a swap is pending: at least 1 unit, with a
  minimum that is never negative;
- law 501: every commit carries a later tick than the last and records it;
- law 502: `spent_today` counts the day's committed value: it restarts on a
  new day, a proposal adds its value, and an answer adds nothing;
- law 503: an accepted proposal came from the agent under the approved model
  with a fresh price and no swap outstanding, and leaves one outstanding; an
  accepted settlement came from the DEX, names the outstanding intent, and
  leaves none; a failure is never accepted;
- law 504: a settlement delivers at least the outstanding minimum and credits
  exactly `amount_out`;
- law 505: a proposal sells at least 1 unit, debits exactly its amount from
  the asset it sells, and leaves at least the reserve of 2 quote or a
  non-negative base;
- law 506: the swap the treasury records is the swap the agent proposed;
- law 507: the swap the treasury records is valued at a positive oracle
  price, commits at most the cap, fits the day's budget, and its minimum
  keeps three quarters of the oracle value, rounded up;
- law 508: a failure reported for the outstanding intent refunds exactly the
  held amount and clears the swap;
- in Rust, because formulas cannot see them: the rejection reason, the exact
  request (the queued intent equals the command, with its number, assets,
  amount, minimum, and deadline), the absence of effects, and the genesis.

A rejection publishes nothing by construction, and the law framework records
law 509 as satisfied for every rejection. That law has no formula, so
`profile.rs` registers it instead of `project.zeno` carrying an always-true
placeholder. As a result, this passes:

```sh
zeno-fcis check project.zeno --require-substantive --require-resolved-paths
```

Law 500's invariant is also claimed by induction, for `zeno-fcis prove`:

```text
claim 600 treasury_stays_within_limits all inductive assume [501, 502] accept [503, 504, 505, 506, 507] failure [508] = pre.100.130 >= 2 && pre.100.131 >= 0 && pre.100.132 >= 0 && pre.100.132 <= 4 && (pre.100.134 == 170 -> pre.100.135 == 0 && pre.100.136 == 0) && (pre.100.134 != 170 -> pre.100.135 >= 1 && pre.100.136 >= 0);
claim 601 budget_never_exceeded all inductive assume [501, 502] accept [505, 507] failure [508] = pre.100.132 >= 0 && pre.100.132 <= 4;
```

Claim 600 states law 500 over the treasury before a decision, and assumes
the clock and day-accounting laws on every commit, the proposal and
settlement laws on accepts, and the refund law on committed failures, which
is where the law manifest enforces them. Each law declares that scope in
`project.zeno`, so elaboration also checks the claims' groups against the
declared scopes, and `authority()` checks the manifest against them before
it builds the authority. Claim 601 states the budget's part
on its own, from five of those laws. For each, CVC5 answers `unsat`: no
transition that satisfies the assumed laws, with the variant fields over
their declared values, starts within the invariant and ends outside it, for
every integer value of every other field. That answer is attested, not
independently checked, and it says nothing about this application until
`tests/claims.rs` checks four things:
- the law checker's own observer, `laws::state_observations`, reads every
  field each invariant reads, and each invariant has its stated value on
  every treasury the schema admits;
- each invariant holds on the exact genesis treasury;
- the manifest enforces laws 501 and 502 on every commit, 503 to 507 on
  accepts, and 508 on committed failures, as the claims assume;
- the manifest enforces each law exactly on the decisions `project.zeno`
  declares for it, and a manifest that bound law 501 to accepts only, or to
  the genesis, is reported.

Stating claim 600 found six transitions that the draft's formulas admitted
and these rules forbid: a buy of 0, which left a pending buy holding 0; a
buy of 1 from 2 quote, into the reserve; a buy that took `spent_today` from
4 to 5; a sell of 1 base from 0; a sell at a price of −1, which took
`spent_today` to −1; and a settlement of −1 base from a pending buy whose
held minimum was −1, a state no commit reaches. Each was a rule the Rust
part of the law checker enforced but no formula stated. Law 505 now states
the positive amount and the two floors, law 507 the budget and the positive
price, and law 500 the non-negative held minimum; `tests/laws.rs` shows each
of the six refused by the formula that now states its rule.

The tests check the running application. Every decision below runs through
admission, the authority, the adapter and the core, the law checker, the
committed patch, and the outbox, and is compared with the expected outcome:
- `tests/conformance.rs` decides 25,094 inputs against a reference model of
  these rules, and 30 examples from `tests/decision-examples.txt`. On each of
  the 4,596 decisions among them that commit, it also evaluates the
  invariants of claims 600 and 601 before and after, as the law checker
  observes the transition:
  - every proposal, 9,600 of them, of which 2,352 commit: every idle
    treasury that keeps the reserve, with quote from 2 through 6, base from
    0 through 3, and every budget position, on the same day and on the
    next, at both prices, in both directions, with every amount and every
    minimum;
  - every callback, 7,760, of which 1,344 commit: every pending swap in both
    directions with every held amount and minimum, at two balances of each
    asset and both budget extremes, plus two idle treasuries, with
    settlements of every amount and failures, naming intents before, at, and
    after the outstanding one, on the same day and on the next;
  - every caller, model, and price at every clock boundary, 2,484, of which
    376 commit: three treasuries, one command of each action, both callers,
    all three models, both prices, ticks before, at, and after the last
    commit, on the next day, and at the last tick, with prices two ticks
    old, one tick old, current, and from the future;
  - a search of everything one day can reach, 5,104 decisions, of which 524
    commit: from genesis, every proposal and every settlement and failure
    naming a tick of the day, at every remaining tick and both prices,
    reaches 230 states. Along every path through them, the accepted
    proposals commit at most the budget, whether or not a swap failed in
    between, and the budget is reached, so the bound is tight. This is the
    multi-step claim: no sequence of proposals within one day commits more
    than the budget;
  - the fields an action ignores, 126: proposals with every intent and
    settlement amount, and callbacks with every direction, amount, and
    minimum, decide the same as with the smallest values;
  - the least minimum, 20: for every swap within the cap, a proposal at the
    least minimum is accepted and records it, and one below it is rejected
    as `slippage_too_wide`; and for every direction, amount, and price, the
    adapter's least minimum is the `div_ceil` above, keeps three quarters of
    the oracle value, and one less does not.

  It also compares the emitted core with a Rust restatement of rules 1
  through 13 on all 6,144 fact tuples, checks the schema bounds, and checks
  that only the reviewed treasury is a valid genesis. The reference model,
  explored on its own over all three days through every command from both
  callers, reaches 13,893 states with balances of at most 17 quote and 13
  base, below the bound of 20: no settlement or refund the model can reach
  would fail to stage.
- `tests/claims.rs` connects the claims to the application: claim 600
  restates law 500 exactly; the manifest enforces each assumed law where
  the claims assume it, refuses a law assumed outside its scope, and
  enforces each law exactly on the decisions `project.zeno` declares; the law
  checker's observer reads every field each invariant reads, and gives each
  invariant its stated value on all 105,840 treasuries the schema admits at
  one tick, which the invariants do not read; and both invariants hold on
  the exact genesis of a new database.
- `tests/laws.rs` gives the law checker decisions a faulty program could
  make, such as a buy that debits one quote too few, a sell valued at its
  amount instead of its price, a settlement that credits the wrong asset, a
  failure that restores the budget, a request with the wrong deadline, or a
  rejection with the wrong reason, and requires it to refuse each one. For
  each of laws 500 through 508 it also gives a decision that breaks that
  law, and requires exactly the laws that decision breaks, and no other, to
  report the violation. The six counterexamples above are each refused by
  the formula that now states their rule.
- `tests/lifecycle.rs` runs the demonstration, a pending swap across a
  database reopen, a late settlement of an earlier swap, a failure that keeps
  the budget committed, and a repeated settlement.
- `tests/determinism.rs` decides 864 inputs (six treasuries, twelve commands,
  both callers, three ticks, and two price-and-model pairs) eight times each
  through `execute_probed`, then again in three child processes with a
  changed environment, and requires every decision digest to match.

The expected outcomes come from a reference model in the test, written from
these rules by the program's author. It catches binding and adapter errors,
not a misreading shared by both. The examples file is the check on that: its
header records who wrote the examples and whether the project's owner has
reviewed them.

The static check covers what the probe runs might not exercise, such as a
clock or a price feed read from inside the decision:

```sh
zeno-fcis purity src/program.rs src/laws.rs synthesized/transition.rs
```

All of these are detectors: agreement shows that these runs matched and that
no error-level rule fired, not that the application is correct for every
possible deployment.

## How ZenoFCIS built and checked this

Every step below ran on 2026-09-24 from a fresh copy of this template's
directory with the CLI built from the ZenoFCIS checkout
(`cargo +1.97.1 build -p zeno-fcis-cli --locked`; `zeno-fcis --version`
prints `zeno-fcis 1.1.0`), Rust 1.97.1, Python 3.12.3, and Node.js v22.23.1.
Outputs are quoted as observed. The gate,
`python3 tools/check_generated_application.py`, repeats the `check`,
`synth`, `purity`, and `cargo` steps on a fresh copy; the `prove` steps need
the pinned solvers, which the gate does not require.

1. `zeno-fcis check project.zeno --require-substantive --require-resolved-paths`
   checked the project: 9 laws and 2 claims, every formula able to constrain
   a transition, every path declared.

   ```text
   $ zeno-fcis check project.zeno --require-substantive --require-resolved-paths
   checked project.zeno: project=1 components=1 claims=2 unresolved_obligations=2 semantic_program_hash=20c1c5476833ac25dd033b13ec8b8359a96fc622edce9cec201ff5184b3042d9
   ```

2. `cargo +1.97.1 build` ran `build.rs`, which lowered the schema with the
   bounds above and generated the typed bindings in `OUT_DIR`. Nothing in
   `src/` restates what generation provides.
3. `zeno-fcis synth run synthesis.json --out synthesized` selected the core:
   `status: selected`, 6,144 inputs covered, 8 assignments evaluated. The
   first assignment, which checks the price before the model, was refuted at
   the first tuple whose model is unapproved and whose price is stale
   (`first_counterexample`). `--check` recomputes the artifact set without
   writing it and reports `current`.
4. `zeno-fcis synth verify synthesis.json --out synthesized --receipt
   rust-conformance.json` compiled the emitted Rust and replayed all 6,144
   inputs: `passed`. `synth run` and `synth verify` with `--target python`
   and `--target javascript` did the same for `transition.py` under Python 3
   and `transition.mjs` under Node 22, with the same certificate and the same
   output digest as Rust.
5. `zeno-fcis purity src/program.rs src/laws.rs synthesized/transition.rs`
   checked the decision code statically.

   ```text
   $ zeno-fcis purity src/program.rs src/laws.rs synthesized/transition.rs
   purity: clean (0 errors, 0 warnings) in 3 files
   ```

6. The solvers are not part of the default gate. They need a tools manifest
   naming the installed binaries by path, version, and SHA-256, and Lean's
   distribution root and tree hash; the library's `docs/FORMAL_TOOLS_RC3.md`
   gives its schema. The manifest is local to the machine and is not part of
   the template. `zeno-fcis doctor --tools zeno-fcis.tools.json` admitted
   the pinned tools:

   ```text
   $ zeno-fcis doctor --tools zeno-fcis.tools.json
   cvc5 1.3.3 e8d7870d57ab55e81619d2373b043da05ea1d37ca393931bdb5d8b9788cd64c4
   z3 4.16.0 e583c4186a45e72411fa2cb2048401eed03f0f8e5f24694676a8f6271a50b765
   lean 4.30.0 3e0d0d3d801675359f2d4cf9815bfdb417b20b92fdd9d48b3b14c95bbae28bbf
   ```

7. `prove` keeps every run's exact input and output under
   `.zeno-fcis/evidence/` next to the project, so these runs were made on a
   copy of this directory. CVC5 answered `unsat` for both induction steps.
   ZenoFCIS classifies that as a proposal and exits with code 2, because the
   proof output is not checked independently: each step is attested, not
   kernel-checked. The scope line says what else the application must
   check, which `tests/claims.rs` does.

   ```text
   $ zeno-fcis prove project.zeno --claim 600 --backend cvc5 --tools zeno-fcis.tools.json
   cvc5 claim 600: UNSAT proposal retained; proof output was not independently checked
   cvc5 claim 600 assumes laws [501, 502] on every commit, [503, 504, 505, 506, 507] on accepts, [508] on committed failures
   cvc5 claim 600 scope: every transition that satisfies the assumed laws preserves the invariant; it holds on every committed state only when the application also observes every value the invariant reads, checks the invariant on its exact genesis state, and enforces each assumed law on the decisions the claim assumes it on
   (exit 2)
   $ zeno-fcis prove project.zeno --claim 601 --backend cvc5 --tools zeno-fcis.tools.json
   cvc5 claim 601: UNSAT proposal retained; proof output was not independently checked
   cvc5 claim 601 assumes laws [501, 502] on every commit, [505, 507] on accepts, [508] on committed failures
   cvc5 claim 601 scope: every transition that satisfies the assumed laws preserves the invariant; it holds on every committed state only when the application also observes every value the invariant reads, checks the invariant on its exact genesis state, and enforces each assumed law on the decisions the claim assumes it on
   (exit 2)
   ```

   Before laws 500, 505, and 507 were completed, the same command on claim
   600 printed `cvc5 claim 600: replayed counterexample retained`, with the
   scope line `a transition that satisfies the assumed laws breaks the
   invariant; its starting state may be unreachable, so strengthen the
   invariant or assume more laws` and exit code 1, once for each of the six
   transitions listed under [What is checked](#what-is-checked); each is
   retained as `counterexample.json`. Claim 601 was refuted twice more on
   the way to its groups: without law 501, a commit at an earlier tick than
   the last moved the day backwards, where law 502 says nothing; and without
   law 508 in its `failure` group, a committed failure could carry a
   proposal, since only the laws in a claim's groups constrain that kind.

8. Z3 also answered `unsat` for both steps. RC3 has no Z3 proof checker, so
   each run is recorded as blocked evidence, with exit code 2. There is no
   Lean export for inductive claims yet, so `--backend lean` selects
   nothing, and Lean was not run.

   ```text
   $ zeno-fcis prove project.zeno --claim 600 --backend z3 --tools zeno-fcis.tools.json
   z3 claim 600 blocked: UnsupportedEvidence
   z3 claim 600 assumes laws [501, 502] on every commit, [503, 504, 505, 506, 507] on accepts, [508] on committed failures
   (exit 2)
   $ zeno-fcis prove project.zeno --claim 601 --backend z3 --tools zeno-fcis.tools.json
   z3 claim 601 blocked: UnsupportedEvidence
   z3 claim 601 assumes laws [501, 502] on every commit, [505, 507] on accepts, [508] on committed failures
   (exit 2)
   $ zeno-fcis prove project.zeno --claim 600 --backend lean --tools zeno-fcis.tools.json
   claim 600 does not select compatible lean
   no compatible claim/backend pair was selected
   (exit 2)
   ```

9. `zeno-fcis graph project.zeno --format mermaid` prints one node,
   `c400[treasury]`: the project has a single component, so no diagram is
   included here.

What the claims say, and what they do not. Claim 600 says: take any
treasury that satisfies law 500's invariant, any command and context whose
variant fields hold declared values, and any decision that satisfies laws
501 and 502, together with 503 to 507 if it accepts or 508 if it commits a
failure; then the treasury after the decision satisfies the invariant too.
That holds for every integer value of every other field, with no range
assumed. Claim 601 says the same for the budget's part, from laws 501, 502,
505, 507, and 508 alone. Together with the checks in `tests/claims.rs` and
the authority's refusal of any decision that breaks an enforced law, every
state this application commits keeps the reserve, a non-negative base, a
day's committed value between 0 and the budget, and the swap bookkeeping.
The multi-step statement, that no sequence of proposals within one day
commits more than the budget, follows from claim 601 with law 502, under
which `spent_today` is the sum of the day's committed values and a failure
restores nothing; `tests/conformance.rs` also checks it by search over the
first day.

They do not say:
- anything about an input the authority did not check: the context,
  including the caller, `now`, the oracle price and its time, and the model
  identity, is a trusted input, and a deployment must supply it itself;
- anything about the market: the guard bounds what the treasury commits
  against the oracle price, not the loss of a swap ZenoDEX executes, and a
  settlement credits exactly what the DEX reports;
- that the laws are the rules: a law that under-describes the program gives
  counterexamples, as six did, and one that over-describes it refuses
  decisions at run time. `tests/conformance.rs` is the check that the laws
  and the program agree, on every admitted input of its grids;
- that the steps are proved: CVC5's `unsat` is attested; `prove` exits 2,
  and the proof text is not checked. Z3's `unsat` is recorded as
  unsupported evidence, and there is no Lean export for inductive claims;
- anything about integer ranges: the step assumes none. The invariants
  carry what they need, and the balance bound of 20 comes from the schema
  and from `the_reference_model_stays_within_the_balance_bound`.

Evaluation is strict: an overflow anywhere in a formula leaves it without a
value, and the law checker does not report such a law satisfied, so the
step asserts each assumed law as defined and true, and would report a
transition where the invariant itself has no value as
`replayed counterexample retained; the claim has no value there (reason)`.
None arose: the invariants compare fields and compute nothing.

## Run this development candidate

This template depends on the current ZenoFCIS checkout. From that checkout,
run:

```sh
python3 tools/check_generated_application.py
```

That gate creates a fresh application using the CLI, checks its synthesis in
Rust, Python, and JavaScript, patches its dependencies to the exact checkout,
checks dependency versions against the workspace lock, then compiles and runs
its tests and demonstration as an isolated package. With these development
dependencies available in a standalone checkout:

```sh
cargo +1.97.1 test --locked
cargo +1.97.1 run --locked -- new-treasury.sqlite
```

The SQLite shell is the `sqlite` feature, on by default. Without it,
`cargo +1.97.1 build --no-default-features` builds the core alone: the
generated bindings, the program, the law checker, the profile, the delivery
adapter, and `authority()`, with no database; the gate checks that it also
compiles for `wasm32-unknown-unknown`. `create`, `invoke`, `journey`, and the demonstration
binary need the feature.

The demonstration requires a new database path. Its scripted agent proposes
a good buy, then, while and after it settles, a proposal with a swap
outstanding, a settlement from the wrong caller, a short settlement, a repeated
settlement, a swap over the cap, one on a stale price, one over the budget,
one with too wide a slippage, and one from an unlisted model. A sell is
accepted, a late answer about the first swap is refused, and the sell fails
and is refunded. The next day shows the reserve, a failure that does not
restore the budget, and the budget restarting; the last day shows a clock
that has not advanced and a sell that settles. It then interrupts request
delivery, reopens the database, and finishes delivery. It prints a JSON
summary whose `story` array tells each step in words with its outcome.
`[profile.dev]` in `Cargo.toml` sets `opt-level = 1` so that the tests run in
seconds; Cargo keeps overflow checks and debug assertions on in that
profile, so nothing that is checked changes.

The destination keeps an in-memory idempotency ledger, which survives the
database reopen only within one process. A real destination must persist its
delivery IDs and entry hashes. The context, including the caller, `now`, the
oracle price and its time, and the model identity, and the principal are
trusted tutorial inputs, not remote authentication: a deployment must
authenticate the caller, read its own clock and its own price feed, and bind
the model identity from its own records before admission. The oracle price is
trusted as given: the guard bounds what the treasury commits against that
price, not the market loss of a swap ZenoDEX executes. Source hashes identify
reviewed example policy, not certified binaries or release evidence. Changing
the source, including the synthesis files, changes policy identity and
requires a new database unless a separately reviewed migration is implemented.
