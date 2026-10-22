"""Bounded subset construction and safety fixed points. Untrusted search only."""
import time


class Limit(ValueError):
    pass


class Budget:
    def __init__(self, states=10000, edges=100000, work=1000000, seconds=30):
        for value, maximum in ((states, 10000), (edges, 100000), (work, 1000000)):
            if type(value) is not int or not 0 <= value <= maximum: raise ValueError('invalid budget')
        if type(seconds) not in (int, float) or not 0 <= seconds <= 30: raise ValueError('invalid time budget')
        self.states, self.edges, self.maximum = states, edges, work
        self.work = 0; self.end = time.monotonic() + seconds

    def charge(self, n=1):
        self.work += n
        if self.work > self.maximum: raise Limit('solver work bound')
        if time.monotonic() >= self.end: raise Limit('solver time bound')


def construct(arena, budget):
    """Beliefs are exact sets after observation histories; actions add environment nodes."""
    nodes = []; index = {}; rows = []; owners = []; bad = []; edge_count = 0
    def intern(belief, action=None):
        budget.charge()
        key = (tuple(sorted(belief)), action)
        if key not in index:
            if len(nodes) >= budget.states: raise Limit('knowledge state bound')
            index[key] = len(nodes); nodes.append(key)
        return index[key]
    def partition(states):
        groups = {}
        for s in states:
            budget.charge(); groups.setdefault(arena.observations[s], set()).add(s)
        return [(obs, intern(B)) for obs, B in sorted(groups.items())]
    roots = [i for _, i in partition(arena.roots)]
    for B, action in nodes:
        budget.charge(len(B))
        owner = arena.owners[B[0]]
        owners.append(1 if action is not None else owner)
        bad.append(any(s in arena.marks for s in B))
        if action is None and owner == 0:
            row = [(a, intern(B, a)) for a, _ in arena.edges[B[0]]]
        else:
            targets = set()
            for s in B:
                for a, t in arena.edges[s]:
                    budget.charge()
                    if action is None or a == action: targets.add(t)
            row = partition(targets)
        edge_count += len(row)
        if edge_count > budget.edges: raise Limit('knowledge edge bound')
        rows.append(row)
    return dict(nodes=[dict(belief=list(B), action=a) for B, a in nodes], roots=roots,
                owners=owners, bad=bad, edges=[[list(e) for e in row] for row in rows])


def safety_certificate(owners, edges, bad, budget):
    """Complement of the environment attractor to Bad, with local rank witnesses."""
    ranks = {i: 0 for i, b in enumerate(bad) if b}
    while True:
        following = {}
        for i, row in enumerate(edges):
            budget.charge(1 + len(row))
            if i in ranks: continue
            targets = [t for _, t in row]
            losing = [t for t in targets if t in ranks]
            if (owners[i] == 1 and losing) or (owners[i] == 0 and len(losing) == len(targets)):
                following[i] = 1 + (min(ranks[t] for t in losing) if owners[i] == 1 else max(ranks[t] for t in losing))
        if not following: break
        ranks.update(following)
    W = sorted(set(range(len(owners))) - ranks.keys()); winning = set(W)
    choices = [[i, next(a for a, t in edges[i] if t in winning)] for i in W if owners[i] == 0]
    counters = [[i, next(a for a, t in edges[i] if t in ranks and ranks[t] < r)]
                for i, r in sorted(ranks.items()) if r and owners[i] == 1]
    return dict(kind='safety', winning=W, strategy=choices, ranks=sorted(map(list, ranks.items())), counterstrategy=counters)


def solve(arena, budget=None):
    budget = budget or Budget()
    try:
        if arena.kind != 'safety': raise ValueError('safety objective required')
        graph = construct(arena, budget) if arena.partial else dict(
            owners=list(arena.owners), edges=[[list(e) for e in row] for row in arena.edges],
            roots=list(arena.roots), bad=[i in arena.marks for i in range(len(arena.owners))])
        if len(graph['owners']) > budget.states or sum(map(len, graph['edges'])) > budget.edges: raise Limit('arena bound')
        cert = safety_certificate(graph['owners'], graph['edges'], graph['bad'], budget)
        return dict(schema='zeno-fcis/arena-certificate/1', game=arena.wire(),
                    knowledge=graph if arena.partial else None, certificate=cert)
    except (Limit, RecursionError) as error:
        return dict(status='UNKNOWN', stop=str(error) or 'recursion bound', work=budget.work)
