# Withdrawal temporal pilot

The withdrawal-queue application promises that an alarm can delay a withdrawal
but never freeze it. This pilot binds two `zal/ladder-temporal/1` statements of
that promise to the application's generated contract and checks each at its
own evidence level. Like every ladder, it has authority none: it grants no
admission, release, publication or deployment permission.

This controller pilot uses deposits, requests and ticks with `now = 0`.
It excludes the template's `TimeoutWithdrawal` action. Its results therefore
cover that explicit projection, rather than every action of the current template.

## The journey

An owner asks to withdraw 2 units on lane A, and the other owner asks on lane
B. The keeper ticks with the alarm raised. The vault pauses for the alarm's
tick and two more, then pays lane A at the fourth tick while ignoring the
alarm (must-serve). The alarm pauses the vault again, and lane B is paid at the
eighth tick, the worst case. Two questions follow:

- bounded: once seven further keeper ticks have followed the tick that
  presents a withdrawal, has it been paid, alarms included?
- unbounded: provided the keeper keeps ticking, is every accepted request
  eventually paid, so that an alarm may delay a withdrawal but never freeze it?

## The formal model

| | Bounded statement | Unbounded statement |
| --- | --- | --- |
| Example | `examples/temporal/alarm_withdrawal_bounded.json` | `examples/temporal/alarm_withdrawal.json` |
| Events | accepted keeper ticks; requests, deposits and rejections between ticks are hidden | every decision, accepted or rejected |
| Formula | `∀sigma ∈ Traces : ∀r ∈ Requests : □(R(r) ∧ Ahead() ⇒ ◇[0,7](C(r)))` | `∀sigma ∈ Traces : A(sigma) ⇒ (∀r ∈ Requests : □(R(r) ⇒ ◇(C(r))))` |
| Mode | finite 12 | unbounded |
| Evidence | finite checked over every history of lengths 1 through 12 | kernel checked by Lean 4.30.0, under the premise A |

`Ahead()` is `○(○(○(○(○(○(○(Tick())))))))`: seven further recorded ticks.
`Requests` holds the two lanes: a lane holds one request until its payout, so
the first payout of a lane after its request is that request's own. `R(r)` is
the acceptance of the owner's request (unbounded) or the tick that presents it
(bounded). `C(r)` is the tick that queues lane r's settlement payout of the
amount its request recorded. `A(sigma)` says the keeper's tick, which the
contract accepts from every vault, is taken infinitely often. It assumes
nothing about alarms, requests or other callers.

The number k = 7 is the contract's own response bound: a presented lane is
paid on or before the eighth tick, counting the presenting tick, and within
k further events is inclusive. The horizon N = 12 is the tick projection's
reachability depth, 4, plus that eight-tick window, so a complete window
starts from every reachable controller state. It covers two complete alarm
cycles of four ticks: three paused ticks, then one must-serve tick.

### Observation adapter

`integrations/zal/withdrawal_pilot.py` reads the contract's decision graph. The
`withdrawal_queue_decisions` example of the unpublished `zeno-fcis-generated-code-tests` crate builds that graph
through the generated contract's checked Authority. It covers every vault
reachable from the genesis within the pilot domain: actions 160, 161 and 162,
lanes 170 and 171, amounts 1 and 2, callers 190 through 193, either alarm value,
and `now = 0`. This is 420 vaults and 40,320 decisions. Over every decision,
the adapter checks:

- initialization: the genesis commits, a changed genesis is refused and
  nothing is pending at genesis;
- membership: each vault is decided over the complete 96-tuple domain, and the
  graph is closed and reachable from the genesis;
- continuity: deposits and rejections leave the controller part unchanged, an
  accepted request turns only its own empty lane arrived, and every tick leaves
  no lane arrived, so the next tick presents each request exactly once;
- identity: a lane holds one request until its payout, and that payout pays the
  request's recorded amount;
- completion: the completion observation equals the contract's payouts;
- exit processing: the keeper's tick is accepted from every vault.

### Bounded claim

The tick projection's 172 boundary vaults reduce to 26 controller classes. The
reduction is checked to be a bisimulation; 26 is also the number of reachable
product states that the application's controller check reports. The
`withdrawal_temporal_pilot` example of `zeno-fcis-formal-tools` enumerates every
distinct observation word of that language, of lengths 1 through 12. It
replays each word through the existing temporal evaluator: 17,481,870 words,
all satisfied, none indeterminate. Shorter replays also compare with an
independent window reading that does not use the temporal code. With k = 6, 88
words of length 8 already fail, so the bound is tight.

### Unbounded claim

A named run assumption is not an event observation, so it cannot be lowered.
The pilot lowers the consequent through the existing route and exports it with
the repository's Lean exporter. The Lean file then adds the controller model:

- a restatement of rule 6;
- the 212 distinct controller rows of the contract's decisions, which the
  kernel checks equal to the restatement;
- the 58 reachable controller states, closed under every action;
- a table of worst-case ticks to payment, at most 8, whose progress properties
  the kernel checks;
- `withdrawal_never_freezes`: for every action sequence from the genesis in
  which the keeper's tick occurs infinitely often, the exported claim holds.

Lean 4.30.0 kernel checked the theorem, with its distribution tree matching
`release/lean-4.30.0-tree.sha256`. Its axioms are exactly `propext`,
`Classical.choice` and `Quot.sound`; it uses no `sorry`.

## Running it

From the repository root, with Lean 4.30.0 unpacked at `ROOT`:

```sh
python3 integrations/zal/check_withdrawal_pilot.py native-build
python3 -m unittest discover -s integrations/zal -p 'test_withdrawal_pilot.py'
python3 integrations/zal/check_withdrawal_pilot.py run --lean ROOT/bin/lean --lean-root ROOT
python3 integrations/zal/check_withdrawal_pilot.py planted --lean ROOT/bin/lean --lean-root ROOT
```

After an intentional contract-source change, refresh the two statements'
derived source revision with
`python3 integrations/zal/check_withdrawal_pilot.py bind-source`, review the
statement's scope, then rerun its checks. A refreshed binding is not proof.

The unit tests replay every word through length 8. `run` replays through
length 12 and prints a receipt binding the contract sources, the graph, both
scope identities, the lowering subject, the exported Lean source and the
toolchain tree. Without `--lean`, the unbounded claim stays an obligation.

| Planted defect | Named check that fails |
| --- | --- |
| Resettable alarm budget: an alarm during a pause restarts it | `test_bounded_claim_finite_checked` (105 words through length 8) and `test_unbounded_kernel_checked` (the progress lemmas are false); a fair run that never pays lane A is then kernel checked |
| Weak next: the evaluator's next is true at the end of a word | `test_strong_endpoint` |
| k one too small: within 6 | `test_bounded_claim_finite_checked` |
| An adapter that drops lane B's completions | `test_adapter_obligations` (the completion obligation) |

## What is not claimed

- The finite result covers histories of at most 12 ticks. The Lean theorem
  covers every length, but of the controller model.
- The link from the contract to that model is the adapter's exhaustive check of
  the reachable decision graph and the kernel's comparison of the extracted
  rows. No Lean theorem covers the generated contract code itself.
- `A` is a premise. Nothing here shows that the keeper keeps ticking.
- Settlement delivery, caller authentication and the alarm's truth are outside
  the contract and outside this pilot.
- The exporter writes temporal binders without grouping parentheses, so the
  exported text places the second lane's conjunct inside the first binder.
  That reading is equivalent here, because the binders range over the natural
  numbers and the captured conjunct does not mention the captured time. The
  theorem proves the proposition exactly as exported.
