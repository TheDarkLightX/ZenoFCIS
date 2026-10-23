# Withdrawal queue

This application holds a pooled vault of at most four units. OwnerA and OwnerB
reserve one or two units in their respective lanes. A keeper normally pays one
due lane per tick. Each owner can also withdraw its reservation at day two,
without an operator or keeper. The amount, lane and settlement destination of
that payout are independently checked.

## State and commands

The state records the balance, each lane's status and reserved amount, pause
count, must-serve flag, priority lane. Empty lanes have zero reserved amount. Occupied lanes hold one
or two units; day two is the fixed withdrawal deadline. Genesis is an empty vault with lane A first.

- Operator may Deposit one or two units, within the four-unit capacity.
- Each lane's owner may RequestWithdrawal on its empty lane if the pooled
  unreserved balance covers the amount. All reservations share the fixed deadline at day two of the finite 0..2
  clock. A request made on day two can be withdrawn immediately; unreserved
  funds therefore still have an owner-driven path out at the final tick.
- Keeper may Tick. During a pause it counts down without paying. An alarm outside
  a pause starts a two-tick pause unless must-serve is set. Otherwise it pays the
  priority lane when both are due, or the only due lane. Payment empties that lane
  and gives the other lane priority. Unpaid arrivals become pending.
- Each lane's owner may TimeoutWithdrawal at or after day two.
  It pays exactly that reservation despite an alarm or pause, leaves the other
  reservation intact and cannot pay a second time. The command's amount is ignored.

The first failed guard determines rejection: wrong caller (200), occupied request
lane (201), insufficient unreserved balance (202), excess deposit (203), empty timeout lane (205), early timeout (206). A rejection
changes no state and emits no delivery. This application has no committed failures.

Context carries caller, alarm and day. Generated app servers supply the day from
the declared clock, in days since 2026-11-01; context files omit the clock field.
The original typed `invoke` helper uses day zero for historical demonstrations;
`invoke_at` accepts the host-authenticated day. The browser demo can simulate it.

## Laws and evidence

`project.zeno` retains laws 500..503. Laws 504..506 require the eligible tick's
payout, exact owner recovery, payout capacity and the fixed deadline. Laws 510..512 bind each
debit to one payout with the same amount, the debited lane and destination
`settlement`; deposits and reservations emit none. Law 513 forbids arbitrary
rejection and checks the first failed guard. Claims 600 and 601 retain their
solvency and pause predicates and include the new timeout transition law.

Law 514 also counts requests from the action: an accepted owner timeout, or an
unpaused tick that is allowed to serve a due lane, requires exactly one request
on channel 300. Its payload is the paid lane and that lane's reserved amount,
and its destination is `settlement`, as required by laws 510..512. Every other
accepted action emits none; rejections and committed failures retain their
independent framework checks. The count check does not rely solely on a positive
balance debit. All four delivery laws inspect the candidate's actual outbox;
the complete delivery count is bounded by one.

The checked policy is generated into `src/v2_contract.rs`; `build.rs` separately
lowers the schema, and `profile.rs` binds every law. Existing synthesized controller
artifacts describe the unchanged keeper step. Their finite controller result
assumes continuing keeper ticks; it does not establish host availability or
qualify the newly added timeout action by itself. The extended deadline path has
separate native scenarios and counterpolicy controls.

The 26 shipped examples are lifted to day zero. Their previous state projection, decisions, reasons and deliveries are
unchanged. `tests/decision-scenarios.txt` adds boundary, wrong-owner, repeated
recovery and other-lane preservation cases. The regression checks in
`tools/test_template_rules.py` use the actual generated Authority to refuse
stalled ticks, omitted payouts and a changed payee lane. Earlier controller,
claim and exhaustive conformance tests remain in the private verification oracle.

```sh
zeno-fcis generate contract crates/zeno-fcis-cli/templates/withdrawal-queue --check
zeno-fcis contract scenario crates/zeno-fcis-cli/templates/withdrawal-queue \
  --file crates/zeno-fcis-cli/templates/withdrawal-queue/tests/decision-scenarios.txt
```

## Policy for the owner

The existing choices remain: a deposit ignores its lane, a keeper tick ignores
the command's lane and amount, and a timeout ignores the command's amount.
Priority selects one of two due lanes. There is no deposit-credit ledger and no
automatic payment without an available owner, keeper or scheduler. These choices
are unchanged by the delivery obligations.

## Scope and background

The vault is pooled; it has no per-owner deposit-credit ledger. A payout is a
durable request to a settlement receiver, not proof of external settlement.
The receiver must deduplicate delivery IDs. Caller authentication and a host that
accepts requests through the deadline remain assumptions. A path to withdrawal
is available; an idle owner is not automatically paid without a keeper or scheduler.

Guarded transitions follow E. W. Dijkstra, “Guarded commands, nondeterminacy and
formal derivation of programs”, CACM 18(8), 1975, DOI 10.1145/360933.360975.
This revision adds independent payout constraints and a conventional owner timeout on a fixed three-day calendar;
prior-art search pending because the repair was offline. It makes no novelty claim.

The server clock holds its final admitted day after the finite horizon. Late
recovery remains available in that terminal phase; no operator clock reset is
needed. This does not automatically submit a refund or release.
The typed `invoke_at` helper uses the same terminal phase, records the bounded
logical day, and continues to refuse negative times.

## Review scope

The default review uses boundary probes for this template. Those probes can miss
wrong-amount payouts on interior inputs. Review the complete declared domain with
`zeno-fcis contract review crates/zeno-fcis-cli/templates/withdrawal-queue --max-tuples 2000000 --format json`.
That run covers 1,866,240 inputs. State whether counterpolicy evidence used the
full-domain review or only the default boundary probes; neither establishes
behavior outside the declared domain.
