"""Revision-bound ladder and ZAL-T statements for the MCP and terminal tools.

Rendering, parsing, comparison, explanation and lowering keep authority none.
Every call reloads the named statement, recomputes its revision and refuses a
stale one. There is no ladder write, acceptance or evaluation path here.
"""

from __future__ import annotations

from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import re
import stat

import ladder as l


SCHEMA = "zal/ladder-tools/1"
EXAMPLES = Path(__file__).resolve().with_name("examples")
MAX_STATEMENTS = 64
MAX_FILE = 256 * 1024
STEM = r"[a-z][a-z0-9_]{0,63}"
STALE = "Stale ladder revision; call zal_ladder_read again"
FAILURES = (l.Refusal, ValueError, KeyError, TypeError, IndexError, AttributeError, OSError, RecursionError)

# What each rung is. The last three come from the rendering registry's inheritance text.
RUNG_MEANINGS = {
    "formula": "symbolic notation with mathematical operators",
    "typed": "the same symbolic statement in ASCII spelling",
    "formal": "a word-for-word reading of the symbols",
    "precise": "complete English built from the declared nouns and clauses",
    **{rung: l.REGISTRY["inheritance"][rung] for rung in ("intuitive", "plain", "question")},
}

NORMALIZATION = ("definition and exists1 expansion, alpha renaming of bound variables, sorting the operands of and/or, "
                 "orienting > and >= as < and <=, sorting the sides of = and !=, removing double negation")

# One explanation per scope-strip field; the drift test binds these keys to the temporal context checks.
SCOPE_MEANINGS = {
    "source_revision": "the recorded observation source the traces come from; another source is another statement",
    "event_schema": "the shape of one event: what is observed before it, as its input, as its outcome and after it",
    "order": "the order of events; next and within-k count events in this order, not seconds",
    "projection": "how recorded events become trace positions; nothing is sorted, merged or dropped unless stated here",
    "mode": "finite N covers every word of 1 through N events; unbounded has no final event and remains a Lean proof obligation until kernel checked",
    "horizon": "the largest finite word length N; within-k needs k below N",
    "assumptions": "the premise that eventually, until and within statements depend on; its versioned definition stays attached",
    "model": "the model the traces are claimed to come from; membership is an obligation, not shown here",
    "environment": "the actors and capabilities the environment may use",
    "initialization": "the separate check of the start, including executions with no events",
    "transitions": "the obligation that every event is a real step of the model",
    "continuity": "the obligation that each event's after-state is the next event's before-state",
    "arithmetic": "checked i128 integers: undefined arithmetic is indeterminate, never wraparound",
    "evidence": "obligation means this statement is a claim still to discharge, not evidence that it holds",
    "domain": "the declared trace domain the outer trace quantifier ranges over",
    "scope_identity": "a hash of every item above plus observation and definition versions; any change gives a new identity, and a hash is an identity, not a proof",
    "observations": "each observation's versioned definition; event observations read the active position, trace observations are fixed once",
    "readings": "registered intuitive or plain wording that applies only in its declared mode and horizon",
}


@dataclass(frozen=True)
class Statement:
    id: str
    revision: str
    data: object
    ctx: l.Context
    tree: tuple


def canonical(value):
    return json.dumps(value, ensure_ascii=True, sort_keys=True, separators=(",", ":"))


def revision_of(data):
    return hashlib.sha256(b"zal/ladder-statement/1\0" + canonical(data).encode()).hexdigest()


def directories():
    found = [("example:ladders/", EXAMPLES / "ladders"), ("example:temporal/", EXAMPLES / "temporal")]
    project = os.environ.get("ZAL_LADDER_DIR")
    if project:
        found.append(("project:", Path(project)))
    return found


def catalogue():
    entries = {}
    for prefix, directory in directories():
        if directory.is_symlink() or not directory.is_dir():
            raise l.Refusal("Ladder statement directory is missing or a symlink: " + prefix.rstrip(":/"))
        for path in sorted(directory.glob("*.json")):
            if re.fullmatch(STEM, path.stem):
                entries[prefix + path.stem] = path
    if len(entries) > MAX_STATEMENTS:
        raise l.Refusal(f"Ladder catalogue exceeds {MAX_STATEMENTS} statements")
    return entries


