"""Independent bounded strategy enumeration and subset-certificate controls."""
import copy
import itertools
import unittest
from arena_spec import parse
from arena_check import check
from knowledge import solve, Budget


def game(owners, edges, bad=(), observations=None, roots=None, partial=True):
    n = len(owners)
    return parse(dict(schema='zeno-fcis/arena/1', observation='partial' if partial else 'perfect',
        roots=list(range(n)) if roots is None else roots, objective=dict(kind='safety',states=list(bad)),
        nodes=[dict(owner=o,observation=(observations or [str(i) for i in range(n)])[i],
                    edges=[[a,t] for a,t in edges[i]]) for i,o in enumerate(owners)]))


def brute_safety(arena):
    """Separate powerset transition relation, then enumerate every memory policy.

    The reference uses no search-produced knowledge graph or fixed-point code.
    Its bound is the full nonempty powerset of each observation class.
    """
    n = len(arena.owners)
    beliefs = [frozenset(i for i in range(n) if mask & (1<<i)) for mask in range(1,1<<n)]
    beliefs = [B for B in beliefs if len({arena.observations[i] for i in B}) == 1]
    def split(targets):
        return {frozenset(t for t in targets if arena.observations[t] == obs)
                for obs in {arena.observations[t] for t in targets}}
    domains = {}; relation = {}
    for B in beliefs:
        first = min(B)
        actions = [a for a,_ in arena.edges[first]] if arena.owners[first] == 0 else [None]
        domains[B] = actions
        for a in actions:
            relation[B,a] = split({t for i in B for label,t in arena.edges[i] if a is None or a == label})
    controlled = [B for B in beliefs if arena.owners[min(B)] == 0]
    roots = split(arena.roots)
    for choices in itertools.product(*(domains[B] for B in controlled)):
        policy = dict(zip(controlled,choices)); seen = set(roots); todo = list(roots)
        for B in todo:
            if B & set(arena.marks): break
            for T in relation[B,policy.get(B)]:
                if T not in seen: seen.add(T); todo.append(T)
        else: return True
    return False


def memory_game():
    return game([1,1,1,0,0,1,1],
        [[('red',1),('blue',2)],[('next',3)],[('next',4)],
         [('a',5),('b',6)],[('a',6),('b',5)],[('stay',5)],[('stay',6)]],
        [6],['start','red','blue','choose','choose','safe','bad'],[0])


