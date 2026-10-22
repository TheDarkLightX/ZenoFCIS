# Library loans with a table of copies

## Journey

The ask: "members borrow and return copies of a book, a member holds at most
two copies, and no copy is lent twice." The librarian desk records each loan
for a member. Check, expand and review the contract:

```text
zeno-fcis check examples/contract-tables/library-loans/project.zeno --require-substantive --require-resolved-paths
zeno-fcis check examples/contract-tables/library-loans/project.zeno --expand
zeno-fcis contract review examples/contract-tables/library-loans
zeno-fcis new library-app --contract examples/contract-tables/library-loans --app
```

`contract lemma --lemma` accepts law 500's table-form formula directly:

```text
zeno-fcis contract lemma examples/contract-tables/library-loans --lemma 'forall i in table 110 where post.100.110[i] != 0 { count j in table 110 { post.100.110[j] == post.100.110[i] } <= 2 }'
```

## Formal model

`table 110 100 copies 105 size 6;` declares the state fields 110 to 115,
`copies_0` to `copies_5`, each the holder of one copy: 0 on the shelf, or a
member from 1 to 3. The command names a copy with the index type
`type 106 index CopyNumber of 110;`, the integers 0 to 5, and a member.

- Law 500: for every copy on loan, at most two copies, itself included, share
  its holder (`forall` with a `count`).
- Law 501: only the librarian desk records loans.
- Law 502: every copy but the chosen one keeps its holder (`forall` with
  `where i != command.101.121`).
- Law 503: a borrow takes the chosen copy from the shelf to the member, and a
  return takes it from that member back to the shelf.

The rules in `v2/policy.json` refuse a visitor desk (200), a copy on loan
(201), a third copy for one member (202) and a return by anyone but the
holder (203). They name the entry fields 110 to 115 directly.

## Difference

Without tables the same contract lists six fields and spells out each law per
copy; "at most two per member" becomes a condition on every three copies.
The expansion is that text, so the checked contract is the same.

## Scope

The domain is 4,096 states, 36 commands and 2 desks: 294,912 input tuples,
which every finite check enumerates. Law 500 is inductive: 2,074 states
satisfy it and all of them are reachable from the empty shelf. These results
hold over the declared finite domain only. The generated interface shows the
six copies as six fields.
