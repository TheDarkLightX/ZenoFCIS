# Computational feasibility and complexity-claim integrity

**Status:** prospective review requirement for new algorithmic routes and changes to
their deployment tier. It does not retroactively qualify existing backends,
establish a new proof rule, or authorize publication.

## Scope and rule

An implementation may be mathematically correct and asymptotically better yet
unusable at attainable scales. Conversely, an unproved hardness assumption
does not establish that an improvement is impossible. Review these as
**different claims**.

For every newly proposed verification, optimization, synthesis, model-counting,
or solver-selection route, register an evidence card in
`docs/feasibility/registry.json` before proposing opt-in qualification or
default use. Research-only prototypes also register a card, but can explicitly
record unmeasured or unverified fields. A new algorithmic backend cannot be
promoted solely from a paper, asymptotic bound, demo, benchmark speedup, or
unchecked solver verdict.

This is a **human review obligation** supported by a structural CI preflight,
not an automated declaration that a benchmark or formal proof is true.

## Five independent gates

1. **Correctness and translation.** State the exact input domain,
   specification, algorithm implementation, assumptions and output meanings.
   Identify an independent reference oracle, differential or metamorphic
   checks, and the soundness/completeness scope of the encoding. Preserve
   concrete counterexample replay and proof/certificate-checking rules.
   Distinguish an untrusted SAT/UNSAT report from an independently proved claim.
2. **Asymptotic claim.** Name the complexity model, input-size parameters,
   hidden parameters and whether the statement is proved, formalized,
   conditional, conjectural or still under review. List every conjecture or
   reduction needed to apply the theorem. Neither ETH/SETH nor any other
   unproved hypothesis is evidence of practical impossibility.
3. **Concrete work and memory.** Estimate and/or measure encoding, solving,
   certificate or witness generation, checking, total end-to-end wall time and
   peak resident memory. Account for preprocessing, allocations, serialization,
   conversions, failed runs and external tools. Report fixed budgets and the
   host/toolchain. A faster central kernel with slower end-to-end performance
   is **not** a speedup for users.
4. **Paired empirical comparison and crossover.** Pin candidate and baseline
   revisions, corpus bytes, trial counts, representative and held-out workload
   families, environment, seeds, time and memory limits. Keep timeout, OOM,
   wrong-answer, refusal and inconclusive outcomes in denominators. Report
   per-case raw results as well as aggregate statistics, variability and
   attempted input-size range. Label crossover **observed**, **not observed**,
   or **not tested**; never infer an actual crossover from big-O notation.
   Declare the intended improvement threshold and safety guardrails before
   viewing held-out results. Record negative results.
5. **Assurance and adoption.** Explain any new trusted computing base,
   solver dependencies, encoding limitations, evidence class, fallback, and
   how UNKNOWN and interrupted execution remain inconclusive. Compare the
   qualified reference route under the *same* correctness, safety and
   resource constraints. A cost gain cannot compensate for a failed
   correctness/assurance gate.

Total cost for a workload includes, at minimum, the observed invocation
wall time and a breakdown of work:

```text
serial phase work = encoding + solving + evidence generation
                  + independent checking + other measured overhead
end-to-end wall time = measured at the invocation boundary
```

These stages should be instrumented separately. For **sequential** phases,
the reported total must not be less than the sum of exclusive phase times.
For **overlapping** phases (for example, two solvers running concurrently),
report the overlap explicitly; end-to-end wall time may be less than the sum
of individual timers. Never add overlapping clocks and call it wall time.
Work and wall time should not be combined into an invented single unit.
If assurance differs, compare feasible routes only after enforcing the
required assurance floor.

## Promotion tiers

| Tier | Allowed | Required evidence |
| --- | --- | --- |
| `research` | Prototype, theoretical or experimental work; publish negative results | Explicit assumptions, unverified scope, limitations, fallback and a registered card. No deployed speed or assurance claim. |
| `qualified-opt-in` | Opt-in experiment on declared supported workloads | Independently checked correctness/translation, reviewed assurance boundary, pinned paired baseline/candidate runs, held-out cases, full cost and failure evidence, predeclared acceptance criteria and reviewer record. |
| `default` | Adopt a route as default in its **bounded, documented scope** | All opt-in obligations, a measured advantage on the declared objective under its resource/assurance guardrails, and no unaddressed material regressions for the supported workload. Explicit escape/fallback and a reviewed change decision. |

A favorable result for one case family is not a universal solver claim. A
backend can remain opt-in, or be selected by a scoped portfolio/router rather
than replacing the baseline globally. A theoretical improvement may be worth
publishing without crossing any practical performance gate.

Changing a verifier's authority, evidence status, ABI, protocol identity or
runtime privileges still requires its separate existing release and
qualification gates. These feasibility cards **grant no authority**.

## Machine-readable record and reviewer boundary

The versioned JSON card schema is `zenofcis/computational-feasibility/1`.
Registered cards live in `docs/feasibility/cards/<id>.json`; register the same
`id` and tier in `docs/feasibility/registry.json`. The
`docs/feasibility/example-research.json` record is illustrative, not a measured
result. The stdlib-only preflight is:

```bash
python3 tools/check_computational_feasibility.py --self-test
python3 tools/check_computational_feasibility.py
```

For a measured card, the benchmark section records 40-hex-digit baseline
and candidate commit IDs, a 64-hex-digit corpus SHA-256, environment,
reproducer, predeclared criteria, development/held-out case counts, trial
count, raw evidence and failure-accounting references. Each baseline/candidate
cost has the numeric fields `encoding_ms`, `solving_ms`,
`evidence_ms`, `checking_ms`, `other_ms`, `total_ms`, and
`peak_rss_bytes`, plus `phase_model` (`sequential` or
`overlapping`) and `overlap_notes`. The summary measurements must be
explained by linked per-case raw data; the preflight does not recompute them.
An unmeasured research record uses null for absent measurements and zero
trials/cases. Complexity-theoretic conjectures without a proven conditional
result use `theory.status = "unreviewed"` and list the conjecture explicitly.

The tool checks identity, field types, digest formats, honest unknown
statuses, tier/evidence consistency, phase-cost arithmetic, card registration,
and whether promotion has required evidence references. It **does not**
execute benchmarks, independently validate a source citation, verify a proof,
check that referenced artifacts exist, or discover an unregistered backend in
the code. Reviewers must check those questions and reject omitted new routes.
Retained benchmark data should be source-bound and privacy-reviewed before
public disclosure; links to non-public artifacts are not verification.

## Research motivation

- Moshe Y. Vardi, [The Moral Hazard of Complexity-Theoretic
  Assumptions](https://cacm.acm.org/opinion/the-moral-hazard-of-complexity-theoretic-assumptions/),
  *Communications of the ACM* (2016): avoid conflating conjectural lower bounds
  with established barriers.
- Newman and Vardi, [FPRAS Approximation of the Matrix Permanent in
  Practice](https://arxiv.org/abs/2012.03367) (2020): asymptotically
  advantageous algorithms may have impractical crossover points.
- Existing ZenoFCIS [symbolic evidence classes](SYMBOLIC_CHECKS.md),
  [release assurance](RELEASE_ASSURANCE.md), and [persistent-collection
  backend policy](PERSISTENT_COLLECTIONS.md) remain controlling for their
  respective obligations.
