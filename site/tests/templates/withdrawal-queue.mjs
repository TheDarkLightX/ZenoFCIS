// The withdrawal-queue examples: how tests/decision-examples.txt is read,
// how each example's vault is reached from genesis, and which laws each
// decision kind evaluates. From project.zeno: actions, lanes, lane statuses,
// and callers by numeric ID; and the laws profile.rs enforces on each
// decision kind (500 to 503 on every commit, 508 on the committed failures
// the application never makes, 509 on rejections).

import { fail, integer, kind, lines, named, reason } from "../examples.mjs";

const ACTIONS = { 160: "Deposit", 161: "RequestWithdrawal", 162: "Tick" };
const LANES = { 170: "A", 171: "B" };
const STATUSES = { 180: "Empty", 181: "Arrived", 182: "Pending" };
const CALLERS = { 190: "Operator", 191: "OwnerA", 192: "OwnerB", 193: "Keeper" };

export const laws = { Accept: [500, 501, 502, 503], CommittedFailure: [500, 508], Reject: [509] };

// Inputs refused before any decision, with the stage that refuses them.
export const refusals = [
  ["{", "input"],
  ['{"command":"Deposit","amount":3,"caller":"Operator","alarm":false}', "admission"],
];

// Examples whose vault no sequence of requests reaches from genesis, with
// the reason; the template's conformance test decides them from the
// admitted vault instead.
export const unreachable = {
  20: "lane A pending with no pause and no must-serve while lane B is the priority lane: only a tick that paid lane A makes lane B the priority, and that tick empties lane A",
};

function vault([balance, laneA, amountA, laneB, amountB, pause, mustServe, priority], line) {
  return {
    balance: integer(balance, line),
    lane_a: named(STATUSES, laneA, line),
    amount_a: integer(amountA, line),
    lane_b: named(STATUSES, laneB, line),
    amount_b: integer(amountB, line),
    pause: integer(pause, line),
    must_serve: mustServe === "1",
    priority: named(LANES, priority, line),
  };
}

// Eight vault fields, then `action lane amount caller alarm | outcome reason`
// and the eight fields after `| payout`, as tests/conformance.rs parses it.
// A deposit ignores the lane and a tick ignores the lane and the amount,
// which the page never sends with them.
export function parse(text) {
  return lines(text).map(({ line, parts: [input, decision, payout] }) => {
    const [action, lane, amount, caller, alarm] = input.slice(8);
    const command = named(ACTIONS, action, line);
    const request = { command, caller: named(CALLERS, caller, line), alarm: alarm === "1" };
    if (command === "RequestWithdrawal") request.lane = named(LANES, lane, line);
    if (command !== "Tick") request.amount = integer(amount, line);
    return {
      line,
      pre: vault(input.slice(0, 8), line),
      request,
      expected: {
        kind: kind(decision[0], line),
        reason: reason(decision[1]),
        post: vault(decision.slice(2, 10), line),
        entries: payout[0] === "-" ? [] : [{ channel: integer(payout[0], line), payload: { paid_lane: named(LANES, payout[1], line), paid_amount: integer(payout[2], line) } }],
      },
    };
  });
}

const deposit = (amount) => ({ command: "Deposit", amount, caller: "Operator", alarm: false });
const request = (lane, amount) => ({ command: "RequestWithdrawal", lane, amount, caller: lane === "A" ? "OwnerA" : "OwnerB", alarm: false });
const tick = (alarm) => ({ command: "Tick", caller: "Keeper", alarm });

// Requests that take an empty vault to each example's vault, written from
// the README's rules: a deposit adds to the balance; a request records an
// arrival; an honored alarm pauses for its tick and two more; a pause that
// ends with a lane due sets must-serve; a paying tick empties the paid lane
// and moves priority to the other. Keyed by the eight fields. The replay
// checks the result.
const PATHS = {
  "0 Empty 0 Empty 0 0 0 A": [],
  "1 Empty 0 Empty 0 0 0 A": [deposit(1)],
  "3 Empty 0 Empty 0 0 0 A": [deposit(2), deposit(1)],
  "4 Empty 0 Empty 0 0 0 A": [deposit(2), deposit(2)],
  "3 Arrived 2 Empty 0 0 0 A": [deposit(2), deposit(1), request("A", 2)],
  "4 Arrived 2 Empty 0 0 0 A": [deposit(2), deposit(2), request("A", 2)],
  "4 Arrived 2 Arrived 2 0 0 A": [deposit(2), deposit(2), request("A", 2), request("B", 2)],
  "4 Pending 2 Pending 2 2 0 A": [deposit(2), deposit(2), request("A", 2), request("B", 2), tick(true)],
  "4 Pending 2 Pending 2 1 0 A": [deposit(2), deposit(2), request("A", 2), request("B", 2), tick(true), tick(true)],
  "4 Pending 2 Pending 2 0 1 A": [deposit(2), deposit(2), request("A", 2), request("B", 2), tick(true), tick(true), tick(false)],
  "2 Empty 0 Pending 2 0 0 B": [deposit(2), deposit(2), request("A", 2), request("B", 2), tick(true), tick(true), tick(false), tick(true)],
  "2 Empty 0 Pending 2 0 1 B": [deposit(2), deposit(2), request("A", 2), request("B", 2), tick(true), tick(true), tick(false), tick(true), tick(true), tick(true), tick(false)],
  "2 Empty 0 Pending 2 1 0 A": [deposit(2), request("B", 2), tick(true), tick(true)],
  "1 Empty 0 Empty 0 1 0 B": [deposit(2), deposit(1), request("A", 2), tick(false), tick(true), tick(false)],
  "1 Empty 0 Empty 0 0 0 B": [deposit(2), deposit(1), request("A", 2), tick(false)],
  "2 Pending 2 Empty 0 0 0 A": [deposit(2), request("A", 2), tick(false), deposit(2), deposit(2), request("A", 2), request("B", 2), tick(false)],
};

export function reach(pre) {
  const key = [pre.balance, pre.lane_a, pre.amount_a, pre.lane_b, pre.amount_b, pre.pause, pre.must_serve ? 1 : 0, pre.priority].join(" ");
  return PATHS[key] ?? fail(`no path from genesis is written for the vault ${key}`);
}
