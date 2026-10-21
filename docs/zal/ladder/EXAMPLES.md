# Generated temporal examples

The withdrawal statements are bound to the withdrawal-queue contract; the
[withdrawal pilot](WITHDRAWAL_PILOT.md) states their evidence. The vault and
interface statements use proposed definitions awaiting owner confirmation.
The following blocks are CLI render output; their statements, scopes and
fixtures carry authority none.

## alarm withdrawal

```text
alarm_withdrawal — zal/ladder-temporal/1 (experimental; authority: none)
Law (holds over the declared domains)
Authority: DECIDED — Owner decision 2026-10-19: a bounded checked claim and an unbounded kernel-checked obligation; every definition is recovered from the withdrawal-queue contract.
Scope (theory: ordered-events-over-integers/1; ordered logical events over checked integers): Every decision of the withdrawal-queue vault, observed through the checked contract adapter.
Trace source: withdrawal-queue contract sources, sha256 8f024fc29c6e81739c816dd9c457e52839947cf1a0642fd587199f7d4fa3e6cb; schema: withdrawal-queue decision/1: pre-vault, command, context, outcome, post-vault and outbox; order: authoritative commit order of the vault decisions, including rejections; projection: retain every decision of the vault, accepted or rejected, without sorting, merging or deduplication
Mode: unbounded (natural-number-indexed observations; no final event)
Assumptions: A(sigma) := A(sigma): the keeper's tick, which the contract accepts from every reachable vault, is taken infinitely often in sigma (weak fairness of exit processing). A assumes nothing about alarms, requests, deposits or other callers, and assumes neither completion nor alarm clearance. Delivery by the settlement rail is outside the contract. [version withdrawal-queue/1]
Model: the generated withdrawal-queue V2 contract evaluated by its checked Authority over the declared command and context domain; the Lean model is its controller part (lanes, pause, must-serve, priority) under the actions tick, accepted request and other, checked row by row against the contract; environment: any command from any caller in any order: operator deposits, owner requests, keeper ticks with either alarm value, and rejected commands; the alarm is taken as given
Execution obligations: initialization the contract genesis (empty vault, both lanes empty, no pause, lane A first) commits and a changed genesis is refused; no request is pending at genesis, so a zero-operation execution holds no request; transition membership every decision is an output of the checked Authority on a vault reachable from the genesis; each Lean model row equals the contract row it abstracts; continuity each post-vault is the next pre-vault; deposits and rejections leave the controller part unchanged, and an accepted request changes only its own empty lane, to arrived
Evidence: obligation; authority: none; arithmetic: checked i128; scope identity: 1a77c2a66e5b051c31f4bbf371f618ea7b530c13174612f2dc9c95e0bca361f4
Observation A [trace; version withdrawal-queue/1]: A(sigma): the keeper's tick, which the contract accepts from every reachable vault, is taken infinitely often in sigma (weak fairness of exit processing). A assumes nothing about alarms, requests, deposits or other callers, and assumes neither completion nor alarm clearance. Delivery by the settlement rail is outside the contract.
Observation R [event; version withdrawal-queue/1]: the decision accepts the lane owner's RequestWithdrawal on lane r (rule 5, laws 501 and 502): the lane was empty and the amount is covered by the unreserved balance; a rejected request is no submission. A lane holds one request until its payout, so r names the request then occupying lane r, and the first lane-r payout after it is its own (checked).
Observation C [event; version withdrawal-queue/1]: the decision is a keeper tick that pays lane r (rule 6, law 501): it queues one settlement payout of lane r with the amount that lane's request recorded, and empties the lane.
Registered reading [placeholder/1; unbounded]: forall sigma in Traces: A(sigma) => (forall r in Requests: always(R(r) => eventually(C(r))))
Registered reading [finite-demonstration/1; finite 4]: forall sigma in Traces: A(sigma) => (forall r in Requests: always(R(r) => eventually(C(r))))
Formula: ∀sigma ∈ Traces : A(sigma) ⇒ (∀r ∈ Requests : □(R(r) ⇒ ◇(C(r))))
Theory: ordered logical events over checked integers. sigma is the declared trace, i its zero-based active position. A prime reads after this event; it is neither negation nor a derivative. Boolean algebra uses a different prime convention outside this theory. Next advances one recorded event. Empty event traces are refused. Initialization is checked separately. Trace quantifiers can be vacuous over an empty execution domain; machine initialization must establish nonemptiness.
Legend: symbol | read as | meaning
∀sigma ∈ Traces :| for every sigma in Traces,| the statement after the colon holds for each sigma in Traces; true if Traces is empty
Traces    | Traces                 | the traces
A(sigma)  | A of sigma             | sigma meets the response assumptions
⇒         | implies                | when the left side is true the right side must be; when it is false there is no demand
(…)       | open ... close         | explicit grouping; the enclosed expression is one operand
∀r ∈ Requests :| for every r in Requests,| the statement after the colon holds for each r in Requests; true if Requests is empty
Requests  | Requests               | the requests
□         | always                 | Every remaining event, including now; other traces require the visible trace quantifier.
R(r)      | R of r                 | r is a legitimate submitted withdrawal
◇         | eventually             | A witness now or later; finite and unbounded modes differ.
C(r)      | C of r                 | r has its matching assets returned
Typed: forall sigma in Traces: A(sigma) => (forall r in Requests: always(R(r) => eventually(C(r))))
Formal reading: For every sigma in Traces, A of sigma is true implies open for every r in Requests, always open R of r is true implies eventually open C of r is true close close close.
Precise English: For every trace sigma, if sigma meets the response assumptions, then (for every request r, at every remaining event, (if r is a legitimate submitted withdrawal, then at this or a later event in this trace, (r has its matching assets returned))).
Intuitive: Provided A, every legitimate submitted withdrawal eventually returns its matching assets, even when alarms occur.
Plain: Provided A, an alarm may delay a withdrawal but never freeze it.
Question: Is it true that provided A, an alarm may delay a withdrawal but never freeze it?
Counterexample search: find a trace sigma such that sigma meets the response assumptions and (there exists a request r such that not always (if r is a legitimate submitted withdrawal, then eventually (r has its matching assets returned))).
For example finite_pending: Decision word, horizon 4: a deposit, then an accepted request on lane A still unpaid when the recorded word ends. It fails the finite target; the unbounded run obligation remains with the Lean theorem. Expected: false (fixture expectation; run native parity checks).
For example alarm_exit: Decision word, horizon 4: the request on lane A is paid two decisions later. A is stipulated for this finite replay, not certified fairness. Expected: true (fixture expectation; run native parity checks).
For example wrong_request: Decision word, horizon 4: a payout of lane B does not complete the request on lane A. Expected: false (fixture expectation; run native parity checks).
For example no_request: Decision word, horizon 4: vacuous response without a request; the pilot separately checks requests during active alarms and a reject-everything control. Expected: true (fixture expectation; run native parity checks).
```

