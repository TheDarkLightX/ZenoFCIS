# Side-channel and covert-channel assurance

ZenoFCIS separates semantic determinism from physical leakage. A pure transition can prove that the same admitted inputs produce the same authority-bearing result, but it cannot by itself prove that two secret values produce indistinguishable timing, cache, scheduling, network, logging, power, or electromagnetic observations on a deployed target.

`zeno-fcis-security` makes that missing assurance surface explicit.

## Threat distinction

- An **intended channel** is an authorized protocol output.
- A **side channel** is an incidental observation through which an attacker may infer information.
- A **covert channel** is a channel an untrusted component can actively modulate to bypass the intended authority graph.
- A **storage channel** changes observer-visible state.
- A **timing or microarchitectural channel** changes when or how shared finite hardware resources are used.

Any production claim must bind the exact observer, observation kind, channel class, deployment, threat model, and empirical or mechanized evidence. “Pure Rust,” “memory safe,” “formally verified,” and “constant time at source level” are not substitutes for that binding.

## Assurance flow

```text
information-flow labels and observer clearances
+ closed observation traces for equal public inputs
+ exact side/covert-channel rules
+ explicit declassification authorities and bit budgets
+ deployment contract and mitigation set
    -> deterministic leakage report

leakage reports
+ symbolic/dynamic/statistical evidence
+ microarchitectural experiments
+ active covert-channel capacity measurements
+ deployment and hardware evidence
    -> fail-closed security promotion report
```

## Information-flow lattice

A `SecurityLabel` contains:

- a confidentiality level;
- an integrity level;
- a canonical set of compartments.

Information may flow from source to target only when the target is at least as confidential, includes every source compartment, and does not demand greater integrity than the source possesses.

Labels are not access-control lists. Capabilities still decide who may perform an operation. Labels constrain how information may propagate after authority has been exercised.

## Observation model

The closed observation registry includes:

- explicit output;
- error class;
- termination;
- output length;
- logical work;
- allocation class;
- branch class;
- memory-access class;
- scheduling;
- storage;
- cache, TLB, predictor, and other microarchitectural behavior;
- network behavior;
- logs and diagnostics;
- power;
- electromagnetic emanations.

Projects may model a subset, but every observer-visible surface in the project threat model must have an explicit rule. An unclassified observation fails closed.

## Rule modes

- `Prohibit`: the observation must not occur.
- `Exact`: value, quantity, shape, and absence of declassification must match across secret variants.
- `BoundedQuantity`: values remain equal while a public quantity may vary within an explicit bound.
- `Declassified`: value differences require the exact authority, purpose, and per-observation bit budget.

The comparison also checks aggregate declassification, observer clearance, trace length, ordering, deployment identity, and required mitigations.

## Deployment contract

Side-channel resistance is deployment specific. The contract binds:

- target artifact;
- processor and platform;
- operating system or hypervisor;
- compiler/toolchain;
- core/cache/memory topology;
- scheduler configuration;
- declared mitigations.

The initial mitigation registry includes constant-time control flow, secret-independent memory access, fixed output size, fixed work and termination, rate limiting, queue/core/cache/memory partitioning, IOMMU isolation, microarchitectural flushing, deterministic scheduling, log redaction, disabled core dumps, secret zeroization, no unreviewed shared mutable state, and independently checked compilation.

A deployment declaration is not evidence that the mitigation works. It is a content-addressed claim that later evidence must reference.

## Evidence and promotion

The production gate distinguishes:

- noninterference proof;
- symbolic constant-time analysis;
- dynamic branch/memory analysis;
- statistical timing analysis;
- microarchitectural experiments;
- storage-channel analysis;
- active covert-channel capacity measurements;
- translation validation;
- deployment audit;
- hardware or firmware attestation.

Each item binds its claim, retained artifact, toolchain, and exact deployment. Capacity evidence is required for every modeled side or covert channel and is evaluated against an explicit maximum and confidence threshold.

## Required project workflow

For each production profile:

1. Define security domains, labels, observers, and clearances.
2. Enumerate every intended, side, and covert observation in the threat model.
3. Define exact rules and declassification budgets.
4. Construct a deployment contract for each supported target.
5. Instrument or model two executions with equal public inputs and different secrets.
6. Produce deterministic trace-comparison reports.
7. Run compiled-code constant-time checks for secret-handling routines.
8. Run statistical timing and microarchitectural experiments on each target class.
9. Attempt active covert-channel transmission and retain a measured upper bound.
10. Import independently checked evidence and evaluate the promotion policy.

## LucyOS and time protection

LucyOS profiles should treat time, scheduling, cache state, TLB state, predictors, interrupts, DMA, and shared devices as first-class resources. A context switch across security domains may require a closed machine plan containing cache/TLB partitioning or flush operations, deterministic scheduling, queue partitioning, and a verified hardware/software contract.

A generic application library cannot enforce those mechanisms. ZenoFCIS records what must hold and what evidence supports it; the Lucy machine layer and target hardware must implement and validate the contract.

## Explicit nonclaims

The crate does not:

- establish physical constant-time execution from source inspection;
- prevent power or electromagnetic leakage;
- prove a processor has no undocumented shared state;
- make an untrusted scheduler deterministic;
- enforce cache partitioning, microarchitectural flushing, IOMMU configuration, or core isolation;
- determine an acceptable covert-channel bandwidth for every project;
- replace red-team testing, hardware evaluation, or independent security review.

A successful report means the supplied closed observations and evidence satisfy the supplied policy for the exact deployment. Production authorization remains a project-specific promotion decision.
