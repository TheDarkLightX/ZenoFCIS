// The inventory-reservation example, as the page presents it: its state
// fields with plain labels, the context and commands in the README's words,
// the reasons in plain words, and the README's scripted demonstration: the
// eleven commands that `journey` in the template's src/lib.rs makes, with
// the decision each expects. The interrupted delivery and the database
// reopen that follow them need the SQLite shell and are not part of the page.

const quantity = { name: "quantity", label: "Units", hint: "1 to 3", kind: "integer", min: 1, max: 3, initial: 1 };

const step = (command, units, authorized, expect, note) => ({ request: { command, quantity: units, authorized }, expect, note });

export const template = {
  name: "inventory-reservation",
  fields: [
    { name: "available", label: "Available" },
    { name: "reserved", label: "Reserved" },
  ],
  help: "Say whether the operator is authorized, then move stock: each button sends one command with its units through the authority.",
  contextLegend: "The request's context",
  context: [
    { name: "authorized", label: "Operator authorized", kind: "flag", initial: true },
  ],
  groups: [
    {
      legend: "Move stock",
      commands: [
        { name: "Reserve", label: "Reserve", fields: [quantity] },
        { name: "Release", label: "Release", fields: [quantity] },
        { name: "Ship", label: "Ship", fields: [quantity] },
        { name: "Restock", label: "Restock", fields: [quantity] },
      ],
    },
  ],
  genesis: "no units available, none reserved, nothing queued",
  outbox: "Shipment requests to warehouse on channel 300 (shipment), with the units shipped. Nothing delivers in this page, so each stays pending with its delivery identity.",
  describe: (request) => `${request.command} ${request.quantity} unit${request.quantity === 1 ? "" : "s"}${request.authorized ? "" : ", operator not authorized"}`,
  reasons: {
    not_authorized: "the operator is not authorized",
    insufficient_available: "fewer units than that are available",
    insufficient_reserved: "fewer units than that are reserved",
    over_capacity: "a shelf would hold more than 5 units",
  },
  demonstration: [
    step("Restock", 3, true, "Accept", "restock 3 units"),
    step("Restock", 3, true, "Reject", "a restock beyond the capacity of 5: over_capacity"),
    step("Restock", 2, true, "Accept", "restock 2: the shelf holds its capacity"),
    step("Reserve", 3, true, "Accept", "reserve 3 for an order"),
    step("Reserve", 3, true, "Reject", "only 2 are available: insufficient_available"),
    step("Release", 1, true, "Accept", "release 1 back to available"),
    step("Ship", 3, true, "Reject", "only 2 are reserved: insufficient_reserved"),
    step("Ship", 2, false, "Reject", "an operator without authorization: not_authorized"),
    step("Ship", 2, true, "Accept", "ship 2: a shipment request is queued"),
    step("Reserve", 3, true, "Accept", "reserve 3 more"),
    step("Ship", 1, true, "Accept", "ship 1"),
  ],
  // The demonstration's end, in the fields the gate's summary in
  // tools/check_generated_application.py names: the units restocked and
  // shipped are summed over the accepted commands, as the demonstration sums
  // them. Nothing delivers in the page, so the requests the gate's shell
  // delivered are the requests pending here.
  summary: (state, steps) => {
    const moved = (action) => steps
      .filter(({ request, report }) => request.command === action && report.decision === "Accept")
      .reduce((total, { request }) => total + request.quantity, 0);
    return {
      available: state.state.available,
      reserved: state.state.reserved,
      restocked: moved("Restock"),
      shipped: moved("Ship"),
      bundles: state.bundles,
      deliveries: state.outbox.filter((entry) => !entry.acknowledged).length,
    };
  },
};
