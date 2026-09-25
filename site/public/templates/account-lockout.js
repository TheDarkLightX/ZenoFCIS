// The account-lockout example, as the page presents it: its state fields,
// the context and commands in the README's words, and the README's scripted
// demonstration: the nine requests that `journey` in the template's
// src/lib.rs makes, with the decision each expects. The interrupted delivery
// and the database reopen that follow them need the SQLite shell and are
// not part of the page.

const step = (command, now, admin, expect, note) => ({ request: { command, now, admin }, expect, note });

export const template = {
  name: "account-lockout",
  fields: ["failed_attempts", "locked_until", "last_seen"],
  help: "Set the request's context, then propose a request. Each button sends one command through the authority.",
  context: [
    { name: "now", label: "Time, in Unix seconds (0 to 4,102,444,800)", kind: "integer", min: 0, max: 4102444800, initial: 1000 },
    { name: "admin", label: "Request marked admin", kind: "flag", initial: false },
  ],
  commands: [
    { name: "LoginSucceeded", label: "Login succeeded", fields: [] },
    { name: "LoginFailed", label: "Login failed", fields: [] },
    { name: "AdminUnlock", label: "Admin unlock", fields: [] },
  ],
  genesis: "every field zero, no bundles, no alerts",
  outbox: "Alerts to security-team on channel 300 (security_alert). Nothing delivers in this page, so each stays pending with its delivery identity.",
  describe: (request) => `${request.command} at ${request.now}${request.admin ? ", admin" : ""}`,
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
