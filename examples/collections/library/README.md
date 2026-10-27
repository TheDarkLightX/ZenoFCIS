# Library members and copies

## Journey

The ask: "members borrow and return copies of a book, a member holds at most
two copies, and no copy is lent twice." Members and copies are maps keyed by
opaque IDs, so the library grows without changing the contract.

```text
zeno-fcis contract collections examples/collections/library
zeno-fcis contract review examples/collections/library
```

## Formal model

`members` maps a `MemberId` to a loan count from 0 to 2; `copies` maps a
`CopyId` to whether it is on loan and its holder, a `MemberId`. A command
names one member (entry 160) and one copy (entry 170). Join and AddCopy
insert entries; Borrow needs a member, a copy on the shelf and a count below
two (reasons 202 to 205); Return needs the holder (206). Laws 502 and 503
record and check the holder.

## Difference

The copy's holder is a stored key. `tally 510 loans_match_copies 110.140 =
count 115 by 146 where 145;` states that, for every member key, its count
equals the number of on-loan copies held by that member. Both maps' changes
are checked together, including a holder changed to or from an unnamed key.
An on-loan copy implies a positive count for its holder, so Return needs no
additional zero-count defense. The holder check remains.

## Scope

Exhaustively checked over the declared admissible view domain, subject to the
[collections premises](../../../docs/COLLECTIONS.md). The grouped tally is an
inductive all-size claim from empty genesis, conditional on runtime framing;
it does not admit arbitrary joins or qualify inconsistent imported maps.

## Lineage

**Background.** P. Wolper, *Expressing Interesting Properties of Programs in
Propositional Temporal Logic*, POPL, 1986, supplies data-independence background.
A. Gupta, I. S. Mumick and V. S. Subrahmanian, *Maintaining Views Incrementally*,
SIGMOD, 1993, studies incremental maintenance of database views.

**What is new here.** The example uses grouped per-key delta obligations across
two maps. Prior-art search pending; no novelty claim is made.

**Scope.** The mathematical argument and its runtime premises are stated in
the collections reference. Small replay checks alone do not prove all sizes.
