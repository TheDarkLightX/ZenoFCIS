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
const PROPOSALS_SHOWN = 3;
const KIND_LABELS = {
  Accept: ["accept", "Accepted"],
  CommittedFailure: ["failure", "Committed failure"],
  Reject: ["reject", "Rejected"],
};
// The module's session, from site/ABI.md: 64 complete requests between
// resets, refused at the input stage after that; and a reply too large for
// its response buffer, which retires the session after a decision may have
// executed.
const SESSION_LIMIT = 64;
const LIMIT_ERROR = "session limit reached";
const LIMIT_MESSAGE = `This example has handled its ${SESSION_LIMIT} requests. Start over to begin a fresh session.`;
const TRANSPORT_MESSAGE = "The module's reply was too large for the page to read. A decision may have "
  + "executed, and the page cannot show it: the state and history shown are from before this request. "
  + "Starting over is required; it begins a fresh session.";
const BOUNDARY_MESSAGE = "The module stopped answering. A decision may have executed, and the page cannot "
  + "show it. Start over to begin a fresh session; if that fails, reload the page.";
// The commit step runs after the authority authorized the decision (see
// site/common/src/demo.rs): the shell publishes first, then the exact
// replay is checked. A refusal there comes after the publication.
const COMMIT_MESSAGE = "The commit step failed after the authority authorized the decision. The decision "
  + "may already have been saved, and the page cannot show the result. Start over to begin a fresh session.";
const CONSUMED_MESSAGE = "This session's shell was consumed by an earlier failed commit step, so nothing more "
  + "can be decided in it. Start over to begin a fresh session.";
