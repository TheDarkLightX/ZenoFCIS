"""Bind a context-free ZAL model to the existing finite knowledge-game checker.

The observation profile is an explicit environment/observer contract, not an
inferred deployment model. No production effects or authoritative decisions.
"""
from dataclasses import dataclass
import hashlib
from pathlib import Path
import sys

from behavior import Model, Refusal, canonical

TRACE_TOOLS = Path(__file__).resolve().parents[2] / "tools" / "trace_game"
if str(TRACE_TOOLS) not in sys.path:
    sys.path.insert(0, str(TRACE_TOOLS))
from arena_spec import parse as parse_arena
from arena_check import check as check_certificate
from knowledge import Budget, Limit, solve

SCHEMA = "zal/observation-game/1"
MAX_WORK = 1_000_000


def need(condition, message):
    if not condition:
        raise Refusal(message)


def names(value, domain, label, nonempty=False):
    need(type(value) is list and all(type(x) is str for x in value), label)
    need(len(value) == len(set(value)) and set(value) <= set(domain), label)
    need(bool(value) or not nonempty, label)
    return tuple(sorted(value))


@dataclass(frozen=True)
class StateView:
    state: str
    owner: int
    observation: str
    events: tuple


@dataclass(frozen=True)
class Profile:
    model_revision: str
    observation: str
    bad_states: tuple
    states: tuple
    costs: tuple
    max_cost: int | None
    service_states: tuple
    response_bound: int | None

    def wire(self):
        return dict(schema=SCHEMA, model_revision=self.model_revision,
                    observation=self.observation, bad_states=list(self.bad_states),
                    states=[dict(state=s.state, owner=s.owner, observation=s.observation,
                                 events=list(s.events)) for s in self.states],
                    costs=[dict(event=e, units=u) for e, u in self.costs],
                    max_cost=self.max_cost, service_states=list(self.service_states),
                    response_bound=self.response_bound)

    @property
    def revision(self):
        return hashlib.sha256(canonical(self.wire()).encode()).hexdigest()


def parse_profile(value, model):
    need(type(model) is Model and not model.facts and not model.obligations,
         "observation games require context-free zal/finite-fsm/1 without unresolved obligations")
    fields = {"schema", "model_revision", "observation", "bad_states", "states", "costs",
              "max_cost", "service_states", "response_bound"}
    need(type(value) is dict and set(value) == fields, "observation profile fields")
    need(value["schema"] == SCHEMA, "observation profile schema")
    need(value["model_revision"] == model.revision, "observation model revision mismatch")
    need(value["observation"] in ("partial", "perfect"), "observation mode")
    bad = names(value["bad_states"], model.states, "bad states")
    service = names(value["service_states"], model.states, "service states")
    bound, cost_bound = value["response_bound"], value["max_cost"]
    need(bound is None or type(bound) is int and 0 <= bound <= 64, "response bound 0..64 or null")
    need((bound is None and not service) or (bound is not None and bool(service)),
         "service states require an explicit response bound")
    need(cost_bound is None or type(cost_bound) is int and 0 <= cost_bound <= 255,
         "abstract cost bound 0..255 or null")
    rows = value["states"]
    need(type(rows) is list and len(rows) == len(model.states), "complete state views required")
    views = []
    for row in rows:
        need(type(row) is dict and set(row) == {"state", "owner", "observation", "events"},
             "state view fields")
        need(type(row["state"]) is str and row["state"] in model.states, "state view name")
        need(type(row["owner"]) is int and row["owner"] in (0, 1), "state owner")
        obs = row["observation"]
        need(type(obs) is str and 0 < len(obs) <= 128 and all(32 <= ord(c) < 127 for c in obs),
             "printable observation label of 1..128 characters")
        events = names(row["events"], model.events, "nonempty permitted event alphabet", True)
        views.append(StateView(row["state"], row["owner"], obs, events))
    need({s.state for s in views} == set(model.states), "duplicate or missing state view")
    if value["observation"] == "partial":
        signatures = {}
        for row in views:
            signature = row.owner, row.events if row.owner == 0 else ()
            need(signatures.setdefault(row.observation, signature) == signature,
                 "observation must reveal owner and enabled controller actions")
    costs = value["costs"]
    need(type(costs) is list and len(costs) == len(model.events), "complete event cost table required")
    charged = []
    for row in costs:
        need(type(row) is dict and set(row) == {"event", "units"}, "cost row fields")
        need(type(row["event"]) is str and row["event"] in model.events, "cost event")
        need(type(row["units"]) is int and 0 <= row["units"] <= 255, "cost units 0..255")
        charged.append((row["event"], row["units"]))
    need({e for e, _ in charged} == set(model.events), "duplicate or missing event cost")
    return Profile(model.revision, value["observation"], bad,
                   tuple(sorted(views, key=lambda s: s.state)), tuple(sorted(charged)),
                   cost_bound, service, bound)


