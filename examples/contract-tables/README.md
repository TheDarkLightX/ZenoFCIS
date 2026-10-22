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

Exhaustively checked over the declared finite domain of 294,912 input tuples.
The recursive examples enumerate 768 and 1,536 input tuples. Every result is
scoped to its declared finite domain and the existing resource limits.

[Permission sets](permission-words/README.md) inherit three distinct permissions as a finite Boolean algebra.

[Constant hierarchy](constant-role-hierarchy/README.md) keeps a fixed role hierarchy in policy data.
