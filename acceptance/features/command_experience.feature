Feature: Public command guidance and diagnostics
  @atdd-command-experience
  Scenario: Keep public guide, diagnostics, link parsing and optional telemetry checks active
    Given embedded public command cards and optional invocation observations
    When command, MCP, diagnostic and reporting tests execute
    Then public lookups and refusal recovery retain their declared contracts
    And advisory observations cannot grant decision authority
