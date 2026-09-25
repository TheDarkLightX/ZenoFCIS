// The order-fulfillment examples: how tests/decision-examples.txt is read,
// how each example's order is reached from genesis, and which laws each
// decision kind evaluates. From project.zeno: statuses, actions, callers,
// and payment actions by numeric ID; and the laws profile.rs enforces on
// each decision kind (500 on every commit, 501 to 505 on accepts, 506 on
// committed failures, 509 on rejections).

import { integer, kind, lines, named, reason } from "../examples.mjs";

const STATUSES = { 160: "Placed", 161: "AwaitingPayment", 162: "Paid", 163: "Shipped", 164: "Delivered", 165: "Cancelled" };
const ACTIONS = { 150: "Checkout", 151: "PaymentCaptured", 152: "PaymentDeclined", 153: "ParcelDispatched", 154: "ParcelDelivered", 155: "CancelOrder" };
const CALLERS = { 170: "Customer", 171: "PaymentProvider", 172: "Carrier" };
const PAYMENT_ACTIONS = { 175: "Capture", 176: "Void" };
const CALLBACKS = ["PaymentCaptured", "PaymentDeclined"];

export const laws = { Accept: [500, 501, 502, 503, 504, 505], CommittedFailure: [500, 506], Reject: [509] };

// Inputs refused before any decision, with the stage that refuses them.
export const refusals = [
  ["{", "input"],
  ['{"command":"PaymentCaptured","callback_attempt":4,"caller":"PaymentProvider"}', "admission"],
];

// Every example's order is reachable from genesis.
export const unreachable = {};

// `pre.120 pre.121 action callback_attempt caller | outcome reason post.120
// post.121 | request`, as tests/conformance.rs parses it. Only a payment
// provider's callback carries `callback_attempt`; the file writes 0 for the
// other actions, which the page never sends.
export function parse(text) {
  return lines(text).map(({ line, parts: [input, decision, queued] }) => {
    const [status, attempts, action, callbackAttempt, caller] = input;
    const [outcome, why, postStatus, postAttempts] = decision;
    const order = (state, count) => ({ status: named(STATUSES, state, line), payment_attempts: integer(count, line) });
    const command = named(ACTIONS, action, line);
    const request = { command, caller: named(CALLERS, caller, line) };
    if (CALLBACKS.includes(command)) request.callback_attempt = integer(callbackAttempt, line);
    let entries = [];
    if (queued[0] === "300") {
      entries = [{ channel: 300, payload: { request_attempt: integer(queued[1], line), request_action: named(PAYMENT_ACTIONS, queued[2], line) } }];
    } else if (queued[0] === "301") {
      entries = [{ channel: 301, payload: { paid_attempt: integer(queued[1], line) } }];
    }
    return {
      line,
      pre: order(status, attempts),
      request,
      expected: { kind: kind(outcome, line), reason: reason(why), post: order(postStatus, postAttempts), entries },
    };
  });
}

// Messages that take a new order to the example's order: each payment
// attempt is a checkout, declined unless it is the one that pays; then the
// status advances. The replay checks the result.
export function reach({ status, payment_attempts: attempts }) {
  const steps = [];
  const customer = (command) => steps.push({ command, caller: "Customer" });
  const provider = (command, attempt) => steps.push({ command, callback_attempt: attempt, caller: "PaymentProvider" });
  const carrier = (command) => steps.push({ command, caller: "Carrier" });
  // A placed order with n attempts made and declined; every other status
  // has its last attempt still open or paid.
  const declined = status === "Placed" || (status === "Cancelled" && attempts === 0) ? attempts : attempts - 1;
  for (let attempt = 1; attempt <= declined; attempt += 1) {
    customer("Checkout");
    provider("PaymentDeclined", attempt);
  }
  if (status === "Placed") return steps;
  if (status === "Cancelled" && attempts === 0) {
    customer("CancelOrder");
    return steps;
  }
  customer("Checkout");
  if (status === "Cancelled") customer("CancelOrder");
  if (["Paid", "Shipped", "Delivered"].includes(status)) provider("PaymentCaptured", attempts);
  if (["Shipped", "Delivered"].includes(status)) carrier("ParcelDispatched");
  if (status === "Delivered") carrier("ParcelDelivered");
  return steps;
}
