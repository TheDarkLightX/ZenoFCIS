"""Untrusted fixed point and certificate construction for finite products."""
from product import build, terminal, GUARDS


def safety(graph, spec=None):
    """nu W. Safe intersect universal Pre(W)."""
    if graph.status!='complete': return None
    W={i for i,s in enumerate(graph.states) if not s.bad and (graph.edges[i] or (spec is not None and terminal(spec,s)))}
    while True:
        following={i for i in W if all(t in W for _,_,t in graph.edges[i])}
        if following==W: return sorted(W)
        W=following


def certify(spec,tables,**budgets):
    graph=build(spec,tables,**budgets)
    W=safety(graph,spec)
    if W is None or not set(graph.roots)<=set(W):
        return dict(status='UNKNOWN' if W is None else 'not safe',stop=graph.stop,graph=graph.wire())
    return dict(schema='zeno-fcis/trace-game-certificate/1.1',spec=spec,graph=graph.wire(),
                certificate=dict(kind='safety',W=W))


def counterexample(graph):
    """Breadth-first discovery, with the complete unreduced event alphabet."""
    todo=list(graph.roots); parent={i:None for i in todo}
    for i in todo:
        if graph.states[i].bad:
            path=[]
            while parent[i] is not None:
                before,event,guard=parent[i]; path.append([event,guard]); i=before
            return dict(root=i,moves=list(reversed(path)),minimality='shortest')
        for event,guard,target in graph.edges[i]:
            if target not in parent:
                parent[target]=(i,event,guard); todo.append(target)
    return None


def fixed_guard(spec,tables,**budgets):
    attempts=[]
    for guard in GUARDS:
        candidate=dict(spec,guard=guard,reactive=False)
        graph=build(candidate,tables,**budgets); W=safety(graph,candidate)
        if W is None: return dict(status='UNKNOWN',stop=graph.stop)
        if set(graph.roots)<=set(W):
            return dict(schema='zeno-fcis/trace-game-certificate/1.1',spec=candidate,graph=graph.wire(),
                        certificate=dict(kind='guard',W=W,guard=guard,grammar=list(GUARDS)))
        attempts.append(dict(schema='zeno-fcis/trace-game-certificate/1.1',spec=candidate,graph=graph.wire(),
                             certificate=dict(kind='counterexample',**counterexample(graph))))
    return dict(schema='zeno-fcis/trace-game-guard-exhaustion/1.1',grammar=list(GUARDS),attempts=attempts)


def unrealizable(spec,tables,**budgets):
    graph=build(spec,tables,**budgets)
    if graph.status!='complete': return dict(status='UNKNOWN',stop=graph.stop)
    rank={i:0 for i,s in enumerate(graph.states) if s.bad}
    strategy={}
    while True:
        following={}
        for i,row in enumerate(graph.edges):
            if i in rank: continue
            for event in sorted({e for e,_,_ in row}):
                targets=[t for e,_,t in row if e==event]
                if targets and all(t in rank for t in targets):
                    following[i]=(1+max(rank[t] for t in targets),event); break
        if not following: break
        for i,(r,event) in following.items(): rank[i]=r; strategy[i]=event
    initial=next((i for i in graph.roots if i in rank),None)
    if initial is None: return dict(status='not established',stop='initial state outside environment attractor')
    return dict(schema='zeno-fcis/trace-game-certificate/1.1',spec=spec,graph=graph.wire(),
                certificate=dict(kind='unrealizability',root=initial,
                                 ranks=[[i,rank[i],strategy.get(i)] for i in sorted(rank)]))
