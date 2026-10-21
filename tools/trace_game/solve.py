"""Untrusted fixed point and certificate construction for finite products."""
from product import build


def safety(graph):
    """nu W. Safe intersect universal Pre(W)."""
    if graph.status!='complete': return None
    W={i for i,s in enumerate(graph.states) if not s.bad and graph.edges[i]}
    while True:
        following={i for i in W if all(t in W for _,_,t in graph.edges[i])}
        if following==W: return sorted(W)
        W=following


def certify(spec,tables,**budgets):
    graph=build(spec,tables,**budgets)
    W=safety(graph)
    if W is None or not set(graph.roots)<=set(W):
        return dict(status='UNKNOWN' if W is None else 'not safe',stop=graph.stop,graph=graph.wire())
    return dict(schema='zeno-fcis/trace-game-certificate/1.1',spec=spec,graph=graph.wire(),
                certificate=dict(kind='safety',W=W))
