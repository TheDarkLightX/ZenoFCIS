# V1 implementation and release review

Reviewed on 2026-09-12 by Codex, with two independent source inspections and
scoped implementation work from actual `claude-opus-5`. The inspections used
an isolated archive of public commit
`cca3f6b30cd81d8af134dc238c06fe7cd5c18b4a`; Codex integrated current main
`0123f8ffe0f29caeecc1bf6417bbf37e485bbf88`, reproduced findings, reviewed patches,
and checked the resulting release source. No model response authorizes a
transition, proves a theorem, or replaces the release gates.

## Confirmed repairs

- **SQLite outbox membership:** a controlled second connection replaced a row
  after history validation and restored it before later validation. The old
  public `next_pending` method returned the substituted entry with internally
  consistent hashes. It now compares the selected entry and authorization
  against the approved cached bundle. The same regression rejects the
  substitution and still returns the restored legitimate delivery. Existing
  corruption-error precedence, replay, crash and acknowledgement tests remain.
- **Python adapter types:** the old adapters silently converted strings to
  booleans, floats to integers, integers to bytes, and `None` to text. The new
  checks reject wrong primitive types, including integer subclasses that can
  override comparisons. Follow-up checks reject malformed structures, wrong
  child classes, dropped sum payloads and namespace collisions. Codex also
  reproduced and fixed ignored map-key bytes and duplicate or unsorted input
  keys. Valid boundary values and canonical replay remain covered. This intentionally tightens invalid caller-input acceptance.
- **Synthesis source creation:** Unix work directories are private from the
  initial creation operation. Source files use exclusive creation and reject
  existing files or symlink targets. Child tool execution still has the
  invoking user's permissions; this is not a process sandbox.
- **QEMU build locking:** the outer Cargo invocation receives an explicit
  output-directory argument without exporting that directory to the
  bootloader's nested installer. A two-process lock regression reproduces the
  old contention. The V1 framebuffer and serial transcript were captured from
  a real freestanding guest boot.
- **Developer dependency:** Hono 4.12.34 was replaced by 4.13.5, the first patch
  for GHSA-gqvv-2mrq-wpjv, GHSA-g6gw-c38x-mqfc and GHSA-crvj-82cr-hjcx. The fresh
  npm audit returned no advisories. Probity remains 1.10.0; no external Rust
  dependency version or Lean pin was upgraded for stable release preparation.

## Earlier issue acceptance criteria

The following records distinguish implemented library mechanisms from adopting
projects' evidence obligations. They do not claim every historical issue is a
current vulnerability or that this table closes it automatically.

| Issue | Reviewed mechanism and remaining boundary |
| --- | --- |
| #54 | Production shell commit accepts privately constructed catalog-authorized transitions; raw bundles cannot mint this witness. |
| #55 | Persisted replay, receipts and outbox rows are checked against decoded authorized bundles. The additional between-read substitution defect is repaired as described above. |
| #56 | Composition frame checks and parallel evidence bind exact claims; existing negative and differential tests cover the refactored binding order. |
| #57 | Transition authorization binds invocation, approved provider, interpreter and deployment policy. A deployment still supplies truthful reviewed implementations and authenticated inputs. |
| #58 | Project laws are first-class manifest and invocation obligations. The framework checks exact coverage and rejects missing or indeterminate evaluations. Each project must supply and test its economic engine, including aggregate multi-effect imbalance; a universal economic engine is not supplied. The earlier documentation overclaim was corrected. |
| #61 | Exhaustive refinement and normalized decisions require independently checked retained evidence and exact coverage; caller-fabricated success records do not mint nominal authority. |
| #62 | Projector qualification, strict proof/plan decoding and authenticated publication bind tree/profile/version and exact semantic relations. The owner-selected relation engine must also enforce and commit to its allowed semantic policies. Shared Rust types do not imply a single deployment policy. Cross-store atomic publication is not supplied. |
| #67 | Parallel authorization requires complete-footprint evidence for exact component claims; the selected project verifier remains responsible for the completeness semantics it attests. |
| #68 | A mechanized end-to-end soundness theorem remains an enhancement outside the V1 product contract. Stable package/API status does not establish that theorem. |
| #72 | Production initialization requires law-verified, policy-bound genesis. The negative conversion doctest now uses valid generic bounds, so unrelated bound errors cannot satisfy it. |
| #73 | Reference and SQLite delivery use the same candidate/ordinal/entry identity. Delivery is at least once; destination idempotency remains required. |
| #74 | Catalog value classifications derive mandatory economic law families for both committing decisions. Correct project classifications and law semantics remain reviewed project inputs. |
| #76 | Commit effects are non-executable evidence; the production shell applies the authorized state update and outbox transaction. Generated comments now state that distinction explicitly. |

## Retained evidence and limits

`security/v1-review.json` records the bounded threat model, hotspot identities,
findings, commands and evidence references. The hotspot baseline was refreshed
only after reviewing its inventory and priority changes; its score is a review
order, not severity or an exploit probability. Some test-only source is
lexically classified as a hotspot; that does not make it a runtime entry point.

The exact committed release, clean-builder package comparison, signed tag and
publication checks are recorded separately by the owner release procedure.
An earlier successful feature-head workflow cannot certify the integrated V1
commit. The pinned Lean 4.30.0 CI checks remain required; an additional local
run using an existing Lean installation does not replace them.

These are source review, executable regressions and bounded conformance
results, not a commissioned security audit or downstream production
qualification. SQLite does not authenticate acknowledgements against direct
DB writes or detect a complete consistent database rollback after restart.
Projector, law, footprint and external verifier semantics still depend on the
reviewed owner-selected implementations and their retained evidence.
