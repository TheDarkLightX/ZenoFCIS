# 04 — Existing synthesis integration and optional neural adapter

Parent: [design](../DESIGN.md). Enable only after 00–03 gates and V2 prerequisites.

## Inputs and outputs

Input: existing synthesis request path plus an explicit transform request and
operator-selected proposer configuration. Output: bounded typed request status,
candidate feedback and replay report. No application activation is an output.

## Requirements

- **NSM-001:** Implementation MUST first inspect and reuse
  `integrations/mcp/zeno_fcis_synthesis.py`, `integrations/mcp/test_server.py` and
  `skills/zenofcis-synthesis-first/SKILL.md`. Do not assume these files implement
  this loop. Proposed tool names `transform_request`, `transform_candidate`,
  `transform_replay` MUST be labeled uninstalled until implemented and tested.
- **NSM-002:** The adapter MUST call library-owned supplied-candidate checking
  and replay. It MUST NOT accept a model/user `passed` flag, mutate core policy,
  deserialize a private witness, or construct Authority/Evaluation/Publication.
- **NSM-003:** A request view sent to the proposer MUST contain only policy-
  permitted data and bounded checker-derived feedback. Provider/model identifiers,
  prompt/response digests, decoding settings, seeds, tokens/money and search
  status MUST be provenance, not assurance. Report provider-asserted fields as
  such. Prompt feedback MUST NOT be described as weight training.
- **NSM-004:** A model may propose complete canonical artifacts. A later closed
  strategy mode MUST have versioned grammar, fixed qualified rule IDs, bounded
  nesting/counts/rounds and no arbitrary code execution. It MUST emit actual DAG
  bytes through the same acceptance path. Unsupported modes MUST be unavailable.
- **NSM-005:** Local/egg/model failures MUST retain only a qualified live or
  successfully replayed incumbent. Complete feasible extraction with unknown
  optimality MAY reach the checker, with that status retained. Partial or
  undecodable extraction MUST NOT. Research reproduction MUST preserve the
  original study's stricter positive-optimality gate and failed runs.
- **NSM-006:** Native egg/ILP/CBC MUST remain optional shell-side dependencies.
  Exact fork/patch, native library identity, transitive licenses/notices, source
  locks and maintenance cost MUST be reviewed before enablement. Published pins
  and fresh Rust/Lean execution remain open qualification inputs, not done work.
- **NSM-007:** Rust/Wasm output MUST NOT inherit canonical DAG equivalence without
  exact target-artifact correspondence. Checked-i64, application decisions,
  resource refinement and identity upgrades MUST remain explicitly disabled
  until separately qualified profiles exist.
- **NSM-008:** Later application integration MUST use roadmap tracks 2 and 4 and
  the existing Authority → Publication → SQLite route. It MUST account for
  reasons, successor/patch, ordered effects, laws/genesis, charges/refusal,
  identities and replay. No erasure of identity or historical-record rewrite
  is permitted to manufacture equality. Natural-language intent adequacy
  remains an external review obligation.

## Failure precedence and scenarios

Reject unsupported tool/profile/provider policy before external dispatch. Use
00–03 precedence for all other failures. Preserve typed reasons through MCP;
transport failure is inconclusive/unavailable, never successful equivalence.

Test fake-provider whole candidates; fabricated success/receipt; prompt attempting
to change domain/policy; no source-disclosure authorization; missing caps;
unsupported strategy code; hidden credentials in diagnostics; source-pinned
replay via MCP; byte/artifact substitution; unknown optimality; target printer
drift; attempted application promotion. Skill instructions MUST explain actual
available capabilities and cannot instruct callers to trust model explanations.

## Source-bound acceptance

Run library checks before adapter tests, then actual packaged CLI/MCP journeys
on supported targets using fake providers. Review no-new-authority and purity
boundaries. Update tool/skill docs only when the matching runtime slice exists;
keep design suggestions distinguishable from installed capabilities.
