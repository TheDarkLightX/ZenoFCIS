# A permission inherited through roles

## Journey

The ask: "permit a request when its role, or any ancestor, grants the
permission."

```text
zeno-fcis check examples/contract-tables/role-inheritance/project.zeno --require-substantive --require-resolved-paths
zeno-fcis check examples/contract-tables/role-inheritance/project.zeno --expand
zeno-fcis contract review examples/contract-tables/role-inheritance --gaps --no-kb
zeno-fcis contract lemma examples/contract-tables/role-inheritance --lemma 'post_permission[2]' --no-kb
zeno-fcis contract probe count examples/contract-tables/role-inheritance --where 'permission[2]' --no-kb
```

## Formal model

Three roles have one parent each in table 110 and one grant flag each in
table 210. Parent 0 means none, and j+1 names role j. `permission[i]` is true
when role i grants the permission or its parent has it. It is computed from
the pre-state, starting with three false entries and using at most three
rounds. A cycle with no grant stays false; a grant propagates through all
roles that inherit it.

The command names one role. The rule accepts exactly when its derived
permission holds, otherwise rejects with reason 200. Requests keep every
stored field unchanged. Laws 502 and 503 check both directions of that
decision. The policy's `permitted` variable contains law 502's circuit
expansion.

`post_permission` states the same closure over the post-state for state
lemmas and genesis checks. Genesis is a two-parent chain with only role 0
granting the permission, so role 2 inherits it after the third round. The
lemma keeps that permission on every run of this request-only model.
Decision fixtures are LLM-generated expectations; the separate source-rule
reference supplies the exhaustive comparison.

## Difference

An expanded scalar predicate lists each possible ancestor path. The derived
rule names inheritance once and gives a definite least result even on
cycles. The compiler shares round values, and expansion exposes the exact
graph used by the finite judges.

## Scope

The declared domain is 512 parent/grant states, three commands and one
context: 1,536 input tuples. Full review has no law refusals or example
disagreements; the gap judge determines every tuple. The state lemma is
inductive from its declared genesis; this request-only model has one reachable
state. These results are exact over the declared finite domains. This example
has one permission and does not model changes to roles, external identity or
execution of a privileged operation.