## alarm withdrawal bounded

```text
alarm_withdrawal_bounded — zal/ladder-temporal/1 (experimental; authority: none)
Law (holds over the declared domains)
Authority: DECIDED — Owner decision 2026-10-19: a bounded checked claim and an unbounded kernel-checked obligation; every definition is recovered from the withdrawal-queue contract.
Scope (theory: ordered-events-over-integers/1; ordered logical events over checked integers): Accepted keeper ticks of the withdrawal-queue vault, observed through the checked contract adapter.
Trace source: withdrawal-queue contract sources, sha256 8f024fc29c6e81739c816dd9c457e52839947cf1a0642fd587199f7d4fa3e6cb; schema: withdrawal-queue tick/1: pre-vault after the decisions since the previous tick, alarm, payout and post-vault; order: authoritative commit order of the accepted keeper ticks; projection: one event per accepted keeper tick; deposits, requests and rejections between two ticks are hidden and reach the next tick only through its pre-vault
Mode: finite 12 (all lengths 1 through horizon)
Assumptions: true := Unconditional requirement over recorded keeper ticks; provided true. [version 1]
Model: the generated withdrawal-queue V2 contract evaluated by its checked Authority over every vault reachable from its genesis and the complete declared command and context domain; environment: any commands from any caller between two ticks, and either alarm value at each tick; the alarm is taken as given
Execution obligations: initialization the contract genesis commits, a changed genesis is refused, and no request is pending at genesis, so a history before its first tick holds no presented request; transition membership every event is a keeper tick decided by the checked Authority on a vault reachable from the genesis; continuity hidden decisions leave the controller part unchanged except that an accepted request turns its own empty lane arrived; every tick leaves no lane arrived, so each accepted request is presented by exactly one tick
Evidence: obligation; authority: none; arithmetic: checked i128; scope identity: b8ff35b9d72e8d74daaf1a5485ec9ee2c19a6b7d395b0f90e50a1b07fcd258e0
Observation R [event; version withdrawal-queue/1]: the tick presents lane r's request: the lane is arrived before the tick because its owner's request was accepted after the previous tick (rule 6 presents every arrived lane, so each accepted request is presented exactly once).
Observation C [event; version withdrawal-queue/1]: the decision is a keeper tick that pays lane r (rule 6, law 501): it queues one settlement payout of lane r with the amount that lane's request recorded, and empties the lane.
Observation Tick [event; version withdrawal-queue/1]: the event is an accepted keeper tick; every event of this projection is one, so it only requires that recorded events exist.
Registered reading [withdrawal-queue/1; finite 12]: forall sigma in Traces: forall r in Requests: always((R(r) and Ahead()) => within(7, C(r)))
Formula: ∀sigma ∈ Traces : ∀r ∈ Requests : □(R(r) ∧ Ahead() ⇒ ◇[0,7](C(r)))
Theory: ordered logical events over checked integers. sigma is the declared trace, i its zero-based active position. A prime reads after this event; it is neither negation nor a derivative. Boolean algebra uses a different prime convention outside this theory. Next advances one recorded event. Empty event traces are refused. Initialization is checked separately. Trace quantifiers can be vacuous over an empty execution domain; machine initialization must establish nonemptiness.
Legend: symbol | read as | meaning
∀sigma ∈ Traces :| for every sigma in Traces,| the statement after the colon holds for each sigma in Traces; true if Traces is empty
Traces    | Traces                 | the traces
∀r ∈ Requests :| for every r in Requests,| the statement after the colon holds for each r in Requests; true if Requests is empty
Requests  | Requests               | the requests
□         | always                 | Every remaining event, including now; other traces require the visible trace quantifier.
(…)       | open ... close         | explicit grouping; the enclosed expression is one operand
R(r)      | R of r                 | r is a legitimate submitted withdrawal
∧         | and                    | both statements must hold
Ahead()   | Ahead of no arguments  | at least seven further keeper ticks are recorded
⇒         | implies                | when the left side is true the right side must be; when it is false there is no demand
◇[0,7]    | eventually within further events| Offsets 0 through the displayed bound inclusive; events are not seconds.
C(r)      | C of r                 | r has its matching assets returned
Typed: forall sigma in Traces: forall r in Requests: always(R(r) and Ahead() => within(7, C(r)))
Formal reading: For every sigma in Traces, for every r in Requests, always open R of r is true and Ahead of no arguments is true implies eventually within 7 further events open C of r is true close close.
Precise English: For every trace sigma, for every request r, at every remaining event, (if r is a legitimate submitted withdrawal and at least seven further keeper ticks are recorded, then at this event or within 7 further events of this trace, (r has its matching assets returned)).
Intuitive: Every withdrawal request is paid by the eighth keeper tick that sees it, counting the tick that first presents it, whenever those ticks are recorded.
Plain: Once seven further keeper ticks have followed the tick that presents a withdrawal, it has been paid, alarms included.
Question: Is it true that once seven further keeper ticks have followed the tick that presents a withdrawal, it has been paid, alarms included?
Counterexample search: find a trace sigma such that there exists a request r such that not always (if r is a legitimate submitted withdrawal and at least seven further keeper ticks are recorded, then now or within the next 7 events, (r has its matching assets returned)).
Definition [withdrawal-queue/1]: Ahead() := ○(○(○(○(○(○(○(Tick())))))))
For example worst_case_bound: Contract tick history: both lanes are presented under a sustained alarm; lane A is paid at the fourth tick and lane B at the eighth, exactly at the bound. Expected: true (fixture expectation; run native parity checks).
For example late_presentation: Contract tick history: the alarm defers lane A at the last recorded tick. Its window is not recorded, so the finite target asks nothing of it; the unbounded obligation keeps it. Expected: true (fixture expectation; run native parity checks).
For example no_request: Contract tick history: vacuous without a request; the pilot separately requires presented requests under active alarms. Expected: true (fixture expectation; run native parity checks).
For example eight_ticks_unpaid: Not a contract history (the pilot checks that the contract excludes it): lane A stays unpaid for eight recorded ticks. Expected: false (fixture expectation; run native parity checks).
```

