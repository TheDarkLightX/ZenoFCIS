"""Compile a finite right/up search film against ZAL's state-machine semantics.

The four operations use exact event histories and native state IDs. All scratch
tables belong to one compilation. The public boundary consumes and returns
immutable JSON text, so callers cannot mutate a checked plan through an alias.
"""
from collections import defaultdict
from dataclasses import dataclass
from itertools import groupby
from operator import attrgetter
import re

from . import builtin, core, semantics


@dataclass(frozen=True)
class Path:
    events: tuple[str, ...]
    state: str

    @property
    def turns(self):
        return sum(a != b for a, b in zip(self.events, self.events[1:]))


def branch_on_choice(path, transitions):
    return tuple(Path(path.events + (event,), state)
                 for event, state in transitions.get(path.state, ()))


def trace_path(events, transitions, start):
    path = Path((), start)
    trace = [path]
    for event in events:
        following = dict(transitions.get(path.state, ()))
        core.need(event in following, "trace leaves the admitted transition table")
        path = Path(path.events + (event,), following[event])
        trace.append(path)
    return tuple(trace)


def merge_equivalent_states(paths, observation=attrgetter("state")):
    """Group histories by a supplied observable key, retaining every witness."""
    return {state: tuple(members) for state, members in groupby(
        sorted(paths, key=observation), key=observation)}


def select_by_cost(paths, cost=attrgetter("turns")):
    """Rank feasible histories only and retain every minimum-cost witness."""
    scored = tuple((cost(path), path) for path in paths)
    minimum = min((value for value, _ in scored), default=None)
    return tuple(path for value, path in scored if value == minimum)


def grid(goal, limit):
    core.keys(goal, {"right", "up"}, where="open grid goal")
    right, up = goal["right"], goal["up"]
    core.need(all(type(v) is int and v >= 1 for v in (right, up))
              and right + up <= limit, f"positive move counts must sum to at most {limit}")
    n = right + up
    nodes, edges, coordinates = [], [], {}
    for depth in range(n + 1):
        for y in range(depth + 1):
            x = depth - y
            state = f"{x},{y}"
            coordinates[state] = [x, y]
            tag = "start" if depth == 0 else "accepting" if (x, y) == (right, up) else "state"
            nodes.append({"id": state, "tag": tag, "fields": {"name": state}})
            if depth < n:
                for event, target in (("R", f"{x+1},{y}"), ("U", f"{x},{y+1}")):
                    edges.append({"id": f"{state}/{event}", "tag": "transition",
                                  "source": [state, "out"], "target": [target, "in"],
                                  "fields": {"event": event, "outcome": "move"}})
    grammar = builtin.fsm_grammar()
    model = builtin.model(grammar, {"nodes": nodes, "edges": edges})
    start, transitions = transition_table(model)
    return model, coordinates, transitions, start


def transition_table(model):
    """Read the declared edges of a deterministic, possibly partial ZAL FSM."""
    table = semantics.fsm_check(builtin.fsm_grammar(), model)
    core.need(table["initial_valid"] and table["deterministic"], "film operations require one start and deterministic transitions")
    transitions = defaultdict(list)
    for edge in model["body"]["edges"]:
        transitions[edge["source"][0]].append((edge["fields"]["event"], edge["target"][0]))
    start = table["initial_states"][0]
    return start, {state: tuple(edges) for state, edges in transitions.items()}


def walk_view(goal):
    model, coordinates, transitions, start = grid(goal, 6)
    n = goal["right"] + goal["up"]
    layers = [(Path((), start),)]
    for _ in range(n):
        layers.append(tuple(child for path in layers[-1] for child in branch_on_choice(path, transitions)))
    leaves = layers[-1]
    fibres = merge_equivalent_states(leaves)
    target = next(node["id"] for node in model["body"]["nodes"] if node["tag"] == "accepting")
    feasible = fibres[target]
    winners = select_by_cost(feasible)
    return {
        "model": model, "modelDigest": core.digest(model), "goal": goal, "n": n,
        "layers": [["".join(p.events) for p in layer] for layer in layers],
        "states": {"".join(p.events): p.state for layer in layers for p in layer},
        "coordinates": coordinates, "routes": ["".join(p.events) for p in leaves],
        "members": {state: ["".join(p.events) for p in paths] for state, paths in fibres.items()},
        "groups": [len(fibres[f"{n-y},{y}"]) for y in range(n + 1)],
        "solutions": ["".join(p.events) for p in feasible], "costs": {"".join(p.events): p.turns for p in leaves},
        "minTurns": winners[0].turns, "optima": ["".join(p.events) for p in winners],
        "fewestTurns": "".join(winners[0].events), "mostTurns": "".join(max(feasible, key=attrgetter("turns")).events),
    }


