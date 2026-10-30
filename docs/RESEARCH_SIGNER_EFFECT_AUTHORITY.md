# Research handoff: authorization-preserving external effects

**Status:** research-only / not installed / not a ZenoFCIS production feature.
**Base inspected:** 57a19ecc6aabfcd6e5107ce0e6135fe2c1a01beb (main, 2026-10-30).
**Crucible pilot:** TheDarkLightX/Zeno-Crucible draft PR #47 (private; maintainer access required).
**Motivation:** EIP-7702 delegation and DAO-style effect ordering, not a claim of an existing ZenoFCIS vulnerability.
**Review authority:** human maintainer plus independent evidence gates; no Crucible report may grant execution or publication rights.

## Problem statement

ZenoFCIS already distinguishes an authenticated, nominal CatalogAuthorizedTransition from an arbitrary bundle and gives the shell explicit, one-shot effects. See:

- docs/CATALOG_AUTHORIZATION_BOUNDARY.md
- docs/AUTHENTICATED_AUTHORITY_BOUNDARY.md
- docs/SHELL_EFFECTS.md
- docs/GENERATED_TYPED_EFFECTS_CHANNELS.md
- docs/AGENT_HANDOFF_BOUNDARY.md
- docs/TRACE_GAME.md

The unclosed *research question* is whether a project-specific external signer or high-authority contract-call adapter can be mounted so that no caller, compromised low-privilege service or recovered stale request can obtain an effect outside the reviewed policy. A pure core is not a security boundary if an alternate signer RPC can produce signatures without traversing it.

The DAO failure class concerns **ordering** of effects relative to accounting changes, not mutability itself. EIP-7702 concerns **authority** to change the executing code of an EOA: a cryptographically valid authorization tuple can install persistent delegation even if the signer organization's policy did not intend it. The relevant protocol document is https://eips.ethereum.org/EIPS/eip-7702 ; the October 2026 Binance case is still under investigation and is only motivation.

## Proposed separation (not a claim of an implemented API)

~~~text
authenticated request
      |
      v
schema/context/actor admission
      |
      v
PURE CORE: derive policy decision and closed proposed effect
      |
      v
nominal CatalogAuthorizedTransition + exact policy/instance/source binding
      |
      v
SHELL: capability binding + replay/freshness checks
      |
      v
POLICY-ENFORCING SIGNER: structured authorization only (no raw-digest escape)
      |
      v
signature / external chain operation / explicit unknown outcome
~~~

The research profile should introduce a **proposed**, separately named external-signing effect; it must not reuse a generic Send effect or silently change the existing ten-port shell registry. No production code, stable schema, effect enumeration, publication token or signature handler is added by this PR.

### Closed conceptual inputs

A proposed signing operation should name the specific project policy, deployment and chain; account/principal; action kind and full exact intended payload; protocol signing envelope; nonce and fresh observed state; delegate address and its reviewed implementation dependencies if applicable; bounds on value/targets; replay identity; approved credential role/quorum; and effect/handler identity. Data need canonical encoding and independently checked semantics, not merely a nonzero hash.

For EIP-7702, the **on-chain signed tuple only contains chain ID, delegate address and nonce**. It does **not** bind an arbitrary application deadline, recipient allowlist, code hash or policy receipt. Those additional controls must be enforced at the actual signer boundary or by independently enforced on-chain account logic; hashes written into ZenoFCIS receipts do not alter what Ethereum verifies. The protocol permits chain ID 0 for cross-chain authorization; a restrictive deployment policy may reject it. Delegation persists after outer execution revert.

## Proposed invariants / falsification targets

