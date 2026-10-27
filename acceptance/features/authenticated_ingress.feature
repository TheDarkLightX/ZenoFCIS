Feature: Authenticated ingress around one durable command connection

  @atdd-authenticated-ingress
  Scenario: Authenticate HTTP commands and recover one durable SQLite connection
    Given an immutable credential-to-principal binding and a checked application contract
    When an HTTP command is committed with its expected revision and original retry key
    Then the original packet is recovered after a lost reply without a second application
    And changed commands, duplicate headers, oversized requests and cross-origin requests refuse
    And committed outbox records survive faults and abrupt process termination
    And the trace carries correlation identifiers without the credential
