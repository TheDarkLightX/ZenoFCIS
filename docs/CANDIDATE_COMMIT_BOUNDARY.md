# Candidate and commit boundary

The semantic transition does not authorize the shell to reconstruct its own
meaning. It returns one closed candidate whose parts share one content-derived
identity:

```text
preconditioned CanonicalPatch
+ authoritative CommitPlan
+ committed OutboxPlan
+ exact profile/command/context/precedence/algorithm/budget bindings
        -> CandidateBody
        -> CandidateId
        -> Receipt
        -> CommitBundle
```

`Reject` has no candidate, patch, authoritative effects, or outbox plan.
`CommittedFailure` owns a complete candidate because authoritative state has
intentionally changed even though the requested operation failed.

The pure reference shell publishes together:

- the exact successor semantic state and root;
- the replay binding;
- the receipt;
- the complete bundle and authoritative plan;
- complete outbox delivery records, including payload and destination data.

A replay is idempotent only when its replay identity, candidate identity, and
complete bundle are all identical. Destination acknowledgement is accepted only
for the exact committed outbox-entry hash.

## Initial restrictions

The initial patch algebra allows structural insertion and deletion only for
record fields and canonical map entries. Vector positions may be updated but
not inserted or deleted because a batch of index-shifting edits needs a separate
simultaneous-edit specification. This restriction prevents sequential patch
application from silently changing the meaning of later vector paths.
