// The agent-treasury-guard example, as the page presents it: its state
// fields with plain labels, the context and commands in the README's words,
// the reasons in plain words, and the README's scripted demonstration: the
// twenty-three messages that the scripted agent and the DEX send in
// `journey` in the template's src/lib.rs, each with the story the template
// tells and the decision the guard must reach. No model is called and no
// network is used: the agent is a fixed list of proposals. The interrupted
// delivery and the database reopen that follow them need the SQLite shell
// and are not part of the page.

const intent = { name: "intent", label: "Intent number", hint: "the tick it was proposed at, 0 to 11", kind: "integer", min: 0, max: 11, initial: 1 };

const context = (caller, now, price, priceTime, model) => ({ caller, now, price, price_time: priceTime, model });
const propose = (direction, amount, minOut) => ({ command: "ProposeSwap", direction, amount, min_out: minOut });
const settled = (intentNumber, amountOut) => ({ command: "SwapSettled", intent: intentNumber, amount_out: amountOut });
const failed = (intentNumber) => ({ command: "SwapFailed", intent: intentNumber });
const step = (now, caller, model, price, priceTime, command, note, expect) => ({
  request: { ...command, ...context(caller, now, price, priceTime, model) },
  expect,
  note,
});
const V2 = "TreasuryAgentV2";
const WHO = { Agent: "The agent", Dex: "The exchange" };

