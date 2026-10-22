# Permissions inherited as sets

The ask: "permit a request only when its role inherits every requested permission."

## Formal model

Two roles each have a parent and a three-bit permission set: read is 1, edit
is 2, publish is 4. A self parent is a root. The derived `permissions[i]`
joins its direct grants with its parent's permissions. All entries start at
zero, and the declared bound is two entries times three atoms: six rounds.
A grantless cycle stays empty; a cycle with grants inherits their union.

The command selects a role and a nonempty requested set. `subset(3, requested,
permissions[i])` tests set inclusion. Numeric ordering would be wrong: 2 is
numerically below 4 but edit is not contained in publish. Requests preserve
all stored fields. Laws check acceptance and rejection in both directions.
`post_permissions` supplies the same closure over the post-state for state
lemmas and genesis. The JSON policy contains law 502's printed expansion.

```text
zeno-fcis check examples/contract-tables/permission-words/project.zeno --require-substantive --require-resolved-paths
zeno-fcis check examples/contract-tables/permission-words/project.zeno --expand
zeno-fcis contract review examples/contract-tables/permission-words --gaps --no-kb
zeno-fcis contract lemma examples/contract-tables/permission-words --lemma 'subset(3, post.100.210[0], post_permissions[0]) && subset(3, post.100.210[1], post_permissions[1])' --no-kb
zeno-fcis contract probe count examples/contract-tables/permission-words --where 'subset(3, u128(4), permissions[1])' --no-kb
```

## Difference

Repeated scalar checks need separate inheritance checks for every permission.
The word rule keeps the permission identities together and uses joins and
meets of a finite Boolean algebra. The compiler lifts each atom to the same
shared Boolean-round representation; it introduces no second evaluator.

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

There are 256 parent/grant states, 14 commands and one context: 3,584 tuples.
The declared finite-domain judges remain exact within their resource limits.
Decision fixtures are LLM-generated expectations; the independent Python
word and Boolean references compare all three-entry maps for widths 2 and 3.
This request-only example does not change roles, authenticate external actors
or execute a privileged operation. Widths 1 through 128 use unsigned values;
larger sets and multiword storage are outside this implementation.
