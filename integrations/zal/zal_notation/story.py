"""Story Driven Development: replayable proposals and local agreement.

The event history is authoritative for this data format, not for human identity.
Every operation is pure; no model-provider call, file access or external effect.
"""
from __future__ import annotations

from copy import deepcopy

from .abstractions import inspect_entry, require_formal, text, validate_entry
from .core import canonical, digest, keys, name, need

MAX_EVENTS = 128


def create(intent, participants, *, rule="unanimous", title="Story Driven Development"):
    need(type(participants) is list, "participants must be a list")
    session = {"schema": "zal.story/1", "title": title, "intent": intent,
               "participants": list(participants), "rule": rule, "events": []}
    replay(session)
    return session


def revision(session):
    return digest(session)


def _menu(menu, selected):
    keys(menu, {"id", "base", "scene", "question", "options"}, {"operation", "source_text"}, where="story menu")
    operation = menu.get("operation", "requirement-choice")
    need(type(operation) is str and operation in ("requirement-choice", "generative-fold"), "unknown proposal operation")
    if operation == "generative-fold":
        need("source_text" in menu, "generative folding must retain its original expression")
    if "source_text" in menu:
        text(menu["source_text"], "original expression")
    name(menu["id"], "menu id")
    need(menu["base"] == (digest(selected) if selected is not None else None), "menu base differs from the selected abstraction")
    text(menu["scene"], "scene")
    text(menu["question"], "question")
    options = menu["options"]
    need(type(options) is list and 2 <= len(options) <= 8, "a menu needs 2..8 options; propose a replacement to add another")
    identifiers = set()
    for option in options:
        keys(option, {"id", "label", "entry"}, where="story option")
        name(option["id"], "option id")
        need(option["id"] not in identifiers, "duplicate option id")
        identifiers.add(option["id"])
        text(option["label"], "option label", 300)
        validate_entry(option["entry"])


def _winner(ballots, participants, rule):
    threshold = len(participants) if rule == "unanimous" else len(participants) // 2 + 1
    for option in sorted(set(ballots.values())):
        if sum(vote == option for vote in ballots.values()) >= threshold:
            return option
    return None


def replay(session):
    canonical(session)
    keys(session, {"schema", "title", "intent", "participants", "rule", "events"}, where="story session")
    need(session["schema"] == "zal.story/1", "unsupported story schema")
    text(session["title"], "session title", 200)
    text(session["intent"], "intent")
    participants = session["participants"]
    need(type(participants) is list and 1 <= len(participants) <= 32, "session needs 1..32 participants")
    for participant in participants:
        name(participant, "participant")
    need(len(set(participants)) == len(participants), "participant labels must be unique")
    need(type(session["rule"]) is str and session["rule"] in ("unanimous", "majority"), "unknown agreement rule")
    events = session["events"]
    need(type(events) is list and len(events) <= MAX_EVENTS, "story event budget exceeded")
    prefix = {**session, "events": []}
    selected, pending = None, None
    ballots, decisions, withdrawals = {}, [], []
    menu_ids = set()
    for index, event in enumerate(events):
        need(type(event) is dict, "event must be an object")
        need(event.get("previous") == digest(prefix), "event predecessor revision mismatch")
        kind = event.get("kind")
        if kind == "propose":
            keys(event, {"kind", "previous", "menu"}, where="proposal")
            need(pending is None, "withdraw the pending menu before proposing a replacement")
            _menu(event["menu"], selected)
            need(event["menu"]["id"] not in menu_ids, "menu id has already been used")
            menu_ids.add(event["menu"]["id"])
            pending, ballots = event["menu"], {}
        elif kind in ("vote", "withdraw"):
            required = {"kind", "previous", "menu_digest"}
            keys(event, required | ({"participant", "option_digest"} if kind == "vote" else {"reason"}), where=kind)
            need(pending is not None and event["menu_digest"] == digest(pending), "no matching pending menu")
            if kind == "withdraw":
                text(event["reason"], "withdrawal reason")
                withdrawals.append({"menu_digest": digest(pending), "ballots": dict(ballots), "reason": event["reason"]})
                pending, ballots = None, {}
            else:
                participant = event["participant"]
                need(type(participant) is str and participant in participants, "participant is not in this session")
                options = {digest(option): option for option in pending["options"]}
                need(type(event["option_digest"]) is str and event["option_digest"] in options, "unknown or changed option")
                need(ballots.get(participant) != event["option_digest"], "identical ballot is already recorded")
                ballots[participant] = event["option_digest"]
                winner = _winner(ballots, participants, session["rule"])
                if winner is not None:
                    selected = options[winner]["entry"]
                    decisions.append({"event_index": index, "menu_digest": digest(pending), "option_digest": winner,
                                      "entry_digest": digest(selected), "ballots": dict(ballots), "rule": session["rule"]})
                    pending, ballots = None, {}
        else:
            need(False, "unknown story event")
        prefix["events"].append(event)
    return deepcopy({"schema": "zal.story-inspection/1", "revision": digest(session),
        "selected": selected, "selected_status": inspect_entry(selected) if selected else None,
        "pending": pending, "ballots": ballots, "decisions": decisions, "withdrawals": withdrawals,
        "status": "awaiting-choices" if pending else "selected" if selected else "exploring",
        "identity": "self-declared local participants; no authentication", "authority": "none"})


