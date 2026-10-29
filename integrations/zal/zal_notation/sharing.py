"""Revision-bound discussion data and a pure, model-independent LLM toolkit.

People's acknowledgements are self-declarations, not authentication or a test of
understanding. Imported reasoning records acquire no status until replayed.
"""
from __future__ import annotations

import re
from copy import deepcopy

from . import core
from .builtin import get_builtin

TRANSFER_PROFILE = "zal/guarded-transfer/1"
TRANSFER_ASSUMPTIONS = [
    "a, r, and q are integers in 0..15.",
    "Accept exactly when 1 <= q <= a and r <= 15 - q.",
    "An accepted step returns (a - q, r + q, Accept), using the prestate.",
    "A rejected step returns (a, r, Reject).",
]
TRANSFER_ANCHORS = (
    "transfer.input", "transfer.guard", "transfer.accept", "transfer.reject",
    "transfer.result", "transfer.observation", "comparison.reject-all",
)
NOTE_KINDS = ("question", "claim", "counterexample", "proposal", "explanation")
AGREEMENT = "I agree to use this model, interpretation, assumptions, and observation for this discussion."
MAX_RECORDS = 128


def _text(value, where, limit=8000, *, empty=False):
    core.need(type(value) is str and (empty or bool(value)) and len(value) <= limit,
              f"{where}: expected bounded text")


def _identity(value, where="context digest"):
    core.need(type(value) is str and re.fullmatch(r"[0-9a-f]{64}", value) is not None,
              f"{where}: expected SHA-256 identity")


def _browser_numbers(value, *, exact=False):
    # Refuse silent precision loss in browser JSON. Large exact numbers can be
    # declared as strings by a grammar with an explicit numeric interpretation.
    if type(value) is dict:
        for child in value.values():
            _browser_numbers(child, exact=exact)
    elif type(value) is list:
        for child in value:
            _browser_numbers(child, exact=exact)
    elif type(value) is int:
        core.need(abs(value) <= 9007199254740991, "shared JSON integer exceeds browser exact range; use an explicit decimal-string representation")
    elif exact and type(value) is float:
        raise core.NotationError("semantic context numbers must be exact integers or explicit symbolic/decimal strings")


def observation(identifier="allocation-and-outcome"):
    if identifier == "allocation-and-outcome":
        return {"id": identifier, "description": "Observe sender, receiver, and the Accept/Reject outcome.",
                "keeps": ["sender", "receiver", "outcome"], "omits": []}
    core.need(identifier == "total", "unsupported transfer observation")
    return {"id": "total", "description": "Observe only total quantity a+r.",
            "keeps": ["total"], "omits": ["allocation", "outcome"]}


def make_context(grammar, model, *, semantics, assumptions, observation):
    context = {"schema": "zal.context/1", "grammar": deepcopy(grammar), "model": deepcopy(model),
               "semantics": deepcopy(semantics), "assumptions": deepcopy(assumptions),
               "observation": deepcopy(observation)}
    validate_context(context)
    return context


