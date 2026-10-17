"""Black-box capture acts for zal/fcis-loop/1, LLM-generated.

Parsing/rendering only; all candidate decisions come from the Rust CLI.
"""
import re

from behavior import Refusal

LEGEND = """capture ID OUT = EXPR; | Capture ID computes OUT as EXPR.
capture-example ID: input (V, ...), output (V, ...); | Capture example ID: input (V, ...), output (V, ...).
examples-equivalent ID: N examples; | Examples equivalent ID: agrees with all N selected examples.
captured-exact ID: N tuples, replay matched; | Captured exact ID: equal on all N declared inputs, with receipt replay matched.
capture-result ID: STATUS; | Capture result ID: STATUS.
"""
RESULTS = {"refuted", "precheck-refuted", "candidate-refused", "inconclusive", "refused", "ready"}
ACT_KINDS = frozenset({"capture", "capture-example", "examples-equivalent", "captured-exact", "capture-result"})


def is_statement(line, english):
    prefixes = ("Capture ", "Examples equivalent ", "Captured exact ") if english else \
        ("capture ", "capture-example ", "examples-equivalent ", "captured-exact ", "capture-result ")
    return line.startswith(prefixes)


def render(a, ctx, english):
    import loop_language as Z
    k, i, g = a.kind, a.id, a.get
    if k == "capture":
        expression = Z.show(g("expr"), ctx, english, want=ctx.output_kind(g("output")))
        return f"Capture {i} computes {g('output')} as {expression}." if english else \
            f"capture {i} {g('output')} = {expression};"
    if k == "capture-example":
        inputs = ", ".join(map(str, g("input")))
        outputs = ", ".join(map(str, g("output")))
        return f"Capture example {i}: input ({inputs}), output ({outputs})." if english else \
            f"capture-example {i}: input ({inputs}), output ({outputs});"
    if k == "examples-equivalent":
        return f"Examples equivalent {i}: agrees with all {g('examples')} selected examples." if english else \
            f"examples-equivalent {i}: {g('examples')} examples;"
    if k == "captured-exact":
        return f"Captured exact {i}: equal on all {g('tuples')} declared inputs, with receipt replay matched." if english else \
            f"captured-exact {i}: {g('tuples')} tuples, replay matched;"
    if k == "capture-result":
        if g("status") not in RESULTS:
            raise Refusal("zal-capture-result")
        return f"Capture result {i}: {g('status')}." if english else f"capture-result {i}: {g('status')};"
    return None


def _values(text, width):
    import loop_language as Z
    tokens = [v.strip() for v in text.split(",")] if text.strip() else []
    if len(tokens) != width or any(not re.fullmatch(r"(?:0|-?[1-9][0-9]{0,18})", t) for t in tokens):
        raise Refusal("zal-capture-values")
    return tuple(Z._integer(t.lstrip("-"), t.startswith("-")) for t in tokens)


def parse(line, ctx, english):
    import loop_language as Z
    terminator = "." if english else ";"
    if not line.endswith(terminator):
        return None
    body = line[:-1].strip()
    pattern = rf"Capture ({Z.ID}) computes (out\d+) as (.+)" if english else rf"capture ({Z.ID}) (out\d+) = (.+)"
    match = re.fullmatch(pattern, body)
    if match:
        i, out, expression = match.groups()
        if out not in ctx.outputs:
            raise Refusal("zal-capture-output")
        node = Z.read(expression, ctx, english)
        if Z.holes_of(node) or Z.params_of(node) or Z.names_of(node) - set(ctx.names):
            raise Refusal("zal-capture-complete-input-expression-required")
        return Z.act("capture", i, output=out, expr=node)
    pattern = rf"Capture example ({Z.ID}): input \((.*?)\), output \((.*?)\)" if english else \
        rf"capture-example ({Z.ID}): input \((.*?)\), output \((.*?)\)"
    match = re.fullmatch(pattern, body)
    if match:
        i, inputs, outputs = match.groups()
        return Z.act("capture-example", i, input=_values(inputs, len(ctx.names)),
                     output=_values(outputs, len(ctx.outputs)))
    pattern = rf"Examples equivalent ({Z.ID}): agrees with all (\d{{1,4}}) selected examples" if english else \
        rf"examples-equivalent ({Z.ID}): (\d{{1,4}}) examples"
    match = re.fullmatch(pattern, body)
    if match:
        n = int(match[2])
        if n > 4096:
            raise Refusal("zal-capture-count-bound")
        return Z.act("examples-equivalent", match[1], examples=n)
    pattern = rf"Captured exact ({Z.ID}): equal on all (\d{{1,4}}) declared inputs, with receipt replay matched" if english else \
        rf"captured-exact ({Z.ID}): (\d{{1,4}}) tuples, replay matched"
    match = re.fullmatch(pattern, body)
    if match:
        n = int(match[2])
        if not 1 <= n <= 4096:
            raise Refusal("zal-capture-count-bound")
        return Z.act("captured-exact", match[1], tuples=n)
    pattern = rf"Capture result ({Z.ID}): ([a-z-]+)" if english else rf"capture-result ({Z.ID}): ([a-z-]+)"
    match = re.fullmatch(pattern, body)
    if match:
        if match[2] not in RESULTS:
            raise Refusal("zal-capture-result")
        return Z.act("capture-result", match[1], status=match[2])
    return None
