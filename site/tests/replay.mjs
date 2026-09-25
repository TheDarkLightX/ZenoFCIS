#!/usr/bin/env node
// Replays the template's decision examples and the README's demonstration
// through the built module, with no browser, and compares every outcome:
// the decision kind, the reason, the account after, the alerts, and the laws
// evaluated. The demonstration must also match the gate's expected summary.
//
// Usage: node site/tests/replay.mjs [MODULE]   (default: site/public/account-lockout.wasm)

import { readFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { instantiate } from "../public/demo-module.js";
import { DEMONSTRATION } from "../public/demonstration.js";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const MODULE = process.argv[2] ?? path.join(ROOT, "site/public/account-lockout.wasm");
// The application as `zeno-fcis new` wrote it: the examples as shipped.
const EXAMPLES = path.join(ROOT, "site/apps/account-lockout/tests/decision-examples.txt");
const GATE_SUMMARY = "import json, sys; sys.path.insert(0, 'tools'); "
  + "import check_generated_application as gate; print(json.dumps(gate.EXAMPLE_TEMPLATES['account-lockout']))";

// From project.zeno: command variants and alert kinds by numeric ID; and the
// laws profile.rs enforces on each decision kind (500 on every commit, 501
// and 502 on accepts, 503 on committed failures, 509 on rejections).
const COMMANDS = { 120: "LoginSucceeded", 121: "LoginFailed", 122: "AdminUnlock" };
const ALERT_KINDS = { Locked: 150, Unlocked: 151 };
const KINDS = { accept: "Accept", reject: "Reject", failure: "CommittedFailure" };
const LAWS_BY_DECISION = { Accept: [500, 501, 502], CommittedFailure: [500, 503], Reject: [509] };
const LOCK_SECONDS = 900;

function fail(message) {
  throw new Error(message);
}

function same(actual, expected, label) {
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expected);
  if (a !== e) fail(`${label}: got ${a}, expected ${e}`);
}

// `pre.110 pre.111 pre.112 command now admin | outcome reason post.110
// post.111 post.112 | alert`, as tests/conformance.rs parses it.
function parseExamples(text) {
  return text.split("\n").map((line) => line.trim()).filter((line) => line && !line.startsWith("#")).map((line) => {
    const [input, decision, alert] = line.split("|").map((part) => part.trim());
    const [failed, until, seen, command, now, admin] = input.split(/\s+/).map(Number);
    const [kind, reason, post110, post111, post112] = decision.split(/\s+/);
    return {
      line,
      pre: [failed, until, seen],
      request: { command: COMMANDS[command] ?? fail(`unknown command ${command}: ${line}`), now, admin: admin === 1 },
      expected: {
        kind: KINDS[kind] ?? fail(`unknown outcome ${kind}: ${line}`),
        reason: reason === "-" ? null : Number(reason),
        post: [Number(post110), Number(post111), Number(post112)],
        alerts: alert === "-" ? [] : [alert.split(/\s+/).map(Number)],
      },
    };
  });
}

// Requests that take a new account to [failed, until, seen]: a lock is three
// failures ending 900 seconds before the deadline; further failures, or a
// login, then run at or after the deadline. The caller checks the result.
function scriptTo([failed, until, seen]) {
  const steps = [];
  const failure = (now) => steps.push({ command: "LoginFailed", now, admin: false });
  if (until > 0) {
    const locked = until - LOCK_SECONDS;
    failure(locked - 20);
    failure(locked - 10);
    failure(locked);
    if (failed === 0 && seen !== locked) steps.push({ command: "LoginSucceeded", now: seen, admin: false });
  }
  for (let i = failed - 1; i >= 0; i -= 1) failure(seen - 10 * i);
  return steps;
}

