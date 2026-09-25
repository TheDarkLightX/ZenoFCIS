// The deploy check's harness. It imports the served artifact's own panel and
// a template's description from the subpath in the query string, mounts the
// panel as the page does, runs the README's demonstration through it, and
// prints the results, with what the panel rendered, for
// site/tests/deploy_check.py to read from the dumped DOM: the decisions, the
// summary, the latest-decision box's verdict, and every hash the panel shows
// by its prefix, which must carry the module's full value. Then, from
// genesis again, it enters each demonstration step into the form and clicks
// its button: every control must send exactly the request the script sends.

const output = document.getElementById("results");
const errors = [];
addEventListener("error", (event) => errors.push(String(event.message)));
addEventListener("unhandledrejection", (event) => errors.push(String(event.reason)));

const parameters = new URLSearchParams(location.search);
const base = new URL(parameters.get("base") ?? "/", location.href);
const names = (parameters.get("templates") ?? "").split(",").filter(Boolean);
const results = {};
const HASH_PREFIX = 12;

const identity = (report) => report.authorization_id ?? report.rejection_id ?? null;
const canonical = (object) => JSON.stringify(Object.fromEntries(Object.entries(object).sort()));

// The latest decision's hash, the state root, and each entry's record must
// show the module's value: the full value behind the prefix, and the prefix
// itself.
function checkHashes(container, history, state) {
  const mismatches = [];
  let checked = 0;
  const revealed = (element, expected, where) => {
    checked += 1;
    const full = element?.dataset.full ?? null;
    const prefix = element?.querySelector(".short")?.textContent ?? null;
    const shown = element?.querySelector(".full")?.textContent ?? null;
    if (full !== expected || shown !== expected || prefix !== `${expected.slice(0, HASH_PREFIX)}…`) {
      mismatches.push({ where, expected, full, prefix, shown });
    }
  };
  revealed(container.querySelector(".latest .hashed"), identity(history[history.length - 1].report), "the latest decision");
  revealed(container.querySelector(".facts .hashed"), state.root, "the state root");
  const entries = container.querySelectorAll(".timeline > li");
  history.forEach(({ report }, index) => {
    checked += 1;
    const expected = identity(report);
    const recorded = entries[index]?.querySelector(".record .hash")?.textContent ?? null;
    const prefix = entries[index]?.querySelector(".entry-body .hash.short")?.textContent ?? null;
    if (recorded !== expected || prefix !== `${expected.slice(0, HASH_PREFIX)}…`) {
      mismatches.push({ where: `entry ${index + 1}`, expected, recorded, prefix });
    }
  });
  return { checked, mismatches };
}

// From genesis, each step of the demonstration is entered into the form
// and sent by its own button; the request the panel sent, and the decision
// it reached, must be the script's.
function checkControls(container, panel, template) {
  const mismatches = [];
  panel.reset();
  for (const [index, step] of template.demonstration.entries()) {
    for (const [key, value] of Object.entries(step.request)) {
      if (key === "command") continue;
      const input = container.querySelector(`.controls [name="${key}"]`);
      if (input === null) mismatches.push({ step: index + 1, missing: key });
      else if (input.type === "checkbox") input.checked = Boolean(value);
      else input.value = String(value);
    }
    const before = panel.history.length;
    container.querySelector(`.controls button[data-command="${step.request.command}"]`)?.click();
    const sent = panel.history.length === before + 1 ? panel.history[before] : null;
    if (sent === null || canonical(sent.request) !== canonical(step.request) || sent.report.decision !== step.expect) {
      mismatches.push({ step: index + 1, expected: step.request, sent: sent?.request ?? null, decided: sent?.report.decision ?? null, expect: step.expect });
    }
  }
  return { checked: template.demonstration.length, mismatches };
}

for (const name of names) {
  try {
    const { mount } = await import(new URL("panel.js", base));
    const { template } = await import(new URL(`templates/${name}.js`, base));
    const container = document.createElement("div");
    document.body.append(container);
    const panel = await mount(container, template);
    await panel.runDemonstration({ instant: true });
    const state = panel.state();
    const history = [...panel.history];
    results[name] = {
      steps: history.map(({ report }) => ({
        decision: report.decision ?? null,
        error: report.error ?? null,
        laws: (report.laws ?? []).map((law) => [law.id, law.status]),
      })),
      summary: template.summary(state, history),
      steps_counted: state.steps,
      status: container.querySelector(".status").textContent,
      timeline: container.querySelectorAll(".timeline > li").length,
      latest: container.querySelector(".latest .badge")?.textContent ?? null,
      proposals: container.querySelectorAll(".proposals > li").length,
      proposals_described: template.proposer ? template.demonstration.filter(template.proposer.filter).length : 0,
      hashes: checkHashes(container, history, state),
      controls: checkControls(container, panel, template),
      errors: [],
    };
  } catch (error) {
    results[name] = { errors: [String(error)] };
  }
}

output.textContent = JSON.stringify({ base: base.href, results, errors });
