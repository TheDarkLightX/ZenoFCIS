"""Immutable explicit finite arenas; input preparation has no runtime authority."""
from dataclasses import dataclass
from game_spec import need


@dataclass(frozen=True)
class Arena:
    owners: tuple
    observations: tuple
    edges: tuple
    roots: tuple
    kind: str
    marks: tuple
    partial: bool

    def wire(self):
        objective = {'kind': self.kind, 'priorities' if self.kind == 'parity' else 'states': list(self.marks)}
        return dict(schema='zeno-fcis/arena/1', observation='partial' if self.partial else 'perfect',
                    roots=list(self.roots), objective=objective,
                    nodes=[dict(owner=o, observation=b, edges=[list(e) for e in row])
                           for o, b, row in zip(self.owners, self.observations, self.edges)])


def indices(values, n):
    return (type(values) is list and all(type(i) is int and 0 <= i < n for i in values)
            and len(set(values)) == len(values))


def parse(value):
    need(type(value) is dict and set(value) == {'schema', 'observation', 'roots', 'nodes', 'objective'}, 'arena fields')
    need(value['schema'] == 'zeno-fcis/arena/1', 'arena schema')
    need(value['observation'] in ('perfect', 'partial'), 'observation mode')
    nodes = value['nodes']
    need(type(nodes) is list and 0 < len(nodes) <= 10000, 'arena state bound')
    n = len(nodes)
    need(indices(value['roots'], n) and value['roots'], 'arena roots')
    owners, observations, edges = [], [], []
    size = 0; classes = {}
    for node in nodes:
        need(type(node) is dict and set(node) == {'owner', 'observation', 'edges'}, 'node fields')
        o, b, row = node['owner'], node['observation'], node['edges']
        need(type(o) is int and o in (0, 1), 'owner 0=controller, 1=environment')
        need(type(b) is str and 0 < len(b) <= 8192, 'observation label')
        need(type(row) is list and row, 'total arena required; use explicit absorbing loops')
        size += len(row); need(size <= 100000, 'arena edge bound')
        for e in row:
            need(type(e) is list and len(e) == 2 and type(e[0]) is str and 0 < len(e[0]) <= 8192
                 and type(e[1]) is int and 0 <= e[1] < n, 'arena edge')
        need(len(set(a for a, _ in row)) == len(row), 'unique action per node')
        if value['observation'] == 'partial':
            signature = (o, tuple(sorted(a for a, _ in row)) if o == 0 else ())
            need(classes.setdefault(b, signature) == signature, 'observation reveals owner and enabled actions')
        owners.append(o); observations.append(b); edges.append(tuple((a, t) for a, t in row))
    objective = value['objective']
    need(type(objective) is dict, 'objective object')
    kind = objective.get('kind')
    need(kind in ('safety', 'reachability', 'buchi', 'co-buchi', 'parity'), 'unsupported objective; quantitative objectives deferred')
    field = 'priorities' if kind == 'parity' else 'states'
    need(set(objective) == {'kind', field}, 'objective fields')
    marks = objective[field]
    if kind == 'parity':
        need(type(marks) is list and len(marks) == n and all(type(p) is int and 0 <= p <= 255 for p in marks), 'parity priorities 0..255')
    else:
        need(indices(marks, n), 'objective state set')
    need(value['observation'] == 'perfect' or kind == 'safety', 'partial observation supports sure safety only')
    return Arena(tuple(owners), tuple(observations), tuple(edges), tuple(value['roots']), kind,
                 tuple(marks), value['observation'] == 'partial')


def load(path):
    # Explicit arenas may be larger than the contract game/1 input.
    with path.open('rb') as stream: raw = stream.read(16 * 1024 * 1024 + 1)
    need(len(raw) <= 16 * 1024 * 1024, 'arena byte bound')
    import json
    from game_spec import unique, reject_number
    return parse(json.loads(raw, object_pairs_hook=unique, parse_float=reject_number, parse_constant=reject_number))