def read_data(path):
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    with os.fdopen(descriptor, "rb") as stream:
        if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
            raise l.Refusal("Ladder statement is not a regular file")
        raw = stream.read(MAX_FILE + 1)
    if len(raw) > MAX_FILE:
        raise l.Refusal("Ladder statement exceeds 256 KiB")
    return json.loads(raw.decode("utf-8"))


def load(statement, revision):
    """Bind one call to the exact statement bytes it reads; stale revisions refuse first."""
    if not isinstance(statement, str) or not isinstance(revision, str):
        raise l.Refusal("Name a ladder statement and its exact revision")
    path = catalogue().get(statement)
    if path is None:
        raise l.Refusal("Unknown ladder statement; call zal_ladder_read for statement IDs")
    data = read_data(path)
    current = revision_of(data)
    if revision != current:
        raise l.Refusal(STALE)
    ctx = l.Context.from_data(data)
    return Statement(statement, current, data, ctx, l.parse(ctx.data["formula"], "typed", ctx))


def temporal(ctx):
    return ctx.data["theory"] == l.TEMPORAL_THEORY


def profile(ctx):
    return l.TEMPORAL_PROFILE if temporal(ctx) else l.PROFILE


def header(kind, s, renderer):
    return {"kind": kind, "schema": SCHEMA, "statement": s.id, "revision": s.revision, "profile": profile(s.ctx),
            "theory": s.ctx.data["theory"], "renderer": renderer}


def evidence(s):
    a = s.ctx.data["authority"]
    status = "obligation" if temporal(s.ctx) else "fixture tables only"
    return {"statement_kind": s.ctx.data["kind"], "authority_status": a["status"], "authority_source": a["source"],
            "evidence": status, "authority": "none"}


def scope_strip(ctx):
    """Structured scope items, each with its value and its fixed meaning."""
    d = ctx.data
    if not temporal(ctx):
        return [{"field": "scope", "value": d.get("scope", "declared finite domains"),
                 "meaning": "the declared finite domains; fixture verdicts judge stipulated tables, not machine reachability"}]
    t, a = d["trace"], d.get("assumptions")
    items = [(key, t[key]) for key in ("source_revision", "event_schema", "order", "projection", "mode")]
    if t["mode"] == "finite":
        items.append(("horizon", t["horizon"]))
    items.append(("assumptions", (a["formula"] + " := " + a["definition"] + " [version " + a["version"] + "]") if a else "none declared"))
    items.extend((key, t[key]) for key in ("model", "environment", "initialization", "transitions", "continuity",
                                           "arithmetic", "evidence", "domain"))
    items.append(("scope_identity", l.scope_identity(ctx)))
    items.append(("observations", [name + " [" + meta["observation"] + "; version " + meta["version"] + "]: " + meta["definition"]
                                   for name, meta in d["symbols"].items()]))
    items.append(("readings", [r["body"] + " [" + r["version"] + "; " + r["mode"] + "]" for r in d.get("readings", ())]))
    return [{"field": key, "value": l.plain_data(value), "meaning": SCOPE_MEANINGS[key]} for key, value in items]


def read_catalogue(renderer="unbound"):
    statements = []
    for sid, path in catalogue().items():
        item = {"id": sid, "source": "example" if sid.startswith("example:") else "project"}
        try:
            data = read_data(path)
            item["revision"] = revision_of(data)
            ctx = l.Context.from_data(data)
            tree = l.parse(ctx.data["formula"], "typed", ctx)
            item.update(status="admitted", profile=profile(ctx), theory=ctx.data["theory"], statement_kind=ctx.data["kind"],
                        authority_status=ctx.data["authority"]["status"], typed=l.render(tree, "typed", ctx),
                        scope_identity=l.scope_identity(ctx) if temporal(ctx) else None)
        except FAILURES as error:
            item.update(status="refused", refusal=str(error)[:500])
        statements.append(item)
    return {"kind": "ladder-catalogue", "schema": SCHEMA, "renderer": renderer, "statements": statements,
            "rungs": list(l.RUNGS), "project_directory": "set" if os.environ.get("ZAL_LADDER_DIR") else "unset",
            "workflow": "Pass a statement's exact revision to every other ladder tool. After a stale refusal or a file change, read again. "
                        "There is no ladder write or acceptance tool.",
            "authority": "none"}


