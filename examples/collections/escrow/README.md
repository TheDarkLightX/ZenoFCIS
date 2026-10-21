# Escrow with many deals

## Journey

The ask: "a buyer opens a deal with a seller and funds it, the seller ships,
then the buyer releases the money or the seller refunds it; money is never
created or lost." Deals are keyed by opaque deal IDs, and the caller is a
party key in the context.

```text
zeno-fcis contract collections examples/collections/escrow
```

## Formal model

`deals` maps a `DealId` to a stage, an amount from 1 to 2, the amount
escrowed, a buyer and a seller. A command names one deal (entry 160).
`summary 120 held` is the escrow across all deals, from 0 to 4. Per-entry
law 500 ties the escrowed amount to the stage, and law 501 keeps the buyer
and seller different; laws 502 and 503 say who may act.

## Difference

The checker covers every equality pattern of four party keys, the deal's
buyer and seller, the command's seller and the caller: 15 patterns.

## Scope

Exhaustively checked over 4,500 admissible views, given the collections
premises. The summary's range bounds the escrow across all deals.
