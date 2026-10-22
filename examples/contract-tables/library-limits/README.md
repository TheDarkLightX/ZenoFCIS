# Library loans with a state limit

The ask: members borrow and return copies, with the per-member loan limit read from state.

## Formal model

Six copies record a holder from 0 (shelf) to 3. State field 160 holds a limit
from 1 to 2; commands preserve it. Law 500 compares each holder's count with
`post.100.160`. Law 505 sums `choose(post.100.110[j] == command.101.122, 1, 0)`
and compares the sum with `pre.100.160`. Select evaluates both arms eagerly,
including their traps and ordinary library work charges.

Rules read `pre.100.110[command.101.121]`, write the post key
`110[command.101.121]`, sum over the table, and use `exists` in the occupied-copy
condition. The loader expands these forms into scalar rules before binding.
Input index ranges must fit the table; no runtime indexing operation is added.

```text
zeno-fcis check examples/contract-tables/library-limits/project.zeno --expand
zeno-fcis contract review examples/contract-tables/library-limits
zeno-fcis contract lemma examples/contract-tables/library-limits --lemma 'forall i in table 110 where post.100.110[i] != 0 { count j in table 110 { post.100.110[j] == post.100.110[i] } <= post.100.160 }'
zeno-fcis contract probe decisions examples/contract-tables/library-limits
```

## Difference

The [fixed-limit example](../library-loans/README.md) keeps its existing laws,
domain and decisions. Its table-form policy expands to the same policy bytes.
This example adds a state field and conditional count, so its source and policy
identities differ. Changing the limit changes when the third rule rejects.

## Scope

The declared domain is 8,192 states, 36 commands and 2 desks: 589,824 input
tuples. The finite judges enumerate that domain. Genesis sets the limit to 2;
a state with limit 1 is declared but unreachable from that genesis. Linear
formula expansion does not reduce the finite domain. The interface still
shows each entry as a field. Decision examples are LLM-generated expectations;
they are regression evidence, not an independent specification of intent.