def _limits(max_states, max_edges, max_work):
    for value, maximum in ((max_states, 10_000), (max_edges, 100_000), (max_work, MAX_WORK)):
        need(type(value) is int and 0 <= value <= maximum, "invalid observation checking limit")


def compile_game(model, profile, *, max_states=10_000, max_edges=100_000, max_work=MAX_WORK):
    """Build every reachable monitored node; limits raise Limit, never truncate.

Environment events are private physical choices. Controller event responses
expose class/reason before the next projected world. Rule identifiers are not
observations. Cost and response monitors add no secret controller inputs.
"""
    _limits(max_states, max_edges, max_work)
    profile = parse_profile(profile.wire(), model)
    views = {v.state: v for v in profile.states}
    costs = dict(profile.costs)
    work = 0

    def charge(n=1):
        nonlocal work
        work += n
        if work > max_work:
            raise Limit("observation compilation work bound")

    # Check the complete source relation, including excluded protocol events.
    decisions = {}
    for state in model.states:
        for event in model.events:
            charge()
            decisions[state, event] = model.decide(state, event, {})
    invariant_bad = {s for s in model.states
                     if any(not f.value(s, {}) for _, f in model.invariants)}
    keys, index, nodes, metadata, transitions = [], {}, [], [], []
    marks = []
    edge_count = 0

    def intern(key):
        charge()
        if key not in index:
            if len(keys) >= max_states:
                raise Limit("observation compilation state bound")
            index[key] = len(keys)
            keys.append(key)
        return index[key]

    def advance(state, spent, elapsed, served, event, decision):
        return _advance(profile, costs, state, spent, elapsed, served, event, decision)

    initially_served = model.initial in profile.service_states
    root = intern(("world", model.initial, 0, 0, initially_served))
    for key in keys:
        charge()
        i = index[key]
        phase = key[0]
        if phase == "cost_exceeded":
            owner, obs, row = 1, {"phase": "cost_exceeded"}, [("hold", i)]
            is_bad = True
            info = dict(phase=phase)
        elif phase == "response":
            _, state, spent, elapsed, served, event = key
            decision = decisions[state, event]
            target = advance(state, spent, elapsed, served, event, decision)
            owner = 1
            obs = dict(phase="response", action=event, outcome=decision["class"], reason=decision["reason"])
            row = [("continue", intern(target))]
            # A failed service/cost obligation is marked on the following world.
            is_bad = False
            info = dict(phase=phase, state=state, event=event, spent=spent, elapsed=elapsed,
                        served=served, outcome=decision["class"], reason=decision["reason"])
        else:
            _, state, spent, elapsed, served = key
            view = views[state]
            owner, obs = view.owner, dict(phase="world", view=view.observation)
            row = []
            is_bad = (state in profile.bad_states or state in invariant_bad or
                      profile.response_bound is not None and not served and elapsed >= profile.response_bound)
            info = dict(phase=phase, state=state, spent=spent, elapsed=elapsed, served=served)
            for event in view.events:
                charge()
                decision = decisions[state, event]
                target_key = (("response", state, spent, elapsed, served, event) if owner == 0
                              else advance(state, spent, elapsed, served, event, decision))
                target = intern(target_key)
                row.append((event, target))
                transitions.append(dict(source=i, event=event, target=target,
                                        outcome=dict(decision), cost_units=costs[event]))
        if profile.observation == "perfect":
            obs["concrete_node"] = i
        edge_count += len(row)
        if edge_count > max_edges:
            raise Limit("observation compilation edge bound")
        nodes.append(dict(owner=owner, observation=canonical(obs), edges=[list(e) for e in row]))
        metadata.append(info)
        if is_bad:
            marks.append(i)

    arena = parse_arena(dict(schema="zeno-fcis/arena/1", observation=profile.observation,
                             roots=[root], objective=dict(kind="safety", states=marks), nodes=nodes))
    return dict(arena=arena, nodes=metadata, transitions=transitions, work=work,
                excluded_events=[dict(state=v.state, events=sorted(set(model.events)-set(v.events)))
                                 for v in profile.states])

