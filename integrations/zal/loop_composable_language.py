"""Additive typed acts for guarded program loops; parse/render only, LLM-generated."""
from __future__ import annotations

import re
from behavior import Refusal

OPERATORS = ("complete", "repair", "canonical", "resubstitute")
ACT_KINDS = ("pipeline", "stages", "strategy", "explore", "mine", "strategy-run")
ENGLISH_PREFIXES = ("Pipeline", "Stages", "Strategy", "Explore", "Mine")
# BEGIN public-example knowledge acts (own exact prefixes only).
ACT_KINDS += ("strategy-kb",)
ENGLISH_PREFIXES += ("StrategyKB",)
# END knowledge act prefixes.


def claims(line: str, english: bool) -> bool:
    """Claim only this family's leading word, leaving other hooks untouched."""
    words = line.split(maxsplit=1)
    return bool(words) and words[0] in (ENGLISH_PREFIXES if english else ACT_KINDS)


def stages(words: tuple[str, ...]) -> tuple[str, ...]:
    if len(words) > 5 or any(w not in OPERATORS for w in words):
        raise Refusal("zal-pipeline-stages-outside-grammar")
    return words


def render(a, ctx, english: bool) -> str | None:
    import loop_language as Z
    # BEGIN public-example knowledge rendering.
    if a.kind == "strategy-kb":
        return f"StrategyKB {a.id} uses {a.get('request')}." if english else f"strategy-kb {a.id}: {a.get('request')};"
    # END knowledge rendering.
    if a.kind == "strategy-run":
        return f"Strategy {a.id} runs {a.get('mode')} using {a.get('problem')}." if english else f"strategy-run {a.id} {a.get('mode')}: {a.get('problem')};"
    if a.kind in ("pipeline", "strategy", "mine"):
        expression = Z.show(a.get("expr"), ctx, english, want=ctx.output_kind(a.get("output")))
        if english:
            noun = {"pipeline": "Pipeline", "strategy": "Strategy", "mine": "Mine"}[a.kind]
            return f"{noun} {a.id} proposes {a.get('output')} as {expression}."
        return f"{a.kind} {a.id} {a.get('output')} = {expression};"
    if a.kind == "stages":
        words = stages(a.get("stages"))
        if english:
            return f"Stages of {a.id}: " + (", then ".join(words) if words else "gate only") + "."
        return f"stages {a.id}: " + (", ".join(words) if words else "gate-only") + ";"
    if a.kind == "explore":
        if a.get("depth") != 1:
            raise Refusal("zal-strategy-runtime-depth-must-be-one")
        return f"Explore {a.id} to depth {a.get('depth')}." if english else f"explore {a.id}: depth {a.get('depth')};"
    return None


def parse(line: str, ctx, english: bool):
    import loop_language as Z
    # BEGIN public-example knowledge parsing.
    knowledge = re.fullmatch(rf"StrategyKB ({Z.ID}) uses (.+)\.", line) if english else re.fullmatch(rf"strategy-kb ({Z.ID}): (.+);", line)
    if knowledge:
        import json
        from loop_strategy_knowledge import request
        id_, text = knowledge.groups()
        canonical = json.dumps(request(text), sort_keys=True, separators=(",", ":"))
        return Z.act("strategy-kb", id_, request=canonical)
    # END knowledge parsing.
    from loop_strategy_modes import KINDS, strict_problem
    mode = re.fullmatch(rf"Strategy ({Z.ID}) runs ([a-z-]+) using (.+)\.", line) if english else re.fullmatch(rf"strategy-run ({Z.ID}) ([a-z-]+): (.+);", line)
    if mode:
        import json
        id_, kind, text = mode.groups()
        if kind not in KINDS:
            raise Refusal("zal-strategy-kind")
        problem = json.dumps(strict_problem(text), sort_keys=True, separators=(",", ":"))
        return Z.act("strategy-run", id_, mode=kind, problem=problem)
    pattern = (rf"(Pipeline|Strategy|Mine) ({Z.ID}) proposes (out\d+) as (.+)\." if english
               else rf"(pipeline|strategy|mine) ({Z.ID}) (out\d+) = (.+);")
    m = re.fullmatch(pattern, line)
    if m:
        kind, id_, output, expression = m.groups()
        kind = kind.lower()
        proposed = Z._propose("family" if ctx.family else "program", id_, output, expression, ctx, english)
        if kind == "mine" and (Z.holes_of(proposed.get("expr")) or proposed.get("expr") == ("keep",)):
            raise Refusal("zal-mine-needs-a-concrete-term")
        return Z.act(kind, id_, output=output, expr=proposed.get("expr"))
    pattern = rf"Stages of ({Z.ID}): (.+)\." if english else rf"stages ({Z.ID}): (.+);"
    m = re.fullmatch(pattern, line)
    if m:
        body = m.group(2)
        empty = "gate only" if english else "gate-only"
        words = () if body == empty else tuple(body.split(", then " if english else ", "))
        return Z.act("stages", m.group(1), stages=stages(words))
    pattern = rf"Explore ({Z.ID}) to depth ([1-3])\." if english else rf"explore ({Z.ID}): depth ([1-3]);"
    m = re.fullmatch(pattern, line)
    if m:
        if int(m.group(2)) != 1:
            raise Refusal("zal-strategy-runtime-depth-must-be-one")
        return Z.act("explore", m.group(1), depth=1)
    return None
