# Neurosymbolic software factory — design and research draft

2026-10-04 documentation packet. ZenoFCIS is being built as a neurosymbolic
software factory for high assurance FCIS applications. This packet specifies a
later bounded optimization loop; it does not demonstrate a complete factory or
install that loop. Smaller V2 and the [2.1 roadmap](../V2_1_FACTORY_PLAN.md)
prerequisites remain first.

- [Decision and continuation note](NOTE.md)
- [Design and conditional arguments](DESIGN.md)
- [Six implementation specifications](specs/INDEX.md)
- [Claude execution handoff](CLAUDE_HANDOFF.md)
- [Dana Edwards draft paper](paper.md), [compiled PDF](paper.pdf) and
  [self-contained LaTeX](paper.tex)
- [Completion and open-obligation checklist](CHECKLIST.md)
- [Document renderer](render_paper.py): `python3 render_paper.py` regenerates
  the self-contained TeX and its embedded Markdown source digest.

The [published experiment](https://github.com/TheDarkLightX/ZenoFCIS/tree/a69ed8db594d95279a46bff0f65185ef67d51f98/experiments/semantic-egraphs)
was symbolic. Its stored results have been replayed with Python; the new neural
loop, pair-cost policy, target correspondence and application promotion remain
proposed. The revised paper preserves failed runs and those evidence boundaries.
The final 11-page PDF compiled in two passes with author/title metadata and no
blocking TeX warnings. The title and table pages were visually inspected; the
[build record](../evidence/v2_1_neurosymbolic/paper-build-20261004.json) records
that document check. Runtime qualification and neural evaluation remain future work.
