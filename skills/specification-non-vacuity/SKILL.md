---
name: specification-non-vacuity
description: Challenge safety specifications for impossible premises, deny-all behavior, missing successful outcomes, and unreachable guarantees. Use before accepting or weakening a formal application contract, especially when a proof could pass without implementing the intended useful behavior.
---

# Specification non-vacuity

A safety statement can hold because the behavior it constrains never occurs.
Review the intended useful behavior alongside the forbidden behavior before
accepting the specification or its proof.

## Identify the obligation

Read the complete requirements and formal contract. Name the legal domain,
initial states, admitted inputs, successful outcomes, rejected outcomes,
observations, assumptions, and required progress. Distinguish supplied facts
from externally established facts, such as caller authorization.

Freeze the original domain and requirements. Do not silently shrink them,
strengthen premises, remove successful cases, or adopt an all-Reject program
to make verification easier. When intent is ambiguous, generate a concrete
distinguishing example and ask the owner to choose its intended outcome.

## Check useful behavior and coverage

1. Produce independently authored examples of required success, required
   rejection, and meaningful boundaries. Require an actual expected successor
   state, reason, and effect/delivery observations when relevant.
2. Check that premises are satisfiable in the declared domain. For stateful
   properties, separately check whether the relevant states/events are reachable
   from the declared genesis or admitted upgrade histories. A satisfying
   arbitrary state is not a reachable witness.
3. Look for rules that cannot fire, contradictory guards, impossible progress,
   omitted alternatives, and properties that never consult the proposed
   implementation. Ask whether a constant-refusal implementation would pass.
4. Check that required successful behavior is represented by the contract and
   obtained from the actual supported execution path. A success witness shows
   existence; it does not prove completeness or success on every required input.
5. Use the strongest supported checker for the domain. Exhaustively check a
   complete finite domain when feasible. For symbolic searches, distinguish
   witnessed SAT, checked UNSAT, and UNKNOWN/timeouts, and state which formulas
   were checked. Never treat failed search as impossibility.

For upgrades, include states inherited through earlier rule changes and
migrations. A target invariant proved from its own genesis does not by itself
hold in every imported state. Check the actual admitted state and the obligations
needed for future preservation.

## Mutate and distinguish

Propose a bounded bad variant and confirm an independent check rejects it:
disable all successful rules, remove an authorization guard, omit an effect,
weaken a conservation equation, or exclude a boundary input. Mutations must
exercise the property under review; unrelated parser/build errors do not count.

When two interpretations compete, search for a legal input or reachable trace
on which their decisions differ. Present the input/trace and both outcomes for
review. An inconclusive search remains inconclusive. Identical examples do not
establish equivalence outside their coverage.

Keep expected behavior independently authored. Do not generate the acceptance
oracle by copying the candidate implementation or replacing failed expectations
with whatever the candidate returns.

## A small example

```text
approve(pending, authorized) = pending AND authorized
safety: approve(pending, authorized) implies authorized
required success: pending=true AND authorized=true gives approval
```

Changing `approve` to always return false still satisfies `safety`, but violates
`required success`. The two obligations expose different defects. The success
example does not authenticate the supplied `authorized` fact or establish a
general liveness theorem.

## Report the boundary

Record the contract revision, domain, independently authored examples,
satisfiability/reachability results, mutation outcomes, tool identities, and
unresolved questions. Separate specification adequacy from implementation
conformance, proof correctness, and external authentication/settlement. Do not
describe finite evidence as covering a larger or different domain.

This workflow is inspired by the fixed-statement checking lesson in
[OpenAI's mathematics repository](https://github.com/openai/math) and
[Lean Comparator](https://github.com/leanprover/comparator); it is our proposed
specification-review method, not an imported proof of application correctness.
ZenoFCIS's `experiments/proof-challenge/README.md` has a concrete changed-helper
control that refuses every approval while leaving the authorization theorem
true. Use the project's existing specification diagnostics, example corpus,
decision checker, and ZAL explanation/trace tools as evidence where available;
the tools do not decide the owner's intent.
