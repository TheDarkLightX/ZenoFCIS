# Keyed collection examples

## Journey

Run `zeno-fcis contract collections examples/collections/token-balances`.
Each example holds maps of any size, keyed by opaque 32-byte identifiers, and
is checked over every admissible view of the entries its commands name:
- [token balances](token-balances/README.md): transfers and mints with a
  total supply kept as a summary;
- [library](library/README.md): members and copies, at most two loans per
  member;
- [escrow](escrow/README.md): many deals keyed by deal ID, with the funds
  held as a summary.

## Formal model

A map lowers to its named entries and summaries; keys become equality
classes. [Keyed collections](../../docs/COLLECTIONS.md) gives the view, the
admissible views and the argument that they cover every map size.

## Difference

A [fixed table](../contract-tables/README.md) enumerates all its entries; a
map enumerates only the entries a command names.

## Scope

Each result holds over the declared view ranges, given the premises in the
collections reference, including runtime framing. This part contains the
language and checker; the map runtime follows separately.
