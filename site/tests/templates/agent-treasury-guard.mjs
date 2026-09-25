// The agent-treasury-guard examples: how tests/decision-examples.txt is
// read, how each example's treasury is reached from genesis, and which laws
// each decision kind evaluates. From project.zeno: actions, directions,
// callers, models, swap states, and assets by numeric ID; and the laws
// profile.rs enforces on each decision kind (500 to 502 on every commit,
// 503 to 507 on accepts, 508 on committed failures, 509 on rejections).

import { fail, integer, kind, lines, named, reason } from "../examples.mjs";

const ACTIONS = { 175: "ProposeSwap", 176: "SwapSettled", 177: "SwapFailed" };
const DIRECTIONS = { 173: "BuyBase", 174: "SellBase" };
const CALLERS = { 178: "Agent", 179: "Dex" };
const MODELS = { 180: "TreasuryAgentV2", 181: "TreasuryAgentV1", 182: "UnlistedModel" };
const SWAPS = { 170: "NoSwap", 171: "PendingBuy", 172: "PendingSell" };
const ASSETS = { 183: "Quote", 184: "Base" };

export const laws = {
  Accept: [500, 501, 502, 503, 504, 505, 506, 507],
  CommittedFailure: [500, 501, 502, 508],
  Reject: [509],
};

// Inputs refused before any decision, with the stage that refuses them.
export const refusals = [
  ["{", "input"],
  ['{"command":"ProposeSwap","direction":"BuyBase","amount":4,"min_out":1,"caller":"Agent","now":1,"price":1,"price_time":1,"model":"TreasuryAgentV2"}', "admission"],
];

// Examples whose treasury no sequence of requests reaches from genesis, with
// the reason; the template's conformance test decides them from the
// admitted treasury instead.
export const unreachable = {
  23: "spent_today 4 with no swap outstanding at tick 3: the budget takes two proposals within day 0, each answered by the DEX before the next, and the second answer could not commit before tick 4, which starts a new day",
};

function treasury([quote, base, spent, seen, pending, amount, minOut], line) {
  return {
    quote: integer(quote, line),
    base: integer(base, line),
    spent_today: integer(spent, line),
    last_seen: integer(seen, line),
    pending: named(SWAPS, pending, line),
    pending_amount: integer(amount, line),
    pending_min_out: integer(minOut, line),
  };
}

// Seven treasury fields `| action direction amount min_out intent amount_out
// | caller now price price_time model | outcome reason` and the seven
// fields after `| request`, as tests/conformance.rs parses it. An action
// ignores the fields it does not use, which the page never sends with it.
export function parse(text) {
  return lines(text).map(({ line, parts: [state, input, context, decision, queued] }) => {
    const [action, direction, amount, minOut, intent, amountOut] = input;
    const [caller, now, price, priceTime, model] = context;
    const command = named(ACTIONS, action, line);
    const request = {
      command,
      caller: named(CALLERS, caller, line),
      now: integer(now, line),
      price: integer(price, line),
      price_time: integer(priceTime, line),
      model: named(MODELS, model, line),
    };
    if (command === "ProposeSwap") {
      Object.assign(request, { direction: named(DIRECTIONS, direction, line), amount: integer(amount, line), min_out: integer(minOut, line) });
    } else {
      request.intent = integer(intent, line);
      if (command === "SwapSettled") request.amount_out = integer(amountOut, line);
    }
    const entries = queued[0] === "-" ? [] : [{
      channel: 300,
      payload: {
        intent_number: integer(queued[0], line),
        asset_in: named(ASSETS, queued[1], line),
        asset_out: named(ASSETS, queued[2], line),
        amount_in: integer(queued[3], line),
        min_amount_out: integer(queued[4], line),
        deadline: integer(queued[5], line),
      },
    }];
    return {
      line,
      pre: treasury(state, line),
      request,
      expected: { kind: kind(decision[0], line), reason: reason(decision[1]), post: treasury(decision.slice(2, 9), line), entries },
    };
  });
}

const V2 = "TreasuryAgentV2";
const propose = (direction, amount, minOut, now, price) => ({ command: "ProposeSwap", direction, amount, min_out: minOut, caller: "Agent", now, price, price_time: now, model: V2 });
const settled = (intent, amountOut, now, price) => ({ command: "SwapSettled", intent, amount_out: amountOut, caller: "Dex", now, price, price_time: now, model: V2 });
const failed = (intent, now, price) => ({ command: "SwapFailed", intent, caller: "Dex", now, price, price_time: now, model: V2 });

// Messages that take the genesis treasury to each example's treasury, along
// the README demonstration's own path: a buy at tick 1 settled at tick 2, a
// sell at tick 3 that fails at tick 4, a buy at tick 5 that fails at tick 6,
// a buy at tick 7 settled at tick 8, and a sell at tick 9. Keyed by the
// seven fields. The replay checks the result.
const BUY_1 = [propose("BuyBase", 2, 2, 1, 1)];
const SETTLED_2 = [...BUY_1, settled(1, 2, 2, 1)];
const SELL_3 = [...SETTLED_2, propose("SellBase", 1, 2, 3, 2)];
const FAILED_4 = [...SELL_3, failed(3, 4, 2)];
const FAILED_6 = [...FAILED_4, propose("BuyBase", 2, 2, 5, 1), failed(5, 6, 1)];
const SETTLED_8 = [...FAILED_6, propose("BuyBase", 2, 2, 7, 1), settled(7, 3, 8, 1)];
const PATHS = {
  "6 1 0 0 NoSwap 0 0": [],
  "4 1 2 1 PendingBuy 2 2": BUY_1,
  "4 3 2 2 NoSwap 0 0": SETTLED_2,
  "4 2 4 3 PendingSell 1 2": SELL_3,
  "4 3 0 4 NoSwap 0 0": FAILED_4,
  "4 3 2 6 NoSwap 0 0": FAILED_6,
  "2 6 0 8 NoSwap 0 0": SETTLED_8,
  "2 3 3 9 PendingSell 3 3": [...SETTLED_8, propose("SellBase", 3, 3, 9, 1)],
};

export function reach(pre) {
  const key = [pre.quote, pre.base, pre.spent_today, pre.last_seen, pre.pending, pre.pending_amount, pre.pending_min_out].join(" ");
  return PATHS[key] ?? fail(`no path from genesis is written for the treasury ${key}`);
}
