// The page: sends each request to the module and renders what the authority
// returned. Every context value, including the time, comes from the form.

import { instantiate } from "./demo-module.js";
import { DEMONSTRATION } from "./demonstration.js";

const LATEST_TIME = 4102444800;
const STEP_DELAY_MS = 400;
const KIND_LABELS = {
  Accept: ["accept", "Accepted"],
  CommittedFailure: ["failure", "Committed failure"],
  Reject: ["reject", "Rejected"],
};

const element = (id) => document.getElementById(id);
const now = element("now");
const admin = element("admin");
const status = element("status");
const timeline = element("timeline");
const outbox = element("outbox");
const buttons = [...document.querySelectorAll("button")];

let demo = null;
let busy = false;
let count = 0;

function setBusy(value) {
  busy = value;
  for (const button of buttons) button.disabled = value || demo === null;
}

function text(tag, content, className) {
  const node = document.createElement(tag);
  node.textContent = content;
  if (className) node.className = className;
  return node;
}

function short(hash) {
  return `${hash.slice(0, 12)}…`;
}

function account(state) {
  return `failed_attempts ${state.failed_attempts}, locked_until ${state.locked_until}, last_seen ${state.last_seen}`;
}

function renderState(state) {
  element("failed-attempts").textContent = state.account.failed_attempts;
  element("locked-until").textContent = state.account.locked_until;
  element("last-seen").textContent = state.account.last_seen;
  element("bundles").textContent = state.bundles;
  element("steps").textContent = state.steps;
  element("root").textContent = state.root;
  outbox.replaceChildren();
  if (state.outbox.length === 0) outbox.append(text("li", "No alerts.", "empty"));
  for (const entry of state.outbox) {
    const item = document.createElement("li");
    item.append(text("strong", `${entry.alert_kind}`), ` to ${entry.destination}, alert_until ${entry.alert_until}, channel ${entry.channel}. `);
    item.append(`${entry.acknowledged ? "Acknowledged" : "Pending"}; delivery `, text("span", short(entry.delivery_id), "hash"));
    outbox.append(item);
  }
}

function describeRequest(request) {
  return `${request.command} at ${request.now}${request.admin ? ", admin" : ""}`;
}

function renderReport(request, report) {
  if (count === 0) timeline.replaceChildren();
  count += 1;
  const item = document.createElement("li");
  const head = document.createElement("div");
  head.className = "entry-head";
  const body = document.createElement("p");
  body.className = "entry-body";
  if (report.error) {
    head.append(text("span", "Refused", "badge refused"), text("span", `#${count} ${describeRequest(request)}`, "request"));
    body.append(`Refused at ${report.stage}, before any decision: ${report.error}`);
  } else {
    const [className, label] = KIND_LABELS[report.decision];
    head.append(text("span", label, "badge " + className), text("span", `#${count} ${describeRequest(request)}`, "request"));
    if (report.reason) body.append(`Reason ${report.reason.id} ${report.reason.name}. `);
    if (report.commit) {
      body.append(`Committed as bundle ${report.bundles}; the exact replay was idempotent. `);
    } else {
      body.append("Nothing was published. ");
    }
    body.append(text("span", `Before: ${account(report.before)}. After: ${account(report.after)}.`, "transition"));
    for (const entry of report.outbox) {
      body.append(text("span", `Queued alert ${entry.alert_kind}, alert_until ${entry.alert_until}.`, "transition"));
    }
    const laws = document.createElement("ul");
    laws.className = "laws";
    for (const law of report.laws) laws.append(text("li", `Law ${law.id} ${law.name}: ${law.status}`));
    const details = document.createElement("details");
    details.append(text("summary", `${report.laws.length} law${report.laws.length === 1 ? "" : "s"} evaluated`), laws);
    body.append(details);
    const [identityKind, identityHash] = report.authorization_id
      ? ["Authorization", report.authorization_id]
      : ["Rejection", report.rejection_id];
    const identity = text("span", `${identityKind} `, "transition");
    identity.append(text("span", identityHash, "hash"));
    body.append(identity);
  }
  item.append(head, body);
  timeline.append(item);
  item.scrollIntoView({ block: "nearest" });
  return report;
}

function announce(request, report) {
  if (report.error) {
    status.textContent = `Refused at ${report.stage}: ${report.error}`;
  } else {
    const reason = report.reason ? ` (${report.reason.name})` : "";
    status.textContent = `${KIND_LABELS[report.decision][1]}${reason}: ${describeRequest(request)}.`;
  }
}

function propose(command) {
  const value = Number(now.value);
  if (!Number.isInteger(value)) {
    status.textContent = "Enter a whole number of seconds for the time.";
    now.focus();
    return;
  }
  const request = { command, now: value, admin: admin.checked };
  const report = demo.step(request);
  renderReport(request, report);
  renderState(demo.state());
  announce(request, report);
}

function reset() {
  const state = demo.reset();
  count = 0;
  timeline.replaceChildren(text("li", "No decisions yet.", "empty"));
  renderState(state);
  status.textContent = "Reset to the exact genesis: every field zero, no bundles, no alerts.";
}

const delay = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds));

async function runDemonstration() {
  setBusy(true);
  try {
    reset();
    for (const step of DEMONSTRATION) {
      now.value = step.now;
      admin.checked = step.admin;
      propose(step.command);
      await delay(STEP_DELAY_MS);
    }
    status.textContent = `The demonstration ran its ${DEMONSTRATION.length} requests; compare the decisions with the README.`;
  } finally {
    setBusy(false);
  }
}

for (const button of document.querySelectorAll("button.propose")) {
  button.addEventListener("click", () => {
    if (!busy) propose(button.dataset.command);
  });
}
element("advance").addEventListener("click", () => {
  const value = Number(now.value);
  now.value = Number.isInteger(value) ? Math.min(value + 10, LATEST_TIME) : 0;
});
element("demonstration").addEventListener("click", () => {
  if (!busy) runDemonstration();
});
element("reset").addEventListener("click", () => {
  if (!busy) reset();
});
element("request").addEventListener("submit", (event) => event.preventDefault());

try {
  // Relative to this script, not to the document, so the page works from any
  // path it is served at.
  const response = await fetch(new URL("account-lockout.wasm", import.meta.url));
  if (!response.ok) throw new Error(`${response.status} ${response.statusText}`);
  demo = await instantiate(await response.arrayBuffer());
  reset();
  setBusy(false);
} catch (error) {
  status.textContent = `The module could not be loaded (${error.message}). Build it with python3 site/build.py and serve site/public over HTTP.`;
}
