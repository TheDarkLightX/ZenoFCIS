// The deploy check's harness. It imports the served artifact's own loader and
// each template's description from the subpath in the query string, runs the
// README's demonstration through each served module, and prints the results
// for site/tests/deploy_check.py to read from the dumped DOM.

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
    const { instantiate } = await import(new URL("demo-module.js", base));
    const { template } = await import(new URL(`templates/${name}.js`, base));
    const response = await fetch(new URL(`${name}.wasm`, base));
    if (!response.ok) throw new Error(`${response.status} ${response.statusText}`);
    const demo = await instantiate(await response.arrayBuffer());
    demo.reset();
    const steps = template.demonstration.map((step) => {
      const report = demo.step(step.request);
      return {
        decision: report.decision ?? null,
        error: report.error ?? null,
        laws: (report.laws ?? []).map((law) => [law.id, law.status]),
      };
    });
    const state = demo.state();
    results[name] = { steps, summary: template.summary(state), steps_counted: state.steps, errors: [] };
  } catch (error) {
    results[name] = { errors: [String(error)] };
  }
}

output.textContent = JSON.stringify({ base: base.href, results, errors });
