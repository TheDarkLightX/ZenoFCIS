# V1 release polish: scope and validation

Base: `63b273aec1eeebb7ab5df94a9e9f33b8c2af5307`.
Branch: `agent/v1-release-polish-20260906`.

This pass corrects the existing adopter and release contracts before stable
Cargo API promotion. It does not introduce a new semantic subsystem or publish
version 1.0.0.

## Selected repairs

- Align Cargo minimum-Rust metadata and generated consumers with the documented
  supported minimum, Rust 1.97.1. Retain all dependency and Lean versions.
- Make foundational error values usable through the standard Rust error trait
  without adding a std requirement or changing error variants or messages.
- Make CLI project-read failures produce versioned machine output when JSON is
  requested, retaining filesystem exit class 3 and human diagnostics.
- Enforce the existing one-MiB source limit while reading CLI input, before
  allocating the complete file or interpreting UTF-8. Size admission uses exit
  3 as a bounded input failure; in-memory parser diagnostics remain unchanged.
- Classify a timed-out external tool as bounded execution failure (exit 3), as
  the CLI reference specifies. Missing tools and unqualified evidence remain
  blocked (exit 2); no successful evidence or authority path changes.
- Improve the diagnostic from the existing admitted-executable regression test
  so an intermittent failure identifies the actual bounded-run error.
- Provide `describe [COMMAND...]` as a deterministic JSON view of the actual
  Clap command tree, defaults, required arguments, value choices, effects, and
  exit classes. Discovery reads no project or tool input and grants no authority.
- Add optional JSON generation/drift results and an executable agent recovery
  loop. Preserve human defaults and read-only checks; report artifact read
  failures separately from missing/modified artifacts. Bound comparisons by
  expected artifact length plus one byte.

Regressions demonstrate the missing error interoperability and CLI contract
failures on the base, then exercise the corrected process-level adopter paths.
Packaging self-tests reject inconsistent Rust metadata. Narrow checks precede
full ATDD, which must run immediately before each implementation commit.

Authoritative state, canonical formats, reason ordering, resource accounting,
nominal authorization, replay, SQLite publication, and outbox delivery are
outside the production delta. Existing coverage of those boundaries remains
required. The process runner's limits, admission checks, and cleanup protocol
are retained.

## Evidence and remaining gates

Focused validation covers:

- All eight foundational errors satisfy `Error + Send + Sync + 'static`; the
  isolated consumer executes `?` conversion into a boxed standard error and
  downcasts it back to the original error. ATDD and CI now run this consumer.
- The five affected foundational crates still compile without default features.
- CLI process tests exercise versioned read failures, exact-limit and one-byte
  excess input, tool timeout classification, command discovery, generation, and
  read-only drift recovery. Discovery is byte-identical with absent or invalid
  local project/tool inputs. Descriptor requiredness agrees with actual parser
  usage errors. Drift tests compare the complete controlled output tree before
  and after every check.
- A read-budget mutation that consumed sixteen extra bytes failed the
  reader-position regression. The exact candidate source was restored afterward.
- The package checker passes its twelve self-tests, including rejection of a
  mismatched minimum Rust version. Documentation, assurance, and ATDD registry
  checks pass. The complete 26-scenario ATDD result and exact commit identity
  belong in the implementation's retained validation receipt.

The independent agent review found no authority or canonical-semantic blocker
in this delta; its descriptor-requiredness correction is included. This bounded
code review does not replace the independent release review below.

The prior admitted-executable test failure under default full-suite concurrency
remains unresolved. This pass improves its failure diagnostic while retaining
its deadline and assertions. Validation receipts must record test concurrency
and any recurrence; bounded local passing runs do not close the root-cause issue.

Stable `1.0.0` promotion still requires the
[V1 release checklist](V1_RELEASE_CHECKLIST.md), including:

1. Resolve final API feedback and every designated core-library release blocker,
   then independently review the exact final source and authority topology.
2. Run the complete release gate and required platform, Miri, fuzz-build,
   supply-chain, formal-tool, and QEMU workflows at that source revision.
3. Produce matching retained checksums from two clean builders. Qualify the
   generated durable application against actual release `.crate` artifacts and
   the packaged CLI. The subsequent
   [packaged-application gate](PACKAGED_APPLICATION_QUALIFICATION.md) implements
   that check; exact final-release evidence is still required.
4. Prepare the reviewed `1.0.0` version, documentation, consumer pins, lockfile,
   and package evidence, then repeat the checklist for the final version.
5. Complete the owner-controlled signed tag, publication, and independent
   artifact verification. Downstream deployment qualification remains separate.

No dependency graph, canonical protocol identifier, Lean version, or proof
authority changes in this pass. Lean remains at `4.30.0`; validation reuses
installed toolchains and build artifacts without installing another Lean copy.
