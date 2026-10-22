# Delegation without cycles

## Journey

The ask: "members delegate their vote, and a delegation that closes a cycle
must be refused."

```text
zeno-fcis check examples/contract-tables/delegation/project.zeno --require-substantive --require-resolved-paths
zeno-fcis check examples/contract-tables/delegation/project.zeno --expand
zeno-fcis contract review examples/contract-tables/delegation --gaps --no-kb
zeno-fcis contract lemma examples/contract-tables/delegation --lemma '!reach[0][0] && !reach[1][1] && !reach[2][2]' --no-kb
zeno-fcis contract probe count examples/contract-tables/delegation --where 'candidate[0][2]' --no-kb
```

## Formal model

Three members occupy the fixed index set of table 110. Each stored entry is
0 for no delegate, or j+1 for member j. A command selects one member and its
new target. `reach[i][j]` is the least fixed point of direct delegation or a
path through another member, computed over the actual post-state. It starts
false and has nine Boolean entries, so the declared bound is nine rounds.

The rule reads `candidate`, the same closure over the prospective delegation
computed from immutable pre-state and command. A reflexive entry refuses the
command with reason 200. Law 500 independently requires every reflexive
entry of actual `reach` to be false on commit and at genesis. Laws 501 and
502 require that exactly the selected entry receives the selected target;
law 550 permits rejection only for a prospective cycle. Clearing an entry can
repair a supplied state that already contains a cycle.

The policy's `cycle` variable contains the circuit expansion of law 550's
predicate. Both the rule and the independent post-state law use the existing
metered library evaluator. Decision fixtures are LLM-generated expectations;
the separate source-rule reference supplies the exhaustive comparison.

## Difference

Without recursion, a scalar cycle check lists paths of every possible length
and repeats them for every member. Here the closure rule states the relation
once, and the law states its forbidden diagonal. Expansion shares each
round's values and shows the exact finite computation.

## Background

Maarten H. van Emden and Robert A. Kowalski, “The Semantics of Predicate
Logic as a Programming Language,” Journal of the ACM 23(4), 1976, studies
fixed-point semantics for logic programs. Krzysztof R. Apt, Howard A. Blair
and Adrian Walker, “Towards a Theory of Declarative Knowledge,” in
*Foundations of Deductive Databases and Logic Programming*, Morgan Kaufmann,
1988, covers stratified logic programs. These are background for the bounded
positive rules; the compiler, meters and finite tests qualify this implementation.

## What is new here

This release adds bounded recursive table declarations, shared Boolean rounds
and their contract-judge integration. The comparative assessment of this
combination is **prior-art search pending**.

## Scope

The declared domain is 64 states, 12 commands and one context: 768 input
tuples. Full review has no law refusals or example disagreements; the gap
judge determines every tuple. The acyclicity lemma is inductive, admits 16
states, and all 16 are reachable from empty delegation. These are exact
results for these finite domains. Vote tallying, external identity and
membership changes are outside this model.
