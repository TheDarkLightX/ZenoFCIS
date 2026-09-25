#!/usr/bin/env node
// Replays each template's decision examples and README demonstration through
// its built module, with no browser, and compares every outcome: the
// decision kind, the reason, the state after, the entries queued, and the
// laws evaluated. Each example's state is reached from genesis by requests
// (tests/templates/<name>.mjs says how), and the demonstration must match
// the gate's expected summary in tools/check_generated_application.py.
//
// Usage: node site/tests/replay.mjs [TEMPLATE...]   (default: every module in site/public)

import { readFileSync, readdirSync } from "node:fs";
import { execFileSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { instantiate } from "../public/demo-module.js";
import { fail } from "./examples.mjs";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const PUBLIC = path.join(ROOT, "site/public");
const GATE_SUMMARIES = "import json, sys; sys.path.insert(0, 'tools'); "
  + "import check_generated_application as gate; print(json.dumps(gate.EXAMPLE_TEMPLATES))";

function same(actual, expected, label) {
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expected);
  if (a !== e) fail(`${label}: got ${a}, expected ${e}`);
}

// The keys of `expected`, taken from `actual`, so that an example compares
// only the fields it names.
function pick(actual, expected) {
  return Object.fromEntries(Object.keys(expected).map((key) => [key, actual?.[key]]));
}

function outcome(report, expected) {
  if (report.error) fail(`refused at ${report.stage}: ${report.error}`);
  return {
    kind: report.decision,
    reason: report.reason ? report.reason.id : null,
    post: pick(report.after, expected.post),
    entries: report.outbox.map((entry, index) => ({
      channel: entry.channel,
      payload: pick(entry.payload, expected.entries[index]?.payload ?? {}),
    })),
  };
}

function checkLaws(report, laws, label) {
  same(report.laws.map((law) => law.id), laws[report.decision], `laws evaluated: ${label}`);
  for (const law of report.laws) {
    if (law.status !== "Satisfied") fail(`law ${law.id} is ${law.status}: ${label}`);
  }
  return report.laws.length;
}

async function replay(name, gate) {
  const { template } = await import(`../public/templates/${name}.js`);
  const examples = await import(`./templates/${name}.mjs`);
  const demo = await instantiate(readFileSync(path.join(PUBLIC, `${name}.wasm`)));

  const genesis = demo.reset();
  same([genesis.bundles, genesis.outbox.length, genesis.steps], [0, 0, 0], "genesis shell");

  const parsed = examples.parse(readFileSync(path.join(ROOT, "site/apps", name, "tests/decision-examples.txt"), "utf8"));
  if (parsed.length < 16) fail(`only ${parsed.length} examples were read`);
  let laws = 0;
  let replayed = 0;
  const skipped = [];
  for (const [index, example] of parsed.entries()) {
    const number = index + 1;
    const why = examples.unreachable[number];
    if (why) {
      skipped.push(`${number}, ${why}`);
      continue;
    }
    demo.reset();
    for (const step of examples.reach(example.pre)) {
      const report = demo.step(step);
      if (report.error || report.decision === "Reject") fail(`cannot reach ${JSON.stringify(example.pre)}: ${JSON.stringify(report)}`);
    }
    const before = demo.state();
    same(pick(before.state, example.pre), example.pre, `state before: ${example.line}`);
    const report = demo.step(example.request);
    same(outcome(report, example.expected), example.expected, `example ${number}: ${example.line}`);
    laws += checkLaws(report, examples.laws, example.line);
    if (report.decision === "Reject") {
      same(report.roots.after, report.roots.before, `root after a rejection: ${example.line}`);
      same(report.after, report.before, `state after a rejection: ${example.line}`);
      same(report.bundles, before.bundles, `bundles after a rejection: ${example.line}`);
    } else {
      // The root is the state's hash, so a commit that leaves the state as it
      // was keeps the root; the bundle count shows the publication.
      same(report.bundles, before.bundles + 1, `bundles after a commit: ${example.line}`);
    }
    replayed += 1;
  }

  same(template.demonstration.map((step) => step.expect), gate.decisions, "the script's expectations against the gate");
  demo.reset();
  const steps = template.demonstration.map((step, index) => {
    const report = demo.step(step.request);
    if (report.error) fail(`demonstration step ${index + 1} refused at ${report.stage}: ${report.error}`);
    laws += checkLaws(report, examples.laws, `demonstration step ${index + 1}`);
    return { request: step.request, report };
  });
  same(steps.map(({ report }) => report.decision), gate.decisions, "demonstration decisions");
  const final = demo.state();
  const summary = template.summary(final, steps);
  same(summary, pick(gate, summary), "demonstration summary");
  same(final.steps, template.demonstration.length, "decisions counted");

  // Bad input is refused before any decision, and the module carries on.
  for (const [text, stage] of examples.refusals) {
    const refusal = demo.stepText(text);
    if (refusal.stage !== stage) fail(`${text} must be refused at ${stage}: ${JSON.stringify(refusal)}`);
  }
  const after = demo.state();
  same([after.state, after.bundles, after.root], [final.state, final.bundles, final.root], "a refusal publishes nothing");

  const unreachable = skipped.length === 0 ? "" : `; not replayed, unreachable from genesis under the rules: ${skipped.join("; ")}`;
  console.log(`replay ${name}: ${replayed} of ${parsed.length} examples from genesis, `
    + `${template.demonstration.length} demonstration steps, ${laws} law statuses: all matched${unreachable}`);
}

const gates = JSON.parse(execFileSync("python3", ["-c", GATE_SUMMARIES], { cwd: ROOT, encoding: "utf8" }));
const names = process.argv.length > 2
  ? process.argv.slice(2)
  : readdirSync(PUBLIC).filter((file) => file.endsWith(".wasm")).map((file) => file.slice(0, -".wasm".length)).sort();
if (names.length === 0) fail("no module in site/public; run site/build.py");
for (const name of names) {
  if (!gates[name]) fail(`no gate summary for ${name}`);
  await replay(name, gates[name]);
}