## ui discovery

```text
ui_discovery — zal/ladder-temporal/1 (experimental; authority: none)
Law (holds over the declared domains)
Authority: OPEN — Proposed, awaiting owner confirmation. Definitions are recovered from the generated-interface design and its first milestone checks; no claim reaches users until the owner confirms them.
Scope (theory: ordered-events-over-integers/1; ordered logical events over checked integers): Proposed definitions, awaiting owner confirmation: recorded interface commits of the generated inventory-reservation interface.
Trace source: proposed generated-interface model; no source is bound yet; schema: interface-commit/1: presentation state, focus, open dialog and route exposure before and after each commit; order: authoritative order of recorded interface commits, including opening a dialog and changing focus; projection: retain each recorded event without sorting, merging or deduplication
Mode: finite 4 (all lengths 1 through horizon)
Assumptions: true := Unconditional finite-word requirement; provided true. [version 1]
Model: proposed: the generated presentation core of the inventory-reservation interface; execution membership is unproved until that model is bound; environment: proposed: any user navigation, inferred hints and preference changes (width, theme, motion); hints may change emphasis only
Execution obligations: initialization the initial view is checked separately: every declared region has data, empty or loading status; transition membership requires exact membership in the generated presentation core; continuity requires post/pre correspondence of focus, dialog and exposure at every retained commit
Evidence: obligation; authority: none; arithmetic: checked i128; scope identity: 306cbf4d025f7284cf6ab55751b91d85f319c2c5f1e517d5378b3c2318a1db1e
Observation Required [event; version proposed/1]: Proposed, awaiting owner confirmation. The action is a command route of the contract (reserve, release, ship or restock): every command variant is a required task at every event, independently of what the interface shows. Busy or uncertain states may prevent invocation, not inspection.
Observation VisualRoute [event; version proposed/1]: Proposed, awaiting owner confirmation. A named control for the action is visible and reachable from the current focus within h=2 navigation actions by pointer or keyboard, honouring the open dialog: leave the dialog, then choose the task in the persistent task navigation. Moving focus inside one region is not counted.
Observation AssistiveRoute [event; version proposed/1]: Proposed, awaiting owner confirmation. The same control is in the accessibility tree with its accessible name and reachable within h=2 keyboard navigation actions; hidden or inert elements cannot receive focus. Actual browser and screen-reader exposure is separate browser evidence.
Registered reading [proposed/1; finite]: forall sigma in Traces: forall a in Actions: always(Required(a) => Discoverable(a))
Formula: ∀sigma ∈ Traces : ∀a ∈ Actions : □(Required(a) ⇒ Discoverable(a))
Theory: ordered logical events over checked integers. sigma is the declared trace, i its zero-based active position. A prime reads after this event; it is neither negation nor a derivative. Boolean algebra uses a different prime convention outside this theory. Next advances one recorded event. Empty event traces are refused. Initialization is checked separately. Trace quantifiers can be vacuous over an empty execution domain; machine initialization must establish nonemptiness.
Legend: symbol | read as | meaning
∀sigma ∈ Traces :| for every sigma in Traces,| the statement after the colon holds for each sigma in Traces; true if Traces is empty
Traces    | Traces                 | the traces
∀a ∈ Actions :| for every a in Actions,| the statement after the colon holds for each a in Actions; true if Actions is empty
Actions   | Actions                | the actions
□         | always                 | Every remaining event, including now; other traces require the visible trace quantifier.
(…)       | open ... close         | explicit grouping; the enclosed expression is one operand
Required(a)| Required of a          | a is required
⇒         | implies                | when the left side is true the right side must be; when it is false there is no demand
Discoverable(a)| Discoverable of a      | a is discoverable
Typed: forall sigma in Traces: forall a in Actions: always(Required(a) => Discoverable(a))
Formal reading: For every sigma in Traces, for every a in Actions, always open Required of a is true implies Discoverable of a is true close.
Precise English: For every trace sigma, for every action a, at every remaining event, (if a is required, then a is discoverable).
Intuitive: Every required action remains discoverable at every recorded UI event.
Plain: A required action remains discoverable.
Question: Is it true that a required action remains discoverable?
Counterexample search: find a trace sigma such that there exists an action a such that not always (if a is required, then a is discoverable).
Definition [proposed/1]: Discoverable(a) := VisualRoute(a) ∧ AssistiveRoute(a)
For example review_dialog_keeps_routes: Opening the review dialog at event 1 keeps every task within two navigation actions: leave the dialog, choose the task. Expected: true (fixture expectation; run native parity checks).
For example modal_hides_reserve: A dialog without a leave route hides the reserve task at event 1. Expected: false (fixture expectation; run native parity checks).
For example assistive_only: The ship control is in the accessibility tree but has no visual route: discoverability needs both modes. Expected: false (fixture expectation; run native parity checks).
For example not_required: Vacuous implication; under the proposed Required every command route is required, so a real interface never shows this word. Expected: true (fixture expectation; run native parity checks).
```

