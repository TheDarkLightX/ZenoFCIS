# System properties of finite transitions

## Purpose

A property is only evidence about a system when checking it consults the
system. The claim exporters described in
[law and claim substance](CLAIM_SUBSTANCE.md) export a claim over unconstrained
observations, so their proofs hold for every system alike.

This package checks a property against an exact `finite-i64/1` transition
program. It checks every admitted input exhaustively, and it can also export
SMT obligations that contain the transition relation. Each route checks the
other.

## Inputs and outputs

A property is a closed Boolean relation over a transition's inputs followed by
its outputs. `zeno_fcis_synthesis::system::Property` holds the relation program
and the equivalent synthesis `Contract`. The same relation language describes
the transition, its reviewed specification, and its properties.

`zeno_fcis_synthesis::system::check_system_property` returns one `SystemCheck`:

| Code | Meaning |
| --- | --- |
| `system-property` | Every admitted input yields a defined, in-domain output that satisfies the property, and some output tuple the domains admit would violate it. |
| `domain-implied` | The property also holds for every output tuple the declared domains admit, so it says nothing about the transition. |
| `not-total` | An admitted input traps or leaves the declared output domains. |
| `violated` | The property is false for this admitted input and the transition's output. |
| `undefined` | The property traps for this admitted input and output. |

`zeno_fcis_formal_tools::export_system_smt` exports three SMT-LIB scripts for
one property. Each script is unsatisfiable exactly when the answer to its
question is yes:

1. Totality: does every admitted input yield a defined output inside the output
   domains? Output domains are checked here, never assumed.
2. Property: does the property hold on the transition's output? This script
   assumes totality, so its answer counts only after totality is proved.
3. Domain only: does the property hold for every output tuple the domains
   admit, without the transition?

`system_verdict` asks a caller-supplied solver function for each answer in
that order and returns one `SystemVerdict` with the same codes. The function
parses its output with `parse_system_answer`. A satisfying model is accepted
only after the program interpreter reproduces its failure.

## Authority boundary

Both routes produce diagnostic evidence about a finite program. They grant no
authorization and do not change canonical bytes, program identities, or the
synthesis checker and emitter identities.

## Trusted dependencies

The exhaustive route trusts the `finite-i64/1` interpreter. The SMT route
trusts the selected solver and this encoding. The pinned differential tests run
both routes on the same programs and require equal verdicts.

## Deterministic resource bounds

`SystemLimits` caps admitted inputs at 65,536 by default. The domain-only
control is capped at 4,194,304 input and output pairs. Exceeding a cap is an
error, never a partial result. The SMT scripts are linear in the number of
program nodes.

## Laws

1. An out-of-domain output is a totality failure, because the executed program
   fails on it. It is never excluded by assuming output domains inside the
   transition relation.
2. A property reaches `system-property` only when the domain-only control finds
   an admitted output tuple that violates it.
3. Stages are reported in a fixed order: totality, then property, then the
   domain-only control. Witnesses are the first failing input in canonical
   lexicographic order.
4. A solver model that the interpreter cannot reproduce is an error, never a
   verdict.

## Negative cases

The tests use the durable-counter template's exact synthesized program. They
first require its canonical bytes to equal `synthesized/program.zcve`. They
then check:

- Refinement of the reviewed `synthesis.json` relation is `system-property`.
- Three properties are each `system-property`: failures never decrease, a
  rejection leaves state unchanged, and the notification matches the new state.
- `post.count <= 3` is `domain-implied`, because the output domain already
  bounds it.
- Raising the capacity guard's bound from 3 to 4 is `not-total` at input
  `[0, 3, 1, 1]`: a recorded failure at `pre.failures = 3`.
- A solver that claims every script is satisfiable is refused, because the
  interpreter cannot reproduce its model.

## Assumptions

- The program's inputs model the executed invocation completely.
- In the template, the Rust adapter maps output codes to decisions, reasons,
  field updates and outbox entries as the reviewed table states.

## Explicit nonclaims

- These results cover the finite program, not the Rust code that maps its
  outputs to builder calls. An exhaustive differential test of that adapter
  against a declared output table is follow-up work. So is deriving the
  initial state and input domains from the executed genesis and admission
  rules.
- A domain-only control that finds a violating tuple does not show that the
  property is the right requirement.
- Local runs with an unpinned solver are supplemental. The pinned CVC5 run is
  in the formal-tools workflow.
