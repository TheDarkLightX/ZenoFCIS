// The account-lockout examples: how tests/decision-examples.txt is read, how
// each example's account is reached from genesis, and which laws each
// decision kind evaluates. From project.zeno: command variants and alert
// kinds by numeric ID; and the laws profile.rs enforces on each decision kind
// (500 on every commit, 501 and 502 on accepts, 503 on committed failures,
// 509 on rejections).

import { fail, integer, kind, lines, named, reason } from "../examples.mjs";

const COMMANDS = { 120: "LoginSucceeded", 121: "LoginFailed", 122: "AdminUnlock" };
const ALERT_KINDS = { 150: "Locked", 151: "Unlocked" };
const LOCK_SECONDS = 900;

export const laws = { Accept: [500, 501, 502], CommittedFailure: [500, 503], Reject: [509] };

// Inputs refused before any decision, with the stage that refuses them.
export const refusals = [
  ["{", "input"],
  ['{"command":"LoginFailed","now":-1,"admin":false}', "admission"],
];

// Every example's account is reachable from genesis.
export const unreachable = {};

// `pre.110 pre.111 pre.112 command now admin | outcome reason post.110
// post.111 post.112 | alert`, as tests/conformance.rs parses it.
export function parse(text) {
  return lines(text).map(({ line, parts: [input, decision, alert] }) => {
    const [failed, until, seen, command, now, admin] = input;
    const [outcome, why, post110, post111, post112] = decision;
    const account = (attempts, deadline, time) => ({
      failed_attempts: integer(attempts, line),
      locked_until: integer(deadline, line),
      last_seen: integer(time, line),
    });
    return {
      line,
      pre: account(failed, until, seen),
      request: { command: named(COMMANDS, command, line), now: integer(now, line), admin: admin === "1" },
      expected: {
        kind: kind(outcome, line),
        reason: reason(why),
        post: account(post110, post111, post112),
        entries: alert[0] === "-" ? [] : [{
          channel: integer(alert[0], line),
          payload: { alert_kind: named(ALERT_KINDS, alert[1], line), alert_until: integer(alert[2], line) },
        }],
      },
    };
  });
}

// Requests that take a new account to the example's account: a lock is three
// failures ending 900 seconds before the deadline; further failures, or a
// login, then run at or after the deadline. The replay checks the result.
export function reach({ failed_attempts: failed, locked_until: until, last_seen: seen }) {
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
  if (until > 0 && failed > 0 && seen < until) fail(`no request reaches a failure count inside a lock: ${JSON.stringify({ failed, until, seen })}`);
  return steps;
}