def validate_context(context):
    core.canonical(context)
    _browser_numbers(context, exact=True)
    core.keys(context, {"schema", "grammar", "model", "semantics", "assumptions", "observation"}, where="shared context")
    core.need(context["schema"] == "zal.context/1", "unsupported shared context schema")
    core.validate_model(context["grammar"], context["model"])
    semantics = context["semantics"]
    core.keys(semantics, {"profile", "parameters", "description"}, where="semantics declaration")
    core.name(semantics["profile"], "semantic profile")
    core.need(type(semantics["parameters"]) is dict, "semantic parameters must be an object")
    _text(semantics["description"], "semantic description")
    assumptions = context["assumptions"]
    core.need(type(assumptions) is list and len(assumptions) <= 64, "assumptions must be a bounded list")
    for item in assumptions:
        _text(item, "assumption")
    observed = context["observation"]
    core.keys(observed, {"id", "description", "keeps", "omits"}, where="observation")
    core.name(observed["id"], "observation id")
    _text(observed["description"], "observation description")
    for key in ("keeps", "omits"):
        core.need(type(observed[key]) is list and len(observed[key]) <= 64, "observation labels must be a bounded list")
        for label in observed[key]:
            core.name(label, "observation label")
        core.need(len(set(observed[key])) == len(observed[key]), "duplicate observation labels")
    core.need(not set(observed["keeps"]) & set(observed["omits"]), "an observation cannot both retain and omit a label")
    executable = {"zal/lambda-normal-order/1": "lambda", "zal/fsm/1": "state-machine", "zal/petri/1": "petri-net"}
    if semantics["profile"] in executable:
        expected_grammar, _, _ = get_builtin(executable[semantics["profile"]])
        core.need(semantics["parameters"] == {}, "this registered semantic profile admits no parameters")
        core.need(context["grammar"] == expected_grammar, "registered semantic profile requires its exact grammar")
    if semantics["profile"] == TRANSFER_PROFILE:
        grammar, _, model = get_builtin("zal-folds")
        core.need(context["grammar"] == grammar and context["model"] == model,
                  "transfer profile requires its exact declared definition, not a reused profile name")
        core.need(semantics["parameters"] == {"max_balance": 15}, "unsupported transfer bounds")
        core.need(assumptions == TRANSFER_ASSUMPTIONS, "transfer checker requires its exact interpretation assumptions")
        core.need(observed == observation(observed["id"]), "transfer observation does not match its declared map")
    if semantics["profile"] == "zal/supplied-tick-clock/1":
        from .clock import validate_context as validate_clock
        validate_clock(context)
    return context


def make_discussion(context, notation, *, title="ZAL discussion"):
    validate_context(context)
    core.validate_pack(context["grammar"], notation)
    presentation = ({"a": 10, "r": 3, "q": 4, "view": "folded", "progress": 1.0, "compare": True}
                    if context["semantics"]["profile"] == TRANSFER_PROFILE else {"folded": []})
    result = {"schema": "zal.discussion/1", "title": title, "context": deepcopy(context),
              "context_digest": core.digest(context), "notation": deepcopy(notation),
              "notes": [], "acknowledgements": [], "reasoning": [],
              "presentation": presentation}
    return validate_discussion(result)


def make_transfer_bundle(*, observation_id="allocation-and-outcome"):
    grammar, notation, model = get_builtin("zal-folds")
    context = make_context(grammar, model,
        semantics={"profile": TRANSFER_PROFILE, "parameters": {"max_balance": 15},
                   "description": "Guarded bounded transfer with explicit Accept and Reject outcomes."},
        assumptions=TRANSFER_ASSUMPTIONS, observation=observation(observation_id))
    return make_discussion(context, notation, title="One transfer, three views")


def demo_transfer_bundle():
    """An explicitly fictional example exchange; no messages are sent to people."""
    bundle = make_transfer_bundle()
    bundle = add_note(bundle, speaker="Alice (example)", kind="question", anchor="transfer.observation",
                      text="Can we discuss this transfer using only the total quantity? What would that hide?")
    bundle = add_reasoning(bundle, {"kind": "transfer-check", "claim": "total-preserved"})
    bundle = add_note(bundle, speaker="Bob (example)", kind="explanation", anchor="transfer.observation", reply_to="n1",
                      text="The total is preserved for all 4,096 inputs in this bounded model. The total view omits allocation and Accept/Reject.")
    bundle = add_reasoning(bundle, {"kind": "transfer-check", "claim": "same-total-as-reject-all"})
    bundle = add_reasoning(bundle, {"kind": "transfer-check", "claim": "full-equivalence-reject-all"})
    bundle["presentation"].update(a=1, r=0, q=1, view="expanded")
    bundle = add_note(bundle, speaker="Alice (example)", kind="counterexample", anchor="comparison.reject-all", reply_to="n2",
                      text="At a=1, r=0, q=1, the guarded policy returns (0,1,Accept); reject-all returns (1,0,Reject). Both totals are 1, so agreement about the total alone does not establish the full behavior.")
    bundle = acknowledge(bundle, speaker="Alice (example)")
    bundle = acknowledge(bundle, speaker="Bob (example)")
    bundle["presentation"].update(a=10, r=3, q=4, view="folded")
    return bundle


