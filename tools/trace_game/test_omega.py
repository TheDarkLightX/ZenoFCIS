"""Independent exhaustive positional-strategy oracle for explicit small games."""
import copy
import itertools
import unittest
from unittest.mock import patch
from arena_spec import parse
from arena_check import check
from omega import solve
from test_knowledge import game
from knowledge import Budget


def objective(arena,kind,marks):
    value=arena.wire(); value['objective']={'kind':kind,'priorities' if kind=='parity' else 'states':list(marks)}
    return parse(value)


def brute(arena):
    """Enumerate both players' positional strategies; evaluate each path directly."""
    owned=[[i for i,o in enumerate(arena.owners) if o==p] for p in (0,1)]
    policies=[list(itertools.product(*(range(len(arena.edges[i])) for i in nodes))) for nodes in owned]
    winners=set()
    for sigma in policies[0]:
        success=set(range(len(arena.owners)))
        for tau in policies[1]:
            chosen=dict(zip(owned[0],sigma)); chosen.update(zip(owned[1],tau))
            for root in tuple(success):
                seen={}; path=[]; at=root
                while at not in seen:
                    seen[at]=len(path); path.append(at); at=arena.edges[at][chosen[at]][1]
                cycle=path[seen[at]:]
                if arena.kind=='safety': won=not set(path).intersection(arena.marks)
                elif arena.kind=='reachability': won=bool(set(path).intersection(arena.marks))
                elif arena.kind=='buchi': won=bool(set(cycle).intersection(arena.marks))
                elif arena.kind=='co-buchi': won=set(cycle)<=set(arena.marks)
                else: won=max(arena.marks[i] for i in cycle)%2==0
                if not won: success.remove(root)
        winners.update(success)
    return winners


def enumerate_games(max_size=2,parity_colors=3):
    counts={kind:0 for kind in ('safety','reachability','buchi','co-buchi','parity')}
    for n in range(1,max_size+1):
        successors=[tuple(i for i in range(n) if mask&(1<<i)) for mask in range(1,1<<n)]
        for rows in itertools.product(successors,repeat=n):
            edges=[[(str(t),t) for t in row] for row in rows]
            for owners in itertools.product((0,1),repeat=n):
                arena=game(owners,edges,partial=False)
                for kind in counts:
                    domains=(itertools.product(range(parity_colors),repeat=n) if kind=='parity' else
                             ([i for i in range(n) if bits&(1<<i)] for bits in range(1<<n)))
                    for marks in domains:
                        current=objective(arena,kind,marks); packet=solve(current)
                        verdict=check(packet,current)
                        got=set(verdict['winning_roots']); expected=brute(current)
                        if got!=expected: raise AssertionError((current.wire(),got,expected,packet))
                        counts[kind]+=1
    return counts


class Omega(unittest.TestCase):
    def test_exhaustive_small_games(self):
        print('exhaustive perfect games:',enumerate_games())

    def test_parity_priority_off_by_one_rejected(self):
        arena=objective(game([0],[[('stay',0)]],partial=False),'parity',[0])
        packet=solve(arena); self.assertEqual(packet['certificate']['winning'],[0])
        # The off-by-one priority game really has the opposite winner.
        flipped=objective(arena,'parity',[1]); forged=solve(flipped)
        self.assertEqual(forged['certificate']['winning'],[])
        forged['game']=arena.wire()
        with self.assertRaisesRegex(ValueError,'adverse parity cycle'): check(forged,arena)

    def test_smaller_odd_subcycle_inside_even_scc_rejected(self):
        arena=objective(game([1,1],[[('odd',0),('even',1)],[('back',0)]],partial=False),'parity',[1,2])
        packet=solve(arena)
        self.assertEqual(check(packet,arena)['losing_roots'],[0,1])
        packet['certificate'].update(winning=[0,1],strategy=[],counterstrategy=[])
        with self.assertRaisesRegex(ValueError,'adverse parity cycle'): check(packet,arena)

    def test_missing_certificate_state_rejected(self):
        arena=objective(game([0,0],[[('stay',0)],[('stay',1)]],partial=False),'buchi',[0,1])
        packet=solve(arena); packet['certificate']['strategy'].pop()
        with self.assertRaisesRegex(ValueError,'strategy coverage'): check(packet,arena)

    def test_unknown(self):
        arena=objective(game([0],[[('stay',0)]],partial=False),'parity',[0])
        for budget in (Budget(states=0),Budget(edges=0),Budget(work=0),Budget(seconds=0)):
            self.assertEqual(solve(arena,budget)['status'],'UNKNOWN')

    def test_zero_seconds_all_target_reachability(self):
        arena=objective(game([0],[[('stay',0)]],roots=[0],partial=False),'reachability',[0])
        result=solve(arena,Budget(seconds=0))
        self.assertEqual(result.get('status'),'UNKNOWN')
        self.assertEqual(result['stop'],'solver time bound')
        self.assertEqual(result['work'],0)
        self.assertEqual(check(solve(arena),arena)['evidence'],'certified winning strategy')

    def test_deadline_on_entry_and_after_packet_assembly(self):
        for partial,kind in ((False,'safety'),(True,'safety'),(False,'reachability'),
                             (False,'buchi'),(False,'co-buchi'),(False,'parity')):
            with self.subTest(partial=partial,kind=kind):
                arena=objective(game([0],[[('stay',0)]],partial=partial),kind,[0])
                self.assertEqual(solve(arena,Budget(seconds=0))['stop'],'solver time bound')
                clock=[0]; wire=type(arena).wire
                def expire(subject):
                    packet=wire(subject); clock[0]=2
                    return packet
                with patch('knowledge.time.monotonic',side_effect=lambda:clock[0]), \
                     patch.object(type(arena),'wire',expire):
                    budget=Budget(seconds=1)
                    result=solve(arena,budget)
                self.assertEqual(result.get('status'),'UNKNOWN')
                self.assertEqual(result['stop'],'solver time bound')
                self.assertEqual(result['work'],budget.work)

    def test_partial_omega_refused(self):
        arena=game([0],[[('stay',0)]])
        with self.assertRaisesRegex(ValueError,'sure safety'): objective(arena,'buchi',[0])


if __name__=='__main__': unittest.main()
