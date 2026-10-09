@v2_2 @g14
Feature: State and actual deliveries obey the same declared accounting law
  Bounded channel counts and checked numeric payload sums read actual candidates
  through the existing library law evaluator and shared meter.

  @atdd-delivery-accounting-laws
  Scenario: Enforce bounded delivery counts and amounts on actual candidates
    Given escrow accounting laws and explicitly bound delivery observations
    When the candidate releases held funds and enqueues matching payments
    Then zero, single, multiple and separate-channel delivery controls satisfy their declared laws
    And a planted double payout is refused by the accounting law even though both entries fit its bound of two
    And the generated Rust application completes its escrow journey and replays its durable history
    And the generated double-payout application refuses preview and submission with law 510 and leaves its store and journal unchanged
    And missing matching payloads, wrong types, overflow and an exceeded actual length bound refuse
    And malformed, ambiguous, unused and genesis-bearing delivery bindings refuse generation
    And declared scopes, mandatory law kinds, shared read charges and unchanged legacy contracts are retained
    And symbolic delivery observations remain explicitly unsupported and cannot receive Proved or Attested
