@v2_3 @language_profiles
Feature: Additional host language profile
  A foreign host consumes the actual generated Rust application without
  becoming a second evaluator. The profile is qualified on Linux x86_64 with
  the pinned OCaml 4.14.1 packages only.

  @atdd-ocaml-scalar-consumer
  Scenario: Consume actual generated Rust applications from the pinned OCaml client
    Given the pinned OCaml 4.14.1 toolchain and fresh C3 and endpoint applications from this source
    When the client and the native applications run the frozen corpus, negatives, installed journey and recovery
    Then every frame, decision report, refusal and store byte equals the native application's
    And both checked-overflow controls are refused before any store exists
