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
and the equivalent relational `Contract`. The same relation language describes
the transition, its reviewed specification, and its properties.

`Property::try_new` accepts up to 32 combined input and output fields, with
between 1 and 16 output fields. These are the existing relation-`Program` input
and transition-`Program` output bounds. A transition with 17 inputs and 9 outputs
can therefore have a system property. Exact domain order and a single Boolean
relation result remain mandatory. `Contract::try_new` and `Sketch::try_new`
retain the narrower synthesis limit of 16 inputs and 16 outputs; obtaining a
contract from a wider property does not allow a matching synthesis sketch.

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
parses its output with `parse_system_answer(kind, stdout, inputs, outputs)`.
A satisfying answer is a `SystemAnswer::Sat { input, output }`. The domain-only
script also requests its proposed outputs (`free_out_0` onward); the other
scripts leave `output` empty.

A model is accepted only when all of these hold:

- it has one value per variable, and every value lies inside its declared
  domain;
- the program interpreter reproduces the failure the model claims: a
  totality failure on an admitted input, a false or undefined property on the
  transition's actual output, or, for the domain-only control, a false or
  undefined property on the proposed admitted input and output.

Otherwise the result is `SystemSolveError::UnreplayableModel`, never a
verdict.

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

The wider system-property constructor does not change these enumeration caps,
the finite semantic profile, or the canonical program and contract data. Its
implementation changes the source-bound synthesis checker identity because the
contract constructor shares a source file with that checker. Retained synthesis
evidence must therefore be regenerated where an exact checker source is required.

## Laws

1. An out-of-domain output is a totality failure, because the executed program
   fails on it. It is never excluded by assuming output domains inside the
   transition relation.
2. A property reaches `system-property` only when the domain-only control finds
   an admitted output tuple that violates it.
3. Stages are reported in a fixed order: totality, then property, then the
   domain-only control. Each stage covers every admitted input before the next
   begins, so a property failure never hides a totality failure. Exhaustive
   witnesses are the first failing input in canonical lexicographic order.
4. A solver model that is outside the declared domains, has the wrong number
   of values, or that the interpreter cannot reproduce is an error, never a
   verdict. This includes domain-only models, whose proposed outputs are
   replayed.

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
- A totality model outside the input domain, such as input 2 for a program
  over {0, 1}, is refused. It used to be reported as a totality failure,
  because the interpreter rejects it at its own domain check.
- A domain-only model for a constant-true property is refused, because no
  admitted pair makes the property false. It used to be accepted without
  replay.
- Doubling over {0, 1} with the property `post == 1` is `not-total` at input 1,
  not `violated` at input 0.
- Over a bounded collection of 80 cases, the solver route and the exhaustive
  route return identical verdicts and witnesses. The cases are 10 one-input
  programs over {0, 1, 2} and 8 properties, including one that traps. They
  cover every verdict, and cases that fail totality on some inputs and the
  property on others. The solver route here uses a reference that answers each
  obligation by enumeration. The pinned CVC5 run repeats the collection and
  requires the same stage.
- Shapes of 17 inputs plus 9 outputs, 31 plus 1, and 16 plus 16 preserve the last
  input in the last output. Exhaustive and pinned CVC5 checks agree on these
  programs and on variants that violate the property or the output domain.
  The ordinary tests also reject truncated or nonviolating wide models and
  check unchanged synthesis bounds and enumeration caps. Singleton padding
  keeps these examples small; they test field handling, not application adequacy.

The model admission, domain-only replay, totality ordering, and 80-case
comparison come from an independent review of commit `521b768`. Its three
probes are kept in `crates/zeno-fcis-formal-tools/tests/system_contract.rs`.

## Controls

Each critical check was removed in a working tree, and the named test failed.
The edits were reverted; none is committed.

| Check removed | Detected by |
| --- | --- |
| Totality decided on every input before the property | `totality_is_decided_on_every_input_before_any_property_result` |
| Domain-only control (always `domain-implied`) | `transition_dependent_property_is_a_system_property` |
| Domain-only control (always `system-property`) | `domain_only_control_reports_properties_the_domains_already_imply` |
| Trapping tuples counted as violations in the control | `solver_route_agrees_with_exhaustive_route_on_small_programs` |
| Admission of totality models | `solver_models_must_be_admitted_before_they_are_replayed` |
| Replay of totality models | `system_verdict_rejects_models_the_interpreter_cannot_replay` |
| Replay of property models | `solver_models_must_be_admitted_before_they_are_replayed` |
| Replay of domain-only models | `domain_only_models_are_replayed_with_their_proposed_outputs` |
| Admission of proposed domain-only outputs | `domain_only_models_are_replayed_with_their_proposed_outputs` |
| Totality handled before the property answer | `solver_route_agrees_with_exhaustive_route_on_small_programs` |
| Output domains kept out of the totality assumption | `system_obligations_never_assume_output_domains_for_totality` |

Two first attempts were not valid controls, and were redone. One named a test
that could not observe the removed check. The other edit left the checked
answer unchanged.

## Assumptions

- The program's inputs model the executed invocation completely. For the
  durable-counter template, `tests/conformance.rs` checks this on every
  admitted input (see below).
- Other adapters map output codes to decisions as their reviewed tables state,
  unless a conformance test like the durable counter's checks them.

## Explicit nonclaims

- These results cover the finite program. For the durable-counter template
  only, the generated application's `tests/conformance.rs` connects them to the
  executed application: on all 64 admitted inputs, the decision, reason, new
  state, and notification produced through admission, the authority, the Rust
  adapter, and the law checker equal the finite program's output read through
  a declared table. Schema admission equals the finite input domain in both
  directions, genesis is exactly zero, and all 16 admitted states are
  reachable from it. Swapping two same-type bindings in the adapter, swapping
  its two reject reasons, or swapping the notification fields fails these
  tests. Twelve examples drafted from the template README are checked the same
  way. Their author had seen the model artifacts; a second reviewer checked
  them against the README alone, and the project's owner accepted them on
  2026-09-23.
- A domain-only control that finds a violating tuple does not show that the
  property is the right requirement.
- Local runs with an unpinned solver are supplemental. The pinned CVC5 run is
  in the formal-tools workflow.
- The reference-solver comparison checks how answers are validated, replayed,
  and combined. Only the pinned CVC5 runs check the SMT encoding itself, and
  only over the collections they cover.
- A positive SMT verdict still rests on the solver's unchecked `unsat` answers
  for totality and the property. Only the witnesses are replayed.
