// The deploy check's harness. It imports the served artifact's own loader and
// demonstration script from the subpath in the query string, runs the
// README's demonstration through the module, and prints the results for
// site/tests/deploy_check.py to read from the dumped DOM.

const results = document.getElementById("results");
const errors = [];
addEventListener("error", (event) => errors.push(String(event.message)));
addEventListener("unhandledrejection", (event) => errors.push(String(event.reason)));

function print(value) {
  results.textContent = JSON.stringify(value);
}

try {
  const base = new URL(new URLSearchParams(location.search).get("base") ?? "/", location.href);
  const { instantiate } = await import(new URL("demo-module.js", base));
  const { DEMONSTRATION } = await import(new URL("demonstration.js", base));
  const response = await fetch(new URL("account-lockout.wasm", base));
  if (!response.ok) throw new Error(`${response.status} ${response.statusText}`);
  const demo = await instantiate(await response.arrayBuffer());
  demo.reset();
  const steps = DEMONSTRATION.map((step) => {
    const report = demo.step({ command: step.command, now: step.now, admin: step.admin });
    return {
      decision: report.decision ?? null,
      error: report.error ?? null,
      laws: (report.laws ?? []).map((law) => [law.id, law.status]),
    };
  });
  const state = demo.state();
  print({
    template: "account-lockout",
    base: base.href,
    steps,
    state: state.account,
    bundles: state.bundles,
    pending: state.outbox.filter((entry) => !entry.acknowledged).length,
    errors,
  });
} catch (error) {
  print({ template: "account-lockout", errors: [...errors, String(error)] });
}
