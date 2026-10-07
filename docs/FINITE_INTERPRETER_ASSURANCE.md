# Finite interpreter paper: implications for V2 assurance

Read on 2026-10-01: Le Qi, [The Finite Interpreter and the Explanatory Halting
Theorem](https://philpapers.org/archive/QITFIR.pdf), especially sections 2,
4.4, 5.2–5.5 and 7.3. This is an engineering assessment, not a machine-checked
formalization or endorsement of the paper.

The paper separates fixed mathematical objects from evolving interpretive
practice. Its general computability result assumes an effective embedding of
arbitrary halting problems; its broader philosophical conclusion additionally
depends on a premise about possible future reinterpretation. Those are stated
assumptions, not a verification result for ZenoFCIS.

## What we can still prove

For fixed implementation E, specification S and admitted input domain D, the
core obligation is:

```text
For every input x in D, E(x) has exactly the result specified by S(x).
For every other input, admission refuses as specified.
```

Here S includes refusal order, checked arithmetic, all decision classes,
successor state, effects, footprints, logical usage and authorization behavior.
The claim quantifies over inputs, rather than observed test cases. It does not
quantify over future replacements for E, S or D. Changes to any of them require
fresh correspondence evidence.

Undecidability of a general family does not prohibit proofs for its individual
members. For example, consider a machine with initial state 0, transition
step(0) = 0 and halted(0) = false. Induction proves that its state remains 0 for
every step count, hence it never halts. A complete general-purpose classifier
for arbitrary machines remains impossible. Treating every particular negative
claim as unprovable would confuse those two quantifiers.

Likewise, a theorem about all possible future reinterpretations does not stop
us proving an integer reader or a complete declared decision relation. It does
not justify leaving application-owned functions or composition bridges
unchecked. V2 retains the full-core completion requirements in
[the implementation plan](V2_VERIFIED_CORE_PLAN.md).

## Changes to our assurance practice

Every qualification should identify its implementation revision, specification,
complete admitted domain, checked functions, bridges, assumptions and remaining
obligations. The pinned source manifests and exact-head receipts already record
parts of this information. Until the mandatory route is complete, a receipt for
one unit must keep the open bridges explicit.

A proof-dependency inventory must distinguish three kinds of leaf: a proved
application-owned component, an explicit trusted-base assumption, and an open
obligation. Packaging an open obligation into an adapter does not convert it
into an assumption acceptable for release. In particular, a natively compared
Value-to-scalar mapping cannot stand in for the original-byte admission proof.

Specification adequacy needs its own review: the declared rules must capture
the intended product behavior. Independent examples and mutations challenge
that boundary. A proposed reinterpretation is a review input; a counterexample
to the fixed claim must satisfy its actual domain and contradict its specified
property. Neither a new objection nor a passing fixture automatically changes
an existing theorem's status.

Marketing should state the checked subject and claim. Once the complete route
is qualified, a suitable statement is: the functional core is formally verified
against its versioned decision specification for all admitted inputs, under the
published trusted-base assumptions. Before that point, name the verified units
and the remaining correspondence obligations. Never imply that an unfinished
unit proves the whole app, or that formal correctness certifies external facts,
effect delivery, or every future requirement.

No runtime restriction follows merely from this paper. Continue implementing
and proving V2, preserving its legal domains and requiring source-bound replay
after changes. The paper is a prompt for precise scope and assumption disclosure,
not a substitute checker or an excuse to weaken the completion gate.
