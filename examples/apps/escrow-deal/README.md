# Escrow deal

"An escrow app: a buyer deposits, the seller marks it shipped, then the buyer
releases the money or the operator refunds it; money is never created or
lost." The operator may refund only after day 7, the seller's shipping
deadline.

## The journey

One server serves three people, each with their own credential. The buyer
signs in and deposits 2 units on day 1. The seller signs in and marks the
goods shipped on day 3; a refund from the seller is refused, because only
the operator refunds. On day 5 the operator's refund is refused: the
deadline has not passed. On day 8 it is accepted, and the buyer's refund of
2 units is delivered. Had the buyer released the money instead, the
seller's payout would have been delivered. Everyone sees the same deal, and
the contract refuses whatever their own credential does not permit. The
server sets the day; nobody types it.

## The contract

This directory is a contract: `project.zeno`, `v2/policy.json`, `ui.json`
and `tests/decision-examples.txt`. `parties/` holds an example parties file.

- The state is one deal: its stage (Open, Funded, Shipped, Released or
  Refunded), the units held, the units paid to the seller and the units
  refunded to the buyer. A deposit is 1 to 3 units.
- A command is Deposit, Mark shipped, Release or Refund, with an amount that
  only Deposit reads. The context is the party, Buyer, Seller or Operator,
  and the day, 0 to 14, which the server sets as the declared
  [server clock](../../../docs/CONTRACT_RULES.md#server-clock), counted in
  days from the epoch 2026-11-01.
- The rules, first match decides: the buyer deposits and releases, the
  seller marks shipped and the operator refunds (reason 200); each step
  needs its stage (reason 201); the operator refunds only after day 7
  (reason 202). A refund is allowed when the deal is funded or shipped, so
  a deposit never sticks. Release pays the held units on the
  seller channel; refund pays them on the buyer channel.
- The laws: each stage holds exactly the money it should; money is
  conserved, and only a deposit adds to the total; the right party takes
  each step; the operator refunds only after day 7; and every unit paid out
  is an actual delivery on that payee's channel, with at most one delivery
  per decision.

## Check and build

From the ZenoFCIS source checkout, with the CLI built:

```sh
target/debug/zeno-fcis check examples/apps/escrow-deal/project.zeno \
  --require-substantive --require-resolved-paths
target/debug/zeno-fcis contract review examples/apps/escrow-deal
target/debug/zeno-fcis contract lemma examples/apps/escrow-deal \
  --lemma 'post.100.112 == 0 || post.100.113 == 0' \
  --lemma 'post.100.111 + post.100.112 + post.100.113 <= 3'
target/debug/zeno-fcis new escrow-deal --contract examples/apps/escrow-deal --app
```

The review enumerates all 172,800 inputs, every day included: the 13
examples agree with the contract, no law refuses a decision from a lawful
state, and 107 of 118 rule mutants are distinguished. The lemma judge finds both lemmas, never paid and
refunded and never more than 3 units, inductive over the declared domain.
`tools/check_app_factory.py --tcp` runs the three-party journey in
`test-data/app-journeys/escrow-deal.json` on one server, with a test clock,
and `tools/check_app_controls.py` plants the clock defects its tests catch.

## Run with three parties

In the generated application, copy the example parties file and create one
private credential file per party. Credential files never belong in version
control; `parties/.gitignore` excludes them.

```sh
cd escrow-deal
mkdir run
cp ../examples/apps/escrow-deal/parties/*.json run/
for name in buyer seller operator; do
  python3 -c 'import os, secrets, sys
fd = os.open(sys.argv[1], os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
with os.fdopen(fd, "w") as stream:
    stream.write(secrets.token_hex(32) + "\n")' "run/$name.credential"
done
echo 1 > run/day
ZENO_APP_PARTIES=run/parties.json ZENO_APP_CLOCK_FILE=run/day \
  cargo +1.97.1 run --locked --offline --bin app-server
```

`ZENO_APP_CLOCK_FILE` sets the day for trying the example; write another
number to the file to move it. Without it the server counts days from the
epoch by its system clock, so after 2026-11-15 every new request is refused
with `Clock`.

Adjust the `cp` source to where your checkout is. Give each person their own
credential. The example gives the seller a view without the buyer's refunds:
the seller's page and replies leave that field out, as display filtering
only. The credential a request presents selects that party's
principal and context; nothing in the request can choose another party.
[Build an app](../../../docs/BUILD_AN_APP.md#several-parties-on-one-server)
describes the parties file and the HTTP API.

## Scope

- One deal per store, with small amounts and a two-week day range, so the
  review can enumerate every input.
- Deliveries go to the example file receiver, which keeps each payload with
  an envelope naming its destination, `seller` or `buyer`. Paying anyone
  needs a reviewed transport, and a successful delivery does not show that a
  payment settled.
- The decision examples were LLM-generated together with the rules, so they
  are not an independent oracle.
