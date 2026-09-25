// The inventory-reservation examples: how tests/decision-examples.txt is
// read, how each example's stock is reached from genesis, and which laws
// each decision kind evaluates. From project.zeno: actions by numeric ID;
// and the laws profile.rs enforces on each decision kind (500 to 502 on
// accepts, 508 on the committed failures the application never makes, 509
// on rejections).

import { integer, kind, lines, named, reason } from "../examples.mjs";

const ACTIONS = { 150: "Reserve", 151: "Release", 152: "Ship", 153: "Restock" };
const LARGEST_QUANTITY = 3;

export const laws = { Accept: [500, 501, 502], CommittedFailure: [500, 508], Reject: [509] };

// Inputs refused before any decision, with the stage that refuses them.
export const refusals = [
  ["{", "input"],
  ['{"command":"Ship","quantity":4,"authorized":true}', "admission"],
];

// Every example's stock is reachable from genesis.
export const unreachable = {};

// `pre.110 pre.111 action quantity authorized | outcome reason post.110
// post.111 | shipment`, as tests/conformance.rs parses it.
export function parse(text) {
  return lines(text).map(({ line, parts: [input, decision, shipment] }) => {
    const [available, reserved, action, quantity, authorized] = input;
    const [outcome, why, postAvailable, postReserved] = decision;
    const stock = (free, held) => ({ available: integer(free, line), reserved: integer(held, line) });
    return {
      line,
      pre: stock(available, reserved),
      request: { command: named(ACTIONS, action, line), quantity: integer(quantity, line), authorized: authorized === "1" },
      expected: {
        kind: kind(outcome, line),
        reason: reason(why),
        post: stock(postAvailable, postReserved),
        entries: shipment[0] === "-" ? [] : [{ channel: integer(shipment[0], line), payload: { shipped_units: integer(shipment[1], line) } }],
      },
    };
  });
}

// Commands that take an empty shelf to the example's stock: the reserved
// units are restocked and reserved in lots of at most 3, then the available
// units are restocked, so that neither field passes its capacity of 5 on the
// way. The replay checks the result.
export function reach({ available, reserved }) {
  const steps = [];
  const move = (command, quantity) => steps.push({ command, quantity, authorized: true });
  for (let left = reserved; left > 0; left -= LARGEST_QUANTITY) {
    const lot = Math.min(left, LARGEST_QUANTITY);
    move("Restock", lot);
    move("Reserve", lot);
  }
  for (let left = available; left > 0; left -= LARGEST_QUANTITY) {
    move("Restock", Math.min(left, LARGEST_QUANTITY));
  }
  return steps;
}
