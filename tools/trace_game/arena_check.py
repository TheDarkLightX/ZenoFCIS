"""Independent local arena certificate checks; no solver or graph-builder imports."""
import time
from arena_spec import parse, need


def mapping(rows, domain, message):
    need(type(rows) is list, message)
    result = {}
    for row in rows:
        need(type(row) is list and len(row) == 2 and type(row[0]) is int
             and row[0] in domain and row[0] not in result, message)
        result[row[0]] = row[1]
    need(set(result) == set(domain), message)
    return result


def knowledge_graph(arena, g):
    """Check every submitted subset equation and reachability, without subset search."""
    need(type(g) is dict and set(g) == {'nodes','roots','owners','bad','edges'}, 'knowledge fields')
    nodes, owners, bad, edges = (g[k] for k in ('nodes','owners','bad','edges'))
    n = len(nodes)
    need(0 < n <= 10000 and len(owners) == len(bad) == len(edges) == n, 'knowledge state coverage')
    need(sum(map(len, edges)) <= 100000, 'knowledge edge bound')
    index = {}; end = time.monotonic() + 30; work = 0
    for i, node in enumerate(nodes):
        need(type(node) is dict and set(node) == {'belief','action'}, 'knowledge reads hidden state')
        B, a = node['belief'], node['action']
        need(type(B) is list, 'belief encoding')
        work += len(B)
        need(work <= 1000000 and time.monotonic() < end, 'knowledge checker bound')
        need(type(B) is list and B and all(type(s) is int and 0 <= s < len(arena.owners) for s in B)
             and B == sorted(set(B)), 'belief encoding')
        need(len({arena.observations[s] for s in B}) == 1, 'belief observation uniformity')
        need(a is None or type(a) is str and arena.owners[B[0]] == 0
             and a in {x for x, _ in arena.edges[B[0]]}, 'knowledge action')
        key = (tuple(B), a); need(key not in index, 'duplicate belief'); index[key] = i
        need(type(owners[i]) is int and owners[i] == (1 if a is not None else arena.owners[B[0]]), 'belief owner')
        need(type(bad[i]) is bool and bad[i] == any(s in arena.marks for s in B), 'belief bad predicate')
    def groups(states):
        parts = {}
        for s in states: parts.setdefault(arena.observations[s], set()).add(s)
        result = []
        for obs, B in sorted(parts.items()):
            target = index.get((tuple(sorted(B)), None))
            need(target is not None, 'missing knowledge state')
            result.append([obs, target])
        return result
    need(g['roots'] == [t for _, t in groups(arena.roots)] and all(type(i) is int for i in g['roots']), 'knowledge roots')
    for i, node in enumerate(nodes):
        B, a = node['belief'], node['action']; expected = []
        if a is None and arena.owners[B[0]] == 0:
            for action, _ in arena.edges[B[0]]:
                target = index.get((tuple(B), action)); need(target is not None, 'missing knowledge state')
                expected.append([action, target])
        else:
            targets = set()
            for s in B:
                work += len(arena.edges[s]) + 1
                need(work <= 1000000 and time.monotonic() < end, 'knowledge checker bound')
                targets.update(t for action, t in arena.edges[s] if a is None or action == a)
            expected = groups(targets)
        need(edges[i] == expected and all(type(t) is int for _, t in edges[i]), 'knowledge transition coverage')
    seen = set(g['roots']); todo = list(seen)
    for i in todo:
        for _, t in edges[i]:
            if t not in seen: seen.add(t); todo.append(t)
    need(len(seen) == n, 'unreachable knowledge state')
    return owners, edges, bad, g['roots']


def safety(owners, edges, bad, cert):
    need(set(cert) == {'kind','winning','strategy','ranks','counterstrategy'}, 'strategy fields; hidden reads forbidden')
    n = len(owners); W = cert['winning']
    need(type(W) is list and all(type(i) is int and 0 <= i < n for i in W)
         and W == sorted(set(W)), 'winning set encoding')
    W = set(W); L = set(range(n)) - W
    sigma = mapping(cert['strategy'], {i for i in W if owners[i] == 0}, 'observation strategy coverage')
    ranks = mapping(cert['ranks'], L, 'certificate state coverage')
    need(all(type(r) is int and 0 <= r <= n for r in ranks.values()), 'rank encoding')
    tau = mapping(cert['counterstrategy'], {i for i in L if owners[i] == 1 and ranks[i]}, 'counterstrategy coverage')
    for i, row in enumerate(edges):
        if i in W:
            need(not bad[i], 'unsafe winning belief')
            selected = [t for a, t in row if a == sigma[i]] if owners[i] == 0 else [t for _, t in row]
            need(selected and all(t in W for t in selected), 'winning closure')
        else:
            r = ranks[i]
            need((r == 0) == bool(bad[i]), 'rank zero iff bad')
            if not r: continue
            selected = [t for a, t in row if a == tau[i]] if owners[i] == 1 else [t for _, t in row]
            need(selected and all(t in ranks and ranks[t] < r for t in selected), 'losing rank decrease')
    return W


