// Shared by every example's panel: builds the controls from the template's
// description, sends each request to the module, and renders what the
// authority returned: the latest decision in plain words, the state, the
// outbox, and the history of decisions, each with its technical record.
// Every context value, including any time, comes from the form; the module
// reads no clock. The page invents nothing: every verdict, reason, changed
// field, queued entry, law status, and hash is the module's report.

import { instantiate } from "./demo-module.js";

const STEP_DELAY_MS = 400;
const HASH_PREFIX = 12;
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

// A schema name in plain words: `alert_kind` reads as "alert kind".
function words(name) {
  return String(name).replaceAll("_", " ");
}

function sentence(text) {
  return text.charAt(0).toUpperCase() + text.slice(1);
}

function short(hash) {
  return `${hash.slice(0, HASH_PREFIX)}…`;
}

// A hash by its first twelve hex digits, with a button that shows the full
// value in its place; `data-full` carries the full value for the checks.
function hashNode(hash) {
  const prefix = node("code", "hash short", short(hash));
  const full = node("code", "hash full", hash);
  full.hidden = true;
  const reveal = node("button", "reveal", "Show all");
  reveal.type = "button";
  reveal.setAttribute("aria-expanded", "false");
  reveal.addEventListener("click", () => {
    const expand = full.hidden;
    full.hidden = !expand;
    prefix.hidden = expand;
    reveal.textContent = expand ? "Hide" : "Show all";
    reveal.setAttribute("aria-expanded", String(expand));
  });
  const wrapper = node("span", "hashed", prefix, full, " ", reveal);
  wrapper.dataset.full = hash;
  return wrapper;
}

// The record's fields in the template's order, then any the template did
// not list, so that nothing the module reports goes unshown.
function orderedKeys(record, first) {
  const rest = Object.keys(record).filter((key) => !first.includes(key)).sort();
  return [...first.filter((key) => key in record), ...rest];
}

function plainRecord(record) {
  return Object.keys(record).sort().map((key) => `${words(key)} ${record[key]}`).join(", ");
}

function plainEntry(entry) {
  return `${words(entry.channel_name ?? `channel ${entry.channel}`)} to ${entry.destination}: ${plainRecord(entry.payload)}`;
}

