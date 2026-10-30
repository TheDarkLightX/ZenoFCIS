Feature: Checked machine-code synthesis
  Let a proposer, the bounded search or a ZAL author produce x86-64 and RV64I
  word kernels whose exact bytes an independent checker decides, and keep the
  claim to the registered ISA model.

  @atdd-machine-code-synthesis
  Scenario: Check, search, emit, replay and execute machine-code kernels under both profiles
    Given the independent profile-1 and profile-2 decoders and checkers and the checked kernel library
    When the proposal, search, emission, replay, library and ZAL word-kernel suites run under normal and optimized Python
    And emitted x86 kernels and every admitted x86 form run on the executing CPU
    Then every library kernel rechecks as equivalent at its recorded cost and planted wrong kernels fail
    And the concrete and SMT models agree with the CPU, including CF, ZF, SF and OF for profile 2
    And RV64I execution, timing claims and global optimality remain outside this scenario
