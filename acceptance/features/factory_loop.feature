@v2_1 @factory
Feature: Check a candidate program before the factory may use it
  Untrusted proposers may suggest a smaller program. The factory accepts one
  only with a receipt from an exhaustive comparison on the verified evaluator.

  @atdd-checked-transform
  Scenario: Accept a candidate program only with an exhaustive equivalence receipt
    Given an original finite program and a candidate with the same input and output ABI
    When the transform checker runs both programs on every tuple of the declared input domain
    Then equal results on every tuple, with the declared Step limit never binding, yield a canonical receipt
    And replaying the receipt against the same programs reproduces it byte for byte
    And the first differing tuple is reported as a counterexample and no receipt is written
    And a binding Step limit, a domain above the cap or a changed ABI never yields an equivalence
