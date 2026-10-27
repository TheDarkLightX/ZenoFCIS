Feature: Build a durable application from a reviewed contract

  @atdd-app-factory-journey
  Scenario: Generate and run the checked core, shell, ingress and UI together
    Given a reviewed standalone contract with complete presentation metadata
    When one command builds its application offline with the copied lock
    Then authenticated HTTP journeys accept and refuse the expected commands
    And SQLite publishes state, receipt, original replay key and outbox atomically
    And the UI enables commands only after an accepting core preview
    And lost replies recover with the original bytes and key after reopening
    And the ingress fault corpus preserves the store and trace assertions
