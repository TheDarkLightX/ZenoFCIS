# Generated temporal examples

These OPEN proposals use placeholder definitions. The following blocks are
CLI render output; their statements, scopes and fixtures carry authority none.

## alarm withdrawal

```text
alarm_withdrawal — zal/ladder-temporal/1 (experimental; authority: none)
Law (holds over the declared domains)
Authority: OPEN — Proposed formalization; placeholder definitions await owner review.
Scope (theory: ordered-events-over-integers/1; ordered logical events over checked integers): Declared ordered event observations; example definitions are placeholders.
Trace source: example-observations/1; schema: pre-input-outcome-post/1; order: authoritative recorded event order, including rejections and retries; projection: retain each recorded event without sorting, merging or deduplication
Mode: unbounded (natural-number-indexed observations; no final event)
Assumptions: A(sigma) := PLACEHOLDER A: recording/execution continues, asset service remains available, and exit processing is weakly fair. A assumes neither completion nor alarm clearance. A finite non-resettable delay budget must independently establish persistent exit enablement. [version placeholder/1]
Model: placeholder composed model; execution membership is unproved; environment: placeholder capabilities; no omitted actor is implicitly excluded
Execution obligations: initialization separate initial-state check, including zero-operation executions; transition membership requires exact source-model membership; continuity requires post/pre correspondence at every retained boundary
Evidence: obligation; authority: none; arithmetic: checked i128; scope identity: cd4352885c53b8a5d576173f3b58f727e223f5452e8a39be62ab1c88ed7cc1af
Observation A [trace; version placeholder/1]: PLACEHOLDER A: recording/execution continues, asset service remains available, and exit processing is weakly fair. A assumes neither completion nor alarm clearance. A finite non-resettable delay budget must independently establish persistent exit enablement.
Observation R [event; version placeholder/1]: PLACEHOLDER legitimacy is fixed at submission before acceptance; request identity is never reused and policy changes cannot erase it.
Observation C [event; version placeholder/1]: PLACEHOLDER matching assets returned to the same frozen request identity; another request cannot witness completion.
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
For example finite_pending: Finite fixture, horizon 4: complete before this recorded word ends. A pending request fails that finite target; the unbounded run obligation remains unresolved. Expected: false (fixture expectation; run native parity checks).
For example alarm_exit: Finite fixture, horizon 4: legitimate request r1 completes despite active alarms. A is stipulated, not certified fairness; the unbounded run obligation remains unresolved. Expected: true (fixture expectation; run native parity checks).
For example wrong_request: Finite fixture, horizon 4: completion of r2 does not complete submitted r1 before this word ends. The unbounded run obligation remains unresolved. Expected: false (fixture expectation; run native parity checks).
For example no_request: Finite fixture, horizon 4: vacuous response without a request; legitimate journeys and reject-everything controls remain required. The unbounded run obligation remains unresolved. Expected: true (fixture expectation; run native parity checks).
```

## ui discovery

```text
ui_discovery — zal/ladder-temporal/1 (experimental; authority: none)
Law (holds over the declared domains)
Authority: OPEN — Proposed formalization; placeholder definitions await owner review.
Scope (theory: ordered-events-over-integers/1; ordered logical events over checked integers): Declared ordered event observations; example definitions are placeholders.
Trace source: example-observations/1; schema: pre-input-outcome-post/1; order: authoritative recorded event order, including rejections and retries; projection: retain each recorded event without sorting, merging or deduplication
Mode: finite 4 (all lengths 1 through horizon)
Assumptions: true := Unconditional finite-word requirement; provided true. [version 1]
Model: placeholder composed model; execution membership is unproved; environment: placeholder capabilities; no omitted actor is implicitly excluded
Execution obligations: initialization separate initial-state check, including zero-operation executions; transition membership requires exact source-model membership; continuity requires post/pre correspondence at every retained boundary
Evidence: obligation; authority: none; arithmetic: checked i128; scope identity: 2f49c1d56ef7161772f7fe634a2bd4f056a4c14b38689f86b19bc38fb65efa7b
Observation Required [event; version placeholder/1]: PLACEHOLDER task-state requirement independent of what the UI displays; Actions={exit}.
Observation Discoverable [event; version placeholder/1]: PLACEHOLDER a named affordance is visually and assistively exposed and reachable from current focus within h=2 legal keyboard navigation actions, including modal restrictions. Rendering and assistive correspondence are unproved.
Registered reading [placeholder/1; finite]: forall sigma in Traces: forall a in Actions: always(Required(a) => Discoverable(a))
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
For example modal_hides_exit: Recorded modal/focus event hides a required affordance at event 1. Expected: false (fixture expectation; run native parity checks).
For example not_required: Vacuous implication; no actual browser rendering or accessibility evidence. Expected: true (fixture expectation; run native parity checks).
```

