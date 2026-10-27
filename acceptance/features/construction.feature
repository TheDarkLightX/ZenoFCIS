Feature: Construct a component from frozen requirements
  A reviewed requirement and owner approval constrain construction over the entire declared domain.

  @atdd-construction-command
  Scenario: Refuse unsupported families and preserve construction authority boundaries
    Given a frozen priority-collection requirement, its owner approval and a layout
    When I invoke construct or request its machine-readable description
    Then unsupported families refuse by name before creating output
    And an existing output directory is preserved
    And construction declares its input, artifact and subprocess effects
