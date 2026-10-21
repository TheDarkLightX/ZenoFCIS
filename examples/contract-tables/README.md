# Contract table examples

## Journey

Run `zeno-fcis check examples/contract-tables/library-loans/project.zeno --expand`
to see the library loans contract with its table expanded. The
[library loans example](library-loans/README.md) lends six copies of a book to
three members, at most two copies each.

## Formal model

A table declares a fixed number of state fields; table laws expand to one
ordinary formula over every entry. See [contract tables](../../docs/CONTRACT_TABLES.md)
for the exact rule.

## Difference

The example is the six-copy version of a contract that, without tables, would
list six fields and repeat each law six times.

## Scope

Exhaustively checked over the declared finite domain of 294,912 input tuples.
Larger tables or domains need the compact checking planned for 2.7.
