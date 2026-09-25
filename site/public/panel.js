// Shared by every example's panel: builds the controls from the template's
// description, sends each request to the module, and renders what the
// authority returned: the state, the outbox, and the timeline of decisions
// with the laws evaluated for each. Every context value, including any
// time, comes from the form; the module reads no clock.

import { instantiate } from "./demo-module.js";

const STEP_DELAY_MS = 400;
const KIND_LABELS = {
  Accept: ["accept", "Accepted"],
  CommittedFailure: ["failure", "Committed failure"],
  Reject: ["reject", "Rejected"],
};

function node(tag, className, ...children) {
  const created = document.createElement(tag);
  if (className) created.className = className;
  created.append(...children);
  return created;
}

function short(hash) {
  return `${hash.slice(0, 12)}…`;
}

// The record's fields in the template's order, then any the template did
// not list, so that nothing the module reports goes unshown.
function orderedKeys(record, first) {
  const rest = Object.keys(record).filter((key) => !first.includes(key)).sort();
  return [...first.filter((key) => key in record), ...rest];
}

function describeRecord(record) {
  return Object.keys(record).sort().map((key) => `${key} ${record[key]}`).join(", ");
}

function describeChanges(before, after, fields) {
  const changes = orderedKeys(after, fields)
    .filter((key) => JSON.stringify(before[key]) !== JSON.stringify(after[key]))
    .map((key) => `${key} ${before[key]} → ${after[key]}`);
  return changes.length === 0 ? "No field changed." : `Changed: ${changes.join(", ")}.`;
}

function describeEntry(entry) {
  return `${entry.channel_name ?? `channel ${entry.channel}`} to ${entry.destination}: ${describeRecord(entry.payload)}`;
}

// One form control per described field; `read` returns its value for a
// request, or throws the message to show.
function control(field) {
  const wrapper = node("div", "field");
  let input;
  if (field.kind === "flag") {
    input = node("input");
    input.type = "checkbox";
    input.checked = Boolean(field.initial);
    wrapper.append(node("label", null, input, ` ${field.label}`));
  } else if (field.kind === "choice") {
    input = node("select");
    for (const [value, label] of field.options) {
      const option = node("option", null, label);
      option.value = value;
      input.append(option);
    }
    input.value = field.initial;
    wrapper.append(node("label", null, `${field.label} `, input));
  } else {
    input = node("input");
    input.type = "number";
    input.inputMode = "numeric";
    input.min = field.min;
    input.max = field.max;
    input.step = 1;
    input.value = field.initial;
    input.required = true;
    wrapper.append(node("label", null, `${field.label} `, input));
  }
  const read = () => {
    if (field.kind === "flag") return input.checked;
    if (field.kind === "choice") return input.value;
    const value = Number(input.value);
    if (!Number.isInteger(value)) {
      input.focus();
      throw new Error(`Enter a whole number for ${field.label}.`);
    }
    return value;
  };
  const write = (value) => {
    if (field.kind === "flag") input.checked = Boolean(value);
    else input.value = value;
  };
  return { name: field.name, wrapper, read, write, input };
}

