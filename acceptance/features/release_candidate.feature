@rc3 @release
Feature: Ship one reproducible review candidate
  Maintainers should be able to verify public documentation, package metadata,
  source policy, and release artifacts from one exact clean source revision.

  @atdd-release-contract
  Scenario: Run the local RC3 release gate
    Given the pinned Rust toolchain and locked dependency graph
    When the release contract acceptance stage runs
    Then formatting lint tests doctests rustdoc assurance and package checks pass
    And the result remains release eligibility evidence rather than publication authority

  @atdd-probity-guardrails
  Scenario: Reject unsafe agent workflow actions deterministically
    Given pinned Probity development tooling and the repository rule configuration
    When the hostile and permitted command corpus is evaluated
    Then destructive release and dependency commands are rejected
    And locked pinned validation commands remain permitted