def anchors(bundle):
    context = bundle["context"]
    model = context["model"]
    result = ["model"]
    if model["kind"] == "term":
        def visit(term, path):
            result.append(path)
            for index, child in enumerate(term["children"]):
                visit(child, f"{path}.children.{index}")
        visit(model["body"], "root")
    else:
        result.extend("nodes." + node["id"] for node in model["body"]["nodes"])
        result.extend("edges." + edge["id"] for edge in model["body"]["edges"])
    if context["semantics"]["profile"] == TRANSFER_PROFILE:
        result.extend(TRANSFER_ANCHORS)
    return result


def validate_discussion(bundle):
    core.canonical(bundle)
    _browser_numbers(bundle)
    core.keys(bundle, {"schema", "title", "context", "context_digest", "notation", "notes",
                       "acknowledgements", "reasoning", "presentation"}, where="discussion")
    core.need(bundle["schema"] == "zal.discussion/1", "unsupported discussion schema")
    _text(bundle["title"], "title", 200)
    validate_context(bundle["context"])
    _identity(bundle["context_digest"])
    core.need(bundle["context_digest"] == core.digest(bundle["context"]), "shared context identity mismatch")
    core.validate_pack(bundle["context"]["grammar"], bundle["notation"])
    valid_anchors = set(anchors(bundle))
    for key in ("notes", "acknowledgements", "reasoning"):
        core.need(type(bundle[key]) is list and len(bundle[key]) <= MAX_RECORDS, f"{key} record budget exceeded")
    seen = set()
    for note in bundle["notes"]:
        core.keys(note, {"id", "kind", "speaker", "text", "context_digest", "anchor", "reply_to"}, {"selection"}, where="discussion note")
        core.name(note["id"], "note id")
        core.need(note["id"] not in seen, "duplicate note id")
        core.need(note["kind"] in NOTE_KINDS, "unknown note kind")
        _text(note["speaker"], "speaker", 80)
        _text(note["text"], "note")
        _identity(note["context_digest"])
        core.name(note["anchor"], "note anchor")
        if note["context_digest"] == bundle["context_digest"]:
            core.need(note["anchor"] in valid_anchors, "note anchor is absent from this model")
            if "selection" in note:
                validate_selection(bundle["context"], note["selection"])
        elif "selection" in note:
            core.need(type(note["selection"]) is dict, "historical note selection must be an inert object")
        core.need(note["reply_to"] is None or type(note["reply_to"]) is str and note["reply_to"] in seen,
                  "reply must reference an earlier note")
        seen.add(note["id"])
    acknowledgements = set()
    for ack in bundle["acknowledgements"]:
        core.keys(ack, {"speaker", "context_digest", "statement", "identity"}, where="acknowledgement")
        _text(ack["speaker"], "speaker", 80)
        _identity(ack["context_digest"])
        _text(ack["statement"], "agreement statement")
        core.need(ack["identity"] == "self-declared", "this format does not authenticate people")
        key = (ack["speaker"], ack["context_digest"])
        core.need(key not in acknowledgements, "duplicate acknowledgement")
        acknowledgements.add(key)
    seen = set()
    for step in bundle["reasoning"]:
        core.keys(step, {"id", "context_digest", "request", "result"}, where="reasoning record")
        core.name(step["id"], "reasoning id")
        core.need(step["id"] not in seen, "duplicate reasoning id")
        seen.add(step["id"])
        _identity(step["context_digest"])
        core.need(type(step["request"]) is dict and type(step["result"]) is dict,
                  "reasoning record must contain a request and replayable result")
    validate_selection(bundle["context"], bundle["presentation"])
    return bundle