def render(statement, revision, rung=None, renderer="unbound"):
    s = load(statement, revision)
    if rung is not None and rung not in l.RUNGS:
        raise l.Refusal("unknown-rung at token 0; expected " + ", ".join(l.RUNGS))
    rungs = {name: l.render(s.tree, name, s.ctx) for name in l.RUNGS if rung in (None, name)}
    legend = [{"symbol": symbol, "read": read, "meaning": meaning} for symbol, read, meaning in l.legend(s.tree, s.ctx)]
    return {**header("ladder-rendering", s, renderer), "rung": rung or "all", "rungs": rungs, "legend": legend,
            "scope": scope_strip(s.ctx), "evidence": evidence(s), "text": l.ladder_text(s.tree, s.ctx, rung),
            "method": "deterministic registry templates; no language model", "authority": "none"}


def uniqueness(text, rung, ctx, admitted):
    """Count the admission stages that make one parse unique, then cross-check with ladder.parse."""
    if rung == "question":
        return {"rung": rung, "unique": True, "counts": None,
                "sentence": "The question rung parses its target with the plain grammar, and its search task must be the "
                            "deterministic task for that target; admission found exactly one expanded normalized meaning."}
    tokens = l.tokenize(text)
    if rung in ("formula", "typed"):
        rules, case = l.symbolic_grammar(temporal(ctx)), False
    else:
        rules, case, tokens = l.grammar(ctx, rung), True, tokens[:-1]
    derivations = l.earley(tokens, rules, sentence_case=case)
    trees = {l.bounded(l.clean(tree)) for tree in derivations}
    typed = []
    for tree in trees:
        try:
            if l.validate(tree, ctx) == "bool":
                typed.append(tree)
        except l.Refusal:
            pass
    meanings = {l.normal(tree, ctx) for tree in typed}
    if meanings != {l.normal(admitted, ctx)}:
        raise l.Refusal("internal: the uniqueness explanation disagrees with admission")
    counts = {"complete_derivations": len(derivations), "distinct_syntax_trees": len(trees),
              "well_typed_formulas": len(typed), "normalized_meanings": len(meanings)}
    plural = lambda n, word: f"{n} {word}{'' if n == 1 else 's'}"
    text = (f"The {rung} grammar gave {plural(counts['complete_derivations'], 'complete derivation')}, "
            f"{plural(counts['distinct_syntax_trees'], 'distinct syntax tree')} and "
            f"{plural(counts['well_typed_formulas'], 'well-typed formula')}, with exactly one expanded normalized meaning.")
    if len(typed) > 1:
        text += " The formulas differ only in ways normalization removes: " + NORMALIZATION + "."
    elif len(trees) > len(typed):
        text += " The other syntax trees are refused by typing."
    return {"rung": rung, "unique": True, "counts": counts, "sentence": text}


def parse(statement, revision, rung, text, renderer="unbound"):
    s = load(statement, revision)
    tree = l.parse(text, rung, s.ctx)
    normal = l.normal(tree, s.ctx)
    return {**header("ladder-parse", s, renderer), "rung": rung, "text": text, "tree": l.plain_data(tree),
            "normalized": l.plain_data(normal), "formula": l.render(tree, "formula", s.ctx),
            "typed": l.render(tree, "typed", s.ctx), "same_as_statement": normal == l.normal(s.tree, s.ctx),
            "uniqueness": uniqueness(text, rung, s.ctx, tree), "scope_identity": l.scope_identity(s.ctx) if temporal(s.ctx) else None,
            "method": "complete Earley parse; exactly one expanded normalized tree is admitted", "authority": "none"}


def difference(a, b):
    found = l.first_difference(a, b)
    return None if found is None else {"path": found[0], "left": l.plain_data(found[1]), "right": l.plain_data(found[2])}


