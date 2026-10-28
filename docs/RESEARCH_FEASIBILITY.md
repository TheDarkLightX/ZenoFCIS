# Computational feasibility standard for 2.7 research

Experimental verification and synthesis algorithms must be assessed on five
independent axes before a practical improvement is claimed. A proof of correctness,
a better asymptotic bound, fewer IR nodes and a measured speedup are different
results. No weighted score may hide a failed or unknown component.

This complements Crucible's proposal, falsification and independent replay
discipline. It applies to new algorithm experiments and performance claims, not
to ordinary documentation, routing or correctness repairs. It does not replace
the existing source-bound acceptance, proof, privacy or release gates.

## Five required assessments

| Component | Required evidence and scope |
| --- | --- |
| Correctness | Exact input/output specification, admitted domain, arithmetic and failure semantics, assumptions, and independent proof or replay. Separate model correctness, implementation correspondence and coverage. Preserve counterexamples, UNKNOWN and refusal. |
| Asymptotic complexity | Mathematically established time and space bounds, their parameters and hypotheses, worst-case versus amortized/expected scope, and the machine/cost model. Include bit lengths and encoding sizes. A fitted curve or a count of high-level operations is not an asymptotic proof. Record an unproved bound as unknown. |
| Concrete resource model | Actual operation counts, constants, peak memory, allocation and encoding/decoding costs. Include preprocessing, search, proof/certificate construction and independent checking. Distinguish logical fuel, compiled instruction counts and wall-clock time; specify arbitrary-precision versus fixed-width arithmetic. Report work already spent on failed and interrupted runs. |
| Empirical crossover | Paired results against the strongest currently qualified applicable ZenoFCIS backend on representative and held-out workloads. Record measured size regimes where each wins, loses or remains inconclusive; include setup and cold/amortized costs, repetitions, variability, coverage, timeouts and resource stops. Do not extrapolate a crossover beyond measured sizes. |
| Assurance preservation | Exact checker and certificate interfaces, independent replay, source/specification bindings, negative controls, and the trusted computing base before/after. Justify every added trusted dependency or assumption. A fast proposer may remain untrusted; a fast self-authored checker is not independent assurance. |

Each axis records its own supported, refuted or unknown status, analysis,
evidence and open questions. Independently evaluated axes do not require five
different people; they require distinct claims and appropriate evidence. A
bounded test is not relabeled as a general theorem. A finite proof may establish
a law for an infinite family, with its exact statement and assumptions retained.

## Baseline and workload contract

Freeze candidate, baseline and checker source/artifact identities, toolchains,
options, corpus, workload roles, seeds, limits and measurement procedure before
comparison. Keep raw observations and commands. A source or assumption change
invalidates the affected evidence; unrelated unchanged evidence may be reused
with its original identity and a reviewed difference.

Select the strongest qualified backend **for the same semantics, input family
and resource regime**. Record the eligible-backend inventory, qualification
evidence, inventory date and selection rationale. When qualified backends are
incomparable, compare the relevant nondominated baselines or justify the narrower
comparison. A convenient brute-force oracle or an older release alone does not
establish that this requirement is met. Report a missing qualified comparison as
unknown. Never narrow the problem or compare incomplete outputs to gain a win.

Representative workloads cover normal and boundary cases, multiple sizes and
known difficult regimes. Held-out cases must not have been used for proposal,
tuning, selection or counterexample-driven repair. Once consulted for those
purposes they become development data; use a new held-out set. Pin stable case
identities as well as exact content hashes, and state whether families are also
held out. Hidden filenames or fresh random seeds alone do not prove independence.

Use equivalent tasks, outputs, correctness requirements and resource limits.
Record hardware/OS, compiler/dependencies, threads, warm/cold state, run ordering
and measurement uncertainty. Count preprocessing and verification separately and
end to end. Do not omit unsuccessful cases from aggregate coverage or timing.
Wall-clock observations are experimental evidence; they do not enter canonical
decision semantics or replace the knowledge system's deterministic usefulness gate.

## Negative results are retained

Keep slower, larger, unsupported, incorrect and inconclusive candidates with their
source, workload, limits, raw observations and explanation. A failed correctness
check may stop an expensive benchmark; record the other axes as unknown rather
than fabricate measurements. A negative or incomplete report is useful research.

Classify each limitation as fundamental, implementation-specific,
workload-specific, mixed or unresolved. State the evidence, confidence and next
discriminating test. A fundamental claim needs a mathematical lower bound or
other appropriate proof under explicit assumptions; timing alone is insufficient.
Implementation-specific explanations need an isolating change/control, and
workload-specific explanations need a justified regime comparison. Retain an
unresolved diagnosis when the evidence does not distinguish these causes.

A candidate with missing components remains research-only. An observed regression
does not disappear because another metric improved. A fully assessed negative
result is not an improved backend. Independent review, applicable source-bound
checkers and the ordinary release process still decide any later adoption.

## Assessment record and mechanical checks

Copy [the versioned template](../experiments/feasibility-template.json) and record
the five axes separately. The current schema is `zeno-fcis/computational-feasibility/2`;
unknown versions refuse. Candidate and baseline artifacts have retained
`artifact` references `{path, sha256, kind: "artifact"}` matching their declared
artifact hashes. Known toolchains have `{description, path, sha256}` pointing to
a retained toolchain manifest; unknown toolchains/artifacts are `null` and cannot
make a record ready for review. Workload rows are `{id, sha256, path}`; one stable
ID must denote the same bytes across roles. An unknown path may remain `null`
for incomplete research, but blocks review readiness. Hashing a separate unrelated
evidence file cannot establish these subject bindings. No workload content or
backend is executed by this check.

Other evidence references point to retained regular files, with SHA-256 identities
and explicit evidence kinds. Keep unpublished experiment
packets outside publication inputs and follow the [publication policy](PUBLICATION_POLICY.md).

```sh
python3 -B tools/check_research_feasibility.py ASSESSMENT.json
python3 -B tools/check_research_feasibility.py ASSESSMENT.json --evidence-root PACKET --require-ready
```

The first command validates the record and reports missing requirements. The
second also checks referenced file bytes and requires the metadata needed for
independent improvement review. A valid incomplete/negative record is retained;
it need not pass `--require-ready`. Exit 0 means the requested record checks
passed, 1 means review prerequisites are missing, and 2 means malformed data,
overlap or evidence-binding failure.

The checker detects missing axes, declared hold-out overlap, unbound evidence and
unsupported timing-only fundamental diagnoses. It does **not** verify a theorem,
reproduce timings, establish genuine unseen-case independence, determine the
strongest backend, certify explanations or grant qualification/promotion authority.
Those remain explicit independent evaluations. Evidence-kind labels and hashes
do not establish truth or authenticity. No network, solver, imported code or
benchmark is executed by this record checker.
