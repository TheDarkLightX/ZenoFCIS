"""Classical explicit Zielonka recursion and attractors. Untrusted search only."""
from knowledge import Budget, Limit, solve as safety_solve


def priorities(arena):
    if arena.kind == 'parity': return arena.marks
    if arena.kind == 'buchi': return tuple(2 if i in arena.marks else 1 for i in range(len(arena.owners)))
    if arena.kind == 'co-buchi': return tuple(0 if i in arena.marks else 1 for i in range(len(arena.owners)))
    raise ValueError('omega objective required')


def attractor(arena, U, targets, player, budget):
    A = set(targets); strategy = {}
    while True:
        following = {}
        for i in sorted(U-A):
            budget.charge(1 + len(arena.edges[i]))
            targets_i = [(a,t) for a,t in arena.edges[i] if t in U]
            if not targets_i: raise ValueError('non-total recursive subgame')
            inside = [(a,t) for a,t in targets_i if t in A]
            if (arena.owners[i] == player and inside) or len(inside) == len(targets_i):
                following[i] = inside[0][0] if arena.owners[i] == player else None
        if not following: return A, strategy
        A.update(following)
        strategy.update((i,a) for i,a in following.items() if a is not None)


def zielonka(arena, p, U, budget):
    budget.charge(1 + len(U))
    if not U: return [set(),set()], [{},{}]
    d = max(p[i] for i in U); player = d % 2; other = 1-player
    top = {i for i in U if p[i] == d}
    A, alpha = attractor(arena,U,top,player,budget)
    W, sigma = zielonka(arena,p,U-A,budget)
    if not W[other]:
        W[player].update(A); sigma[player].update(alpha)
        for i in sorted(top):
            if arena.owners[i] == player:
                sigma[player][i] = next(a for a,t in arena.edges[i] if t in U)
        return W,sigma
    B,beta = attractor(arena,U,W[other],other,budget)
    V,tau = zielonka(arena,p,U-B,budget)
    V[other].update(B)
    tau[other].update(beta)
    tau[other].update(sigma[other])
    return V,tau


def reachability(arena,budget):
    U=set(range(len(arena.owners)))
    A,sigma=attractor(arena,U,set(arena.marks),0,budget)
    # Include ranks so target attainment, rather than infinite closure, is checked.
    ranks={i:0 for i in arena.marks}
    while set(ranks)!=A:
        budget.charge(sum(map(len,arena.edges))+len(U))
        following={}
        for i in sorted(A-ranks.keys()):
            targets=[t for a,t in arena.edges[i] if arena.owners[i]==1 or a==sigma[i]]
            if all(t in ranks for t in targets): following[i]=1+max(ranks[t] for t in targets)
        if not following: raise ValueError('attractor rank extraction')
        ranks.update(following)
    tau={i:next(a for a,t in arena.edges[i] if t not in A)
         for i in U-A if arena.owners[i]==1}
    return dict(kind='reachability',winning=sorted(A),strategy=sorted(map(list,sigma.items())),
                counterstrategy=sorted(map(list,tau.items())),ranks=sorted(map(list,ranks.items())))


def solve(arena,budget=None):
    if arena.kind=='safety': return safety_solve(arena,budget)
    budget=budget or Budget()
    try:
        budget.charge(0)  # Check even when the attractor needs no iterations.
        n=len(arena.owners)
        if n>budget.states or sum(map(len,arena.edges))>budget.edges: raise Limit('arena bound')
        if arena.kind=='reachability': cert=reachability(arena,budget)
        else:
            W,sigma=zielonka(arena,priorities(arena),set(range(n)),budget)
            cert=dict(kind=arena.kind,winning=sorted(W[0]),
                      strategy=sorted(map(list,sigma[0].items())),
                      counterstrategy=sorted(map(list,sigma[1].items())))
        packet=dict(schema='zeno-fcis/arena-certificate/1',game=arena.wire(),knowledge=None,certificate=cert)
        budget.charge(0)  # Include certificate assembly in the solver deadline.
        return packet
    except (Limit,RecursionError) as error:
        return dict(status='UNKNOWN',stop=str(error) or 'recursion bound',work=budget.work)
