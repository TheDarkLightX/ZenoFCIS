# ZenoStructures v0.6.1 repaired checkpoint

**Status:** research-only, unmounted, strict reference semantics  
**Base:** public v0.6 checkpoint  
**Purpose:** close the ZenoDEX falsification packet before any Rust or runtime port

This checkpoint repairs the inherited v0.5 defects and the new v0.6 defects at both construction and point of use. It does not promote any data structure, receipt, root, qualification, or certificate into ZenoFCIS authority.

## Shared repair law

Every derived aggregate used by an authority-bearing operation must be freshly reconstructed from its exact retained source. A caller-supplied semantic cell, projection cell, progress index, checksum, digest, Boolean certificate, receipt shape, or locally recomputable root cannot substitute for the source relation.

Every receipt or qualification is bound to an authority-owned store-current context and reverified when consumed. Private construction alone is not treated as sufficient.

## Closed falsifiers

The repaired public source is required to reject:

1. a causally downward-open FQAT frontier seal;
2. a constructor path that bypasses FQAT causal closure;
3. PFCT projection cells not freshly derived from the exact records;
4. CWCRM semantic cells not freshly derived from the exact contributions;
5. SIDF caller-selected derived progress indexes;
6. CPSQ or CRQG checksum-only stutter certificates;
7. any claimed stutter carrying an effect or outbox entry, or changing complete durable-layout bytes;
8. a PFPL credit token owned by two open versions;
9. fabricated, stale, crossed, or incomplete proof-context qualifications;
10. a PSHF observation without a prior append-only publication receipt;
11. CHA failure on a one-node, zero-edge identity atlas;
12. LFRL caller-supplied debt-discharge evidence or verifier digests;
13. an LFRL rollback permit crossed between distinct closed-debt states;
14. reuse of one rollback permit.

## Mathematical corrections

`PCFP` is a compatible partial-assignment **poset** with meet and partial join. Incompatible assignments have no join and return a canonical incompatibility witness. It is not represented as a lattice.

CHA remains a finite permutation atlas in the proved/tested core. Partial, lossy, type-changing, and state-dependent migrations are modeled separately through explicit relation, loss-fiber, and authority obligations rather than being smuggled into the bijection theorem.

## Stutter closure

A progress-erasing edge is admitted only after an authority-selected verifier establishes all of:

```text
before durable-layout bytes == after durable-layout bytes
before semantic observation == after semantic observation
complete effect plan is empty
complete outbox plan is empty
current context matches
receipt is valid for the exact event and verifier
```

Progress comparison includes effect and outbox identity.

## LFRL correction

The public LFRL path separates:

```text
immutable reusable complement information
exact loss atoms from a closed schema manifest
exact target-created-field atoms from that manifest
verifier-produced loss-recovery receipts
verifier-produced created-field-removal receipts
one-use rollback authorization permit
separate permit-consumption state
```

A permit cannot be minted while any debt remains open. Every discharge receipt and rollback permit is reverified against the authority-owned current context at point of use. The permit binds the exact closed-debt state on which it was issued and cannot cross to a sibling state with different discharge lineage.

The Python reference uses MAC-bound receipts to model opacity and point-of-use verifier binding. A Rust port must use private nominal construction and an authority-owned verifier or authenticated signature boundary.

## Public/private boundary

This public branch contains candidate contracts, first falsifiers, prior-art pressure, public tests, mutation receipts, bounded-domain declarations, opaque campaign identities, formal companion specifications selected for publication, and ZenoFCIS handoff material.

The owner-private research implementation and internal execution history remain separate. They are not a public reproduction requirement for a selected candidate's observable laws.

## Evidence boundary

The sealed public source archive and release receipt are conversation/review artifacts associated with this branch. Exact test counts, source manifest, hash-seed replay, clean-extraction replay, and archive hashes belong in that receipt. This branch does not treat an archive hash as scientific or runtime authority.

Prepared Lean, Julia, ESSO, or Rust companions count as evidence only when their exact source, toolchain, command, and result are retained separately.

## Production nonclaim

Nothing in this checkpoint may enter `CatalogCommitAuthority`, authenticated publication, SQLite publication, migration writers, delivery workers, recovery, or a ZenoDEX entrypoint merely because it validates.

The ordinary chain remains mandatory:

```text
authenticated command and current state/context
  -> deterministic evaluation
  -> nominal authorization
  -> exact receipt and bundle lineage
  -> atomic publication and recovery
  -> committed outbox effects only
  -> no alternate acceptance path
```