def _advance(profile, costs, state, spent, elapsed, served, event, decision):
    next_spent = spent + costs[event] if profile.max_cost is not None else 0
    if profile.max_cost is not None and next_spent > profile.max_cost:
        return ("cost_exceeded",)
    next_served = served or decision["state"] in profile.service_states
    next_elapsed = (min(elapsed + 1, profile.response_bound) if
                    profile.response_bound is not None and not served else elapsed)
    return ("world", decision["state"], next_spent, next_elapsed, next_served)


def _header(model, profile, limits):
    return dict(schema="zal/observation-game-result/1", model_revision=model.revision,
                  profile_revision=profile.revision, authority="none", limits=limits,
                  scope=dict(semantics="context-free finite ZAL model; declared environment only",
                             objective="sure safety", observation=profile.observation,
                             responses="controller action, outcome class and reason are observed",
                             environment="physical branch labels are hidden; target views are observed",
                             cost="declared abstract event units; no runtime or time bound",
                             completion="not claimed" if profile.response_bound is None else
                             "first service visit within the declared number of original model transitions"))


def _adequacy(profile, arena):
    return dict(service_required=profile.response_bound is not None,
                controller_nodes=sum(n["owner"] == 0 for n in arena["nodes"]),
                bad_nodes=len(arena["objective"]["states"]))


def check(model, profile, *, max_states=10_000, max_edges=100_000, max_work=MAX_WORK, seconds=30):
    """Independent backend certificate replay; no certificate for incomplete work."""
    _limits(max_states, max_edges, max_work)
    profile = parse_profile(profile.wire(), model)
    limits = dict(max_states=max_states, max_edges=max_edges, max_work=max_work)
    result = _header(model, profile, limits)
    try:
        # Compile and solve share the work allowance; the checker has its own
        # fixed independent bounds as documented by arena_check.
        compiled = compile_game(model, profile, max_states=max_states, max_edges=max_edges, max_work=max_work)
        packet = solve(compiled["arena"], Budget(max_states, max_edges, max_work-compiled["work"], seconds))
        if packet.get("status") == "UNKNOWN":
            result.update(status="UNKNOWN", stop=packet["stop"], work=compiled["work"]+packet["work"])
            return result
        verdict = check_certificate(packet, compiled["arena"])
        result.update(status="CHECKED" if not verdict["losing_concrete_roots"] else "UNREALIZABLE",
                      packet=packet, verdict=verdict,
                      compilation={**compiled, "arena": compiled["arena"].wire()},
                      adequacy=_adequacy(profile, packet["game"]))
        return result
    except (ValueError, RecursionError) as error:
        result.update(status="UNKNOWN", stop=str(error) or "recursion bound")
        return result


def verify(model, profile, result):
    """Rebuild within the declared search allowance and independently check the certificate.

The bounded search is repeated to validate the admitted work allowance. The
certificate checker, not that repeated search, establishes the game verdict.
Neither work units nor the solver deadline measure native runtime resources.
"""
    profile = parse_profile(profile.wire(), model)
    need(type(result) is dict and result.get("schema") == "zal/observation-game-result/1",
         "observation result schema")
    need(result.get("model_revision") == model.revision and result.get("profile_revision") == profile.revision,
         "stale observation subject")
    need(result.get("status") in ("CHECKED", "UNREALIZABLE"), "UNKNOWN is not a certificate")
    limits = result.get("limits")
    need(type(limits) is dict and set(limits) == {"max_states", "max_edges", "max_work"},
         "observation result limits")
    _limits(**limits)
    header = _header(model, profile, limits)
    need(set(result) == set(header) | {"status", "packet", "verdict", "compilation", "adequacy"}
         and all(canonical(result[k]) == canonical(v) for k, v in header.items()), "observation result scope mismatch")
    compiled = compile_game(model, profile, **limits)
    need(canonical(result.get("compilation")) == canonical({**compiled, "arena": compiled["arena"].wire()}),
         "observation adapter mismatch")
    budget = Budget(limits["max_states"], limits["max_edges"], limits["max_work"]-compiled["work"])
    replay = solve(compiled["arena"], budget)
    need(replay.get("status") != "UNKNOWN", "observation search allowance cannot reproduce completion")
    verdict = check_certificate(result["packet"], compiled["arena"])
    expected = "CHECKED" if not verdict["losing_concrete_roots"] else "UNREALIZABLE"
    need(result["status"] == expected and canonical(result.get("verdict")) == canonical(verdict), "observation verdict mismatch")
    need(canonical(result["adequacy"]) == canonical(_adequacy(profile, compiled["arena"].wire())), "observation adequacy mismatch")
    return verdict
