# Escrow deal

"An escrow app: a buyer deposits, the seller marks it shipped, then the buyer
releases the money or the operator refunds it; money is never created or
lost."

## The journey

One server serves three people, each with their own credential. The buyer
signs in and deposits 2 units. The seller signs in and marks the goods
shipped; a refund from the seller is refused, because only the operator
refunds. The buyer releases the money, and the seller's payout of 2 units
is delivered. In another deal the operator refunds a funded deposit to the
buyer before shipping. Each person sees the same deal and only the commands
their own credential permits.

## The contract

This directory is a contract: `project.zeno`, `v2/policy.json`, `ui.json`
and `tests/decision-examples.txt`. `parties/` holds an example parties file.

- The state is one deal: its stage (Open, Funded, Shipped, Released or
  Refunded), the units held, the units paid to the seller and the units
  refunded to the buyer. A deposit is 1 to 3 units.
- A command is Deposit, Mark shipped, Release or Refund, with an amount that
  only Deposit reads. The context is the party: Buyer, Seller or Operator.
- The rules, first match decides: the buyer deposits and releases, the
  seller marks shipped and the operator refunds (reason 200); each step
  needs its stage (reason 201). A refund is allowed when the deal is funded
  or shipped, so a deposit never sticks. Release pays the held units on the
  seller channel; refund pays them on the buyer channel.
- The laws: each stage holds exactly the money it should; money is
  conserved, and only a deposit adds to the total; the right party takes
  each step; and every unit paid out is an actual delivery on that payee's
  channel, with at most one delivery per decision.

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

The review enumerates all 11,520 inputs: the 10 examples agree with the
contract, no law refuses a decision from a lawful state, and 98 of 107 rule
mutants are distinguished. The lemma judge finds both lemmas, never paid and
refunded and never more than 3 units, inductive over the declared domain.
`tools/check_app_factory.py --tcp` runs the three-party journey in
`test-data/app-journeys/escrow-deal.json` on one server.

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
ZENO_APP_PARTIES=run/parties.json cargo +1.97.1 run --locked --offline --bin app-server
```

Adjust the `cp` source to where your checkout is. Give each person their own
credential. The credential a request presents selects that party's
principal and context; nothing in the request can choose another party.
[Build an app](../../../docs/BUILD_AN_APP.md#several-parties-on-one-server)
describes the parties file and the HTTP API.

## Scope

- One deal per store, with small amounts, so the review can enumerate every
  input. There are no time-outs: the operator refunds when asked.
- Deliveries go to the example file receiver, which keeps each payload with
  an envelope naming its destination, `seller` or `buyer`. Paying anyone
  needs a reviewed transport, and a successful delivery does not show that a
  payment settled.
- The decision examples were LLM-generated together with the rules, so they
  are not an independent oracle.
