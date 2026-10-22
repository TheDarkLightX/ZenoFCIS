# Contract tables

## Journey

The ask: "members borrow and return copies of a book, a member holds at most
two copies, and no copy is lent twice." With six copies, the state needs one
holder per copy, and every law is about each copy, or about how many copies
one member holds. Written field by field, that is six fields and laws that
repeat themselves six times. A table says it once:

```text
type 106 index CopyNumber of 110;
table 110 100 copies 105 size 6;
field 121 101 copy 106;
law 500 at_most_two_per_member on commit, genesis =
  forall i in table 110 where post.100.110[i] != 0 {
    count j in table 110 { post.100.110[j] == post.100.110[i] } <= 2
  };
law 502 other_copies_unchanged on commit =
  forall i in table 110 where i != command.101.121 { post.100.110[i] == pre.100.110[i] };
```

The [library loans example](../examples/contract-tables/library-loans/README.md)
is this contract in full. To see exactly what is checked, print the expansion:

```text
zeno-fcis check examples/contract-tables/library-loans/project.zeno --expand
```

It prints the project with every table form replaced by the declarations and
formulas it stands for, and reports the size of the input domain on stderr:
10 inputs and 294,912 input tuples for the example. `--format json` puts the
same text, the tables and every input's number of values in one document.
The command parses the printed text again and prints nothing unless it gives
the same canonical project as the original.

Everything else is unchanged. `check`, `generate contract`, `contract review`,
`contract lemma`, the other judges and `new --contract` read the table source
and see its expansion. The rules in `v2/policy.json` and `ui.json` name the
entry fields by their IDs, 110 to 115 in the example, as they name any field.
A `--lemma` text may not contain braces, so a lemma over a table is the
formula `--expand` prints for it.

## Formal model

Expansion happens while the project is parsed, before elaboration, so a
project with tables and its printed expansion have the same canonical bytes,
the same identity and the same checks. The rules are:

- `table ID OWNER name TYPE size N;` declares, for each entry k from 0 to N-1,
  the field `ID + k` of record OWNER, named `name_k`, of type TYPE. N is 1 to
  64. Entry IDs follow from the table's ID alone, not from the order of the
  declarations. The IDs `ID` to `ID + N - 1` must be free: a table is refused
  when any of them is the ID of another declaration of any kind, or of the
  project.
- `type ID index Name of T;` is `type ID int Name in 0..=N-1;` for a table T
  of N entries.
- `ROOT.OWNER.T[k]` is `ROOT.OWNER.(T + k)`. The index k is an integer literal
  or an index variable that a table form binds; it must lie in 0..N-1.
- `forall i in table T { F }` is `F_0 && F_1 && ... && F_N-1`, joined left to
  right, where F_k is F with i replaced by the integer k. With `where C`, each
  entry is `(C_k -> F_k)`.
- `exists i in table T { F }` is `F_0 || ... || F_N-1`; with `where C`, each
  entry is `(C_k && F_k)`.
- `choose(C, A, B)` is a scalar Select value: evaluate C, A and B eagerly,
  then select A when C is true and B otherwise. Both arms must have the
  same scalar kind. An overflow, bad cast or division trap in the unselected
  arm still refuses evaluation. It lowers to the existing library Select:
  the condition, both arms and Select incur their ordinary node/work charges,
  including work measured before a refusal; there is no branch work discount.
- `sum i in table T { V }` is `V_0 + ... + V_N-1`, joined left to right.
  `where C` keeps static index/constant filtering compatible with earlier
  sources. When C reads an input, each entry becomes `choose(C_k, V_k, 0)`;
  V_k is eager even when C_k is false.
- A count in a value position, including one compared with a state or command
  value, is `sum i in table T { choose(C, 1, 0) }`. It has N Select terms
  and N-1 additions, with the condition evaluated once per entry. For example:
  `count j in table 110 { post.100.110[j] == command.101.122 } <= pre.100.120`.
  Counts can also appear on the right of a comparison or inside arithmetic.
- For compatibility, an isolated `count i in table T { C } OP K`, with OP one of `<=`, `<`, `>=`, `>`, `==`,
  `!=` and K an integer constant, compares the number of entries for which C
  holds with K. `count <= K` is the conjunction, over every set of K+1 entries
  in ascending order, of `!(C_a && C_b && ...)`. `count >= K` is the
  disjunction, over every set of K entries, of `(C_a && C_b && ...)`.
  `< K` is `<= K-1`, `> K` is `>= K+1`, `== K` is `>= K && <= K`, and `!= K`
  is its negation. Each clause list is joined in a balanced tree, left half
  first. A count beyond 0..N is `true` or `false` as the comparison requires,
  and a count whose conditions read only constants is evaluated.

So the example's law 500 is six implications, one per copy k: when field
110+k is not 0, none of the 20 sets of three copies all have copy k's holder.

Errors point at the table source: at the `table` declaration, the `[` of an
entry, or the start of a table form. A diagnostic inside a form's body is
reported once, not once per entry. A table of more than 64 entries, or an
expansion that would exceed the formula-node limit of 1,000,000, stops with
the resource-limit diagnostic `ZENO-E0207` at the form; for a `count`, the
diagnostic gives the number of clauses it would need.

### What a count costs

`count i in table T { C } <= K` is C(N, K+1) clauses of K+1 copies of C,
over a table of N entries; a count inside a `forall` is expanded once per
entry of the `forall`. `check --expand` prints each count's clauses and how
many times it is expanded, so the cost is visible before it reaches the limit:

| N | K | Clauses for `<= K` |
|---|---|---|
| 6 | 1 | 15 |
| 6 | 2 | 20 |
| 16 | 2 | 560 |
| 32 | 2 | 4,960 |
| 64 | 1 | 2,016 |
| 64 | 2 | 41,664 |
| 64 | 3 | 635,376 |

`>= K` costs C(N, K) clauses of K copies, and `==` and `!=` cost both.
These constant-bound forms retain their existing expansion and identities.
For a linear constant-bound form, write
`sum i in table T { choose(C, 1, 0) } <= K` explicitly. At N = 6, 16 and
64 it has 6, 16 and 64 Select terms instead of 20, 560 and 41,664 clauses
for K = 2. A nonconstant-bound count uses this linear form automatically.
`check --expand` distinguishes Select terms from combinatorial clauses.

## Difference

Before tables, a contract with six copies spelled out six fields and repeated
each law per copy; "at most two per member" had to be written as a condition
on every triple of copies. Tables change only how the project is written.
Existing sources retain their expansion and canonical bytes. Select adds a
new scalar encoding tag, leaving earlier tags intact; projects that use it
have the identity of that expanded formula. It uses the existing library
Select evaluator. Finite results remain scoped to the declared domain.

## Scope

- A table is a fixed number of fields. Its entries count toward the 64
  decision inputs a contract may have, and every check still enumerates the
  whole expanded domain. The example's 294,912 tuples are within the default
  cap of 1,048,576 input tuples of the finite checks. Beyond the cap,
  `contract review` reports bounded probes instead of the whole domain and
  the judges are inconclusive, as for any contract.
- `count` grows with the number of entry sets: C(N, K+1) clauses for
  `<= K`. It suits small tables and small bounds.
- In law formulas an entry index is a literal or a bound index variable,
  not an input. A dynamic entry is written as a `forall` with
  `where i == command...`, as in the example.
- `v2/policy.json` and `ui.json` have no table forms; they use entry IDs,
  and the generated interface shows each entry as its own field.
- Linear formulas reduce expansion size, not the finite input domain.
  Checking large domains without enumeration remains separate work.
