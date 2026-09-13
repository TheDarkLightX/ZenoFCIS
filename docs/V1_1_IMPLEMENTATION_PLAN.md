# V1.1 implementation and release contract

Baseline: `af0af5ea44426f20681e498ce12c26cdfff51b2c` on PR #102.
V1.0 is already published. This work is an additive V1.1 release, with the
existing canonical protocols and Lean 4.30.0 unchanged.

## Required outcomes

1. Import a bounded canonical completion plan, independently choose its model,
   and verify the plan before exposing checked commands. Provide CLI discovery,
   structured failures, and inert files sufficient to reproduce a failed check.
2. Generate a small prepared-counter application whose command contains an
   ordered bounded batch. Preparation owns admitted inputs and the exact
   invocation, root, and version. Only its complete result reaches the existing
   authority. The authority independently executes the approved transition and
   checks the complete result, laws, receipt, and outbox before SQLite commits.
3. Demonstrate capacity and exit obligations on that concrete profile, including
   complete encoded publication data. Reject over-capacity work before handing
   any authorized transition to storage. Keep an eligible decrement available
   from each nonterminal admitted state under the reviewed allowed context.
4. Retain tests for cancellation, chunk failure, staleness, competing commits,
   exact replay, restart, delivery interruption, and zero partial publication.
   Compare complete runtime outcomes with the independent finite model.
5. Keep a V1.0 consumer source unchanged while qualifying it against V1.1,
   preserve existing wire vectors, and measure larger finite cases without
   claiming a speedup before measurement.
6. Update the coherent 36-crate release set to 1.1.0; review and merge the exact
   candidate, qualify packages, sign the new tag, publish in dependency order,
   and verify downloaded artifacts, installed consumers, and hosted docs.

## Ownership and limits

The application owns schema, laws, allowed-context policy, command ordering,
authentication, and its delivery destination. Library checking and canonical
decoding own acceptance; models and imported files remain advisory. The new
example is a bounded counter and notification workflow, not financial settlement
or production deployment qualification.

Prepared work is private scratch. Cancellation discards it; interruption recovery
replays the original command. No serialized accumulator is trusted, no state or
effect is published by an intermediate chunk, and no new pending-operation
storage protocol is introduced. SQLite remains the sole atomic publication
boundary. External notification deduplication remains a destination obligation.

The complete publication-size check belongs to the pure application admission
path before the authorized candidate reaches the shell. The fixed profile and
full finite transition corpus must demonstrate that every declared exit fits
the same envelope. Logical resource bounds do not claim an OS time or RAM limit.

## Design and preservation checks

Use the existing IR, canonical decoder, budget meter, generated typed project,
nominal authorization, and SQLite shell. Add direct functions and one small
example profile rather than a new framework, solver, proof backend, or generic
checkpoint system. New command/profile identifiers have distinct meanings;
existing V1.0 protocol identifiers and accepted serialized values are retained.

Completion import must reject alternate profiles, wrong independently supplied
models, invalid ranks/commands, noncanonical encodings, truncation, trailing
bytes, and excessive bytes/nodes/depth/collection sizes. Parsing never creates
authority. The existing verifier remains the constructor of checked plans.

The prepared application must bind command, context, principal, replay identity,
pre-root, and starting version before work. It compares preparation with the
independently authorized complete post-state; it never edits a candidate or
reconstructs outbox effects in the shell. A mismatch, changed head, or oversized
publication returns no candidate to storage. Tests exercise simultaneous
failures and retained inputs as well as successful chunk partitions.

Each commit requires `python3 tools/atdd.py run --all` immediately beforehand.
Source-bound manifests and security inventory are refreshed after source changes.
All release metadata uses the public noreply identity; raw local evidence and
private marker files remain outside published assets. No published tag moves.
