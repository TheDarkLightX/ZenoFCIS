// The README's scripted demonstration: the nine requests that `journey` in
// the template's src/lib.rs makes, with the decision each expects. The
// interrupted delivery and the database reopen that follow them need the
// SQLite shell and are not part of the page.

export const DEMONSTRATION = [
  { command: "LoginFailed", now: 1000, admin: false, expect: "CommittedFailure", note: "a first failed login is recorded" },
  { command: "LoginFailed", now: 1010, admin: false, expect: "CommittedFailure", note: "a second failure" },
  { command: "LoginFailed", now: 1020, admin: false, expect: "CommittedFailure", note: "the third failure locks the account until 1920 and queues an alert" },
  { command: "LoginSucceeded", now: 1500, admin: false, expect: "Reject", note: "a login during the lock" },
  { command: "LoginSucceeded", now: 1000, admin: false, expect: "Reject", note: "a stale clock" },
  { command: "AdminUnlock", now: 1600, admin: false, expect: "Reject", note: "an unlock without admin" },
  { command: "LoginSucceeded", now: 1920, admin: false, expect: "Accept", note: "the lock has expired" },
  { command: "LoginFailed", now: 2000, admin: false, expect: "CommittedFailure", note: "one more failure" },
  { command: "AdminUnlock", now: 2010, admin: true, expect: "Accept", note: "an administrator unlocks and queues an alert" },
];
