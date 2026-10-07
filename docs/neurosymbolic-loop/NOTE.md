# Continuation note — neurosymbolic factory design

2026-10-04. Documentation delivered; runtime and experiments are not implemented.

ZenoFCIS is being built as a neurosymbolic software factory for high assurance
FCIS applications. The neural role is discovery and authoring; qualified checks
admit exact artifacts under frozen contracts. This packet extends 2.1 track 6
with a bounded proposal → optional e-graph search → independent checker → typed
feedback loop. Finish smaller V2 and preserve all roadmap prerequisites first.

The decision is to build supplied-candidate Boolean `transform` before a model
adapter. The original remains fixed; only genuine full-domain checked results
can replace its admitted fallback. Componentwise costs are bounded against the
original; incumbent updates are strictly lexicographic. Resume re-admits/replays
before reuse, with consumed and unresolved reservations retained. No receipt
grants application authority or correctness of natural-language requirements.

Source custody:

- Published study: [immutable commit a69ed8db594d95279a46bff0f65185ef67d51f98](https://github.com/TheDarkLightX/ZenoFCIS/tree/a69ed8db594d95279a46bff0f65185ef67d51f98/experiments/semantic-egraphs).
- Planning inputs: dirty `/tmp/zenofcis-v2-simplify`, old HEAD
  `1ed6f88f7b1098bed6c409bc9aa3c12c99d43c3c`; read as a working snapshot,
  not a qualified final source revision. Repin before implementation.
- [Study review](../V2_1_EGRAPH_REVIEW.md),
  [transform design](../V2_1_TRANSFORM_DESIGN.md),
  [roadmap](../V2_1_FACTORY_PLAN.md) and
  [fresh Python replay](../evidence/v2_1_egraphs/replay-20261004.json).

Historical evidence remains symbolic only: local 895, semantic 816 instructions,
18 wins/78 ties/four regressions; the old protected node-count selection is 812.
Both earlier failed extraction configurations remain recorded. The successful
configuration was exploratory. Fresh Python replay checks stored results; no
fresh Rust/Lean execution or neural, byte-cost or application benefit is claimed.

New files are indexed in [README.md](README.md): [DESIGN.md](DESIGN.md), six
[specifications](specs/INDEX.md), [Claude handoff](CLAUDE_HANDOFF.md), matching
[Markdown paper](paper.md)/[LaTeX paper](paper.tex), and [checklist](CHECKLIST.md).
The paper is a revised draft authored by Dana Edwards. It does not overwrite
the published study. The generated TeX embeds the Markdown digest and uses no
external bibliography or figures. Root owns integration, compilation and review.

Next bounded task, only after V2/prerequisite confirmation: freeze the Boolean
codec/ABI/observation contract and its distinguishing fixtures, then implement
the supplied-candidate admission/checker slice in the existing synthesis crate.
Do not begin with paid models, egg/CBC adoption or application promotion. The
handoff provides exact reading order, ownership, gates and unavailable modes.
