# Algebraic preservation in a Rust refactor

Pure functions support algebraic reasoning, but functional architecture does not require representing every program as applicative syntax. A Rust typestate API and an inspectable program representation solve different problems: the first restricts construction and call sequences; the second lets tools inspect, interpret, or transform a program.

## Select the meaning to preserve

Name the legal domain, operations, observations, and proposed equations. Ordinary total function composition has identity and associativity; this does not make every operation inside a function associative. The relevant domain and semantics must justify each rewrite.

For example, checked integer addition is not associative when overflow refusal is observable. Both expressions below start with the same valid `i8` values; their intermediate results differ:

```rust
fn main() {
    let (a, b, c) = (127_i8, 1_i8, -1_i8);
    let left = a.checked_add(b).and_then(|ab| ab.checked_add(c));
    let right = b.checked_add(c).and_then(|bc| a.checked_add(bc));
    assert_eq!(left, None);
    assert_eq!(right, Some(127));
}
```

Likewise, removing an apparently unused computation can remove a checked overflow or change measured resource usage in an eager evaluator. Pure input/output code can still have observable refusal semantics.

For a finite program comparison, fix the original domain and compare the actual executable artifacts under a declared observation profile. A complete result must cover every admitted input; sampling, domain shrinking, timeout, or partial enumeration cannot establish equivalence. Do not add a new comparison engine when the project already owns this acceptance route.

Preserve exactly the observations required by the profile. If an optimization legitimately changes resource usage, specify and check that refinement separately, including budgets and resource-observing laws. Update the artifact identity and dependent evidence. Do not silently treat changed usage, failure behavior, or ordered effects as irrelevant.

## Connection to the supplied paper

Nakahata's [*Foundations of Algebraic Architecture Theory*](https://arxiv.org/abs/2609.27638), particularly the introduction and §§1.8–1.9, makes the retained structure and laws an explicit choice. It distinguishes preserving observable reads from preserving operations; its protocol model specifies transition maps, observations, and path equations. Extending finite local descriptions to all protocol paths requires the stated compatibility conditions and induction.

The practical inference for Rust is to specify preservation before choosing the representation. Typestate alone does not discharge those equations. A claimed reuse of the paper's theorems needs a correspondence between the implementation's semantics and the theorem's objects and assumptions; the paper does not automatically verify a Rust refactor. This skill uses the architectural lesson, not an independently replayed proof of the paper's formalization.

ZenoFCIS's `docs/V2_1_FACTORY_PLAN.md`, section 6, already separates functional equivalence from application equivalence. The latter covers complete decisions and authoritative observations. Follow the current implementation and qualification status: the plan is not evidence that an optimizer is released.