export const template = {
  name: "agent-treasury-guard",
  fields: [
    { name: "quote", label: "Quote held" },
    { name: "base", label: "Base held" },
    { name: "spent_today", label: "Spent today" },
    { name: "last_seen", label: "Last commit's tick" },
    { name: "pending", label: "Outstanding swap" },
    { name: "pending_amount", label: "Outstanding amount" },
    { name: "pending_min_out", label: "Outstanding minimum" },
  ],
  help: "Set the context the shell would supply, then send the agent's proposal or the exchange's answer. The guard decides what the treasury commits.",
  contextLegend: "The request's context",
  context: [
    { name: "caller", label: "Caller", kind: "choice", options: [["Agent", "The agent"], ["Dex", "The exchange (DEX)"]], initial: "Agent" },
    { name: "now", label: "Tick now", hint: "0 to 11; a day is 4 ticks", kind: "integer", min: 0, max: 11, initial: 1 },
    { name: "price", label: "Oracle price", hint: "quote per base, 1 or 2", kind: "integer", min: 1, max: 2, initial: 1 },
    { name: "price_time", label: "Price seen at tick", hint: "0 to 11", kind: "integer", min: 0, max: 11, initial: 1 },
    { name: "model", label: "Model the agent runs", kind: "choice", options: [[V2, "TreasuryAgentV2 (approved)"], ["TreasuryAgentV1", "TreasuryAgentV1"], ["UnlistedModel", "UnlistedModel"]], initial: V2 },
  ],
  groups: [
    {
      legend: "The agent proposes a swap",
      commands: [
        {
          name: "ProposeSwap",
          label: "Propose the swap",
          fields: [
            { name: "direction", label: "Direction", kind: "choice", options: [["BuyBase", "Buy base with quote"], ["SellBase", "Sell base for quote"]], initial: "BuyBase" },
            { name: "amount", label: "Amount sold", hint: "1 to 3", kind: "integer", min: 1, max: 3, initial: 2 },
            { name: "min_out", label: "Least amount to receive", hint: "0 to 3", kind: "integer", min: 0, max: 3, initial: 2 },
          ],
        },
      ],
    },
    {
      legend: "The exchange replies",
      commands: [
        { name: "SwapSettled", label: "Settled", fields: [intent, { name: "amount_out", label: "Amount delivered", hint: "0 to 3", kind: "integer", min: 0, max: 3, initial: 2 }] },
        { name: "SwapFailed", label: "Failed", fields: [intent] },
      ],
    },
  ],
  genesis: "6 quote, 1 base, nothing spent, tick 0, no swap outstanding, nothing queued",
  outbox: "Swap requests to zenodex on channel 300 (swap_request), shaped after ZenoDEX's SwapIntent: the intent number, the assets in and out, the amount in, the least amount out, and the deadline. Nothing delivers in this page, so each stays pending with its delivery identity.",
  // The scripted agent's proposals, shown on their own: the guard refuses
  // most of them.
  proposer: {
    label: "The scripted agent's proposals",
    help: "The agent is a fixed list of proposals, good and bad, from the template's demonstration; no model is called. Each line shows what the agent proposes and what the guard decided in the script. Send one to see the guard decide it against the treasury as it is now.",
    filter: (step) => step.request.caller === "Agent",
  },
  // The request in plain words.
  describe: (request) => {
    const who = WHO[request.caller] ?? request.caller;
    const when = `at tick ${request.now}, price ${request.price} seen at tick ${request.price_time}${request.model === V2 ? "" : `, model ${request.model}`}`;
    if (request.command === "ProposeSwap") {
      const swap = request.direction === "BuyBase"
        ? `buy base with ${request.amount} quote, for at least ${request.min_out} base`
        : `sell ${request.amount} base, for at least ${request.min_out} quote`;
      return `${who} proposes to ${swap}, ${when}`;
    }
    if (request.command === "SwapSettled") return `${who} reports intent ${request.intent} settled, delivering ${request.amount_out}, ${when}`;
    return `${who} reports intent ${request.intent} failed, ${when}`;
  },
  // Each reason the README's rules name, in plain words.
  reasons: {
    wrong_caller: "this caller may not send that command",
    clock_not_advanced: "the tick is not later than the last commit's",
    unapproved_model: "the model is not approved",
    stale_price: "the price is from the future, or more than one tick old",
    swap_outstanding: "a swap is already outstanding",
    no_swap_outstanding: "no swap is outstanding",
    stale_callback: "the answer is about an earlier swap, not the outstanding one",
    short_settlement: "the settlement delivers less than the swap's minimum",
    over_trade_cap: "the swap is worth more than the per-trade cap of 3",
    over_daily_budget: "the swap would take the day over its budget of 4",
    slippage_too_wide: "the minimum keeps less than three quarters of the oracle value",
    below_reserve: "the treasury would drop below its reserve of 2 quote, or sell more base than it holds",
    swap_failed: "the exchange reported the swap failed: the held amount is refunded, and the budget is not restored",
  },
  demonstration: [
    // Day 0: a good buy, then answers and proposals the guard refuses.
    step(1, "Agent", V2, 1, 1, propose("BuyBase", 2, 2), "agent buys 2 base with 2 quote at price 1, for at least 2 base", "Accept"),
    step(2, "Agent", V2, 1, 2, propose("BuyBase", 1, 1), "agent proposes again while the buy is outstanding", "Reject"),
    step(2, "Agent", V2, 1, 2, settled(1, 2), "agent, not the DEX, reports the buy settled", "Reject"),
    step(2, "Dex", V2, 1, 2, settled(1, 1), "DEX settles the buy with 1 base, below the minimum of 2", "Reject"),
    step(2, "Dex", V2, 1, 2, settled(1, 2), "DEX settles the buy with 2 base", "Accept"),
    step(3, "Dex", V2, 1, 3, settled(1, 2), "DEX repeats the settlement; nothing is outstanding", "Reject"),
    step(3, "Agent", V2, 2, 3, propose("SellBase", 2, 3), "agent sells 2 base at price 2: worth 4, over the cap of 3", "Reject"),
    step(3, "Agent", V2, 1, 1, propose("BuyBase", 1, 1), "agent buys on a price observed 2 ticks ago", "Reject"),
    step(3, "Agent", V2, 1, 3, propose("BuyBase", 3, 3), "agent buys 3: with 2 already spent today, over the budget of 4", "Reject"),
    step(3, "Agent", V2, 1, 3, propose("BuyBase", 2, 1), "agent buys 2 for at least 1 base: below three quarters of the oracle value", "Reject"),
    step(3, "Agent", "UnlistedModel", 1, 3, propose("BuyBase", 1, 1), "an unlisted model proposes a buy", "Reject"),
    step(3, "Agent", V2, 2, 3, propose("SellBase", 1, 2), "agent sells 1 base at price 2 for at least 2 quote, filling the budget", "Accept"),
    // Day 1: a late answer about the first swap, a failure, the reserve,
    // and a budget that a failure does not restore.
    step(4, "Dex", V2, 2, 4, settled(1, 2), "DEX settles intent 1 again, late: the outstanding intent is 3", "Reject"),
    step(4, "Dex", V2, 2, 4, failed(3), "DEX reports the sell failed: the base is refunded, and the new day restarts the budget", "CommittedFailure"),
    step(5, "Agent", V2, 1, 5, propose("BuyBase", 3, 3), "agent buys 3 with 4 quote: that would leave 1, below the reserve of 2", "Reject"),
    step(5, "Agent", V2, 1, 5, propose("BuyBase", 2, 2), "agent buys 2 base with 2 quote", "Accept"),
    step(6, "Dex", V2, 1, 6, failed(5), "DEX reports the buy failed: the quote is refunded, the 2 stay committed", "CommittedFailure"),
    step(7, "Agent", V2, 1, 7, propose("BuyBase", 3, 3), "agent buys 3: with 2 still committed today, over the budget", "Reject"),
    step(7, "Agent", V2, 1, 7, propose("BuyBase", 2, 2), "agent buys 2 base with 2 quote", "Accept"),
    // Day 2: the budget restarts, the clock must advance, and a sell settles.
    step(8, "Dex", V2, 1, 8, settled(7, 3), "DEX settles the buy with 3 base; the new day restarts the budget", "Accept"),
    step(8, "Agent", V2, 1, 8, propose("BuyBase", 1, 1), "agent proposes at the tick of the last commit", "Reject"),
    step(9, "Agent", V2, 1, 9, propose("SellBase", 3, 3), "agent sells 3 base at price 1 for at least 3 quote", "Accept"),
    step(10, "Dex", V2, 1, 10, settled(9, 3), "DEX settles the sell with 3 quote", "Accept"),
  ],
  // The demonstration's end, in the fields the gate's summary in
  // tools/check_generated_application.py names. Nothing delivers in the
  // page, so the requests the gate's shell delivered are the requests pending here.
  summary: (state) => ({
    quote: state.state.quote,
    base: state.state.base,
    spent_today: state.state.spent_today,
    last_seen: state.state.last_seen,
    swap: state.state.pending,
    bundles: state.bundles,
    deliveries: state.outbox.filter((entry) => !entry.acknowledged).length,
  }),
};