// One form control per described field, named after the field; `read`
// returns its value for a request, or throws the message to show.
function control(field) {
  const wrapper = node("div", `field ${field.kind}`);
  const name = node("span", "name", field.label);
  const hint = field.hint ? [node("span", "hint", field.hint)] : [];
  let input;
  if (field.kind === "flag") {
    input = node("input");
    input.type = "checkbox";
    input.checked = Boolean(field.initial);
    wrapper.append(node("label", null, input, name));
  } else if (field.kind === "choice") {
    input = node("select");
    for (const [value, label] of field.options) {
      const option = node("option", null, label);
      option.value = value;
      input.append(option);
    }
    input.value = field.initial;
    wrapper.append(node("label", null, name, ...hint, input));
  } else {
    input = node("input");
    input.type = "number";
    input.inputMode = "numeric";
    input.min = field.min;
    input.max = field.max;
    input.step = 1;
    input.value = field.initial;
    input.required = true;
    wrapper.append(node("label", null, name, ...hint, input));
  }
  input.name = field.name;
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

function fieldset(legend, fields, buttons = []) {
  const group = node("fieldset", "group", node("legend", null, legend));
  if (fields.length > 0) group.append(node("div", "fields", ...fields));
  if (buttons.length > 0) group.append(node("div", "buttons", ...buttons));
  return group;
}

export async function mount(container, template) {
  const fieldNames = template.fields.map((field) => field.name);
  const labels = new Map(template.fields.map((field) => [field.name, field.label]));
  const label = (key) => labels.get(key) ?? words(key);

  const context = template.context.map(control);
  const commands = [];
  const form = node("form", "controls");
  form.addEventListener("submit", (event) => event.preventDefault());
  form.append(node("p", "help", template.help));
  if (context.length > 0) {
    form.append(fieldset(template.contextLegend, context.map((field) => field.wrapper)));
  }
  for (const group of template.groups) {
    // One control per distinct field in the group, in the order the fields
    // first appear: a field that two commands share is entered once. Each
    // command still sends its own fields, then the context.
    const controls = new Map();
    const buttons = [];
    for (const command of group.commands) {
      const fields = command.fields.map((field) => {
        if (!controls.has(field)) controls.set(field, control(field));
        return controls.get(field);
      });
      const button = node("button", "send", command.label);
      button.type = "button";
      button.disabled = true;
      button.dataset.command = command.name;
      button.addEventListener("click", () => {
        if (!busy) propose(command.name, fields);
      });
      commands.push({ command, fields, button });
      buttons.push(button);
    }
    form.append(fieldset(group.legend, [...controls.values()].map((field) => field.wrapper), buttons));
  }
  const demonstrationButton = node("button", "run", "Run the demonstration");
  demonstrationButton.type = "button";
  const resetButton = node("button", "reset", "Start over");
  resetButton.type = "button";
  form.append(node("div", "actions", demonstrationButton, resetButton));

  const status = node("p", "status", "Loading the module.");
  status.id = `${template.name}-status`;
  status.setAttribute("role", "status");
  status.setAttribute("aria-live", "polite");

  const latestHeading = node("h3", null, "Latest decision");
  latestHeading.id = `${template.name}-latest-heading`;
  const latestBody = node("div", "latest-body", node("p", "empty", "No decision yet. Send a request, or run the demonstration."));
  const latest = node("section", "latest", latestHeading, latestBody);
  latest.id = `${template.name}-latest`;
  latest.setAttribute("aria-live", "polite");
  latest.setAttribute("aria-atomic", "true");
  latest.setAttribute("aria-labelledby", latestHeading.id);

  const facts = node("dl", "facts");
  facts.id = `${template.name}-state`;
  const outbox = node("ol", "outbox", node("li", "empty", "Nothing queued."));
  const timeline = node("ol", "timeline", node("li", "empty", "No decisions yet."));
  timeline.id = `${template.name}-timeline`;

  // A template may show its scripted proposer on its own: each of its
  // proposals from the demonstration, with a button that sends it against
  // the state as it is now.
  const proposerButtons = [];
  const proposer = template.proposer ? node("section", "panel proposer",
    node("h3", null, template.proposer.label),
    node("p", "help", template.proposer.help),
    node("ol", "proposals", ...template.demonstration.filter(template.proposer.filter).map((step) => {
      const button = node("button", "send", "Send");
      button.type = "button";
      button.disabled = true;
      button.addEventListener("click", () => {
        if (!busy) send(step.request, step.note, true);
      });
      proposerButtons.push(button);
      return node("li", null, node("div", "proposal",
        node("span", "text",
          node("span", "request", template.describe(step.request)),
          node("span", "note", `${step.note}: ${KIND_LABELS[step.expect][1].toLowerCase()} in the script`)),
        button));
    }))) : null;

  container.replaceChildren(
    form,
    status,
    latest,
    node("div", "panels",
      node("section", "panel", node("h3", null, "State"), facts),
      node("section", "panel", node("h3", null, "Outbox"), node("p", "help", template.outbox), outbox)),
    ...(proposer ? [proposer] : []),
    node("section", "panel",
      node("h3", null, "Every decision, in order"),
      node("p", "help", "Accepted decisions and committed failures are published; a rejection changes nothing; a request the schema or the authority refuses never becomes a decision. Each entry's technical record holds the laws evaluated and the hashes."),
      timeline),
  );

  const buttons = [...commands.map(({ button }) => button), ...proposerButtons, demonstrationButton, resetButton];
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
    for (const key of orderedKeys(state.state, fieldNames)) {
      facts.append(node("dt", null, label(key), " ", node("code", "schema", key)), node("dd", null, String(state.state[key])));
    }
    facts.append(
      node("dt", null, "Decisions made"), node("dd", null, String(state.steps)),
      node("dt", null, "Decisions committed", " ", node("code", "schema", "bundles")), node("dd", null, String(state.bundles)),
      node("dt", null, "State fingerprint", " ", node("code", "schema", "state root")), node("dd", null, hashNode(state.root)),
    );
    outbox.replaceChildren();
    if (state.outbox.length === 0) outbox.append(node("li", "empty", "Nothing queued."));
    for (const entry of state.outbox) {
      outbox.append(node("li", null,
        node("strong", null, sentence(words(entry.channel_name ?? `channel ${entry.channel}`))),
        ` (channel ${entry.channel}) to ${entry.destination}: ${plainRecord(entry.payload)}. `,
        `${entry.acknowledged ? "Acknowledged" : "Pending"}; delivery `, hashNode(entry.delivery_id)));
    }
  }

  // The verdict and the request, as the head of the latest box and of each
  // history entry.
  function verdict(request, report) {
    const [className, kind] = report.error ? ["refused", "Refused"] : KIND_LABELS[report.decision];
    return [node("span", `badge ${className}`, kind), node("span", "request", template.describe(request))];
  }

  // The decision in plain words: the reason, what changed, and what was
  // queued.
  function plainLines(report) {
    if (report.error) return [node("p", "reason", `Refused at ${report.stage}, before any decision: ${report.error}`)];
    const lines = [];
    if (report.reason) {
      const plain = template.reasons[report.reason.name] ?? words(report.reason.name ?? report.reason.id);
      lines.push(node("p", "reason", `${sentence(plain)}. `, node("span", "code", `${report.reason.name} ${report.reason.id}`)));
    }
    if (report.decision === "Reject") {
      lines.push(node("p", "changes", "Nothing changed, and nothing was queued."));
      return lines;
    }
    const changes = orderedKeys(report.after, fieldNames)
      .filter((key) => JSON.stringify(report.before[key]) !== JSON.stringify(report.after[key]))
      .map((key) => `${label(key)} ${report.before[key]} → ${report.after[key]}`);
    lines.push(node("p", "changes", changes.length === 0 ? "No field changed." : `Changed: ${changes.join("; ")}.`));
    for (const entry of report.outbox) lines.push(node("p", "queued", `Queued ${plainEntry(entry)}.`));
    return lines;
  }

  function identity(report) {
    return report.authorization_id ? ["Authorization", report.authorization_id] : ["Rejection", report.rejection_id];
  }

  // The entry's technical record: the request as sent, the reason's code,
  // the authorization or rejection hash, the bundle and its replay check,
  // and the laws evaluated with their statuses.
  function record(request, report) {
    const list = node("dl", "record-facts");
    const fact = (term, ...detail) => list.append(node("dt", null, term), node("dd", null, ...detail));
    fact("Request", node("code", "json", JSON.stringify(request)));
    if (report.error) {
      fact("Refused at", report.stage);
      fact("Error", report.error);
    } else {
      if (report.reason) fact("Reason", `${report.reason.id} ${report.reason.name}`);
      const [kind, hash] = identity(report);
      fact(kind, node("code", "hash", hash));
      fact("Publication", report.commit
        ? `bundle ${report.bundles}; commit ${report.commit.status}, exact replay ${report.commit.replay}`
        : "nothing was published");
      fact("Laws evaluated", node("ul", "laws", ...report.laws.map((law) => node("li", null, `Law ${law.id} ${law.name}: ${law.status}`))));
    }
    return node("details", "record", node("summary", null, "Technical record"), list);
  }

  function renderLatest(request, report) {
    const lines = [node("div", "verdict", ...verdict(request, report)), ...plainLines(report)];
    if (!report.error) {
      const [kind, hash] = identity(report);
      lines.push(node("p", "identity", `${kind} `, hashNode(hash)));
    }
    latestBody.replaceChildren(...lines);
  }

  // `reveal` scrolls the latest decision into view: only for a decision the
  // viewer caused, never for the demonstration decided when the page
  // loaded, so the page stays at its top.
  function renderReport(request, report, note, reveal) {
    if (count === 0) timeline.replaceChildren();
    count += 1;
    const head = node("div", "entry-head", node("span", "number", `#${count}`), ...verdict(request, report));
    if (note) head.append(node("span", "note", note));
    const body = node("div", "entry-body", ...plainLines(report));
    if (!report.error) {
      const [kind, hash] = identity(report);
      body.append(node("p", "identity", `${kind} `, node("code", "hash short", short(hash))));
    }
    timeline.append(node("li", null, head, body, record(request, report)));
    renderLatest(request, report);
    if (reveal) latest.scrollIntoView({ block: "nearest" });
  }

  function send(request, note, reveal) {
    const report = demo.step(request);
    history.push({ request, report });
    renderReport(request, report, note, reveal);
    renderState(demo.state());
    // The latest box announces the decision; the status keeps to loading,
    // the demonstration, starting over, and input errors.
    if (reveal) status.textContent = "";
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
    send(request, null, true);
  }

  const genesis = `the starting state, its genesis: ${template.genesis}.`;

  function reset() {
    const state = demo.reset();
    count = 0;
    history.length = 0;
    timeline.replaceChildren(node("li", "empty", "No decisions yet."));
    latestBody.replaceChildren(node("p", "empty", "No decision yet. Send a request, or run the demonstration."));
    renderState(state);
    status.textContent = `Started over, at ${genesis}`;
  }

  const delay = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds));

  // Shows each request in the form, then sends it. `instant` skips the pause
  // between requests; `onLoad` says the run happened when the page loaded,
  // which the status then says, and which must not scroll the page.
  async function runDemonstration({ instant = false, onLoad = false } = {}) {
    setBusy(true);
    try {
      reset();
      const total = template.demonstration.length;
      for (const [index, step] of template.demonstration.entries()) {
        const fields = commands.find(({ command }) => command.name === step.request.command)?.fields ?? [];
        for (const field of [...fields, ...context]) field.write(step.request[field.name]);
        send(step.request, step.note, !onLoad);
        if (!instant) {
          status.textContent = `Running the demonstration: request ${index + 1} of ${total}.`;
          await delay(STEP_DELAY_MS);
        }
      }
      status.textContent = `The demonstration from the template's README, decided in this browser${onLoad ? " when the page loaded" : ""}: `
        + `${history.length} requests. Compare each decision with the README, send your own request, or start over.`;
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
  status.textContent = `Ready, at ${genesis}`;
  setBusy(false);
  return { reset, runDemonstration, history, state: () => demo.state(), status, latest };
}
