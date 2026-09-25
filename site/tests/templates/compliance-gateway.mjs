// The compliance-gateway examples: how tests/decision-examples.txt is read,
// how each example's standing is reached from genesis, and which laws each
// decision kind evaluates. From project.zeno: regions, counterparty risks,
// and rules by numeric ID; and the laws profile.rs enforces on each decision
// kind (500 on every commit, 501 and 502 on accepts, 503 on committed
// failures, 509 on rejections).

import { integer, kind, lines, named, reason } from "../examples.mjs";

const REGIONS = { 160: "Allowed", 161: "Restricted", 162: "Sanctioned" };
const RISKS = { 165: "Low", 166: "Medium", 167: "High" };
const RULES = {
  170: "SanctionedRegion", 171: "FrozenAccount", 172: "UnverifiedHighRisk", 173: "RestrictedLarge",
  174: "UnverifiedLarge", 175: "HighRiskCounterparty", 176: "RestrictedRegion", 177: "UnverifiedTransfer",
  178: "PartiallyVerifiedLarge", 179: "RepeatOffender", 180: "MediumRiskLarge", 181: "DefaultAllow",
};

export const laws = { Accept: [500, 501, 502], CommittedFailure: [500, 503], Reject: [509] };

// Inputs refused before any decision, with the stage that refuses them.
export const refusals = [
  ["{", "input"],
  ['{"command":"Screen","region":"Allowed","amount_band":5,"counterparty_risk":"Low","identity_tier":2,"reviewer":false}', "admission"],
];

// Every example's standing is reachable from genesis.
export const unreachable = {};

// `pre.120 action region band risk tier reviewer | outcome reason post.120 |
// notice`, as tests/conformance.rs parses it: a ticket on channel 300 names
// the rule and the amount band, an alert on channel 301 the rule and the
// strikes on record. A reinstatement ignores the transfer fields, which the
// page never sends with it.
export function parse(text) {
  return lines(text).map(({ line, parts: [input, decision, notice] }) => {
    const [strikes, action, region, band, risk, tier, reviewer] = input;
    const [outcome, why, postStrikes] = decision;
    const context = { identity_tier: integer(tier, line), reviewer: reviewer === "1" };
    const request = action === "150"
      ? { command: "Screen", region: named(REGIONS, region, line), amount_band: integer(band, line), counterparty_risk: named(RISKS, risk, line), ...context }
      : { command: "Reinstate", ...context };
    let entries = [];
    if (notice[0] === "300") {
      entries = [{ channel: 300, payload: { ticket_rule: named(RULES, notice[1], line), ticket_amount_band: integer(notice[2], line) } }];
    } else if (notice[0] === "301") {
      entries = [{ channel: 301, payload: { alert_rule: named(RULES, notice[1], line), alert_strikes: integer(notice[2], line) } }];
    }
    return {
      line,
      pre: { strikes: integer(strikes, line) },
      request,
      expected: { kind: kind(outcome, line), reason: reason(why), post: { strikes: integer(postStrikes, line) }, entries },
    };
  });
}

// Requests that take a clean standing to the example's: one transfer to a
// sanctioned region per strike, each blocked. The replay checks the result.
export function reach({ strikes }) {
  return Array.from({ length: strikes }, () => ({
    command: "Screen", region: "Sanctioned", amount_band: 0, counterparty_risk: "Low", identity_tier: 3, reviewer: false,
  }));
}