| ID | Proposed law | Required negative control |
| --- | --- | --- |
| FAE-01 | Every external-signing effect emitted by the mounted shell has an exact, privately constructed, freshly policy-authorized origin. | A raw signed-digest API or another shell caller bypasses the origin; test must detect it. |
| FAE-02 | The signer verifies the structured operation and policy *within its own enforceable trust boundary* before producing signature bytes. | Compromised upstream/DevOps service supplies validly formed but disallowed delegation. |
| FAE-03 | Chain, account, nonce, action class, delegate and reviewed behavior are bound to the approval; never trust a caller-supplied boolean that says policy accepted. | Wrong chain, wildcard chain ID 0 (when prohibited), stale nonce, substituted delegate, proxy/implementation drift. |
| FAE-04 | A false, unknown, stale or ambiguous approval results in **no new signing effect**. | Crash/retry/timeout/unknown outcome, reply substitution, premature success. |
| FAE-05 | One-shot effect identity is replay-safe across crashes and concurrent submissions; no alternate path signs a different payload under the same approved identity. | Retried authorization with changed bytes, replay after policy rotation, double use. |
| FAE-06 | An external send cannot exploit a pre-commit balance to perform more value-moving callbacks than the invariant permits. | DAO-style nested callback before accounting update. |
| FAE-07 | Delegation changes are treated as a separately classified privileged action, not as a routine transaction or ordinary Send. | Existing EOA changes executable code without separate governance approval. |
| FAE-08 | Observed failure of the outer Ethereum transaction must not be equated with rollback of EIP-7702 authorization; recovery checks actual account delegation. | Revert after authorization processing; persistent delegation remains. |
| FAE-09 | Authorization scope survives changes in signer, deployment, policy, delegate code and storage layout only if the exact reviewed assumptions remain true. | Substitute source/handler, mutable proxy implementation or EOA delegation chain. |
| FAE-10 | Proof/evidence does not mint signing authority. The deployment's actual capability graph is checked, including forbidden alternate handlers and raw signing methods. | Forge Crucible output, self-declared proof result, test-only handler promoted to production. |

A stronger formal statement is conditional, not absolute:

    for all signatures emitted by the policy-controlled signer,
        exists a unique freshly authorized decision
        binding the exact signed operation and permitted capability.

It **cannot** guarantee that all signatures accepted by Ethereum came from that signer: an unrestricted original EOA key or external raw-signing service can sign independently. Proving signer containment requires inspection of the actual HSM/MPC/remote signing API and all routes to it.

## Crucible experiment contract

Crucible PR #47 includes a preregistered finite research toy: 96 signer/delegation cases, an explicit gated-versus-raw-signing adversary, persistent authorization on reverted execution, chain-ID 0 negative control, code-identity drift, and debit-first/send-first reentrancy. It is a **counterexample/requirements lab**, not a native ZAL lowerer or proof. See experiments/authority-boundary/claim-matrix.json for FCIS-specific adoption obligations and status.

Research sequence:

1. Human-lock the intended meaning and the threat model, including signer compromise versus EOA-key compromise.
2. Expand Crucible finite state and adversary schedule: nested callback, concurrent nonce drift, independent policy gate, signer bypass and code upgrade. Register mutants and independent oracles, keep failed witnesses.
3. Add a ZAL representation *only after* its dialect and exact semantics are pinned; do not invent syntax or assign authority to a diagram.
4. Check source-bound refinement of the real signed bytes to the reviewed intent; separately audit signer-level permission enforcement.
5. Test mounted shell and every concrete signing adapter, not merely a reference interpreter. Qualification must include refusal precedence, effect ordering, replay and unknown outcomes.
6. Keep this as an additive experimental profile until the security gate has independently passed. No change to release claims, catalog versions or production API from a research receipt.

## Security limits

- Pure immutable decisions reduce the state space and permit stronger proofs, but do not secure a bypassed signing primitive.
- Debit-before-send is useful against DAO-style reentrancy, but shell atomics, rollback, token callbacks and cross-chain settlement require independent testing.
- Approval at signing time does not ensure immutable future behavior behind a delegated address. Proxies and storage-selected implementations must be constrained or treated as residual risk.
- Expiry and value limits not in the EIP-7702 signature are not on-chain restrictions on an unrestricted EOA.
- Existing signed authorizations may remain useful after off-chain policy revocation. Recovery must account for on-chain state and nonce competition.
- A fully compromised EOA key is outside the protection of an off-chain FCIS core alone.

**No automatic promotion.** This PR changes only research documents and explicitly creates no executable new authority.
