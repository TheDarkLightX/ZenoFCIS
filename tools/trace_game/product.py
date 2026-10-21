"""Finite v1 products. Search is untrusted; certificate_check.py decides evidence."""
from dataclasses import dataclass
import time
from check import core, composition, starts, moves, advance

GUARDS = ('true', 'actor == 0', 'false')
FULL = dict(accounts=3, signers=1, oracle_choices=3, approval_ages=3, arbitrary_order=True)


@dataclass(frozen=True)
class State:
    native: tuple
    reference: tuple
    coalition: int
    entitlement: int = 0
    remaining: int = 0
    stage: int = 0
    bad: bool = False

    def wire(self):
        return [list(self.native), list(self.reference), self.coalition,
                self.entitlement, self.remaining, self.stage, self.bad]


@dataclass(frozen=True)
class Graph:
    states: tuple
    roots: tuple
    edges: tuple  # per state: (event, response, target)
    status: str
    stop: str

    def wire(self):
        return dict(states=[s.wire() for s in self.states], roots=list(self.roots),
                    edges=[[list(e) for e in row] for row in self.edges],
                    status=self.status, stop=self.stop)


def description(model, guard='actor == 0', reactive=False, empty=False):
    return dict(model=model, guard=guard, reactive=reactive, empty=empty,
                powers=dict(FULL), unfair_delay=False, only_objective=None,
                disabled_objective=None, move_order='environment first, controller responds',
                observations=['actor', 'request'], strategy_grammar=list(GUARDS))


def roots(spec):
    return tuple(State(tuple(s), tuple(s), c) for s in starts(spec['model']) for c in (1, 2, 3))


def snapshot(state):
    # Only relative age is relevant to enabledness and timeout. No request ID is read.
    age = 2-state.remaining if state.entitlement else 0
    return dict(state=list(state.native), reference=list(state.reference), time=age,
                pending=(dict(id=0, submitted=0, entitlement=state.entitlement)
                         if state.entitlement else None), monitor=age, stage=state.stage)


def events(spec, state):
    if state.bad or spec['empty']:
        return []
    return moves(spec, snapshot(state), state.coalition)


def responses(spec):
    return GUARDS if spec['reactive'] else (spec['guard'],)


def table_name(model, guard):
    if model == 'composition':
        return 'lawful-composition'
    return {'true':'guard-true', 'actor == 0':'lawful', 'false':'guard-false'}[guard]


def step(spec, tables, state, event, guard):
    action = events(spec, state)[event]
    native = tables[table_name(spec['model'], guard)][tuple(state.native)+tuple(action['request'])]
    after, _, violations, _ = advance(spec, snapshot(state), action, native)
    pending = after['pending']
    remaining = max(0, 2-(after['time']-pending['submitted'])) if pending else 0
    return State(tuple(after['state']), tuple(after['reference']), state.coalition,
                 pending['entitlement'] if pending else 0, remaining,
                 min(4, after['stage']), bool(violations))


def build(spec, tables, max_states=10000, max_edges=100000, seconds=30):
    """Enumerate reachable product including all controller responses, or UNKNOWN.

    Builders own all mutation; Graph and State expose only tuples and primitives.
    A partial graph can be inspected but can never support a certificate.
    """
    states = list(roots(spec)); index = {s:i for i,s in enumerate(states)}
    edges = []; work = 0; end = time.monotonic()+seconds
    initial = tuple(range(len(states)))
    def stopped(reason):
        return Graph(tuple(states), initial, tuple(edges), 'UNKNOWN', reason)
    if len(states) > max_states:
        return stopped('state budget')
    for state in states:
        row = []
        if time.monotonic() >= end:
            return stopped('time budget')
        allowed = events(spec, state)
        if not state.bad and not allowed:
            return stopped('empty environment')
        for event in range(len(allowed)):
            for guard in responses(spec):
                if work >= max_edges:
                    return stopped('edge budget')
                if time.monotonic() >= end:
                    return stopped('time budget')
                try:
                    successor = step(spec, tables, state, event, guard)
                except (KeyError, ValueError):
                    return stopped('native domain not closed')
                if successor not in index:
                    if len(states) >= max_states:
                        return stopped('state budget')
                    index[successor] = len(states); states.append(successor)
                row.append((event, guard, index[successor])); work += 1
        edges.append(tuple(row))
    return Graph(tuple(states), initial, tuple(edges), 'complete', 'exhausted finite product')
