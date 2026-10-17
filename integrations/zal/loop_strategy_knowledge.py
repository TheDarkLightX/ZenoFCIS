"""Read-only public-example strategy knowledge acts. No private KB acquisition."""
import re
from behavior import Refusal
import loop_doors as D
from loop_strategy_modes import strict_problem


def request(text):
    value = strict_problem(text)
    if set(value) - {"operation", "query", "have", "want", "max_steps", "max_states", "max_work"}:
        raise Refusal("strategy-kb-request-fields")
    op = value.get("operation")
    if op not in ("query", "check", "plan"):
        raise Refusal("strategy-kb-operation")
    if op == "query":
        if type(value.get("query")) is not str or not value["query"] or len(value["query"]) > 4096:
            raise Refusal("strategy-kb-query")
        if "have" in value or "want" in value:
            raise Refusal("strategy-kb-operation-fields")
    elif op == "check":
        if any(k in value for k in ("query", "have", "want")):
            raise Refusal("strategy-kb-operation-fields")
    else:
        if "query" in value:
            raise Refusal("strategy-kb-operation-fields")
        for key in ("have", "want"):
            names = value.get(key)
            if (type(names) is not list or not 1 <= len(names) <= 18
                    or any(type(n) is not str or len(n) > 128
                           or re.fullmatch(r"[a-z][a-z0-9-]*", n) is None for n in names)):
                raise Refusal("strategy-kb-type-list")
    for key, limit in (("max_steps", 16), ("max_states", 4096), ("max_work", 2_000_000)):
        if key in value and (type(value[key]) is not int or not 0 <= value[key] <= limit):
            raise Refusal("strategy-kb-budget")
    return value


def route(workspace, state, ctx, acts, seq):
    if len(acts) != 1:
        raise Refusal("strategy-kb-one-act")
    a = acts[0]
    value = request(a.get("request"))
    workspace._contract(state)
    arguments = ["loop", "strategy", "--kb-op", value["operation"]]
    if "query" in value:
        arguments += ["--query", value["query"]]
    for key in ("have", "want"):
        if key in value:
            arguments += ["--" + key, ",".join(value[key])]
    for key, flag in (("max_steps", "--kb-max-steps"), ("max_states", "--kb-max-states"),
                      ("max_work", "--max-search-work")):
        if key in value:
            arguments += [flag, str(value[key])]
    code, report = D.run(workspace.cli(state), arguments)
    if report.get("status") in ("qualified", "accepted", "captured", "determined"):
        raise Refusal("strategy-kb-has-no-result-authority")
    workspace._fact(state, ctx, a.id, f"{report.get('status', 'refused')}: structural knowledge proposal only",
                    seq, {"report": report, "exit_code": code})
