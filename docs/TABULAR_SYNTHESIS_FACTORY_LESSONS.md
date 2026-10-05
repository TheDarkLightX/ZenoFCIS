# Strategy synthesis and privacy: lessons for the factory

Assessed 2026-10-02. These are research inputs to the planned V2.1 tracks;
they add no implemented capability or V2 completion claim.

## What the papers establish

[Tabular Data Synthesis with Differential Privacy: A Survey
(2411.03351v1)](https://arxiv.org/html/2411.03351v1) separates statistical
fidelity, downstream utility and privacy evaluation. Synthetic records can
disclose information; a differential privacy claim needs a specified randomized
mechanism and composition argument. A utility score cannot supply that argument.
See sections 2.2 and 5.

[Creation begins with understanding: LLMs as strategy designers for
privacy-preserving tabular data synthesis
(2608.29674)](https://arxiv.org/pdf/2608.29674) presents TabSSD: local dependence
summaries guide an LLM to propose executable strategies, which are evaluated
locally before selection. The study reports results on 12 datasets and uses
empirical privacy metrics. Its discussion identifies formal privacy mechanisms
for summary extraction and validation as future work. Sending summaries instead
of raw records therefore must not be described as a differential privacy proof.
See Methods, Algorithm 1 and Discussion, pages 9–12.

## Proposed ZenoFCIS application

The transferable idea is to have agents propose explicit strategies. Our
acceptance boundary should require supported declarations and checked complete
decisions, rather than accepting arbitrary generated Python because it ran.
This is our proposed adaptation, not a capability demonstrated by either paper.

For the existing V2.1 specification-review and integration tracks:

- Give the proposer reviewed schemas, rules, independent examples and complete
  obligations. Bind compact context to its full source; a summary does not
  replace reading the handoff or checking omitted constraints.
- Generate several candidates within a declared search budget. Admit and check
  each candidate before ranking usefulness. If none qualify, return an explicit
  unsuccessful search without publishing a candidate.
- Ask for witnesses distinguishing alternative policies. Replay them through
  the supported semantics and retain the owner's chosen meaning.
- Measure maintenance across repeated changes, including failed attempts and
  human review. A runnable candidate is only one acceptance condition.

An optional future component-family experiment is a privacy-budget controller.
It could check reservations, exact arithmetic, refusal and durable release
authorization. That theorem would establish controller behavior. Differential
privacy of a released dataset would additionally require checked mechanism,
sensitivity, randomness and composition obligations, including any released
summaries or data-dependent selection signals. The papers do not supply a
drop-in proof of such a ZenoFCIS component.

Explicit randomness can make execution a pure function of an input tape, but
deterministic replay alone does not prove a probability-distribution guarantee.
Recording or publishing that tape also needs a separate information-release
contract. Empirical privacy metrics and a formal privacy parameter must have
different names and evidence labels.

No TabSSD, sampler or privacy dependency is installed. Arbitrary statistical
model execution is outside the current supported V2 producer profile. Finish
the [original V2 contract](V2_VERIFIED_CORE_PLAN.md) first; evaluate these ideas
within the existing [V2.1 tracks](V2_1_FACTORY_PLAN.md).
