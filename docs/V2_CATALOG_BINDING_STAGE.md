# Complete schema and reviewed V2 policy binding

This bounded unit owns `execution_v2/catalog.rs` and its subtree. Its base is
`1ed6f88f7b1098bed6c409bc9aa3c12c99d43c3c`; copied dependencies and root-owned
registrations remain development source requiring fresh qualification.

Definition of done: a private-construction, borrowed `BoundCatalog` checks
complete original roots, all channel links and every delivery in every branch,
then compares the entire reviewed policy with the authority serializer. The sole
public constructor, `bind_original`, calls `schema::admit` exactly once and retains
its private Checked token by value. A private `bind_checked` composes the stages. All
owned executable helpers have total exact contracts, zero executable requires,
and whole-source pinned proof/translated coverage, independent native checks,
and meaningful proof and coverage controls on frozen bytes.

The predicate is: supplied policy length is allowed; composition admits the
complete descriptor; state root equals the schema's declared root and the
actual State Record; command/context roots independently match their original
leaf or record forms; all ordered channel links match the complete channel
family and original destination/payload types; every Bytes/Text delivery
expression is a constant satisfying the original length and ASCII bounds;
the full library policy encoding succeeds and equals the reviewed bytes.
Refusals have the corresponding order: policy Size, original Schema admission,
Descriptor, Roots, Channels,
Deliveries, Encoding, Policy. Every refusal returns only its typed error.

Bool and full exact i64-backed I128 bounds, arbitrary valid variant-code
permutations, generic zero/max IDs, empty records and unused full-width schema
definitions remain legal. There is no narrowing of a wider original input
bound. Tuple, vector, map, nested record and payload Sum are excluded by the
original checked flat-schema profile. Root is never an alias of Field(0).
Complete numeric and Enum/Sum channel domains match the original definitions;
composition independently checks dynamic candidate values in those domains.
Idempotency remains a complete explicit reviewed descriptor domain.

One result owning the schema witness and borrowing its immutable subjects, plus
direct pure predicates avoid a second schema copy,
policy serializer, authority flag or mutable cache. This is a new admission
boundary, not a refactor of the prior execution path. Existing composition
admission and post-candidate domain checks remain independent acceptance checks.
Borrowed immutable Rust references prevent mutation while the result is live;
private fields prevent construction or replacement by external consumers.
No invocation is executed and no state, meter or outbox is mutated by binding.

Proof plan: prove first-match lookup and complete set/record correspondence;
prove quantifier-based all-channel/all-branch checks with loop prefixes;
compose the existing exact descriptor and policy contracts in refusal order;
prove exact borrowing getters. Challenge full bounds, unused branches, root
substitution, policy trailing bytes, and byte/text constraints independently.
Pin Verus 0.2026.09.27.3cf1832 with verifier Rust 1.98.1, native Rust 1.97.1,
and the actual shared production module closure. Never substitute a model for
an unresolved upstream proof. Receipt status must distinguish those failures.

The owned gate checks the entire translated function inventory, every runtime
contract and body, and specification bodies. Six semantic controls must fail
both independently authored native fixtures and their specific proof obligations:
postconditions, or the all-branch loop invariant for the omitted-outbox control;
four controls must still verify and then fail their intended coverage check.
Compiler failures and solver resource exhaustion do not count as killed controls.
The verifier uses two threads and a solver rlimit of 20; this is a proof-search
budget and does not alter any runtime limit. Evidence paths must be new.

The separate native receipt includes public Cargo integration, four external
custody refusals and the retained composition corpus. For the shared target,
its otherwise-unused compiler cfg includes the exact source-map digest, avoiding
cross-worktree reuse of a stale relative Cargo dep-info file. No application Rust
may refer to that cfg. The receipt records its flags, pinned tools, source hashes
and every exit status; the original default-cache failures remain retained.

The framing schema32 values are reviewed policy fields; this stage does not
recompute SHA or prove a hash adapter. Exact original schema and policy bytes
provide identity correspondence. Human review establishes intended semantics;
frontend/.zeno intent correspondence, mandatory Authority construction, source
identity registration, combined native/Miri/acceptance/review/CI and the wider
V2 ledger remain separate obligations. Compiler/erasure, vstd, solver, allocator
and platform are named trusted dependencies; construction has no physical cost
claim. This unit grants no commit, deployment or release authority.
