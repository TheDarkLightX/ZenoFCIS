# A hierarchy fixed by policy

The ask: "keep the role hierarchy in policy data and inherit grants from it."

## Formal model

`const table 110 parents 105 size 3 [0,1,2];` fixes a three-role chain.
Zero means no parent, and j+1 names role j. Its entries are typed literals,
not state fields. The `ancestor` stratum starts false, closes the hierarchy
in at most nine rounds, and is evaluated once while the policy is parsed.
Its final Boolean lookup feeds the later permission strata. Those read
three stored grant flags, so requests still depend on the current state.

```text
zeno-fcis check examples/contract-tables/constant-role-hierarchy/project.zeno --expand
zeno-fcis contract review examples/contract-tables/constant-role-hierarchy --gaps --no-kb
zeno-fcis contract lemma examples/contract-tables/constant-role-hierarchy --lemma 'post_permission[2]' --no-kb
zeno-fcis contract probe count examples/contract-tables/constant-role-hierarchy --where 'permission[2]' --no-kb
```

## Difference

The state-table journey carries three parent fields on every request.
This version binds the effective constant lookup into the compiled formulas
and carries only grant flags. No parent values are read at invocation time.
The printed expansion exposes the exact scalar formulas used by the judges;
its JSON decision variable contains law 502's expansion.

## Scope

Eight grant states, three commands and one context give 24 input tuples.
The hierarchy is fixed for this policy; changing it requires loading a new
policy. Native comparison with the state-table version projects away the
fixed parent fields and compares class, reason, grant successor and empty
outbox on every input. Policy/schema identities intentionally differ when
the stored parent fields are removed. These results are exact over the
declared finite domains and do not authenticate roles or execute privileges.
