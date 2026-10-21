# Token balances

## Journey

The ask: "the issuer mints to an account, anyone transfers between accounts,
and the total supply changes only by minting." Accounts are keyed by opaque
IDs, with no limit on their number below the map's capacity of 1,048,576.

```text
zeno-fcis contract collections examples/collections/token-balances
zeno-fcis contract review examples/collections/token-balances
```

## Formal model

`map 110 100 accounts 103 104` maps an `AccountId` to an account record
with a balance from 0 to 3. A command names two accounts, the sender
(entry 160) and the receiver (entry 170); in the view, 160 and 170 say whether
each exists and 161 and 171 are their balances. `summary 120 supply` is the
sum of all balances, from 0 to 6. The rules refuse a mint by anyone but the
issuer (200), a transfer from a missing account (201), a transfer above the
balance (202), and anything above the account or supply limit (203). A
transfer to oneself changes nothing. Law 501, per entry, says an absent
account holds nothing.

## Difference

The two keys may name one account. The checker covers that equality pattern:
if the self-transfer rule credited the receiver, the summary delta would
fail there, with the supply unchanged and the balance up.

## Scope

Exhaustively checked over 2,304 admissible views, given the collections
premises. The supply's range of 0 to 6 bounds the total over all accounts;
a wider range enlarges the view domain.
