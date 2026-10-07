---
name: rust-typestate-newtype
description: Use when designing or refactoring Rust validation boundaries or workflows with illegal operation sequences, repeated validation, shared result accumulation, or state-specific data. Choose checked newtypes, consuming typestate transitions, or explicit enums to preserve invariants with the smallest clear design; distinguish type guarantees from formal verification. Applies to ordinary Rust and FCIS systems, including ZenoFCIS.
---

# Rust typestate and checked newtypes

Encode a lasting value invariant or a meaningful workflow restriction in a type. Start with the invariant, not a pattern. Use this skill for a concrete validation or sequencing problem; do not convert every function into a state machine.

## Choose the representation

Before editing, name the invalid value or illegal operation that callers can currently express. Identify its constructor, downstream consumers, and whether its truth depends on changing external state.

| Problem | Prefer | Avoid |
| --- | --- | --- |
| A value must satisfy a lasting property | Checked newtype with a private field and fallible constructor | Revalidating the same immutable value everywhere |
| Different states permit different operations or contain different data | Separate state structs and consuming transitions | Flags plus methods that must inspect every flag |
| Alternatives are selected at runtime | Purpose-named enum with exhaustive matching | Nested `Result`s or empty collections as implicit business outcomes |
| A simple linear transformation needs no state restriction | Ordinary function and ordinary data | A typestate framework with no invalid sequence to prevent |
| Validity depends on current policy, database state, time, or resource limits | Explicit runtime check with its context | Treating an earlier checked type as permanent authorization |

Prefer concrete state structs. Add marker generics only when they remove substantial repetition without admitting irrelevant state combinations. Use `Result` for fallible technical operations; choose business outcome names that make the decision clear.

## Establish and preserve the invariant

1. Keep the wrapped value private; construct it through `TryFrom` or a named checked constructor. Return a useful failure. Carry the checked type through domain signatures instead of immediately erasing it to the primitive.
2. Inspect every construction and mutation route: public fields, sibling modules, `Default`, deserialization, conversions, builders, interior mutability, and unchecked helpers. Ensure they preserve the invariant or restrict them to an explicitly trusted scope. Deserialization should delegate to checked construction when serialized input is untrusted.
3. Distinguish immutable values from linear capabilities. A quantity may be `Copy` or `Clone`; an authorization token or one-use workflow state usually must not be. Do not provide a public conversion from a report or serialized bytes into an authorization capability.
4. Immutable getters and `AsRef` can be useful. Add `Deref` only if the exposed operations preserve the invariant and the domain meaning stays clear. Avoid `DerefMut`, unchecked infallible conversions, and mutable access that can invalidate a checked value.
5. Use consuming `self` transitions when the old state must cease to be usable. Return the next state or an explicit branching outcome. Keep only the data relevant to that state; preserve needed recovery data on failure.

Rust privacy restricts external construction; code inside the defining module is still trusted to obey the invariant. `#[must_use]` encourages handling but is not an execution guarantee. Consuming a non-cloneable instance prevents its reuse through ordinary safe Rust; it does not establish global idempotency, durable settlement, or exclusive authority over external resources.

## Preserve the functional-core boundary

Put deterministic decisions over supplied data in the core. Return a decision and any effect intents. Put clocks, environment reads, storage, network calls, and actual effect execution in the shell; a typed shell workflow may encode the order of those operations.

Use owned state values to replace result/error vectors passed around and modified by unrelated steps. Exclusive invocation-local scratch mutation is compatible with a pure invocation when it cannot escape or affect another invocation. Do not classify a function solely from an `&mut` parameter, or copy large buffers merely to remove the spelling of mutation.

Pass external dependencies only to the shell steps that need them. A type checked against a snapshot needs runtime identity/version checks, resource checks, and an atomic current-state comparison before a durable commit. Type-level sequencing does not replace these checks.

## Validate the change

Verify useful behavior and boundary cases through the public API. For an important construction or ordering boundary, include an external-consumer negative compilation example that fails for the intended reason, plus a positive consumer that succeeds. Also check the constructor's semantic cases; a compilation failure cannot prove the constructor validates correctly.

Run the project's required checks. Preserve existing behavioral tests, replay rules, and proof obligations. Report separately what types exclude, what tests demonstrate, and what a proof establishes. Measure performance only if making a performance claim; moving data by ownership does not establish zero runtime cost.

For an algebraic refactor, name the operations, legal domain, and observations the change must preserve. Check a proposed law under actual overflow, refusal, resource-meter, and effect-order semantics. Pure functions alone do not justify rewriting every program as applicative syntax or assuming familiar numeric laws hold.

See [the compiled reservation example](references/examples.md) for a small checked value and consuming workflow. For ZenoFCIS, also read [the existing checked program route](references/zenofcis.md). When a refactor relies on equations or optimization, read [algebraic preservation](references/algebraic-preservation.md).

## Evidence behind this skill

Heuer, Lu, and Haase, *Functional State Machines in Rust: Typestate and Newtype Patterns (Experience Report)*, FUNARCH 2026, [DOI 10.1145/3830438.3830958](https://doi.org/10.1145/3830438.3830958). The authors studied three production-service refactorings at one company and interviewed four engineers. They report useful invariant enforcement and testability, with additional boilerplate and mixed readability judgments. Those observations support selective use, not universal performance, maintainability, or correctness claims.

Primary language examples: [Rust's type-based state transitions](https://doc.rust-lang.org/book/ch18-03-oo-design-patterns.html) and [private-field encapsulation](https://doc.rust-lang.org/book/ch18-01-what-is-oo.html). The constructor discipline follows [Parse, don't validate](https://lexi-lambda.github.io/blog/2019/11/05/parse-don-t-validate/).
