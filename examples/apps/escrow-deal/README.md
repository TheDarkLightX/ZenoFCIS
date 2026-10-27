# Escrow deal

"An escrow app: a buyer deposits, the seller marks it shipped, then the buyer
releases the money or a timeout refunds it; money is never created or
lost." Any authenticated party can return an unconfirmed payment to the buyer
after day 7. The original operator Refund command keeps the same deadline.

## The journey

One server serves three people, each with their own credential. The buyer
signs in and deposits 2 units on day 1. The seller signs in and marks the
goods shipped on day 3; the seller's original Refund command is refused,
because that command belongs to Operator. On day 5 the operator's refund is refused: the
deadline has not passed. On day 8 it is accepted, and the buyer's refund of
2 units is delivered. Had the buyer released the money instead, the
seller's payout would have been delivered. Everyone sees the same deal, and
the contract refuses whatever their own credential does not permit. The
server sets the day; nobody types it.

## The contract

This directory is a contract: `project.zeno`, `v2/policy.json`, `ui.json`
and `tests/decision-examples.txt`. `parties/` holds an example parties file.

- The state records one deal's stage, held units, seller payments and buyer refunds. Stage deadlines are fixed constants
  in the laws. Deposits contain 1 to 3 units.
- Deposit belongs to Buyer; MarkShipped belongs to Seller. Buyer can explicitly
  Release a shipped deal immediately, waiving the dispute window, as before.
  The original Refund command belongs to Operator and works after day 7.
- TimeoutRefund is available to every party after day 7 while Funded or Shipped;
  it always refunds Buyer, even when Seller invokes it. This supplies recovery
  without an available operator or a cooperative seller.
- Buyer may ConfirmDelivery while Shipped through day 7. Buyer may DisputeDelivery before day 9 for a refund;
  at or after day 9, anyone may ReleaseConfirmed, which always pays Seller.
  This fixed calendar leaves at least two days between the confirmation cutoff
  and release, within the 0..14 clock. Earlier confirmation gives a longer window.
- The server supplies the day as the declared [server clock](../../../docs/CONTRACT_RULES.md#server-clock),
  counted from 2026-11-01. Timed recovery assumes that the host keeps accepting
  requests through the deadline and authenticates each party correctly.
- Independent laws bind the action, caller, predecessor stage, exact successor,
  first failed guard, conservation, deadline, delivery count, amount and destination.
  Shipping alone cannot pay Seller, and a release cannot precede shipping.
- The reachable run properties in `v2/run-properties.json` establish at most
  one seller payout, no seller payout after a buyer refund, and at most one
  payout request per decision, over the declared finite domain.

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

The revised source has 13 shipped examples and additional timeout, dispute,
terminal-clock and repeated-request scenarios. The former 172,800-input review
count described the previous schema; it does not qualify this revision. The
repair evidence records the new native games, table attractors and replayed
certificates with their exact capabilities. The final finite-game check records each reachable held-state instance and
an honest-buyer exit without the operator.
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
epoch by its system clock. After 2026-11-15 the clock holds its final admitted
day, so late recovery stays available (see the end of this page).

Adjust the `cp` source to where your checkout is. Give each person their own
credential. The example gives the seller a view without the buyer's refunds:
the seller's page and replies leave that field out, as display filtering
only. The credential a request presents selects that party's
principal and context; nothing in the request can choose another party.
[Build an app](../../../docs/BUILD_AN_APP.md#several-parties-on-one-server)
describes the parties file and the HTTP API.

## Scope

- One deal per store, with small amounts and a two-week day range, so the
  declared state and clock bounds are explicit. No unbounded-time claim is made.
- Deliveries go to the example file receiver, which keeps each payload with
  an envelope naming its destination, `seller` or `buyer`. Paying anyone
  needs a reviewed transport, and a successful delivery does not show that a
  payment settled.
- The decision examples were LLM-generated together with the rules, so they
  are not an independent oracle.

## Background and changes

Deadline recovery and explicit lifecycle guards follow the conventional escrow
pattern and guarded transitions (E. W. Dijkstra, “Guarded commands,
nondeterminacy and formal derivation of programs”, CACM 18(8), 1975,
DOI 10.1145/360933.360975). This revision adds independent transition and delivery
laws, a payer timeout and a delivery-confirmed dispute cutoff at day 9. It makes no
novelty claim; prior-art search pending because this repair was offline.
A successful local delivery is a durable request to the receiver, not proof of
external settlement. Receiver deduplication and real-world delivery confirmation
remain deployment obligations.

The server clock holds its final admitted day after the finite horizon. Late
recovery remains available in that terminal phase; no operator clock reset is
needed. This does not automatically submit a refund or release.
