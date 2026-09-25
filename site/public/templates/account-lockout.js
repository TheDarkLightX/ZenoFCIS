// The account-lockout example, as the page presents it: its state fields
// with plain labels, the context and commands in the README's words, the
// reasons in plain words, and the README's scripted demonstration: the nine
// requests that `journey` in the template's src/lib.rs makes, with the
// decision each expects. The interrupted delivery and the database reopen
// that follow them need the SQLite shell and are not part of the page.

const step = (command, now, admin, expect, note) => ({ request: { command, now, admin }, expect, note });
const LABELS = { LoginSucceeded: "Login succeeded", LoginFailed: "Login failed", AdminUnlock: "Admin unlock" };

export const template = {
  name: "account-lockout",
  fields: [
    { name: "failed_attempts", label: "Failed attempts" },
    { name: "locked_until", label: "Locked until" },
    { name: "last_seen", label: "Last decision's time" },
  ],
  help: "Set the request's time and whether it is marked admin, then send the login's result. Each button sends one command through the authority.",
  contextLegend: "The request's context",
  context: [
    { name: "now", label: "Time", hint: "Unix seconds, 0 to 4,102,444,800", kind: "integer", min: 0, max: 4102444800, initial: 1000 },
    { name: "admin", label: "Marked admin", kind: "flag", initial: false },
  ],
  groups: [
    {
      legend: "Send a request",
      commands: [
        { name: "LoginSucceeded", label: LABELS.LoginSucceeded, fields: [] },
        { name: "LoginFailed", label: LABELS.LoginFailed, fields: [] },
        { name: "AdminUnlock", label: LABELS.AdminUnlock, fields: [] },
      ],
    },
  ],
  genesis: "every field zero, no bundles, no alerts",
  outbox: "Alerts to security-team on channel 300 (security_alert). Nothing delivers in this page, so each stays pending with its delivery identity.",
  describe: (request) => `${LABELS[request.command] ?? request.command} at ${request.now}${request.admin ? ", marked admin" : ""}`,
  reasons: {
    clock_regressed: "the time is earlier than the last decision's",
    account_locked: "the account is locked",
    not_admin: "the request is not marked admin",
    login_failed: "the login failed, and the attempt is recorded",
  },
  demonstration: [
    step("LoginFailed", 1000, false, "CommittedFailure", "a first failed login is recorded"),
    step("LoginFailed", 1010, false, "CommittedFailure", "a second failure"),
    step("LoginFailed", 1020, false, "CommittedFailure", "the third failure locks the account until 1920 and queues an alert"),
    step("LoginSucceeded", 1500, false, "Reject", "a login during the lock"),
    step("LoginSucceeded", 1000, false, "Reject", "a stale clock"),
    step("AdminUnlock", 1600, false, "Reject", "an unlock without admin"),
    step("LoginSucceeded", 1920, false, "Accept", "the lock has expired"),
    step("LoginFailed", 2000, false, "CommittedFailure", "one more failure"),
    step("AdminUnlock", 2010, true, "Accept", "an administrator unlocks and queues an alert"),
  ],
  // The demonstration's end, in the fields the gate's summary in
  // tools/check_generated_application.py names. Nothing delivers in the
  // page, so the alerts the gate's shell delivered are the alerts pending here.
  summary: (state) => ({
    failed_attempts: state.state.failed_attempts,
    locked_until: state.state.locked_until,
    last_seen: state.state.last_seen,
    bundles: state.bundles,
    deliveries: state.outbox.filter((entry) => !entry.acknowledged).length,
  }),
};
