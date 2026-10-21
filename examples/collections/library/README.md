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

The copy's holder is a stored key: in the view it is a key class, equal or
not to the member's. The checker found that a return with a loan count of 0
would fail, because no per-entry law links a member's count to the copies it
holds; the return rule now refuses it.

## Scope

Exhaustively checked over 160 admissible views, given the collections
premises. Cross-map invariants, such as a count equal to the copies held, are
not expressible per entry.