def same(statement, revision, other_statement=None, other_revision=None, rung=None, text=None, renderer="unbound"):
    """Compare normal forms, and for temporal statements the complete scope identity."""
    by_statement = other_statement is not None or other_revision is not None
    if by_statement == (rung is not None or text is not None) or by_statement and (other_statement is None or other_revision is None) \
            or not by_statement and (rung is None or text is None):
        raise l.Refusal("Compare with another statement and its revision, or with a rung and text")
    a = load(statement, revision)
    result = header("ladder-same", a, renderer)
    if by_statement:
        b = load(other_statement, other_revision)
        result.update(other={"statement": b.id, "revision": b.revision, "theory": b.ctx.data["theory"]})
        if a.ctx.data["theory"] != b.ctx.data["theory"]:
            return {**result, "status": "not-compared", "same": None,
                    "summary": "Different theories; the statements are not compared.", "authority": "none"}
        other_tree, other_ctx = b.tree, b.ctx
    else:
        other_tree, other_ctx = l.parse(text, rung, a.ctx), a.ctx
        result.update(other={"rung": rung, "text": text})
    left, right = l.normal(a.tree, a.ctx), l.normal(other_tree, other_ctx)
    if temporal(a.ctx):
        scope = {"left": l.scope_identity(a.ctx), "right": l.scope_identity(other_ctx)}
    else:
        vocabulary = lambda ctx: canonical(l.plain_data({key: ctx.data[key] for key in ("domains", "symbols", "definitions")}))
        scope = {"left": hashlib.sha256(vocabulary(a.ctx).encode()).hexdigest(),
                 "right": hashlib.sha256(vocabulary(other_ctx).encode()).hexdigest()}
    scope_kind = "scope identity" if temporal(a.ctx) else "declared vocabulary"
    if left == right and scope["left"] == scope["right"]:
        status, summary = "same", f"Same meaning: the expanded normal forms are identical and the {scope_kind} is identical."
    elif left == right:
        status, summary = "same-formula-different-scope", f"The normal forms are identical, but the {scope_kind} differs, so these are different statements."
    else:
        status, summary = "different-normal-forms", ("The normal forms differ. This does not show the meanings differ: "
                                                     "equivalent statements can have different normal forms.")
    return {**result, "status": status, "same": status == "same", "summary": summary,
            "evidence": {"normalized_left": l.plain_data(left), "normalized_right": l.plain_data(right),
                         "first_difference": difference(left, right), scope_kind.replace(" ", "_"): scope,
                         "normalization": NORMALIZATION,
                         "soundness": "Each normalization step preserves meaning, so identical normal forms with an identical "
                                      + scope_kind + " mean the same; the comparison is not complete."},
            "authority": "none"}


def lower(statement, revision, renderer="unbound"):
    from temporal_lowering import IMPLEMENTATION_ID, lower as inert
    s = load(statement, revision)
    result = inert(s.tree, s.ctx)
    return {**header("ladder-lowering", s, renderer), "status": "inert-claim" if result.source else "unsupported-lowering",
            "subject": result.subject, "obligations": list(result.obligations), "source": result.source,
            "lowering_identity": IMPLEMENTATION_ID, "scope": scope_strip(s.ctx),
            "note": "An inert native claim for the existing Rust parser and temporal constructors. Lowering evaluates nothing, "
                    "admits nothing and proves nothing; a native parser or export pass qualifies only its syntax.",
            "authority": result.authority}


