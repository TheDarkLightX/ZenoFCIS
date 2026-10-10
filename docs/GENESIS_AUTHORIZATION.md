# Policy-bound genesis authorization

## Boundary

An authoritative store is created only from a private-construction
`CatalogAuthorizedGenesis<H, P, L, I>`. A schema-admitted value is necessary
input to the ceremony, but it is not commit authority.

The owning `CatalogCommitAuthority` pins one `GenesisPolicyBinding` containing
the expected initial semantic root, reviewed genesis source and configuration,
retained evidence, and a unique deployment-instance identity. Those values,
the catalog, provider, state domain, execution policy, and complete project-law
set contribute to `AuthorizationPolicy::policy_id`.

Genesis law evaluation is separate from the three-way transition decision
algebra. It does not add a fourth decision. Every `LawDefinition` states
whether it applies to genesis. State-invariant laws must apply; rejection and
committed-failure laws cannot apply. The reviewed `ProjectLawEngine` must
return exactly one satisfied observation for every genesis-applicable law.

## Inputs and outputs

Inputs:

- one exact `ProjectCatalog` and approved commitment provider;
- one state-domain and execution/deployment binding;
- one `GenesisPolicyBinding`;
- one complete verified project-law set and reviewed law engine;
- one schema-admitted root state.

Successful output:

- a nominal `CatalogAuthorizedGenesis` binding the policy, exact initial state
  and root, complete genesis-law evaluation, and canonical authorization bytes.

The pure `AuthorizedShellState` and SQLite creation APIs consume that nominal
value. Existing SQLite stores reopen without accepting a replacement initial
state.

## Authority and trusted dependencies

The authority owns the catalog, policy, provider, law set, law engine, state
domain, and genesis binding. The caller supplies only the admitted initial
state selected for evaluation. It cannot construct a successful genesis
witness, evaluation, or identifier directly.

Trusted dependencies are ZCVE/1 encoding, the approved commitment provider,
schema/catalog admission, semantic-root hashing, the reviewed project-law
engine, retained evidence verification performed when the law set was built,
and SQLite transaction/durability behavior for the persistent adapter.

## Deterministic resource bounds

- Initial-state depth, nodes, collection sizes, and bytes retain schema and
  admitted-value bounds.
- Genesis evaluation uses the law set's `LawLimits`, including at most 4,096
  definitions and observations.
- `GenesisPolicyBinding` and the genesis authorization contain a fixed number
  of 32-byte commitments plus one bounded canonical state envelope and one
  bounded law evaluation.
- SQLite creation writes one fixed genesis row and one semantic-state row in a
  single immediate transaction.

Wall-clock time and host allocation are not protocol evidence.

## Laws and negative cases

1. Raw initial-state non-authority: no production shell creation API accepts a
   `SchemaAdmittedEnvelope` by itself.
2. Exact root: the recomputed state root equals the policy's expected genesis
   root.
3. Exact policy: changing source, configuration, retained evidence,
   deployment instance, catalog, provider, state domain, execution binding, or
   law set changes the policy or prevents authorization.
4. Complete laws: the law engine returns exactly the genesis-applicable law
   identifiers; missing, extra, duplicate, violated, indeterminate, or failed
   observations prevent authorization.
5. Mandatory invariants: every state-invariant definition applies at genesis;
   reject-only and committed-failure-only framework laws do not.
6. Creation/reopen separation: creation consumes a nominal witness; reopen
   accepts no caller-supplied initial value.
7. Persistence binding: SQLite stores and revalidates exact genesis bytes,
   identity, policy, state domain, initial root, and initial state before use.
8. Version-zero equality: until the first authorized transition commits, the
   current semantic row must remain byte-for-byte the authorized genesis state
   at the exact genesis root.

Negative tests cover schema-valid law violation, changed root and binding
fields, another deployment policy, raw-envelope compile failure, creation over
an initialized store, unsupported prior schemas, and persisted genesis
tampering, including a self-consistent replacement semantic state at version
zero.

## Assumptions and explicit nonclaims

Owner review must select truthful genesis source/configuration/evidence and a
law engine whose implementation matches the registered claims. Nonzero hashes
bind those selections; they do not prove them.

This boundary does not invent project economics, prove an engine sound, attest
a deployment, migrate SQLite schema v3, or reconstruct the full later
authorization/bundle/outbox row set. Issue #55 remains a separate persistence
qualification blocker. Bounded tests are not an unbounded correctness proof or
an independent audit.