def validate_selection(context, p):
    if context["semantics"]["profile"] != TRANSFER_PROFILE:
        core.keys(p, {"folded"}, where="generic presentation")
        core.need(type(p["folded"]) is list and len(p["folded"]) <= 512 and all(type(path) is str for path in p["folded"]),
                  "generic folded view must be a bounded path list")
        return p
    core.keys(p, {"a", "r", "q", "view", "progress", "compare"}, where="presentation")
    for name in ("a", "r", "q"):
        core.need(type(p[name]) is int and 0 <= p[name] <= 15, "transfer selection must be in 0..15")
    core.need(p["view"] in ("folded", "expanded", "abstract"), "unknown transfer view")
    core.need(type(p["progress"]) in (int, float) and 0 <= p["progress"] <= 1, "progress must be in 0..1")
    core.need(type(p["compare"]) is bool, "compare must be Boolean")
    return p


def add_note(bundle, *, speaker, text, kind="question", anchor="model", reply_to=None):
    validate_discussion(bundle)
    result = deepcopy(bundle)
    number = len(result["notes"]) + 1
    used = {note["id"] for note in result["notes"]}
    while f"n{number}" in used:
        number += 1
    result["notes"].append({"id": f"n{number}", "kind": kind, "speaker": speaker, "text": text,
                            "context_digest": result["context_digest"], "anchor": anchor, "reply_to": reply_to,
                            "selection": deepcopy(result["presentation"])})
    return validate_discussion(result)


def acknowledge(bundle, *, speaker, statement=AGREEMENT):
    validate_discussion(bundle)
    result = deepcopy(bundle)
    result["acknowledgements"].append({"speaker": speaker, "context_digest": result["context_digest"],
                                      "statement": statement, "identity": "self-declared"})
    return validate_discussion(result)


def run_request(bundle, request):
    validate_discussion(bundle)
    core.need(type(request) is dict, "reasoning request must be an object")
    context = bundle["context"]
    if request.get("kind") == "clock-transition":
        core.keys(request, {"kind", "state", "position", "event"}, where="clock transition")
        from .clock import PROFILE, step
        core.need(context["semantics"]["profile"] == PROFILE, "clock operation requires its declared profile")
        return {"status": "executed", "context_digest": core.digest(context),
                "adapter": PROFILE, "scope": "one supplied event in the declared clock domain",
                "result": step(context, request["state"], request["position"], request["event"]), "authority": "none"}
    if request.get("kind") == "transfer-check":
        core.keys(request, {"kind", "claim"}, where="transfer check")
        core.need(context["semantics"]["profile"] == TRANSFER_PROFILE, "transfer rule cannot be applied to another interpretation")
        from .reasoning import check_transfer_claim
        return check_transfer_claim(request["claim"])
    expected = {"lambda-beta": "zal/lambda-normal-order/1", "fsm-transition": "zal/fsm/1", "petri-fire": "zal/petri/1"}
    kind = request.get("kind")
    core.need(type(kind) is str, "reasoning kind must be text")
    if kind not in expected:
        return {"status": "unsupported", "reason": "No registered semantic rule for this operation.", "authority": "none"}
    core.need(context["semantics"]["profile"] == expected[kind], "declared interpretation does not admit this rule")
    from .reasoning import reasoning_step
    return reasoning_step(context["grammar"], context["model"], request)


def add_reasoning(bundle, request):
    result = deepcopy(validate_discussion(bundle))
    receipt = run_request(bundle, request)
    number = len(result["reasoning"]) + 1
    used = {step["id"] for step in result["reasoning"]}
    while f"r{number}" in used:
        number += 1
    result["reasoning"].append({"id": f"r{number}", "context_digest": bundle["context_digest"],
                                "request": deepcopy(request), "result": receipt})
    return validate_discussion(result)


