// The compliance-gateway example, as the page presents it: its state field
// with a plain label, the context and commands in the README's words, the
// reasons in plain words, and the README's scripted demonstration: the
// twelve requests that `journey` in the template's src/lib.rs makes, with
// the decision each expects. The interrupted delivery and the database
// reopen that follow them need the SQLite shell and are not part of the page.

const screen = (region, band, risk, tier, expect, note) => ({
  request: { command: "Screen", region, amount_band: band, counterparty_risk: risk, identity_tier: tier, reviewer: false },
  expect,
  note,
});
const REGIONS = { Allowed: "an allowed region", Restricted: "a restricted region", Sanctioned: "a sanctioned region" };
const reinstate = (tier, reviewer, expect, note) => ({
  request: { command: "Reinstate", identity_tier: tier, reviewer },
  expect,
  note,
});

export const template = {
  name: "compliance-gateway",
  fields: [{ name: "strikes", label: "Strikes" }],
  help: "Describe the customer, then screen a transfer or ask for a reinstatement. The shell would band the amount, rate the counterparty, verify the identity tier, and authenticate the reviewer; here the form supplies those facts.",
  contextLegend: "The customer and the caller",
  context: [
    { name: "identity_tier", label: "Identity tier", hint: "0 is unverified, up to 3", kind: "integer", min: 0, max: 3, initial: 2 },
    { name: "reviewer", label: "Caller is a compliance reviewer", kind: "flag", initial: false },
  ],
  groups: [
    {
      legend: "Screen a transfer",
      commands: [
        {
          name: "Screen",
          label: "Screen the transfer",
          fields: [
            { name: "region", label: "Region", kind: "choice", options: [["Allowed", "Allowed"], ["Restricted", "Restricted"], ["Sanctioned", "Sanctioned"]], initial: "Allowed" },
            { name: "amount_band", label: "Amount band", hint: "0 under 100, 1 under 1,000, 2 under 10,000, 3 under 50,000, 4 above", kind: "integer", min: 0, max: 4, initial: 1 },
            { name: "counterparty_risk", label: "Counterparty risk", kind: "choice", options: [["Low", "Low"], ["Medium", "Medium"], ["High", "High"]], initial: "Low" },
          ],
        },
      ],
    },
    {
      legend: "Reinstate the account",
      commands: [{ name: "Reinstate", label: "Reinstate", fields: [] }],
    },
  ],
  genesis: "no strikes, nothing queued",
  outbox: "Review tickets to review-queue on channel 300 (review_ticket), naming the rule and the amount band, and block alerts to compliance-team on channel 301 (block_alert), naming the rule and the strikes on record. Nothing delivers in this page, so each stays pending with its delivery identity.",
  describe: (request) => request.command === "Screen"
    ? `Screen a transfer to ${REGIONS[request.region] ?? request.region}, amount band ${request.amount_band}, ${request.counterparty_risk.toLowerCase()} risk, identity tier ${request.identity_tier}${request.reviewer ? ", by a reviewer" : ""}`
    : `Reinstate the account, identity tier ${request.identity_tier}${request.reviewer ? ", by a reviewer" : ""}`,
  reasons: {
    not_reviewer: "the caller is not a reviewer",
    no_strikes: "the account has no strikes",
    sanctioned_region: "blocked by the rule sanctioned_region: the region is sanctioned",
    frozen_account: "blocked by the rule frozen_account: three strikes freeze the account",
    unverified_high_risk: "blocked by the rule unverified_high_risk: an unverified customer and a high-risk counterparty",
    restricted_large: "blocked by the rule restricted_large: a large transfer to a restricted region",
    unverified_large: "blocked by the rule unverified_large: a large transfer by an unverified customer",
  },
  demonstration: [
    screen("Allowed", 1, "Low", 2, "Accept", "a verified customer sends under 1,000 to an allowed region: allowed by default_allow"),
    screen("Restricted", 2, "Low", 2, "Accept", "under 10,000 to a restricted region: held by restricted_region, and a review ticket is queued"),
    screen("Sanctioned", 0, "Low", 3, "CommittedFailure", "a fully verified customer sends to a sanctioned region: blocked by sanctioned_region, one strike, and an alert"),
    reinstate(3, false, "Reject", "someone who is not a reviewer asks to reinstate: not_reviewer"),
    screen("Allowed", 3, "High", 0, "CommittedFailure", "an unverified customer sends under 50,000 to a high-risk counterparty: blocked by unverified_high_risk, two strikes"),
    screen("Allowed", 2, "Low", 2, "Accept", "a verified customer sends under 10,000: with two strikes on record, held by repeat_offender"),
    screen("Restricted", 4, "Medium", 3, "CommittedFailure", "over 50,000 to a restricted region: blocked by restricted_large, three strikes, and the account is frozen"),
    screen("Allowed", 0, "Low", 3, "CommittedFailure", "under 100 to an allowed region: blocked by frozen_account, the strikes stay at three"),
    reinstate(3, true, "Accept", "a reviewer reinstates the account: no strikes"),
    reinstate(3, true, "Reject", "the reviewer reinstates again: no_strikes"),
    screen("Allowed", 4, "Low", 0, "CommittedFailure", "an unverified customer sends over 50,000: blocked by unverified_large, one strike"),
    screen("Allowed", 4, "Medium", 3, "Accept", "a fully verified customer sends over 50,000, medium risk: held by medium_risk_large, and a review ticket is queued"),
  ],
  // The demonstration's end, in the fields the gate's summary in
  // tools/check_generated_application.py names: a screening that is
  // accepted without a ticket is allowed, one accepted with a ticket is
  // held, and a committed failure is blocked, as the demonstration counts
  // them. Nothing delivers in the page, so the notices the gate's shell
  // delivered are the notices pending here.
  summary: (state, steps) => {
    const screenings = steps.filter(({ request }) => request.command === "Screen");
    return {
      strikes: state.state.strikes,
      allowed: screenings.filter(({ report }) => report.decision === "Accept" && report.outbox.length === 0).length,
      held: screenings.filter(({ report }) => report.decision === "Accept" && report.outbox.length > 0).length,
      blocked: screenings.filter(({ report }) => report.decision === "CommittedFailure").length,
      bundles: state.bundles,
      deliveries: state.outbox.filter((entry) => !entry.acknowledged).length,
    };
  },
};
