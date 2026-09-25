// The order-fulfillment example, as the page presents it: its state fields
// with plain labels, the context and commands in the README's words, the
// reasons in plain words, and the README's scripted demonstration: the
// eleven messages that `journey` in the template's src/lib.rs makes, with
// the decision each expects. The interrupted delivery and the database
// reopen that follow them need the SQLite shell and are not part of the page.

const attempt = { name: "callback_attempt", label: "Attempt the answer is about", hint: "0 to 3", kind: "integer", min: 0, max: 3, initial: 1 };

const message = (command, caller, callbackAttempt, expect, note) => ({
  request: callbackAttempt === null ? { command, caller } : { command, callback_attempt: callbackAttempt, caller },
  expect,
  note,
});
const LABELS = {
  Checkout: "Checkout",
  PaymentCaptured: "Payment captured",
  PaymentDeclined: "Payment declined",
  ParcelDispatched: "Parcel dispatched",
  ParcelDelivered: "Parcel delivered",
  CancelOrder: "Cancel order",
};
const CALLERS = { Customer: "the customer", PaymentProvider: "the payment provider", Carrier: "the carrier" };

export const template = {
  name: "order-fulfillment",
  fields: [
    { name: "status", label: "Status" },
    { name: "payment_attempts", label: "Payment attempts" },
  ],
  help: "Choose the caller, then send a message: an action from the customer, a carrier's report, or a payment provider's answer naming the attempt it is about.",
  contextLegend: "The request's context",
  context: [
    { name: "caller", label: "Caller", kind: "choice", options: [["Customer", "Customer"], ["PaymentProvider", "Payment provider"], ["Carrier", "Carrier"]], initial: "Customer" },
  ],
  groups: [
    {
      legend: "Send a message",
      commands: [
        { name: "Checkout", label: LABELS.Checkout, fields: [] },
        { name: "ParcelDispatched", label: LABELS.ParcelDispatched, fields: [] },
        { name: "ParcelDelivered", label: LABELS.ParcelDelivered, fields: [] },
        { name: "CancelOrder", label: LABELS.CancelOrder, fields: [] },
      ],
    },
    {
      legend: "The payment provider answers about an attempt",
      commands: [
        { name: "PaymentCaptured", label: LABELS.PaymentCaptured, fields: [attempt] },
        { name: "PaymentDeclined", label: LABELS.PaymentDeclined, fields: [attempt] },
      ],
    },
  ],
  genesis: "Placed, with no payment attempt, nothing queued",
  outbox: "Payment requests to payment-provider on channel 300 (payment_request), numbered with the attempt, and shipping requests to carrier on channel 301 (shipping_request). Nothing delivers in this page, so each stays pending with its delivery identity.",
  describe: (request) => `${LABELS[request.command] ?? request.command}${request.callback_attempt === undefined ? "" : ` for attempt ${request.callback_attempt}`}, from ${CALLERS[request.caller] ?? request.caller}`,
  reasons: {
    wrong_caller: "this caller may not send that action",
    invalid_transition: "the order's status does not allow that action",
    stale_callback: "the answer names an earlier payment attempt",
    attempts_exhausted: "three payment attempts have been made",
    payment_declined: "the payment was declined: the order returns to Placed and keeps its attempt count",
  },
  demonstration: [
    message("Checkout", "Customer", null, "Accept", "the customer checks out: attempt 1, and a capture request is queued"),
    message("PaymentDeclined", "PaymentProvider", 1, "CommittedFailure", "the provider declines attempt 1: the order returns to Placed"),
    message("PaymentCaptured", "PaymentProvider", 1, "Reject", "a late capture of the declined attempt: the order is Placed"),
    message("Checkout", "Customer", null, "Accept", "a second checkout: attempt 2"),
    message("PaymentDeclined", "PaymentProvider", 1, "Reject", "a late decline of attempt 1: stale_callback"),
    message("PaymentCaptured", "Customer", 2, "Reject", "a capture from the customer: wrong_caller"),
    message("PaymentCaptured", "PaymentProvider", 2, "Accept", "the provider captures attempt 2: paid, and a shipping request is queued"),
    message("PaymentCaptured", "PaymentProvider", 2, "Reject", "the same capture again: the order is already Paid"),
    message("CancelOrder", "Customer", null, "Reject", "a cancellation after payment"),
    message("ParcelDispatched", "Carrier", null, "Accept", "the carrier dispatches the parcel"),
    message("ParcelDelivered", "Carrier", null, "Accept", "the carrier delivers it"),
  ],
  // The demonstration's end, in the fields the gate's summary in
  // tools/check_generated_application.py names. Nothing delivers in the
  // page, so the requests the gate's shell delivered are the requests pending here.
  summary: (state) => ({
    order_status: state.state.status,
    payment_attempts: state.state.payment_attempts,
    bundles: state.bundles,
    deliveries: state.outbox.filter((entry) => !entry.acknowledged).length,
  }),
};
