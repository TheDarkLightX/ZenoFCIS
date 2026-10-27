"""Untrusted fixed point and certificate construction for finite products."""
from product import build, terminal, GUARDS
import time

SOLVER_WORK = 1_000_000


class Meter:
    """Exclusive invocation scratch; shared across fixed-guard attempts."""
    def __init__(self):
        self.work=0; self.end=time.monotonic()+30; self.stop=''

    def charge(self, amount):
        self.work+=amount
        if self.work>SOLVER_WORK: self.stop='solver work budget'
        if time.monotonic()>=self.end: self.stop='solver time budget'
        return not self.stop


def safety(graph, spec=None, meter=None):
    """nu W. Safe intersect universal Pre(W)."""
    if graph.status!='complete': return None
    W={i for i,s in enumerate(graph.states) if not s.bad and (graph.edges[i] or (spec is not None and terminal(spec,s)))}
    meter=meter or Meter()
    while True:
        if not meter.charge(len(W)+sum(len(graph.edges[i]) for i in W)): return None
        following={i for i in W if all(t in W for _,_,t in graph.edges[i])}
        if following==W: return sorted(W)
        W=following


def certify(spec,tables,solver_meter=None,**budgets):
    graph=build(spec,tables,**budgets)
    meter=solver_meter or Meter()
    if spec['model']=='contract' and graph.status=='complete':
        if spec['objective']['kind']=='reachability':
            return contract_reachability(spec,graph,meter)
    W=safety(graph,spec,meter)
    if W is None or not set(graph.roots)<=set(W):
        if spec['model']=='contract' and W is not None:
            return dict(schema='zeno-fcis/trace-game-certificate/1.1',spec=spec,graph=graph.wire(),
                        certificate=dict(kind='counterexample',**counterexample(graph)))
        return dict(status='UNKNOWN' if W is None else 'not safe',stop=graph.stop if graph.status!='complete' else 'solver budget',graph=graph.wire())
    return dict(schema='zeno-fcis/trace-game-certificate/1.1',spec=spec,graph=graph.wire(),
                certificate=dict(kind='bounded' if spec.get('horizon') is not None else 'safety',W=W))


def contract_reachability(spec,graph,meter):
    """Universal ranked attractor for a fixed controller, environment owns events."""
    packet=dict(schema='zeno-fcis/trace-game-certificate/1.1',spec=spec,graph=graph.wire())
    if spec.get('horizon') is not None:
        return dict(packet,certificate=dict(kind='bounded'))
    ranks={i:0 for i,s in enumerate(graph.states) if s.stage}
    while True:
        if not meter.charge(len(graph.states)+sum(map(len,graph.edges))):
            return dict(status='UNKNOWN',stop=meter.stop)
        following={i:1+max(ranks[t] for _,_,t in row)
                   for i,row in enumerate(graph.edges) if i not in ranks and row and
                   all(t in ranks for _,_,t in row)}
        if not following: break
        ranks.update(following)
    if set(graph.roots)<=ranks.keys():
        return dict(packet,certificate=dict(kind='reachability',ranks=sorted(ranks.items())))
    losing=set(range(len(graph.states)))-ranks.keys()
    choices={i:next((e,g,t) for e,g,t in graph.edges[i] if t in losing) for i in losing}
    root=next(i for i in graph.roots if i in losing)
    at=root; seen={}; moves=[]
    while at not in seen:
        seen[at]=len(moves); e,g,at=choices[at]; moves.append([e,g])
    return dict(packet,certificate=dict(kind='reachability-counterexample',root=root,moves=moves,
                repeat_at=seen[at],avoidance=[[i,*choices[i][:2]] for i in sorted(choices)]))


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
        if W is None: return dict(status='UNKNOWN',stop=graph.stop if graph.status!='complete' else 'solver budget')
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
    strategy={}; work=0; end=time.monotonic()+30
    while True:
        work+=sum(len(row) for row in graph.edges)
        if work>SOLVER_WORK or time.monotonic()>=end: return dict(status='UNKNOWN',stop='attractor budget')
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


def reactive(spec,tables,**budgets):
    """Perfect-information upper bound, then conservative observation-uniform extraction.

    A failed uniform extraction is UNKNOWN, never an impossibility certificate.
    Grammar: a total finite lookup from (current actor, current request) to G.
    No native, hidden, reference, future outcome, or monitor field is readable.
    """
    from product import events
    spec=dict(spec,reactive=True)
    graph=build(spec,tables,**budgets)
    if graph.status!='complete': return dict(status='UNKNOWN',stop=graph.stop)
    W={i for i,s in enumerate(graph.states) if not s.bad}
    work=0; end=time.monotonic()+30
    while True:
        work+=sum(len(graph.edges[i])*(1+len({e for e,_,_ in graph.edges[i]})) for i in W)
        if work>SOLVER_WORK or time.monotonic()>=end: return dict(status='UNKNOWN',stop='reactive fixed point budget')
        following={i for i in W if all(any(t in W for e,g,t in graph.edges[i] if e==event)
                                      for event in {e for e,_,_ in graph.edges[i]})}
        if following==W: break
        W=following
    if not set(graph.roots)<=W: return dict(status='not established',stop='initial state outside reactive fixed point')
    cells={}
    for i in sorted(W):
        for event,action in enumerate(events(spec,graph.states[i])):
            observation=(action['actor'],tuple(action['request']))
            safe={g for e,g,t in graph.edges[i] if e==event and t in W}
            cells[observation]=cells.get(observation,set(GUARDS)) & safe
    if any(not choices for choices in cells.values()):
        return dict(status='UNKNOWN',stop='no observation-uniform strategy extracted')
    rules=[dict(actor=a,request=list(r),guard=next(g for g in GUARDS if g in choices))
           for (a,r),choices in sorted(cells.items())]
    return dict(schema='zeno-fcis/trace-game-certificate/1.1',spec=spec,graph=graph.wire(),
                certificate=dict(kind='strategy',W=sorted(W),strategy=dict(grammar='observation-table/1',
                                 reads=['actor','request'],order=spec['move_order'],rules=rules)))
