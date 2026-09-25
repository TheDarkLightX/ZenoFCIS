// The withdrawal-queue example, as the page presents it: its state fields
// with plain labels, the context and commands in the README's words, the
// reasons in plain words, and the README's scripted demonstration: the
// sixteen commands that `journey` in the template's src/lib.rs makes, with
// the decision each expects. The interrupted delivery and the database
// reopen that follow them need the SQLite shell and are not part of the page.

const amount = { name: "amount", label: "Amount", hint: "1 or 2", kind: "integer", min: 1, max: 2, initial: 2 };

const deposit = (units, caller, expect, note) => ({ request: { command: "Deposit", amount: units, caller, alarm: false }, expect, note });
const request = (lane, units, caller, expect, note) => ({ request: { command: "RequestWithdrawal", lane, amount: units, caller, alarm: false }, expect, note });
const tick = (alarm, expect, note) => ({ request: { command: "Tick", caller: "Keeper", alarm }, expect, note });
const CALLERS = { Operator: "the operator", OwnerA: "the owner of lane A", OwnerB: "the owner of lane B", Keeper: "the keeper" };

export const template = {
  name: "withdrawal-queue",
  fields: [
    { name: "balance", label: "Balance" },
    { name: "lane_a", label: "Lane A" },
    { name: "amount_a", label: "Lane A's amount" },
    { name: "lane_b", label: "Lane B" },
    { name: "amount_b", label: "Lane B's amount" },
    { name: "pause", label: "Pause ticks left" },
    { name: "must_serve", label: "Must serve" },
    { name: "priority", label: "Priority lane" },
  ],
  help: "Deposit as the operator, request a withdrawal as a lane's owner, then tick as the keeper. Raise the alarm and keep ticking: an honored alarm pauses payouts for its own tick and two more, then must-serve pays a due lane whatever the alarm says. A recorded request is paid within 8 ticks: delayed, never frozen.",
  contextLegend: "The request's context",
  context: [
    { name: "caller", label: "Caller", kind: "choice", options: [["Operator", "Operator"], ["OwnerA", "Owner of lane A"], ["OwnerB", "Owner of lane B"], ["Keeper", "Keeper"]], initial: "Operator" },
    { name: "alarm", label: "Alarm raised", kind: "flag", initial: false },
  ],
  groups: [
    {
      legend: "Deposit, or request a withdrawal from a lane",
      commands: [
        { name: "Deposit", label: "Deposit", fields: [amount] },
        {
          name: "RequestWithdrawal",
          label: "Request a withdrawal",
          fields: [
            { name: "lane", label: "Lane", kind: "choice", options: [["A", "A"], ["B", "B"]], initial: "A" },
            amount,
          ],
        },
      ],
    },
    {
      legend: "The keeper's tick",
      commands: [{ name: "Tick", label: "Tick", fields: [] }],
    },
  ],
  genesis: "no balance, both lanes empty, no pause, lane A first, nothing queued",
  outbox: "Payout requests to settlement on channel 300 (payout), with the paid lane and amount. Nothing delivers in this page, so each stays pending with its delivery identity.",
  describe: (request) => {
    const who = CALLERS[request.caller] ?? request.caller;
    const alarm = request.alarm ? ", alarm raised" : "";
    if (request.command === "Deposit") return `Deposit ${request.amount}, from ${who}${alarm}`;
    if (request.command === "RequestWithdrawal") return `Request ${request.amount} from lane ${request.lane}, by ${who}${alarm}`;
    return `Tick, from ${who}${alarm}`;
  },
  reasons: {
    wrong_caller: "this caller may not send that command",
    lane_occupied: "the lane already holds a request",
    insufficient_balance: "the amount exceeds the unreserved balance",
    over_capacity: "the vault would hold more than 4",
  },
  demonstration: [
    deposit(2, "Operator", "Accept", "the operator deposits 2"),
    deposit(2, "Operator", "Accept", "and 2 more: the vault holds its capacity of 4"),
    deposit(1, "Operator", "Reject", "a deposit above the capacity: over_capacity"),
    request("A", 2, "OwnerB", "Reject", "lane B's owner asks on lane A: wrong_caller"),
    request("A", 2, "OwnerA", "Accept", "lane A's owner requests 2"),
    request("A", 1, "OwnerA", "Reject", "a second request on lane A: lane_occupied"),
    request("B", 2, "OwnerB", "Accept", "lane B's owner requests 2"),
    tick(true, "Accept", "the keeper ticks with the alarm raised: the alarm is honored, nothing is paid, and the pause begins"),
    tick(true, "Accept", "the pause counts down"),
    tick(true, "Accept", "the pause ends with both lanes due: must-serve"),
    tick(true, "Accept", "must-serve pays lane A on the fourth tick, alarm or not"),
    tick(true, "Accept", "the alarm is honored again: a second pause"),
    tick(false, "Accept", "the pause counts down"),
    tick(false, "Accept", "the pause ends with lane B due: must-serve"),
    tick(true, "Accept", "must-serve pays lane B on the eighth tick, the bound"),
    request("A", 1, "OwnerA", "Reject", "a request with no unreserved balance: insufficient_balance"),
  ],
  // The demonstration's end, in the fields the gate's summary in
  // tools/check_generated_application.py names: the units deposited are
  // summed over the accepted deposits, what was paid is what left the
  // balance, and the payout ticks count the accepted ticks that queued a
  // payout, as the demonstration counts them. Nothing delivers in the page,
  // so the payouts the gate's shell delivered are the payouts pending here.
  summary: (state, steps) => {
    const deposited = steps
      .filter(({ request, report }) => request.command === "Deposit" && report.decision === "Accept")
      .reduce((total, { request }) => total + request.amount, 0);
    const payoutTicks = [];
    let ticks = 0;
    for (const { request, report } of steps) {
      if (request.command !== "Tick" || report.decision !== "Accept") continue;
      ticks += 1;
      if (report.outbox.length > 0) payoutTicks.push(ticks);
    }
    return {
      balance: state.state.balance,
      deposited,
      paid: deposited - state.state.balance,
      payout_ticks: payoutTicks,
      bundles: state.bundles,
      deliveries: state.outbox.filter((entry) => !entry.acknowledged).length,
    };
  },
};
