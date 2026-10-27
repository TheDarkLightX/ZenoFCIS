# Compliance gateway

This finite example screens transfers under a declared policy. It is not a claim
of compliance with any jurisdiction's regulations. The checked V2 Authority reads
`v2/policy.json` and independently evaluates the laws in `project.zeno` before
publishing. The original `rules.txt` and synthesis artifacts retain the same rule
order and historical behavior; their LLM-generated runtime is retained in the
private verification oracle, rather than used by the normal V2 decision.

## Prioritized policy

Standing is a strike count from 0 to 3, initially zero. Inputs include region,
amount band 0..4, risk, identity tier 0..3, and a trusted reviewer flag. `Screen`
selects the first matching rule in the following order:

| Rule | Condition | Result |
| --- | --- | --- |
| SanctionedRegion | Sanctioned region | Block |
| FrozenAccount | Three strikes | Block |
| UnverifiedHighRisk | Tier 0 and high risk | Block |
| RestrictedLarge | Restricted region and band >=3 | Block |
| UnverifiedLarge | Allowed region, tier 0 and band >=3 | Block |
| HighRiskCounterparty | High risk | Hold |
| RestrictedRegion | Restricted region | Hold |
| UnverifiedTransfer | Tier 0 and band >=1 | Hold |
| PartiallyVerifiedLarge | Tier 1 and band >=3 | Hold |
| RepeatOffender | At least two strikes and band >=2 | Hold |
| MediumRiskLarge | Medium risk and band 4 | Hold |
| DefaultAllow | Otherwise | Allow |

A Block is a committed failure: it adds a strike, capped at three, and queues
one alert naming the selected rule and new strike count to `compliance-team`
on channel 301. A Hold accepts without changing strikes and queues one ticket
naming the selected rule and amount band to `review-queue` on channel 300.
An Allow accepts without changing strikes or queuing anything; it does not
report a selected rule in an outbox message. These are screening decisions and
not actual transfer authorizations to a bank or settlement service.

`Reinstate` accepts only for a trusted reviewer and an account with at least one
strike. It clears strikes and queues nothing. Wrong reviewer rejects with 200;
an authorized reviewer with no strikes rejects with 201. Screen cannot be
arbitrarily rejected. Every rejection changes nothing and queues nothing.

## Independent laws

Original laws 500–503 retain strike bounds, accepted-state preservation,
reinstatement authority and block accounting. Law 504 excludes every required
block from acceptance. Law 505 requires the first blocking rule's failure reason;
506 permits rejection only for the documented failed reinstatement guards.
Laws 510–513 bind the exact ticket or alert count, channel, rule variant, amount
band or post-strike count, and recipient, including absence of unrelated notices.
The generated structural reject law 909 checks unchanged state and no effects
alongside project law 506; policy-conformance law 991 also remains mandatory.
Claim 600 retains the inductive strike bound.

The policy is finite: 2,880 admitted input tuples and 28 shipped examples. Native
contract review checks all of them. Counterpolicy tests separately require refusal
of block-to-allow, deny-all, missing-ticket and wrong-rule outcomes. A law-removal
control demonstrates that a valid screening can be wrongly rejected if law 506
is removed, then verifies its exact restoration.

## Boundaries and checks

Identity tier and reviewer status are host-authenticated context, not claims a
remote user may choose. There is no custody ledger or persistent held-transfer
record. Ticket resolution, review staffing, transfer execution and message delivery
are external responsibilities. The example does not guarantee timely review,
privacy of screening inputs, or eventual movement of money. The demonstration
receiver's idempotency ledger lasts only within one process.

Use `zeno-fcis generate contract <directory>` and its `--check` mode to reproduce
the V2 declaration. `contract review` compares decisions with the examples;
`contract lemma` checks the strike invariant; `tools/test_template_rules.py`
exercises independent counterpolicies. Earlier native conformance, determinism,
and synthesis tests remain historical reference checks of the same policy.

Generate with `zeno-fcis new --template compliance-gateway <directory>` and run
`cargo +1.97.1 test --offline` in that application. The default `sqlite` feature
supplies the durable demonstration; the checked declarations also build without
it. Source identity changes require a new database or a reviewed migration.

## Lineage

Prioritized decision rules and guarded transitions are established techniques.
For guarded commands see E. W. Dijkstra, “Guarded Commands, Nondeterminacy and
Formal Derivation of Programs,” Communications of the ACM 18(8), 1975,
[doi:10.1145/360933.360975](https://doi.org/10.1145/360933.360975).
Prior-art search pending; no novelty claim is made.
