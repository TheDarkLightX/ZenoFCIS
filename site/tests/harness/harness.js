// The deploy check's harness. It imports the served artifact's own panel and
// a template's description from the subpath in the query string, mounts the
// panel as the page does, runs the README's demonstration through it, and
// prints the results, with what the panel rendered, for
// site/tests/deploy_check.py to read from the dumped DOM.

const output = document.getElementById("results");
const errors = [];
addEventListener("error", (event) => errors.push(String(event.message)));
addEventListener("unhandledrejection", (event) => errors.push(String(event.reason)));

const parameters = new URLSearchParams(location.search);
const base = new URL(parameters.get("base") ?? "/", location.href);
const names = (parameters.get("templates") ?? "").split(",").filter(Boolean);
const results = {};

for (const name of names) {
  try {
    const { mount } = await import(new URL("panel.js", base));
    const { template } = await import(new URL(`templates/${name}.js`, base));
    const container = document.createElement("div");
    document.body.append(container);
    const panel = await mount(container, template);
    await panel.runDemonstration({ instant: true });
    const state = panel.state();
    results[name] = {
      steps: panel.history.map(({ report }) => ({
        decision: report.decision ?? null,
        error: report.error ?? null,
        laws: (report.laws ?? []).map((law) => [law.id, law.status]),
      })),
      summary: template.summary(state, panel.history),
      steps_counted: state.steps,
      status: container.querySelector(".status").textContent,
      timeline: container.querySelectorAll(".timeline > li").length,
      proposals: container.querySelectorAll(".proposals > li").length,
      proposals_described: template.proposer ? template.demonstration.filter(template.proposer.filter).length : 0,
      errors: [],
    };
  } catch (error) {
    results[name] = { errors: [String(error)] };
  }
}

output.textContent = JSON.stringify({ base: base.href, results, errors });