def _append(session, expected_revision, event):
    replay(session)
    need(expected_revision == revision(session), "stale story revision")
    result = deepcopy(session)
    result["events"].append({**deepcopy(event), "previous": expected_revision})
    replay(result)
    return result


def propose(session, expected_revision, menu):
    return _append(session, expected_revision, {"kind": "propose", "menu": menu})


def vote(session, expected_revision, menu_digest, participant, option_digest):
    """Human-facing operation; deliberately absent from the LLM proposal toolkit."""
    return _append(session, expected_revision, {"kind": "vote", "menu_digest": menu_digest,
                    "participant": participant, "option_digest": option_digest})


def withdraw(session, expected_revision, menu_digest, reason):
    return _append(session, expected_revision, {"kind": "withdraw", "menu_digest": menu_digest, "reason": reason})


def requirement_diff(before, after):
    validate_entry(after)
    if before is not None:
        validate_entry(before)
    previous = {row["id"]: row for row in before["requirements"]} if before else {}
    following = {row["id"]: row for row in after["requirements"]}
    return {"added": [deepcopy(following[k]) for k in sorted(following.keys() - previous.keys())],
            "removed": [deepcopy(previous[k]) for k in sorted(previous.keys() - following.keys())],
            "changed": [{"before": deepcopy(previous[k]), "after": deepcopy(following[k])}
                        for k in sorted(previous.keys() & following.keys()) if previous[k] != following[k]],
            "context_changed": (before["discussion"]["context_digest"] if before and before["discussion"] else None) !=
                               (after["discussion"]["context_digest"] if after["discussion"] else None)}


def export_contract(session):
    state = replay(session)
    need(state["pending"] is None, "resolve or withdraw the pending menu before exporting")
    need(state["selected"] is not None, "no selected abstraction to export")
    return {"schema": "zal.story-contract/1", "session": deepcopy(session),
            "selected": deepcopy(state["selected"]), "entry_digest": digest(state["selected"]),
            "decisions": deepcopy(state["decisions"]), "formalization": inspect_entry(state["selected"]),
            "agreement": "local-policy-satisfied; self-declared participants", "authority": "none"}


def import_contract(packet):
    encoded = canonical(packet)
    keys(packet, {"schema", "session", "selected", "entry_digest", "decisions", "formalization", "agreement", "authority"}, where="story contract")
    need(encoded == canonical(export_contract(packet["session"])), "export differs from replayed story decisions")
    return deepcopy(packet["session"])


def execute(entry, request):
    """Scenario execution has no access to story ballot or requirement mutation."""
    from .sharing import run_request
    return run_request(require_formal(entry), request)