def replay_discussion(bundle):
    validate_discussion(bundle)
    records = []
    for step in bundle["reasoning"]:
        if step["context_digest"] != bundle["context_digest"]:
            records.append({"id": step["id"], "status": "stale-context"})
            continue
        replay = run_request(bundle, step["request"])
        records.append({"id": step["id"], "status": "matches-replay" if core.canonical(replay) == core.canonical(step["result"]) else "result-mismatch",
                        "replayed_result_status": replay.get("status", "unspecified")})
    return {"status": "replayed" if all(row["status"] == "matches-replay" for row in records) else "unresolved",
            "records": records, "context_digest": bundle["context_digest"], "authority": "none",
            "scope": "Recomputed registered operations; notes and self-declared acknowledgements are not proofs."}


def inspect_discussion(bundle):
    validate_discussion(bundle)
    current = bundle["context_digest"]
    return {"schema": "zal.discussion-inspection/1", "status": "structurally-valid", "context_digest": current,
            "model_digest": core.model_digest(bundle["context"]["grammar"], bundle["context"]["model"]),
            "notation_digest": core.digest(bundle["notation"]), "anchors": anchors(bundle),
            "semantics": deepcopy(bundle["context"]["semantics"]), "assumptions": deepcopy(bundle["context"]["assumptions"]),
            "observation": deepcopy(bundle["context"]["observation"]),
            "current_acknowledgements": [ack["speaker"] for ack in bundle["acknowledgements"] if ack["context_digest"] == current],
            "stale_acknowledgements": [ack["speaker"] for ack in bundle["acknowledgements"] if ack["context_digest"] != current],
            "stale_notes": [note["id"] for note in bundle["notes"] if note["context_digest"] != current],
            "reasoning_records": len(bundle["reasoning"]), "reasoning_status": "not-replayed",
            "identity": "self-declared; no authentication or test of human understanding", "authority": "none"}


def compare_discussions(left, right):
    validate_discussion(left)
    validate_discussion(right)
    differences = [key for key in ("grammar", "model", "semantics", "assumptions", "observation")
                   if core.canonical(left["context"][key]) != core.canonical(right["context"][key])]
    return {"status": "same-context" if not differences else "different-context", "differences": differences,
            "left_context": left["context_digest"], "right_context": right["context_digest"],
            "same_notation": core.digest(left["notation"]) == core.digest(right["notation"]),
            "scope": "Exact shared interpretation; semantic equivalence of different contexts requires a separate check.",
            "authority": "none"}


