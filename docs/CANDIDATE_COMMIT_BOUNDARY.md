# Candidate and commit boundary

The semantic transition does not authorize the shell to reconstruct its own
meaning. It returns one closed candidate whose parts share one content-derived
identity:

```text
preconditioned CanonicalPatch
+ non-executable CommitPlan evidence
+ durable OutboxPlan obligations
+ exact profile/command/context/precedence/algorithm/budget bindings
        -> CandidateBody
        -> CandidateId
        -> Receipt
        -> CommitBundle
```

`Reject` has no candidate, patch, commit evidence, or outbox plan.
`CommittedFailure` owns a complete candidate because authoritative state has
intentionally changed even though the requested operation failed.

These relationships establish structural consistency. A `CommitBundle` does
not prove that the exact reviewed transition produced it for an externally
admitted command, context, principal, replay identity, provider, interpreter,
and deployment. `zeno-fcis-authority` supplies that separate nominal boundary.
Only its private-construction `CatalogAuthorizedTransition` may enter a
production commit port.

The pure reference shell function `apply_reference_bundle` publishes together:

- the exact successor semantic state and root;
- the replay binding;
- the receipt;
- the complete bundle and non-executable commit-evidence plan;
- complete outbox delivery records, including payload and destination data.

A replay is idempotent only when its replay identity, candidate identity, and
complete bundle are all identical. Destination acknowledgement is accepted only
for the exact committed outbox-entry hash.

The SQLite production port consumes the authorization value, not a raw bundle
or a caller-selected replay identity. The replay identity is already bound into
the admitted invocation and the resulting candidate.

`CommitPlan` is never interpreted as executable work. Every external action,
including value movement, must be represented as a catalogued durable outbox
obligation. See [Commit evidence and durable outbox model](COMMIT_EVIDENCE_AND_OUTBOX_MODEL.md).

## Initial restrictions

The initial patch algebra allows structural insertion and deletion only for
record fields and canonical map entries. Vector positions may be updated but
not inserted or deleted because a batch of index-shifting edits needs a separate
simultaneous-edit specification. This restriction prevents sequential patch
application from silently changing the meaning of later vector paths.
