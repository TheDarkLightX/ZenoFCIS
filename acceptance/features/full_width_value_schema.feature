Feature: Explicit full-width value and schema boundaries

  @atdd-full-width-value-schema
  Scenario: Admit nominal full-width values through explicit codec2 and Schema2 boundaries
    Given an explicitly selected experimental full-width value and schema profile
    When owned values and canonical envelopes are admitted through Schema2
    Then full unsigned and signed ranges retain their nominal types and exact bytes
    And stale identities, wrong widths, invalid ranges and exceeded resources are refused
    And the default legacy profile retains its existing meaning
    And structural admission grants no publication or effect authority