function outcome(report) {
  if (report.error) fail(`refused at ${report.stage}: ${report.error}`);
  const after = report.after;
  return {
    kind: report.decision,
    reason: report.reason ? report.reason.id : null,
    post: [after.failed_attempts, after.locked_until, after.last_seen],
    alerts: report.outbox.map((entry) => [entry.channel, ALERT_KINDS[entry.alert_kind] ?? fail(`unknown alert ${entry.alert_kind}`), entry.alert_until]),
  };
}

function checkLaws(report, label) {
  same(report.laws.map((law) => law.id), LAWS_BY_DECISION[report.decision], `laws evaluated: ${label}`);
  for (const law of report.laws) {
    if (law.status !== "Satisfied") fail(`law ${law.id} is ${law.status}: ${label}`);
  }
  return report.laws.length;
}

function account(state) {
  return [state.account.failed_attempts, state.account.locked_until, state.account.last_seen];
}

const demo = await instantiate(readFileSync(MODULE));

const genesis = demo.reset();
same(account(genesis), [0, 0, 0], "genesis account");
same([genesis.bundles, genesis.outbox.length, genesis.steps], [0, 0, 0], "genesis shell");

const examples = parseExamples(readFileSync(EXAMPLES, "utf8"));
if (examples.length < 16) fail(`only ${examples.length} examples were read`);
let laws = 0;
for (const example of examples) {
  demo.reset();
  for (const step of scriptTo(example.pre)) {
    const report = demo.step(step);
    if (report.error || report.decision === "Reject") fail(`cannot reach ${example.pre}: ${JSON.stringify(report)}`);
  }
  const before = demo.state();
  same(account(before), example.pre, `account before: ${example.line}`);
  const report = demo.step(example.request);
  same(outcome(report), example.expected, `example: ${example.line}`);
  laws += checkLaws(report, example.line);
  if (report.decision === "Reject") {
    same(report.roots.after, report.roots.before, `root after a rejection: ${example.line}`);
    same(report.after, report.before, `account after a rejection: ${example.line}`);
    same(report.bundles, before.bundles, `bundles after a rejection: ${example.line}`);
  } else {
    // The root is the state's hash, so a commit that leaves the account as it
    // was keeps the root; the bundle count shows the publication.
    same(report.bundles, before.bundles + 1, `bundles after a commit: ${example.line}`);
  }
}

const summary = JSON.parse(execFileSync("python3", ["-c", GATE_SUMMARY], { cwd: ROOT, encoding: "utf8" }));
same(DEMONSTRATION.map((step) => step.expect), summary.decisions, "the script's expectations against the gate");
demo.reset();
const decisions = DEMONSTRATION.map((step, index) => {
  const report = demo.step({ command: step.command, now: step.now, admin: step.admin });
  if (report.error) fail(`demonstration step ${index + 1} refused at ${report.stage}: ${report.error}`);
  laws += checkLaws(report, `demonstration step ${index + 1}`);
  return report.decision;
});
same(decisions, summary.decisions, "demonstration decisions");
const final = demo.state();
same([...account(final), final.bundles],
  [summary.failed_attempts, summary.locked_until, summary.last_seen, summary.bundles], "demonstration summary");
// Nothing delivers here, so the alerts the gate's shell delivered stay pending.
same(final.outbox.filter((entry) => !entry.acknowledged).length, summary.deliveries, "pending alerts");
same(final.steps, DEMONSTRATION.length, "decisions counted");

// Bad input is refused before any decision, and the module carries on.
for (const [text, stage] of [["{", "input"], ['{"command":"LoginFailed","now":-1,"admin":false}', "admission"]]) {
  const refusal = demo.stepText(text);
  if (refusal.stage !== stage) fail(`${text} must be refused at ${stage}: ${JSON.stringify(refusal)}`);
}
same([...account(demo.state()), demo.state().bundles], [...account(final), final.bundles], "a refusal publishes nothing");

console.log(`replay: ${examples.length} examples, ${DEMONSTRATION.length} demonstration steps, ${laws} law statuses: all matched`);