class Knowledge(unittest.TestCase):
    def test_memory_from_observation_history(self):
        arena = memory_game(); packet = solve(arena)
        self.assertEqual(check(packet,arena)['evidence'],'certified winning strategy')
        # Both concrete choice states have the same current observation; their
        # singleton beliefs retain the earlier red/blue observation.
        actions = {tuple(packet['knowledge']['nodes'][i]['belief']): a
                   for i,a in packet['certificate']['strategy']}
        self.assertEqual(actions[(3,)],'a'); self.assertEqual(actions[(4,)],'b')

    def test_hidden_state_strategy_rejected(self):
        arena = game([0,0,1,1], [[('a',2),('b',3)],[('a',3),('b',2)],
            [('stay',2)],[('stay',3)]], [3], ['hidden','hidden','safe','bad'],[0,1])
        packet = solve(arena)
        self.assertEqual(check(packet,arena)['evidence'],'certified unrealizability')
        # A full-state policy falsely presents separate singleton roots.
        forged = solve(game(arena.owners,arena.edges,arena.marks,roots=[0],partial=False))
        forged['game'] = arena.wire()
        with self.assertRaises(ValueError): check(forged,arena)
        packet['certificate']['hidden'] = [[0,'a'],[1,'b']]
        with self.assertRaisesRegex(ValueError,'hidden reads'): check(packet,arena)

    def test_hidden_state_policy_rejected(self):
        arena = game([0,0,1,1], [[('a',2),('b',3)],[('a',3),('b',2)],
            [('stay',2)],[('stay',3)]], [3], ['hidden','hidden','safe','bad'],[0,1])
        # The planted full-state policy splits indistinguishable initial states.
        fake = arena.wire()
        for i,node in enumerate(fake['nodes']): node['observation']=str(i)
        forged=solve(parse(fake)); forged['game']=arena.wire()
        with self.assertRaisesRegex(ValueError,'missing knowledge state'): check(forged,arena)

    def test_missing_certificate_state_rejected(self):
        arena=game([1,1],[[('stay',0)],[('stay',1)]],[1],roots=[0,1],partial=False)
        packet=solve(arena); packet['certificate']['ranks']=[]
        with self.assertRaisesRegex(ValueError,'certificate state coverage'): check(packet,arena)

    def test_missing_state_rejected(self):
        arena=memory_game(); packet=solve(arena)
        packet['knowledge']['nodes'].pop()
        with self.assertRaisesRegex(ValueError,'state coverage'): check(packet,arena)

    def test_limits_unknown(self):
        arena=memory_game()
        for budget in (Budget(states=1),Budget(edges=1),Budget(work=0),Budget(seconds=0)):
            self.assertEqual(solve(arena,budget)['status'],'UNKNOWN')

    def test_snapshot_and_owned_result(self):
        arena=memory_game(); before=arena.wire(); packet=solve(arena)
        snapshot=copy.deepcopy(packet); verdict=check(packet,arena)
        verdict['winning_roots'].clear()
        self.assertEqual(verdict['winning_concrete_roots'],[0])
        verdict['root_mapping'][0]['concrete_roots'].clear()
        self.assertEqual(verdict['winning_concrete_roots'],[0])
        self.assertEqual(packet,snapshot)
        packet['game']['nodes'][0]['edges'].clear()
        self.assertEqual(arena.wire(),before)

    def test_root_identifier_namespaces(self):
        for partial in (False,True):
            with self.subTest(partial=partial):
                arena=game([1,1],[[('stay',0)],[('stay',1)]],[0],['bad','safe'],[1],partial)
                result=check(solve(arena),arena)
                self.assertEqual(result['winning_concrete_roots'],[1])
                self.assertEqual(result['winning_roots'],[1])
                self.assertEqual(result['losing_concrete_roots'],[])
                self.assertEqual(result['losing_roots'],[])
                self.assertEqual(result['winning_knowledge_roots'],[0] if partial else [])
                self.assertEqual(result['losing_knowledge_roots'],[])
                self.assertEqual(result['root_mapping'],
                                 [dict(knowledge_root=0,concrete_roots=[1])] if partial else [])

    def test_root_mapping_preserves_initial_uncertainty(self):
        arena=game([0,0,1,1,1],[[('a',2),('b',3)],[('a',3),('b',2)],
            [('stay',2)],[('stay',3)],[('stay',4)]],[3],
            ['hidden','hidden','safe','bad','independent'],[0,1,4])
        result=check(solve(arena),arena)
        self.assertEqual(result['winning_concrete_roots'],[4])
        self.assertEqual(result['losing_concrete_roots'],[0,1])
        self.assertEqual(result['winning_roots'],[4])
        self.assertEqual(result['losing_roots'],[0,1])
        self.assertEqual(result['winning_knowledge_roots'],[1])
        self.assertEqual(result['losing_knowledge_roots'],[0])
        self.assertEqual(result['root_mapping'],[dict(knowledge_root=0,concrete_roots=[0,1]),
                                               dict(knowledge_root=1,concrete_roots=[4])])

    def test_all_two_state_partial_games(self):
        count=0
        for n in (1,2):
            for destinations in itertools.product(range(n),repeat=2*n):
                edges=[list(zip(('a','b'),destinations[2*i:2*i+2])) for i in range(n)]
                for owners in itertools.product((0,1),repeat=n):
                    for observations in ([str(i) for i in range(n)], ['hidden']*n) if n>1 else (['hidden'],):
                        if len(set(observations))==1 and len(set(owners))>1: continue
                        for bad_bits in range(1<<n):
                            bad=[i for i in range(n) if bad_bits & (1<<i)]
                            for root_bits in range(1,1<<n):
                                roots=[i for i in range(n) if root_bits & (1<<i)]
                                arena=game(owners,edges,bad,observations,roots)
                                packet=solve(arena)
                                got=check(packet,arena)['evidence']=='certified winning strategy'
                                self.assertEqual(got,brute_safety(arena),arena.wire())
                                count+=1
        print('exhaustive partial safety games:',count)


if __name__=='__main__': unittest.main()