## vault drain

```text
vault_drain — zal/ladder-temporal/1 (experimental; authority: none)
Law (holds over the declared domains)
Authority: OPEN — Proposed formalization; placeholder definitions await owner review.
Scope (theory: ordered-events-over-integers/1; ordered logical events over checked integers): Declared ordered event observations; example definitions are placeholders.
Trace source: example-observations/1; schema: pre-input-outcome-post/1; order: authoritative recorded event order, including rejections and retries; projection: retain each recorded event without sorting, merging or deduplication
Mode: finite 4 (all lengths 1 through horizon)
Assumptions: true := Unconditional finite-word requirement; provided true. [version 1]
Model: placeholder composed model; execution membership is unproved; environment: placeholder capabilities; no omitted actor is implicitly excluded
Execution obligations: initialization separate initial-state check, including zero-operation executions; transition membership requires exact source-model membership; continuity requires post/pre correspondence at every retained boundary
Evidence: obligation; authority: none; arithmetic: checked i128; scope identity: ebf0e9c927d76119aa10e5d7bac99f88e37a85adbdb6eea70ec9c87276854510
Observation Loss [event; version placeholder/1]: PLACEHOLDER one-asset policy: D(q) := v(q)+p(q)<v0+c(q); Loss := D(pre) or D(post). p counts independently justified entitlement discharges, never approval alone. v0=10, integer asset units. Other assets, valuation and allowed losses await review.
Registered reading [placeholder/1; finite]: forall sigma in Traces: always(not Loss())
Formula: ∀sigma ∈ Traces : □(¬Loss())
Theory: ordered logical events over checked integers. sigma is the declared trace, i its zero-based active position. A prime reads after this event; it is neither negation nor a derivative. Boolean algebra uses a different prime convention outside this theory. Next advances one recorded event. Empty event traces are refused. Initialization is checked separately. Trace quantifiers can be vacuous over an empty execution domain; machine initialization must establish nonemptiness.
Legend: symbol | read as | meaning
∀sigma ∈ Traces :| for every sigma in Traces,| the statement after the colon holds for each sigma in Traces; true if Traces is empty
Traces    | Traces                 | the traces
□         | always                 | Every remaining event, including now; other traces require the visible trace quantifier.
(…)       | open ... close         | explicit grouping; the enclosed expression is one operand
¬         | not                    | true exactly when the following statement is false
Loss()    | Loss of no arguments   | uncompensated vault loss is present before or after the event
Typed: forall sigma in Traces: always(not Loss())
Formal reading: For every sigma in Traces, always open not Loss of no arguments is true close.
Precise English: For every trace sigma, at every remaining event, (not uncompensated vault loss is present before or after the event).
Intuitive: No admitted operation sequence reaches uncompensated vault loss.
Plain: No admitted operation sequence drains the vault.
Question: Is it true that no admitted operation sequence drains the vault?
Counterexample search: find a trace sigma such that not never (uncompensated vault loss is present before or after the event).
For example legitimate_withdrawal: v0=10,c=0; v=7,p=3 offsets the withdrawal; initial D checked separately. Expected: true (fixture expectation; run native parity checks).
For example partial_drain: v0=10,c=0; v=7,p=0 is a loss, including a partial drain. Expected: false (fixture expectation; run native parity checks).
For example initial_disaster: D(pre) is already true; no-operation initialization remains a separate obligation. Expected: false (fixture expectation; run native parity checks).
```