def toolkit():
    from .ontology_tools import tool_definitions as ontology_tools
    from .story import toolkit as story_toolkit

    def spec(name, description, required, properties):
        return {"name": "zal_" + name, "description": description,
                "input_schema": {"type": "object", "required": required, "additionalProperties": False, "properties": properties}}
    obj = {"type": "object"}
    txt = {"type": "string"}
    bundle = {"bundle": obj}
    result = {"schema": "zal.toolkit/1", "transport": "Pure JSON function calls; no model provider required.",
            "tools": [
                spec("inspect", "Read the exact shared context, usable anchors, and current/stale self-declared acknowledgements.", ["bundle"], bundle),
                spec("note", "Add a revision-anchored question, claim, counterexample, proposal, or explanation; this grants no proof status.", ["bundle", "speaker", "text"], {**bundle, "speaker": txt, "text": txt, "kind": {"enum": list(NOTE_KINDS)}, "anchor": txt, "reply_to": {"type": ["string", "null"]}}),
                spec("acknowledge", "Record a self-declared agreement to use this exact context, not authenticated identity or proven understanding.", ["bundle", "speaker"], {**bundle, "speaker": txt, "statement": txt}),
                spec("reason", "Apply a registered scoped operation and retain its request/result for replay.", ["bundle", "request"], {**bundle, "request": obj}),
                spec("replay", "Recompute every current-context reasoning record and detect changed or stale results.", ["bundle"], bundle),
                spec("compare", "Compare exact model, interpretation, assumptions, and observation; styles can differ.", ["left", "right"], {"left": obj, "right": obj}),
                spec("relation", "Check finite one-step equivalence, refinement or abstraction with explicit observation maps.", ["left", "right"], {"left": obj, "right": obj, "relation": {"enum": ["equivalence", "refinement", "abstraction"]}, "input_map": {"type": ["array", "null"]}, "left_observation": {"type": ["array", "null"]}, "right_observation": {"type": ["array", "null"]}, "exact": {"type": "boolean"}}),
                spec("lens_source", "Build the exact finite Mealy context from inert contract data using the fixed local finite library.", ["contract"], {"contract": obj}),
                spec("lens_discover", "List registered Lens candidates, not proofs or all possible abstractions.", ["source"], {"source": obj}),
                spec("lens_apply", "Construct a candidate with explicit preservation/loss. Imported data cannot load adapters.", ["source", "lens_id", "parameters"], {"source": obj, "lens_id": txt, "parameters": obj, "budget": {"type": "integer", "minimum": 0, "maximum": 1000000}}),
                spec("lens_check", "Regenerate and check a candidate against the exact source within a finite work budget.", ["source", "application", "budget"], {"source": obj, "application": obj, "budget": {"type": "integer", "minimum": 0, "maximum": 1000000}}),
                spec("lens_verify", "Replay a receipt; matching refuted/unsupported evidence is not a checked claim.", ["source", "application", "receipt"], {"source": obj, "application": obj, "receipt": obj}),
                spec("lens_restore", "Return the retained exact source; this does not invert a lossy view.", ["source", "application"], {"source": obj, "application": obj}),
            ], "rules": {"transfer-check": ["total-preserved", "same-total-as-reject-all", "full-equivalence-reject-all"],
                            "lambda-beta": {}, "fsm-transition": {"state": "state id", "event": "literal event"}, "petri-fire": {"transition": "transition id"}},
            "authority": "none"}
    story_tools = story_toolkit()
    result["tools"].extend(story_tools["tools"])
    result["tools"].extend(ontology_tools())
    try:
        from category_profile.tools import toolkit as category_toolkit
    except ImportError:
        pass  # Standalone notation authoring does not require the optional host.
    else:
        result["tools"].extend(category_toolkit()["tools"])
    result["human_operations"] = story_tools["human_operations"]
    return result


def dispatch_tool(name, arguments, *, category_host=None):
    if type(name) is str and name.startswith("zal_library_category_"):
        try:
            from category_profile.host import dispatch_tool as category_dispatch
        except ImportError:
            return {"status": "Unsupported", "authority": "none", "reason": "Optional category host is not installed."}
        return category_dispatch(name, arguments, context=category_host)
    if type(name) is str and name.startswith(("zal_sdd_", "zal_library_")):
        from .story import dispatch
        return dispatch(name, arguments)
    core.canonical(arguments)
    definitions = {entry["name"]: entry["input_schema"] for entry in toolkit()["tools"]}
    core.need(type(name) is str and name in definitions, "unknown ZAL discussion tool")
    schema = definitions[name]
    core.keys(arguments, set(schema["required"]), set(schema["properties"]) - set(schema["required"]), "tool arguments")
    if name.startswith("zal_ontology_"):
        from .ontology_tools import dispatch
        return dispatch(name, arguments)
    if name.startswith("zal_lens_"):
        from .lenses import dispatch
        return dispatch({"action": name.removeprefix("zal_lens_"), **arguments})
    if name == "zal_inspect":
        return inspect_discussion(arguments["bundle"])
    if name == "zal_note":
        return add_note(**arguments)
    if name == "zal_acknowledge":
        return acknowledge(**arguments)
    if name == "zal_reason":
        return add_reasoning(arguments["bundle"], arguments["request"])
    if name == "zal_replay":
        return replay_discussion(arguments["bundle"])
    if name == "zal_compare":
        return compare_discussions(arguments["left"], arguments["right"])
    from .reasoning import check_relation
    return check_relation(**arguments)