## vault drain

```text
vault_drain — zal/ladder-temporal/1 (experimental; authority: none)
Law (holds over the declared domains)
Authority: OPEN — Proposed, awaiting owner confirmation. Definitions are recovered from the existing vault, stablecoin and oracle relations; no claim reaches users until the owner confirms them.
Scope (theory: ordered-events-over-integers/1; ordered logical events over checked integers): Proposed definitions, awaiting owner confirmation: every event of the composed vault, oracle, collateral and governance machine.
Trace source: proposed composed vault model; no source is bound yet; schema: pre-input-outcome-post/1; order: authoritative recorded event order, including rejections and retries; projection: retain each recorded event without sorting, merging or deduplication
Mode: finite 4 (all lengths 1 through horizon)
Assumptions: true := Unconditional finite-word requirement; provided true. [version 1]
Model: proposed: the composed vault, oracle, collateral and governance machine; execution membership is unproved until that model exists; environment: proposed: the declared attacker capabilities (compromised signer keys up to a declared count, stale pre-signed approvals, many accounts, atomic batches, flash liquidity and price-setting power on thin markets); a capability left out is a stated blind spot
Execution obligations: initialization initial custody v0 of each asset is fixed at genesis and checked separately, including executions with no operation; transition membership requires exact membership in the composed model; continuity requires post/pre correspondence of custody, credits and discharges at every retained boundary
Evidence: obligation; authority: none; arithmetic: checked i128; scope identity: 2cf90258c78ab6b4f3d21930fe10fb5a17ccbeedf112a6e87bd1c620a4ade7e0
Observation v [event; version proposed/1]: Proposed, awaiting owner confirmation. Units of the asset held in every reserved custody account of the machine (pools, deposits, stakes, surplus and claim custody), in that asset's own units; no price converts one asset into another.
Observation c [event; version proposed/1]: Proposed, awaiting owner confirmation. Cumulative authenticated external funding of the asset into custody since genesis. Internal reward or claim moves are reclassifications, neither credit nor discharge.
Observation p [event; version proposed/1]: Proposed, awaiting owner confirmation. Cumulative exact external payouts and burns of the asset, each bound to an entitlement the relation computes from independently evaluated inputs and consumed once; an approval alone, including a compromised signer's, never discharges.
Observation v0 [event; version proposed/1]: Proposed, awaiting owner confirmation. Custody of the asset at genesis, a constant of the trace.
Registered reading [proposed/1; finite]: forall sigma in Traces: forall a in Assets: always(not Loss(a))
Formula: ∀sigma ∈ Traces : ∀a ∈ Assets : □(¬Loss(a))
Theory: ordered logical events over checked integers. sigma is the declared trace, i its zero-based active position. A prime reads after this event; it is neither negation nor a derivative. Boolean algebra uses a different prime convention outside this theory. Next advances one recorded event. Empty event traces are refused. Initialization is checked separately. Trace quantifiers can be vacuous over an empty execution domain; machine initialization must establish nonemptiness.
Legend: symbol | read as | meaning
∀sigma ∈ Traces :| for every sigma in Traces,| the statement after the colon holds for each sigma in Traces; true if Traces is empty
Traces    | Traces                 | the traces
∀a ∈ Assets :| for every a in Assets, | the statement after the colon holds for each a in Assets; true if Assets is empty
Assets    | Assets                 | the assets
□         | always                 | Every remaining event, including now; other traces require the visible trace quantifier.
(…)       | open ... close         | explicit grouping; the enclosed expression is one operand
¬         | not                    | true exactly when the following statement is false
Loss(a)   | Loss of a              | a suffers uncompensated vault loss before or after the event
Typed: forall sigma in Traces: forall a in Assets: always(not Loss(a))
Formal reading: For every sigma in Traces, for every a in Assets, always open not Loss of a is true close.
Precise English: For every trace sigma, for every asset a, at every remaining event, (not a suffers uncompensated vault loss before or after the event).
Intuitive: No admitted operation sequence reaches uncompensated vault loss in any asset.
Plain: No admitted operation sequence drains the vault.
Question: Is it true that no admitted operation sequence drains the vault?
Counterexample search: find a trace sigma such that there exists an asset a such that not never (a suffers uncompensated vault loss before or after the event).
Definition [proposed/1]: Loss(a) := v(a)+p(a) < v0(a)+c(a) ∨ v′(a)+p′(a) < v0(a)+c′(a)
For example legitimate_withdrawal: Collateral v0=10, c=0: a withdrawal of 3 discharged against its entitlement leaves v=7, p=3; initial custody is checked separately. Expected: true (fixture expectation; run native parity checks).
For example partial_drain: Collateral v0=10, c=0: v=7 with no discharge is a loss, including a partial drain. Expected: false (fixture expectation; run native parity checks).
For example approval_is_no_discharge: A transfer of 6 backed only by signer approval leaves p unchanged, so the second event is a loss. Expected: false (fixture expectation; run native parity checks).
For example legitimate_credit: An authenticated deposit of 5 raises custody and credit together. Expected: true (fixture expectation; run native parity checks).
For example initial_disaster: Custody 9 below v0=10 before the first event: D(pre) holds; zero-operation initialization remains a separate check. Expected: false (fixture expectation; run native parity checks).
```