def transfer_view(goal):
    model, coordinates, transitions, start = grid(goal, 10)
    events = "R" * goal["right"] + "U" * goal["up"]
    trace = trace_path(events, transitions, start)
    replay = semantics.fsm_trace(builtin.fsm_grammar(), model, list(events))
    core.need(replay["status"] == "completed" and replay["final_state"] == trace[-1].state,
              "constructed route failed native replay")
    return {"goal": goal, "modelDigest": core.digest(model), "events": events,
            "points": [coordinates[p.state] for p in trace], "possibilities": 2 ** len(events)}


# Canonical identities of the entire essay document and its registered ZAL replay.
# Never accept an edited verdict merely because its counters still look plausible.
ESSAY_RECORD = "cb6ba3032892246a83b0fe3c213198d96c3bb980199fef3f3a5e4a3ed9d8dc8c"
ESSAY_REPLAY = "9c615dd96d0ea6d0d74539579d04a80c5f45f6e463575f52252ddc61f6ffa583"


def checked_intervals(document, replay):
    core.need(core.digest(document) == ESSAY_RECORD and core.digest(replay) == ESSAY_REPLAY,
              "unsupported or changed byte synthesis record or registered replay")
    record, = document["records"]
    definition = document["definition"]
    view = interval_view({"lower": definition["lower"], "upper": definition["upper"]})
    recorded = {(r["candidate"]["lower"], r["candidate"]["upper"]): r["result"]
                for r in record["result"]["candidates"]}
    core.need(len(view["candidates"]) == len(recorded), "candidate family differs from replay")
    for candidate in view["candidates"]:
        lower, upper, misses = candidate["l"], candidate["h"], candidate["misses"]
        result = recorded[lower, upper]
        witness = None
        if misses:
            byte = misses[0]
            witness = {"byte": byte, "actual": lower <= byte < upper,
                       "expected": view["lo"] <= byte < view["hi"]}
        core.need(result["checked_cases"] == 256 and result["mismatch_count"] == len(misses)
                  and result["counterexample"] == witness, "finite interval replay disagrees")
        candidate["id"] = result["candidate_digest"]
    return {**view, "record": record["id"], "documentDigest": ESSAY_RECORD, "replayDigest": ESSAY_REPLAY}


def interval_view(interval):
    core.keys(interval, {"lower", "upper"}, where="byte interval")
    low, high = interval["lower"], interval["upper"]
    core.need(type(low) is int and type(high) is int and 0 <= low < high <= 256,
              "byte interval must have integer bounds 0 <= lower < upper <= 256")
    def neighbours(bound):
        return sorted({max(0, min(256, bound + offset)) for offset in (-1, 0, 1)})
    lowers, uppers = neighbours(low), neighbours(high)
    candidates = []
    for lower in lowers:
        for upper in uppers:
            if lower >= upper:
                continue
            misses = [b for b in range(256) if (low <= b < high) != (lower <= b < upper)]
            identity = core.digest({"profile": "zal.film-interval/1", "lower": lower, "upper": upper})
            candidates.append({"id": identity, "l": lower, "h": upper, "misses": misses})
    return {"lo": low, "hi": high, "lowers": lowers, "uppers": uppers, "candidates": candidates,
            "profile": "zal.film-interval/1", "checkedInputs": 256}



