Feature: Zeno Commons, the shared state of the software factory
  @atdd-zeno-commons
  Scenario: Share factory state, negative memory and coordination through Zeno Commons
    Given agents registered with a unique name and a hash ID
    When they lease what they use, post status, send numbered messages and record what they learn
    Then concurrent writers lose nothing and the hash-chained history verifies
    And a lease on a folder protects its parent and everything inside it
    And read receipts only move forward and done is refused while leases are held
    And advise returns relevant lessons and refutations but not superseded ones
    And factory tools report events only when the store is configured, without changing their results
    And each planted defect fails its named test