export async function mount(container, template) {
  const context = template.context.map(control);
  const commands = template.commands.map((command) => ({ command, fields: command.fields.map(control) }));
  const proposeButtons = [];
  const form = node("form", "controls");
  form.addEventListener("submit", (event) => event.preventDefault());
  form.append(node("p", "help", template.help));
  if (context.length > 0) {
    form.append(node("div", "context", ...context.map((field) => field.wrapper)));
  }
  for (const { command, fields } of commands) {
    const button = node("button", "propose", command.label);
    button.type = "button";
    button.disabled = true;
    button.addEventListener("click", () => {
      if (!busy) propose(command.name, fields);
    });
    proposeButtons.push(button);
    form.append(node("div", "row command", ...fields.map((field) => field.wrapper), button));
  }
  const demonstrationButton = node("button", null, "Run the README's demonstration");
  demonstrationButton.type = "button";
  const resetButton = node("button", null, "Reset to genesis");
  resetButton.type = "button";
  form.append(node("div", "row", demonstrationButton, resetButton));

  const status = node("p", "status", "Loading the module.");
  status.id = `${template.name}-status`;
  status.setAttribute("role", "status");
  status.setAttribute("aria-live", "polite");
  const facts = node("dl", "facts");
  facts.id = `${template.name}-state`;
  const outbox = node("ol", "outbox", node("li", "empty", "Nothing queued."));
  const timeline = node("ol", "timeline", node("li", "empty", "No decisions yet."));

  // A template may show its scripted proposer on its own: each of its
  // proposals from the demonstration, with a button that sends it against
  // the state as it is now.
  const proposerButtons = [];
  const proposer = template.proposer ? node("section", "panel proposer",
    node("h3", null, template.proposer.label),
    node("p", "help", template.proposer.help),
    node("ol", "proposals", ...template.demonstration.filter(template.proposer.filter).map((step) => {
      const button = node("button", "propose", "Send this proposal");
      button.type = "button";
      button.disabled = true;
      button.addEventListener("click", () => {
        if (!busy) send(step.request, step.note);
      });
      proposerButtons.push(button);
      return node("li", null,
        node("span", "request", template.describe(step.request)),
        node("span", "note", `${step.note}: ${KIND_LABELS[step.expect][1].toLowerCase()} in the script`),
        button);
    }))) : null;

  container.replaceChildren(
    form,
    status,
    node("div", "panels",
      node("section", "panel", node("h3", null, "State"), facts),
      node("section", "panel", node("h3", null, "Outbox"), node("p", "help", template.outbox), outbox)),
    ...(proposer ? [proposer] : []),
    node("section", "panel",
      node("h3", null, "Decisions"),
      node("p", "help", "Accepted and committed failures are published; rejections change nothing; a request the schema or the authority refuses never becomes a decision."),
      timeline),
  );

  const buttons = [...proposeButtons, ...proposerButtons, demonstrationButton, resetButton];
  let demo = null;
  let busy = false;
  let count = 0;
  // Every request sent since the last reset, with the module's report.
  const history = [];

  function setBusy(value) {
    busy = value;
    for (const button of buttons) button.disabled = value || demo === null;
  }

  function renderState(state) {
    facts.replaceChildren();
    for (const key of orderedKeys(state.state, template.fields)) {
      facts.append(node("dt", null, node("code", null, key)), node("dd", null, String(state.state[key])));
    }
    facts.append(
      node("dt", null, "Committed bundles"), node("dd", null, String(state.bundles)),
      node("dt", null, "Decisions"), node("dd", null, String(state.steps)),
      node("dt", null, "State root"), node("dd", "hash", state.root),
    );
    outbox.replaceChildren();
    if (state.outbox.length === 0) outbox.append(node("li", "empty", "Nothing queued."));
    for (const entry of state.outbox) {
      outbox.append(node("li", null,
        node("strong", null, entry.channel_name ?? `channel ${entry.channel}`),
        ` (channel ${entry.channel}) to ${entry.destination}: ${describeRecord(entry.payload)}. `,
        `${entry.acknowledged ? "Acknowledged" : "Pending"}; delivery `,
        node("span", "hash", short(entry.delivery_id))));
    }
  }

  function renderReport(request, report, note) {
    if (count === 0) timeline.replaceChildren();
    count += 1;
    const head = node("div", "entry-head");
    const body = node("p", "entry-body");
    if (report.error) {
      head.append(node("span", "badge refused", "Refused"), node("span", "request", `#${count} ${template.describe(request)}`));
      body.append(`Refused at ${report.stage}, before any decision: ${report.error}`);
    } else {
      const [className, label] = KIND_LABELS[report.decision];
      head.append(node("span", `badge ${className}`, label), node("span", "request", `#${count} ${template.describe(request)}`));
      if (note) head.append(node("span", "note", note));
      if (report.reason) body.append(`Reason ${report.reason.id} ${report.reason.name}. `);
      body.append(report.commit ? `Committed as bundle ${report.bundles}; the exact replay was idempotent. ` : "Nothing was published. ");
      body.append(node("span", "transition", describeChanges(report.before, report.after, template.fields)));
      for (const entry of report.outbox) body.append(node("span", "transition", `Queued ${describeEntry(entry)}.`));
      const laws = node("ul", "laws", ...report.laws.map((law) => node("li", null, `Law ${law.id} ${law.name}: ${law.status}`)));
      body.append(node("details", null, node("summary", null, `${report.laws.length} law${report.laws.length === 1 ? "" : "s"} evaluated`), laws));
      const [identityKind, identityHash] = report.authorization_id ? ["Authorization", report.authorization_id] : ["Rejection", report.rejection_id];
      body.append(node("span", "transition", `${identityKind} `, node("span", "hash", identityHash)));
    }
    const item = node("li", null, head, body);
    timeline.append(item);
    item.scrollIntoView({ block: "nearest" });
  }

  function announce(request, report) {
    if (report.error) {
      status.textContent = `Refused at ${report.stage}: ${report.error}`;
    } else {
      const reason = report.reason ? ` (${report.reason.name})` : "";
      status.textContent = `${KIND_LABELS[report.decision][1]}${reason}: ${template.describe(request)}.`;
    }
  }

  function send(request, note) {
    const report = demo.step(request);
    history.push({ request, report });
    renderReport(request, report, note);
    renderState(demo.state());
    announce(request, report);
    return report;
  }

  function propose(command, fields) {
    const request = { command };
    try {
      for (const field of [...fields, ...context]) request[field.name] = field.read();
    } catch (error) {
      status.textContent = error.message;
      return;
    }
    send(request);
  }

  function reset() {
    const state = demo.reset();
    count = 0;
    history.length = 0;
    timeline.replaceChildren(node("li", "empty", "No decisions yet."));
    renderState(state);
    status.textContent = `Reset to the exact genesis: ${template.genesis}.`;
  }

  const delay = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds));

  // Shows each request in the form, then sends it. `instant` skips the pause
  // between requests; `onLoad` says the run happened when the page loaded.
  async function runDemonstration({ instant = false, onLoad = false } = {}) {
    setBusy(true);
    try {
      reset();
      for (const step of template.demonstration) {
        const fields = commands.find(({ command }) => command.name === step.request.command)?.fields ?? [];
        for (const field of [...fields, ...context]) field.write(step.request[field.name]);
        send(step.request, step.note);
        if (!instant) await delay(STEP_DELAY_MS);
      }
      status.textContent = `The README's demonstration, decided in this browser${onLoad ? " when the page loaded" : ""}: `
        + `${history.length} requests. Compare the decisions with the README, propose your own, or reset to genesis.`;
    } finally {
      setBusy(false);
    }
  }

  demonstrationButton.addEventListener("click", () => {
    if (!busy) runDemonstration();
  });
  resetButton.addEventListener("click", () => {
    if (!busy) reset();
  });

  // Relative to this script, not to the document, so the page works from any
  // path it is served at.
  const response = await fetch(new URL(`${template.name}.wasm`, import.meta.url));
  if (!response.ok) throw new Error(`${response.status} ${response.statusText}`);
  demo = await instantiate(await response.arrayBuffer());
  reset();
  setBusy(false);
  return { reset, runDemonstration, history, state: () => demo.state(), status };
}
