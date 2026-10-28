"""An explicit clock abstraction and its supplied-tick transition semantics."""
from __future__ import annotations

from .builtin import generic_pack, model, term
from .core import keys as keys  # noqa: PLC0414 - preserve the research module attribute
from .core import need, validate_model

PROFILE = "zal/supplied-tick-clock/1"
STATES = ("running", "stopped")
EVENTS = ("tick", "pause", "resume", "reset")
ASSUMPTIONS = ["Ticks are supplied events; this model makes no physical time accuracy claim.",
               "One event is processed atomically; state and position are the only changing values."]
OBSERVATION = {"id": "whole-clock", "description": "Observe acceptance, reason, control state and position.",
               "keeps": ["accepted", "reason", "state", "position"], "omits": []}


def grammar():
    return {"schema": "zal.grammar/1", "id": "zal/supplied-tick-clock", "version": 1, "kind": "term", "root": "Clock",
            "constructors": {
                "Clock": {"result": "Clock", "fields": {"positions": "int", "boundary": "str"}, "children": ["Rule"] * 4},
                "Rule": {"result": "Rule", "fields": {"event": "str", "definition": "str"}, "children": []}}}


def definition(positions=60, boundary="wrap"):
    need(type(positions) is int and 2 <= positions <= 64, "clock positions must be 2..64")
    need(type(boundary) is str and boundary in ("wrap", "stop"), "unknown clock boundary rule")
    edge = "set position to 0 and remain running" if boundary == "wrap" else "keep position and become stopped"
    return term("Clock",
        term("Rule", event="tick", definition=f"When running: increment below {positions - 1}; at {positions - 1}, {edge}. When stopped: reject unchanged."),
        term("Rule", event="pause", definition="When running: become stopped and keep position. Otherwise reject unchanged."),
        term("Rule", event="resume", definition="When stopped: become running and keep position. Otherwise reject unchanged."),
        term("Rule", event="reset", definition="Become running at position 0 from either control state."),
        positions=positions, boundary=boundary)


def validate_context(context):
    need(context["grammar"] == grammar(), "clock profile requires its exact grammar")
    validate_model(context["grammar"], context["model"])
    body = context["model"]["body"]
    need(body["tag"] == "Clock", "clock root required")
    positions, boundary = body["fields"]["positions"], body["fields"]["boundary"]
    need(body == definition(positions, boundary), "clock expansion differs from its registered definition")
    need(context["semantics"]["profile"] == PROFILE and context["semantics"]["parameters"] == {}, "invalid clock profile")
    need(context["assumptions"] == ASSUMPTIONS and context["observation"] == OBSERVATION, "clock interpretation domain/observation differs")
    return positions, boundary


def entry(positions=60, boundary="wrap"):
    from .abstractions import make_entry
    from .sharing import make_context, make_discussion
    g = grammar()
    m = model(g, definition(positions, boundary))
    context = make_context(g, m, semantics={"profile": PROFILE, "parameters": {},
        "description": "The supplied-tick clock transition relation defined by this profile."},
        assumptions=ASSUMPTIONS, observation=OBSERVATION)
    narrative = (f"A clockkeeper watches a dial with {positions} positions. Each tick advances the running dial. "
                 + ("After the final position it returns to zero." if boundary == "wrap" else "At the final position the next tick stops the clock.")
                 + " The keeper can pause, resume, or reset the clock.")
    requirements = [
        {"id": "initial", "statement": "Initially the clock is running at position zero.", "anchors": ["root"]},
        {"id": "tick", "statement": m["body"]["children"][0]["fields"]["definition"], "anchors": ["root.children.0"]},
        *[{"id": child["fields"]["event"], "statement": child["fields"]["definition"], "anchors": [f"root.children.{i}"]}
          for i, child in enumerate(m["body"]["children"][1:], 1)]]
    return make_entry("clock_" + boundary, f"{positions}-position clock: {boundary}", story=narrative,
        purpose="Describe and implement a clock driven by explicit ticks.",
        discussion=make_discussion(context, generic_pack(g), title="The clockkeeper"),
        requirements=requirements, tags=["clock", "counter", "hardware", "software"],
        examples=[f"At position {positions - 1}, inspect the next tick.", "A paused clock refuses a tick without changing state."],
        laws=[{"id": "range", "statement": f"Position always belongs to 0..{positions - 1}.", "anchors": ["root"]},
              {"id": "rejection", "statement": "Rejected events preserve both position and control state.", "anchors": ["root"]}])


def step(context, state, position, event):
    positions, boundary = validate_context(context)
    need(type(state) is str and state in STATES and type(event) is str and event in EVENTS, "unknown clock state or event")
    need(type(position) is int and 0 <= position < positions, "position outside the declared clock domain")
    accepted, next_state, next_position = True, state, position
    if event == "reset":
        next_state, next_position = "running", 0
    elif event == "pause" and state == "running":
        next_state = "stopped"
    elif event == "resume" and state == "stopped":
        next_state = "running"
    elif event == "tick" and state == "running":
        if position + 1 < positions:
            next_position = position + 1
        elif boundary == "wrap":
            next_position = 0
        else:
            next_state = "stopped"
    else:
        accepted = False
    return {"accepted": accepted, "reason": "ok" if accepted else "not_enabled", "state": next_state, "position": next_position}


def native_source(context):
    """Emit ZAL profile 2; qualification is a separate native correspondence check."""
    positions, boundary = validate_context(context)
    end = "accept running, position = 0" if boundary == "wrap" else "accept stopped"
    return f"""zal 2;
machine story_clock;
states running, stopped;
events tick, pause, resume, reset;
context none;
var position in 0..={positions - 1} initially 0;
initial running;
step atomic; frame control_and_variables;
default reject not_enabled unchanged;
rule tick_forward: running + tick [position < {positions - 1}] -> accept running, position += 1;
rule tick_boundary: running + tick [position == {positions - 1}] -> {end};
rule pause_clock: running + pause [true] -> accept stopped;
rule resume_clock: stopped + resume [true] -> accept running;
rule reset_running: running + reset [true] -> accept running, position = 0;
rule reset_stopped: stopped + reset [true] -> accept running, position = 0;
invariant bounded_position: position >= 0 & position < {positions};
"""


def story_demo(participants=None, rule="unanimous"):
    from .story import create, propose, revision
    session = create("We want a clock. Let us decide what happens at the edge of its dial.",
                     participants if participants is not None else ["Participant One", "Participant Two"],
                     rule=rule, title="Story Driven Development: the clockkeeper")
    menu = {"id": "clock_boundary", "base": None,
            "scene": "The clockkeeper sees the hand at position 59. Another tick arrives.",
            "question": "Should the clock return to zero or stop at the end of its dial?",
            "options": [{"id": mode, "label": "Return to zero" if mode == "wrap" else "Stop at the boundary", "entry": entry(60, mode)}
                        for mode in ("wrap", "stop")]}
    return propose(session, revision(session), menu)