// The stages that refuse before any decision, and what each one checks.
const BEFORE_DECISION = {
  input: "the module could not read the page's request",
  admission: "the schema refused a value",
  authority: "the authority refused to admit or execute the request",
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

// A refused request in plain words, by the stage that refused it. Input,
// admission, and authority refuse before any decision, so nothing changed.
// The commit step refuses after the authority authorized the decision,
// which may already have been saved; a reply the page could not read comes
// after a decision may have executed; a module that stopped answering is
// worded the same way. None of those three claims a rollback, and each ends
// the session: only starting over continues. `detail` is the module's own
// text, shown as secondary text.
export function describeRefusal(report) {
  const stage = report.stage;
  const error = String(report.error ?? "");
  if (stage === "transport") return { kind: "transport", label: "Not shown", text: TRANSPORT_MESSAGE, detail: error, ends: true };
  if (stage === "boundary") return { kind: "transport", label: "Not shown", text: BOUNDARY_MESSAGE, detail: error, ends: true };
  if (stage === "commit") {
    const consumed = error.startsWith("the shell was consumed");
    return { kind: "commit", label: consumed ? "Refused" : "Not shown", text: consumed ? CONSUMED_MESSAGE : COMMIT_MESSAGE, detail: error, ends: true };
  }
  if (stage === "input" && error.startsWith(LIMIT_ERROR)) return { kind: "limit", label: "Refused", text: LIMIT_MESSAGE, detail: error, ends: false };
  const why = BEFORE_DECISION[stage] ?? `the module refused the request at its ${stage} stage`;
  return { kind: "refused", label: "Refused", text: `Refused before any decision: ${why}. Nothing changed.`, detail: error, ends: false };
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
  // An enumerated value in the template's plain words; the raw value stays
  // in the technical record and beside the state line.
  const values = template.values ?? {};
  const plainValue = (key, value) => values[key]?.[String(value)] ?? String(value);

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
  // The session's count, beside the actions: how many of the module's 64
  // requests this session has used, or that it has used them all.
  const session = node("p", "session");
  session.id = `${template.name}-session`;
  form.append(node("div", "actions", demonstrationButton, resetButton, session));

  const status = node("p", "status", "Loading the module.");
  status.id = `${template.name}-status`;
  status.setAttribute("role", "status");
  status.setAttribute("aria-live", "polite");

  // The latest decision: its verdict and plain lines are the live region;
  // the hash, whose button reveals the full value, sits outside it so that
  // revealing does not announce the decision again.
  const latestHeading = node("h3", null, "Latest decision");
  latestHeading.id = `${template.name}-latest-heading`;
  const latestBody = node("div", "latest-body", node("p", "empty", "No decision yet. Send a request, or run the demonstration."));
  latestBody.setAttribute("aria-live", "polite");
  latestBody.setAttribute("aria-atomic", "true");
  const latestIdentity = node("div", "latest-identity");
  const latest = node("section", "latest", latestHeading, latestBody, latestIdentity);
  latest.id = `${template.name}-latest`;
  latest.setAttribute("aria-labelledby", latestHeading.id);

  const facts = node("dl", "facts");
  facts.id = `${template.name}-state`;
  const outbox = node("ol", "outbox", node("li", "empty", "Nothing queued."));
  const timeline = node("ol", "timeline", node("li", "empty", "No decisions yet."));
  timeline.id = `${template.name}-timeline`;
  // The history is collapsed behind its count; every entry stays in the
  // document.
  const historySummary = node("summary", null, "No decisions yet");
  const history = node("details", "history", historySummary,
    node("p", "help", "Accepted decisions and committed failures are published; a rejection changes nothing; a request the schema or the authority refuses never becomes a decision. Each entry's technical record holds the laws evaluated and the hashes."),
    timeline);

  // A template may show its scripted proposer on its own: each of its
  // proposals from the demonstration, with a button that sends it against
  // the state as it is now. The first few are shown; a button shows the rest.
  const proposerButtons = [];
  let proposer = null;
  if (template.proposer) {
    const proposals = template.demonstration.filter(template.proposer.filter);
    const items = proposals.map((step, index) => {
      const button = node("button", "send", "Send");
      button.type = "button";
      button.disabled = true;
      button.addEventListener("click", () => {
        if (!busy) send(step.request, step.note, true);
      });
      proposerButtons.push(button);
      const item = node("li", null, node("div", "proposal",
        node("span", "text",
          node("span", "request", template.describe(step.request)),
          node("span", "note", `${step.note}: ${KIND_LABELS[step.expect][1].toLowerCase()} in the script`)),
        button));
      item.hidden = index >= PROPOSALS_SHOWN;
      return item;
    });
    proposer = node("section", "panel proposer",
      node("h3", null, template.proposer.label),
      node("p", "help", template.proposer.help),
      node("ol", "proposals", ...items));
    if (items.length > PROPOSALS_SHOWN) {
      const more = node("button", "more", `Show all ${items.length} proposals`);
      more.type = "button";
      more.setAttribute("aria-expanded", "false");
      more.addEventListener("click", () => {
        const expand = more.getAttribute("aria-expanded") !== "true";
        for (const [index, item] of items.entries()) item.hidden = !expand && index >= PROPOSALS_SHOWN;
        more.textContent = expand ? `Show the first ${PROPOSALS_SHOWN} proposals` : `Show all ${items.length} proposals`;
        more.setAttribute("aria-expanded", String(expand));
      });
      proposer.append(more);
    }
  }

  container.replaceChildren(
    form,
    status,
    latest,
    node("div", "panels",
      node("section", "panel", node("h3", null, "State"), facts),
      node("section", "panel", node("h3", null, "Outbox"), node("p", "help", template.outbox), outbox)),
    ...(proposer ? [proposer] : []),
    node("section", "panel", history),
  );

  const sendButtons = [...commands.map(({ button }) => button), ...proposerButtons];
  let demo = null;
  let busy = false;
  let count = 0;
  // Requests this session has sent to the module, which counts them
  // against its limit; `exhausted` once it has used them all, and `broken`
  // after a refusal that ended the session, until the next reset.
  let used = 0;
  let exhausted = false;
  let broken = false;
  // Every request sent since the last reset, with the module's report.
  const sent = [];

  function setBusy(value) {
    busy = value;
    for (const button of sendButtons) button.disabled = value || demo === null || exhausted || broken;
    demonstrationButton.disabled = value || demo === null;
    resetButton.disabled = value || demo === null;
  }

  function refreshSession() {
    exhausted = exhausted || used >= SESSION_LIMIT;
    session.classList.toggle("limit", exhausted || broken);
    session.textContent = broken ? "This session ended. Start over to begin a fresh one."
      : exhausted ? LIMIT_MESSAGE : `${used} of ${SESSION_LIMIT} requests used in this session.`;
    setBusy(busy);
  }

  function renderState(state) {
    facts.replaceChildren();
    // Plain labels and values; the schema's own names and raw values are
    // one hover away here, and shown in each decision's technical record.
    for (const key of orderedKeys(state.state, fieldNames)) {
      const raw = String(state.state[key]);
      const plain = plainValue(key, raw);
      const term = node("dt", null, label(key));
      term.title = key;
      const value = node("dd", null, plain);
      if (plain !== raw) value.title = raw;
      facts.append(term, value);
    }
    const committed = node("dt", null, "Decisions committed");
    committed.title = "bundles";
    facts.append(
      node("dt", null, "Decisions made"), node("dd", null, String(state.steps)),
      committed, node("dd", null, String(state.bundles)),
      node("dt", null, "State fingerprint (state root)"), node("dd", null, hashNode(state.root)),
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
    const [className, kind] = report.error
      ? [describeRefusal(report).kind, describeRefusal(report).label]
      : KIND_LABELS[report.decision];
    return [node("span", `badge ${className}`, kind), node("span", "request", template.describe(request))];
  }

  function changed(report) {
    return orderedKeys(report.after, fieldNames)
      .filter((key) => JSON.stringify(report.before[key]) !== JSON.stringify(report.after[key]));
  }

  // The decision in plain words: the reason, what changed, and what was
  // queued; or the refusal, by its stage.
  function plainLines(report) {
    if (report.error) {
      const refusal = describeRefusal(report);
      return [node("p", "reason", `${refusal.text} `, node("span", "code", refusal.detail))];
    }
    const lines = [];
    if (report.reason) {
      const plain = template.reasons[report.reason.name] ?? words(report.reason.name ?? report.reason.id);
      lines.push(node("p", "reason", `${sentence(plain)}. `, node("span", "code", `${report.reason.name} ${report.reason.id}`)));
    }
    if (report.decision === "Reject") {
      lines.push(node("p", "changes", "Nothing changed, and nothing was queued."));
      return lines;
    }
    // Each changed field as a small chip: its label, then before → after.
    const changes = changed(report);
    lines.push(changes.length === 0
      ? node("p", "changes", "No field changed.")
      : node("div", "changes", node("span", "changes-label", "Changed"),
        node("ul", "chips", ...changes.map((key) => node("li", null, node("span", "key", label(key)),
          ` ${plainValue(key, report.before[key])} → ${plainValue(key, report.after[key])}`)))));
    for (const entry of report.outbox) lines.push(node("p", "queued", `Queued ${plainEntry(entry)}.`));
    return lines;
  }

  function identity(report) {
    return report.authorization_id ? ["Authorization", report.authorization_id] : ["Rejection", report.rejection_id];
  }

  // The entry's technical record: the request as sent, the reason's code,
  // the fields changed with their raw values, the authorization or rejection
  // hash, the bundle and its replay check, and the laws evaluated with their
  // statuses.
  function record(request, report) {
    const list = node("dl", "record-facts");
    const fact = (term, ...detail) => list.append(node("dt", null, term), node("dd", null, ...detail));
    fact("Request", node("code", "json", JSON.stringify(request)));
    if (report.error) {
      fact("Stage", report.stage);
      fact("Module's text", report.error);
    } else {
      if (report.reason) fact("Reason", `${report.reason.id} ${report.reason.name}`);
      const raw = changed(report).map((key) => `${key} ${report.before[key]} → ${report.after[key]}`);
      if (raw.length > 0) fact("Fields changed", node("code", "json", raw.join("; ")));
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
    latestBody.replaceChildren(node("div", "verdict", ...verdict(request, report)), ...plainLines(report));
    latestIdentity.replaceChildren();
    if (!report.error) {
      const [kind, hash] = identity(report);
      latestIdentity.append(node("p", "identity", `${kind} `, hashNode(hash)));
    }
  }

  // `reveal` scrolls the latest decision into view: only for a decision the
  // viewer caused, never for the demonstration decided when the page
  // loaded, so the page stays at its top.
  function renderReport(request, report, note, reveal) {
    if (count === 0) timeline.replaceChildren();
    count += 1;
    historySummary.textContent = `All ${count} decision${count === 1 ? "" : "s"}, in order`;
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

  // Sends one request and renders the module's report. After a refusal that
  // ended the session, the state stays as it was shown, since the module can
  // no longer report it, and only starting over continues.
  function send(request, note, reveal) {
    let report;
    try {
      report = demo.step(request);
    } catch (error) {
      report = { error: error.message, stage: "boundary" };
    }
    sent.push({ request, report });
    const refusal = report.error ? describeRefusal(report) : null;
    if (refusal?.kind !== "limit") used += 1;
    renderReport(request, report, note, reveal);
    if (refusal?.ends) {
      broken = true;
      status.textContent = refusal.text;
    } else {
      renderState(demo.state());
      // The latest box announces the decision; the status keeps to loading,
      // the demonstration, starting over, the session, and input errors.
      if (reveal) status.textContent = "";
    }
    refreshSession();
    if (exhausted && !broken && (reveal || refusal?.kind === "limit")) status.textContent = LIMIT_MESSAGE;
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
    used = 0;
    exhausted = false;
    broken = false;
    sent.length = 0;
    timeline.replaceChildren(node("li", "empty", "No decisions yet."));
    historySummary.textContent = "No decisions yet";
    latestBody.replaceChildren(node("p", "empty", "No decision yet. Send a request, or run the demonstration."));
    latestIdentity.replaceChildren();
    renderState(state);
    refreshSession();
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
      status.textContent = `The README's demonstration, decided in this browser${onLoad ? " when the page loaded" : ""}: `
        + `${sent.length} requests.`;
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
  // `send` is for the checks: it pushes a request past the disabled buttons.
  return { reset, runDemonstration, send: (request) => send(request, null, false), history: sent, state: () => demo.state(), status, latest, session };
}