def check(packet, expected=None):
    need(packet['schema'] == 'zeno-fcis/arena-certificate/1', 'arena certificate schema')
    need(set(packet) == {'schema','game','knowledge','certificate'}, 'certificate packet fields')
    arena = parse(packet['game'])
    if expected is not None: need(arena == expected, 'arena subject mismatch')
    if arena.partial:
        owners, edges, bad, roots = knowledge_graph(arena, packet['knowledge'])
    else:
        need(packet['knowledge'] is None, 'perfect information knowledge field')
        owners, edges, roots = arena.owners, arena.edges, arena.roots
        bad = [i in arena.marks for i in range(len(owners))]
    cert = packet['certificate']
    if arena.kind == 'safety':
        need(cert['kind'] == 'safety', 'certificate objective')
        W = safety(owners, edges, bad, cert)
    else:
        W = omega(arena, cert)
    return dict(evidence='certified winning strategy' if set(roots) <= W else 'certified unrealizability',
                objective=arena.kind, winning_roots=[i for i in roots if i in W],
                losing_roots=[i for i in roots if i not in W], states=len(owners),
                scope='sure winning in the declared finite arena; no native execution or runtime authority')


def cyclic_components(vertices, edges):
    """Iterative Kosaraju; returns SCCs with a cycle, including singleton loops."""
    seen=set(); order=[]
    for root in sorted(vertices):
        if root in seen: continue
        seen.add(root); stack=[(root,iter(edges[root]))]
        while stack:
            i,following=stack[-1]
            target=next(following,None)
            if target is None:
                order.append(i); stack.pop()
            elif target in vertices and target not in seen:
                seen.add(target); stack.append((target,iter(edges[target])))
    reverse={i:[] for i in vertices}
    for i in vertices:
        for t in edges[i]:
            if t in vertices: reverse[t].append(i)
    seen=set()
    for root in reversed(order):
        if root in seen: continue
        C={root}; seen.add(root); todo=[root]
        for i in todo:
            for t in reverse[i]:
                if t not in seen: seen.add(t); C.add(t); todo.append(t)
        if len(C)>1 or root in edges[root]: yield C


def omega(arena,cert):
    """Check positional witnesses by closure and *all* adverse priority cycles.

    An SCC's largest priority alone is insufficient: it may contain a smaller
    adverse subcycle. Check each priority-induced subgraph separately.
    """
    need(cert['kind']==arena.kind,'certificate objective')
    need(set(cert)==({'kind','winning','strategy','counterstrategy','ranks'} if arena.kind=='reachability'
                     else {'kind','winning','strategy','counterstrategy'}),'omega certificate fields')
    n=len(arena.owners); W=cert['winning']
    need(type(W) is list and all(type(i) is int and 0<=i<n for i in W) and W==sorted(set(W)), 'winning state coverage')
    W=set(W); regions=[W,set(range(n))-W]
    witnesses=[mapping(cert['strategy'],{i for i in W if arena.owners[i]==0 and
                         (arena.kind!='reachability' or i not in arena.marks)},'controller strategy coverage'),
               mapping(cert['counterstrategy'],{i for i in regions[1] if arena.owners[i]==1},'environment strategy coverage')]
    if arena.kind=='parity': p=arena.marks
    elif arena.kind=='buchi': p=tuple(2 if i in arena.marks else 1 for i in range(n))
    elif arena.kind=='co-buchi': p=tuple(0 if i in arena.marks else 1 for i in range(n))
    else: p=None
    end=time.monotonic()+30; work=0
    for player,R in enumerate(regions):
        selected={}
        for i in R:
            if arena.kind=='reachability' and i in arena.marks and player==0:
                selected[i]=[]; continue
            choice=witnesses[player].get(i)
            row=[t for a,t in arena.edges[i] if arena.owners[i]!=player or a==choice]
            need(row and all(t in R for t in row),'omega winning closure')
            selected[i]=row
        if arena.kind=='reachability':
            if player==0:
                ranks=mapping(cert['ranks'],R,'reachability rank coverage')
                need(all(type(r) is int and 0<=r<=n for r in ranks.values()),'reachability rank encoding')
                for i in R:
                    need((ranks[i]==0)==(i in arena.marks),'reachability target rank')
                    need(all(ranks[t]<ranks[i] for t in selected[i]),'reachability rank decrease')
            else: need(not R.intersection(arena.marks),'avoidance target exclusion')
            continue
        for d in sorted({p[i] for i in R}):
            if d % 2 == player: continue
            vertices={i for i in R if p[i]<=d}
            work+=len(R)+sum(len(selected[i]) for i in vertices)
            need(work<=1000000 and time.monotonic()<end,'omega checker bound')
            for C in cyclic_components(vertices,selected):
                need(not any(p[i]==d for i in C),'adverse parity cycle')
    return W
