Feature: Compatible coordination stores
  @atdd-commons-rust
  Scenario: Check the Rust coordination store against the Python reference
    Given the Rust command and the independent Python reference
    When both implementations read and write bounded test stores
    Then live records, leases, hash chains and MCP results agree
    And unavailable archive operations retain their explicit refusal