def toolkit():
    obj, txt = {"type": "object"}, {"type": "string"}
    definitions = [
        ("zal_sdd_create", "Start a story from an intent and explicit participants.", ["intent", "participants"],
         {"intent": txt, "participants": {"type": "array", "items": txt}, "rule": {"enum": ["unanimous", "majority"]}, "title": txt}),
        ("zal_sdd_inspect", "Replay a story's exact decisions, drafts and pending menu.", ["session"], {"session": obj}),
        ("zal_sdd_propose", "Propose a menu of complete snapshots, including partial meanings with open questions. Does not vote.",
         ["session", "expected_revision", "menu"], {"session": obj, "expected_revision": txt, "menu": obj}),
        ("zal_sdd_withdraw", "Withdraw an unanswered menu; retain its history and prior ballots.",
         ["session", "expected_revision", "menu_digest", "reason"], {"session": obj, "expected_revision": txt, "menu_digest": txt, "reason": txt}),
        ("zal_library_register", "Add an explicitly defined abstraction to a supplied local collection.", ["library", "entry"], {"library": obj, "entry": obj}),
        ("zal_library_search", "Find abstractions by declared labels and examples; lexical retrieval only.", ["library", "query"], {"library": obj, "query": txt}),
        ("zal_library_lenses", "Enumerate registered Lens candidates on one exact entry; labels and hints do not certify applicability.",
         ["library", "entry_digest"], {"library": obj, "entry_digest": txt, "registry": obj}),
        ("zal_library_register_lens", "Add an inert custom Lens definition to a supplied registry; no code or proof status is installed.",
         ["registry", "proposed"], {"registry": obj, "proposed": obj}),
        ("zal_library_lens_propose", "Retain an authored source/target correspondence, including drafts and analogies; no applicability proof or adoption.",
         ["library", "entry_digest", "lens_id", "target_entry", "correspondence"],
         {"library": obj, "entry_digest": txt, "lens_id": txt, "target_entry": obj, "correspondence": obj, "registry": obj}),
        ("zal_library_lens_apply", "Construct a supported structural/finite view bound to the exact library entry.",
         ["library", "entry_digest", "lens_id"], {"library": obj, "entry_digest": txt, "lens_id": txt, "parameters": obj, "registry": obj, "budget": {"type": "integer", "minimum": 0, "maximum": 1000000}}),
        ("zal_library_lens_check", "Check the bound view; structural evidence does not prove the entry's meaning or laws.",
         ["library", "entry_digest", "application"], {"library": obj, "entry_digest": txt, "application": obj, "budget": {"type": "integer", "minimum": 0, "maximum": 1000000}}),
        ("zal_library_lens_verify", "Replay entry-bound evidence; a matching refuted receipt remains refuted.",
         ["library", "entry_digest", "application", "receipt"], {"library": obj, "entry_digest": txt, "application": obj, "receipt": obj}),
        ("zal_library_lens_restore", "Return the retained entry; do not infer an inverse of its view map.",
         ["library", "entry_digest", "application"], {"library": obj, "entry_digest": txt, "application": obj}),
        ("zal_library_compose", "Construct closed lambda composition while retaining constituent identities.",
         ["first", "second"], {"first": obj, "second": obj, "identifier": txt}),
        ("zal_library_normalize", "Create a separate alpha-normalized closed-lambda artifact, retaining the original definition and evidence.",
         ["entry"], {"entry": obj}),
    ]
    return {"schema": "zal.story-toolkit/1", "tools": [
        {"name": n, "description": d, "input_schema": {"type": "object", "required": r,
          "properties": p, "additionalProperties": False}} for n, d, r, p in definitions],
        "human_operations": ["vote"], "authority": "none"}


def dispatch(tool, arguments):
    from .abstractions import compose_lambda, enumerate_lenses, register, search
    from .normalizer import alpha_normalize
    canonical(arguments)
    specs = {row["name"]: row["input_schema"] for row in toolkit()["tools"]}
    need(type(tool) is str and tool in specs, "unknown story proposal tool")
    spec = specs[tool]
    keys(arguments, set(spec["required"]), set(spec["properties"]) - set(spec["required"]), "tool arguments")
    if tool == "zal_library_register_lens":
        from .lens_library import register as register_lens
        return register_lens(**arguments)
    if tool.startswith("zal_library_lens_"):
        from . import lens_library
        lens_operations = {"zal_library_lens_propose": lens_library.propose,
                           "zal_library_lens_apply": lens_library.apply,
                           "zal_library_lens_check": lens_library.check,
                           "zal_library_lens_verify": lens_library.verify_receipt,
                           "zal_library_lens_restore": lens_library.restore}
        return lens_operations[tool](**arguments)
    operations = {"zal_sdd_create": create, "zal_sdd_inspect": lambda session: replay(session),
                  "zal_sdd_propose": propose, "zal_sdd_withdraw": withdraw,
                  "zal_library_register": register, "zal_library_search": search,
                  "zal_library_lenses": enumerate_lenses,
                  "zal_library_compose": compose_lambda, "zal_library_normalize": alpha_normalize}
    return operations[tool](**arguments)