VIEWS = {"trace", "branch", "budget", "merge", "select", "language", "programs", "reuse", "relation"}
PILOT_NAMES = {"walk": "trace", "tree": "branch"}


def prose(source, fields, limit):
    core.need(type(source) is str and source.strip() and len(source) <= limit
              and all(c.isprintable() for c in source), "invalid film text")
    def replace(match):
        key = match[1]
        core.need(key in fields, f"unknown film field {key}")
        return str(fields[key])
    result = re.sub(r"\{(\w+)\}", replace, source)
    core.need("{" not in result and "}" not in result and "-->" not in result, "invalid caption syntax")
    return result


def compile_film(source: str, evidence: str | None = None, replay: str | None = None) -> str:
    """Return one theme-independent, replayable plan with integer frame boundaries."""
    doc = core.loads(source)
    core.keys(doc, {"format", "title", "walk", "shots"}, {"essay", "transfer", "budget", "evidence", "replay", "interval"})
    pilot = doc["format"] == "essay-film/1"
    core.need(pilot or doc["format"] == "zal.search-film/1", "unsupported film format")
    raw = doc["shots"]
    core.need(type(raw) is list and 1 <= len(raw) <= 64, "film requires 1..64 shots")
    walk = walk_view(doc["walk"])
    budget = doc.get("budget", 1)
    core.need(type(budget) is int and 1 <= budget <= min(3, len(walk["routes"])), "budget must be 1..3 trials")
    fields = {"moves": walk["n"], "routes": len(walk["routes"]), "endpoints": len(walk["groups"]),
              "solutions": len(walk["solutions"]), "optima": len(walk["optima"]), "budget": budget,
              "first": walk["routes"].index(walk["solutions"][0]) + 1}
    successful_trials = sum(route in walk["solutions"] for route in walk["routes"][:budget])
    fields["trialHits"] = successful_trials
    fields["untested"] = len(walk["routes"]) - budget
    core.need("interval" not in doc or (evidence is None and replay is None and
              "evidence" not in doc and "replay" not in doc), "choose a native interval or a recorded synthesis")
    title = prose(doc["title"], fields, 120)
    shots, frame, transfer, byte_data = [], 0, None, None
    for row in raw:
        core.keys(row, {"view", "seconds", "title", "caption"}, {"path"}, where="film shot")
        view = row["view"]
        core.need(type(view) is str, "view must be a string")
        if pilot:
            view = PILOT_NAMES.get(view, view)
        core.need(view in VIEWS, "unknown film view")
        seconds = row["seconds"]
        core.need(type(seconds) in (float, int) and 2 <= seconds <= 120, "shot must last 2..120 seconds")
        end = frame + round(seconds * 30)
        core.need(end <= 30 * 600, "film exceeds ten minutes")
        shot = {"view": view, "startFrame": frame, "endFrame": end, "seconds": (end-frame)/30,
                "title": prose(row["title"], fields, 120), "caption": prose(row["caption"], fields, 220)}
        if "path" in row:
            core.need(view == "trace" and type(row["path"]) is str and row["path"] in walk["states"]
                      and bool(row["path"]), "expected a nonempty legal prefix for a trace shot")
            shot["path"] = row["path"]
        if view == "programs" and byte_data is None:
            if "interval" in doc:
                byte_data = interval_view(doc["interval"])
            else:
                core.need(evidence is not None and replay is not None, "programs view requires record and replay")
                byte_data = checked_intervals(core.loads(evidence), core.loads(replay))
        if view == "reuse" and transfer is None:
            core.need("transfer" in doc, "reuse requires a transfer goal")
            transfer = transfer_view(doc["transfer"])
        if pilot and view == "budget":
            core.need(not set(walk["routes"][:budget]).intersection(walk["solutions"]),
                      "the unsuccessful-budget view requires all displayed trials to miss")
        shots.append(shot)
        frame = end
    return core.canonical({"schema": "zal.search-film-plan/1", "sourceDigest": core.digest(doc),
                           "title": title, "fps": 30, "frames": frame, "shots": shots, "walk": walk,
                           "transfer": transfer, "budget": budget, "bytes": byte_data, "authority": "none"})