def explain(statement, revision, rung=None, text=None, renderer="unbound"):
    """Say which rung admits a text, why its parse is unique and what the scope strip means."""
    s = load(statement, revision)
    if rung is not None and rung not in l.RUNGS:
        raise l.Refusal("unknown-rung at token 0; expected " + ", ".join(l.RUNGS))
    own = text is None
    text = s.ctx.data["formula"] if own else text
    attempts = []
    for name in ([rung] if rung else ["typed"] if own else list(l.RUNGS)):
        try:
            tree = l.parse(text, name, s.ctx)
            attempts.append({"rung": name, "admitted": True, "meaning": RUNG_MEANINGS[name],
                             "uniqueness": uniqueness(text, name, s.ctx, tree), "tree": tree})
        except FAILURES as error:
            attempts.append({"rung": name, "admitted": False, "meaning": RUNG_MEANINGS[name], "refusal": str(error)[:500]})
    admitted = [item for item in attempts if item["admitted"]]
    meanings = {l.normal(item["tree"], s.ctx) for item in admitted}
    summary = []
    if not admitted:
        status = "refused"
        summary.append("No tried rung admits this text; each refusal is named below.")
    elif len(meanings) > 1:
        status = "ambiguous-across-rungs"
        summary.append("Different rungs admit this text with different meanings; name the rung explicitly.")
    else:
        status = "admitted"
        names = ", ".join(item["rung"] for item in admitted)
        summary.append(f"Rung used: {names} ({'; '.join(item['meaning'] for item in admitted)}).")
        summary.append("Why the parse is unique: " + admitted[0]["uniqueness"]["sentence"])
        same_as = l.normal(admitted[0]["tree"], s.ctx) == l.normal(s.tree, s.ctx)
        summary.append("It has the same normalized meaning as the statement's formula." if same_as else
                       "Its normalized meaning differs from the statement's formula.")
    refused = [item for item in attempts if not item["admitted"]]
    if admitted and refused:
        summary.append("Not admitted at: " + ", ".join(item["rung"] for item in refused) + "; each attempt names its refusal.")
    elif not admitted:
        summary.extend(f"At {item['rung']}: {item['refusal']}" for item in refused)
    scope = scope_strip(s.ctx)
    summary.extend(("Scope " + item["field"].replace("_", " ")).replace("Scope scope", "Scope") + ": " + item["meaning"] + "."
                   for item in scope)
    for item in attempts:
        item.pop("tree", None)
    return {**header("ladder-explanation", s, renderer), "status": status, "text": text, "own_formula": own,
            "attempts": attempts, "scope": scope, "evidence": evidence(s), "summary": summary,
            "method": "complete parses at the named rungs and fixed registry meanings; no language model", "authority": "none"}


def mcp_call(name, arguments, renderer="unbound"):
    a = arguments
    if name == "zal_ladder_read":
        return read_catalogue(renderer)
    if name == "zal_ladder_render":
        return render(a["statement"], a["revision"], a.get("rung"), renderer)
    if name == "zal_ladder_parse":
        return parse(a["statement"], a["revision"], a["rung"], a["text"], renderer)
    if name == "zal_ladder_same":
        return same(a["statement"], a["revision"], a.get("other_statement"), a.get("other_revision"),
                    a.get("rung"), a.get("text"), renderer)
    if name == "zal_ladder_lower":
        return lower(a["statement"], a["revision"], renderer)
    if name == "zal_ladder_explain":
        return explain(a["statement"], a["revision"], a.get("rung"), a.get("text"), renderer)
    raise l.Refusal("Unknown tool")


def human_text(result):
    """Readable terminal view of one ladder tool result."""
    kind = result["kind"]
    if kind == "ladder-catalogue":
        lines = ["Ladder statements (authority: none):"]
        for item in result["statements"]:
            lines.append(f"{item['id']}  {item.get('revision', '')[:12]}  " +
                         (f"{item['profile']}  {item['typed']}" if item["status"] == "admitted" else "refused: " + item["refusal"]))
        return "\n".join(lines)
    lines = [f"{result['statement']} at {result['revision'][:12]} | {result['profile']} | authority: none"]
    if kind == "ladder-rendering":
        return lines[0] + "\n" + result["text"].rstrip()
    if kind == "ladder-parse":
        lines.extend([f"Rung {result['rung']}: {result['formula']}", "Typed: " + result["typed"],
                      result["uniqueness"]["sentence"],
                      "Same meaning as the statement's formula." if result["same_as_statement"] else
                      "Different normalized meaning from the statement's formula."])
    elif kind == "ladder-same":
        lines.append(result["summary"])
        if result.get("evidence", {}).get("first_difference"):
            lines.append("First differing subtree at " + result["evidence"]["first_difference"]["path"])
    elif kind == "ladder-lowering":
        lines.extend(["Lowering subject: " + result["subject"], *("Obligation: " + item for item in result["obligations"])])
        lines.append("Native source (inert; authority: none):\n" + result["source"].rstrip() if result["source"] else
                     "Unsupported lowering; readable obligation retained")
    elif kind == "ladder-explanation":
        lines.extend(result["summary"])
    return "\n".join(lines)
