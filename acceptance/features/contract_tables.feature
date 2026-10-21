Feature: Fixed-size contract tables
  @atdd-contract-tables
  Scenario: Expand fixed-size tables exactly into ordinary fields and laws
    Given a contract that declares a table of entries, an index type and table laws
    When the project is parsed
    Then every entry is an ordinary field whose ID follows from the table's ID
    And entry IDs that meet any other declaration are refused
    And each forall, exists, sum and count is the documented expansion over every entry
    And the printed expansion parses to the same canonical project
    And errors point at the table source
    And the library loans example decides through its expanded laws
    And each planted expansion defect fails its named test
