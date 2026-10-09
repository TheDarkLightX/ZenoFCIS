Feature: Bounded canonical collections and profile-bound FIFO messages
  Pure reusable state preserves old snapshots and refuses excess retained data.
  Supplied profile IDs do not establish payload schema validity or delivery authority.

  @atdd-bounded-collections-pipes
  Scenario: Bound immutable canonical collections and profile-bound FIFO messages
    Given explicit entry, item and complete snapshot byte bounds
    When map, set, FIFO and profile-bound pipe operations are evaluated
    Then replacement, pending retry, refusal and acknowledgment obey their contracts
    And canonical bytes match independent models across collection backends
