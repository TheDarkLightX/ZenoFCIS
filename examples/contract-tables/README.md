# Contract table examples

## Journey

Run `zeno-fcis check examples/contract-tables/library-loans/project.zeno --expand`
to see the library loans contract with its table expanded. The
[library loans example](library-loans/README.md) lends six copies of a book to
three members, at most two copies each.

The [delegation example](delegation/README.md) refuses delegation cycles.
The [role inheritance example](role-inheritance/README.md) derives a permission
from roles and their ancestors, including cyclic parent data.

## Formal model

A table declares a fixed number of state fields; table laws expand to one
ordinary formula over every entry. See [contract tables](../../docs/CONTRACT_TABLES.md)
for the exact rule. A derived table computes Boolean entries as a least fixed
point over those indices and shares the values of each bounded round.

## Difference

The example is the six-copy version of a contract that, without tables, would
list six fields and repeat each law six times.
Recursive examples replace repeated bounded path checks with a closure rule.

## Scope

Exhaustively checked over the declared finite domain of 294,912 input tuples.
The recursive examples enumerate 768 and 1,536 input tuples. Every result is
scoped to its declared finite domain and the existing resource limits.

[Permission sets](permission-words/README.md) inherit three distinct permissions as a finite Boolean algebra.

[Constant hierarchy](constant-role-hierarchy/README.md) keeps a fixed role hierarchy in policy data.
