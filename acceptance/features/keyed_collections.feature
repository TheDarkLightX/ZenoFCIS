Feature: Keyed collections
  @atdd-keyed-collections
  Scenario: Check maps of any size by data independence over their named entries
    Given a contract whose state holds maps keyed by opaque 32-byte identifiers
    When the project is parsed
    Then each map lowers to the entries its commands name, with keys as equality classes
    And keys compare only by equality and maps are read only through named entries
    And every admissible view, over every equality pattern, decides lawfully
    And aliased entries write one value and every summary changes by its entries' change
    And each planted defect fails its named test
