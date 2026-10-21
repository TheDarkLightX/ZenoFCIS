#!/usr/bin/env python3
"""Independent exact-integer check of the finite own-command sequence graph."""
import argparse
from collections import Counter, defaultdict
from copy import deepcopy
import hashlib
import json
from pathlib import Path
from check_command_refinement import Q, ident, integer, balance, effective, gain, invariant, successor


def rewards(s, who):
    actor = ident(who)
    h = [0, 0, 0]
    p, t = s['pool'], s['staking']
    fronts = {v[0]: v for v in p['fronts']}
    for owner, initial, tag, snap in p['deposits']:
        if owner == actor:
            _, a, z = gain(p, initial, snap)
            h[0] += a
            h[2] += z * (fronts[tag][1] if tag else Q) // Q
    for owner, k, initial, snap in p['fronts']:
        if owner == actor:
            h[2] += gain(p, initial, snap, True)[1] * (Q-k) // Q
    for owner, stake, sz, sa in t['positions']:
        if owner == actor:
            h[0] += (integer(t['f'][1])-integer(sa))*stake//Q
            h[1] += (integer(t['f'][0])-integer(sz))*stake//Q
    return h


def holdings(s, who):
    actor = ident(who)
    h = rewards(s, who)
    rows = balance(s)
    for i, asset in enumerate([1, 0, 2]):
        h[i] += rows.get((actor, asset), 0)
    for v in s['vaults']:
        if v[1] == actor and v[2] == 'Active':
            v = effective(s, v)
            h[1] += max(0, v[3]*s['price']//10**8-v[4]) + v[5]
    h[0] += sum(n for owner, n in s['surplus'] if owner == actor)
    h[1] += sum(gain(s['pool'], n, snap)[0] for owner, n, _, snap in s['pool']['deposits'] if owner == actor)
    h[2] += sum(stake for owner, stake, _, _ in s['staking']['positions'] if owner == actor)
    return h


def benefit(pre, post, who, c, legs):
    b = [0, 0, 0]
    kind, args, actor = c['kind'], c['args'], ident(who)
    old, new = rewards(pre, who), rewards(post, who)
    if kind.startswith('liquidate_'):
        # Every compensation leg has an explicit payer and recipient.
        for payer, recipient, asset, n in legs:
            if recipient == actor:
                assert payer == ident(90 if asset == 1 else 92)
                b[[1, 0, 2][asset]] += n
        assert who == 6 and old == [0, 0, 0] and new == [0, 0, 0], 'finite liquidation benefit domain'
    # Only actual fee events can create new fee claims. Claim release is zero.
    if kind in ('open', 'redeem') or kind == 'adjust' and args[1][0] == 1:
        # Derive the new fee entitlement from the pre-event accumulators and
        # exact fee legs already independently validated by successor().
        # A poststate claim increase alone is never a benefit exemption.
        t = pre['staking']
        position = next((v for v in t['positions'] if v[0] == actor), None)
        if position:
            for i, asset, index in [(0, 1, 1), (1, 0, 0)]:
                fee = sum(n for _, recipient, a, n in legs if recipient == ident(97) and a == asset)
                f = integer(t['f'][index])
                snap = integer(position[2+index])
                stake = position[1]
                b[i] += ((f+fee*Q//t['total']-snap)*stake//Q - (f-snap)*stake//Q)
    if kind == 'fund' and args[0] == actor:
        b[[1, 0, 2][args[1]]] += args[2]
    return b


def dominates(after, before):
    return all(a >= b for a, b in zip(after, before)) and any(a > b for a, b in zip(after, before))


def alphabet(s, who):
    cs = []
    def add(kind, args): cs.append({'kind': kind, 'args': args})
    for c in [0, 700*Q, 1200*Q]:
        for principal in [0, 500*Q, 1000*Q]: add('open', [c, principal])
    for c in [[0, 0], [1, Q], [-1, Q]]:
        for d in [[0, 0], [1, Q], [-1, Q]]: add('adjust', [c, d])
    for kind in ['close', 'surplus', 'move_gain', 'pool_claim', 'stake_claim']: add(kind, [])
    for n in [0, 1, Q]:
        add('deposit', [n, None]); add('deposit', [n, ident(5)])
        for kind in ['withdraw', 'stake', 'unstake']: add(kind, [n])
    for k in [0, Q//2, Q]: add('register', [k])
    vs = {v[0]: v for v in s['vaults']}
    for maximum in [0, Q, 500*Q, 1005*Q, 2010*Q]:
        hint = 0
        for key in reversed(s['order']):
            v = effective(s, vs[key])
            if v[3]*s['price']*10**10//v[4] < 11*Q//10: continue
            n = min(maximum, v[4]-v[5])
            hint = (v[3]-n*10**8//s['price'])*10**20//(v[4]-n)
            break
        for partial in [0, hint]: add('redeem', [maximum, 0, partial])
    if who == 6:
        # Vault IDs are policy-derived, not owner IDs: recover the exact six
        # identifiers from the first-step alphabet, then check stability below.
        for key in LIQUIDATION_TARGETS: add('liquidate_direct', [key])
        add('liquidate_lowest', [8]); add('liquidate_batch', list(reversed(s['order'])))
        for asset in [1, 2]: add('fund', [ident(1), asset, 1])
    return Counter(json.dumps(c, sort_keys=True) for c in cs)


LIQUIDATION_TARGETS = []

def check_node(row):
    invariant(row['state'])
    assert row['holdings'] == holdings(row['state'], row['who'])


def check_dominance(end, before):
    assert not dominates(end, before)


def must_reject(name, run):
    try:
        run()
    except AssertionError:
        return name
    raise AssertionError('control survived: ' + name)


def check(folder):
    nodes = {}
    for line in (folder/'nodes.jsonl').open():
        row = json.loads(line)
        assert row['node'] not in nodes
        check_node(row)
        nodes[row['node']] = row
    edges = [json.loads(line) for line in (folder/'edges.jsonl').open()]
    global LIQUIDATION_TARGETS
    LIQUIDATION_TARGETS = [e['command']['args'][0] for e in edges if e['seed'] == 0 and e['who'] == 6 and e['depth'] == 1 and e['command']['kind'] == 'liquidate_direct']
    assert len(set(LIQUIDATION_TARGETS)) == 6
    groups = defaultdict(list)
    initial = {}
    attempts, admitted = Counter(), Counter()
    first = Counter(); second = Counter()
    for e in edges:
        key = (e['seed'], e['who'], e['depth'], e['pre'], tuple(e['benefits_before']))
        groups[key].append(e)
        pre, post = nodes[e['pre']], nodes[e['post']]
        assert pre['who'] == post['who'] == e['who']
        if e['depth'] == 1:
            assert e['multiplicity'] == 1 and e['benefits_before'] == [0, 0, 0]
            initial[e['seed'], e['who']] = pre['holdings']
            first[e['seed'], e['who'], e['post'], tuple(e['benefit'])] += 1
        else:
            second[e['seed'], e['who'], e['pre'], tuple(e['benefits_before'])] = e['multiplicity']
        attempts[e['depth']] += e['multiplicity']
        if e['error'] is None:
            admitted[e['depth']] += e['multiplicity']
            successor({'actor': ident(e['who']), 'command': e['command'], 'issuance': 0, 'pre': pre['state'], 'post': post['state'], 'legs': e['legs']})
            assert e['benefit'] == benefit(pre['state'], post['state'], e['who'], e['command'], e['legs'])
        else:
            assert e['pre'] == e['post'] and e['benefit'] == [0, 0, 0] and e['legs'] == []
        assert pre['state']['epoch'] == post['state']['epoch'] and pre['state']['price'] == post['state']['price']
        end = [a-b-c for a, b, c in zip(post['holdings'], e['benefits_before'], e['benefit'])]
        check_dominance(end, initial[e['seed'], e['who']])
    assert set(initial) == {(s, u) for s in range(17) for u in range(1, 7)}
    for key, rows in groups.items():
        actual = Counter(json.dumps(e['command'], sort_keys=True) for e in rows)
        assert actual == alphabet(nodes[key[3]]['state'], key[1]), (key, actual-alphabet(nodes[key[3]]['state'], key[1]))
        assert len({e['multiplicity'] for e in rows}) == 1
    # Canonical state IDs certify the prefix quotient and its multiplicity;
    # no rejected or duplicate prefix disappears from the sequence denominator.
    projected_first, projected_second = Counter(), Counter()
    for (s, u, n, b), count in first.items(): projected_first[s, u, nodes[n]['root'], b] += count
    for (s, u, n, b), count in second.items(): projected_second[s, u, nodes[n]['root'], b] += count
    assert projected_first == projected_second
    assert attempts == {1: 17*316, 2: 17*(5*51**2+61**2)}
    report = {'schema': 'zusd-sequence-check/1', 'level': 'finite checked', 'max_length': 2, 'seeds': 17, 'users': 6, 'attempts_by_length': dict(attempts), 'admissions_by_length': dict(admitted), 'stored_nodes': len(nodes), 'evaluated_edges': len(edges), 'prefix_groups': len(groups), 'record_sha256': {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in folder.glob('*.jsonl')}, 'nonclaims': ['arbitrary lengths', 'all u128 states', 'coalitions', 'backend truth or external custody']}
    # Mutate the law and evidence, not the runtime: each named defect must fail.
    insolvent = next(n for n in nodes.values() if any(v[2]=='Active' and effective(n['state'],v)[3]*n['state']['price']//10**8 < effective(n['state'],v)[4] for v in n['state']['vaults']))
    owner = next(v[1] for v in insolvent['state']['vaults'] if v[2]=='Active' and effective(insolvent['state'],v)[3]*insolvent['state']['price']//10**8 < effective(insolvent['state'],v)[4])
    s = insolvent['state'];u = int(owner,16); correct = holdings(s,u)
    full_debt = correct.copy()
    for v in s['vaults']:
        if v[1]==owner and v[2]=='Active':
            v=effective(s,v);full_debt[1]+=min(0,v[3]*s['price']//10**8-v[4])
    mutated = deepcopy(insolvent)
    mutated['who'] = u
    mutated['holdings'] = full_debt
    killed = [must_reject('recourse_debt', lambda: check_node(mutated))]
    claim = deepcopy(next(n for n in nodes.values() if any(rewards(n['state'],n['who']))))
    claim['holdings'] = [x+y for x,y in zip(claim['holdings'],rewards(claim['state'],claim['who']))]
    killed.append(must_reject('claim_counted_twice', lambda: check_node(claim)))
    killed.append(must_reject('one_atom_dominance', lambda: check_dominance([1,0,0],[0,0,0])))
    report['law_controls_killed'] = killed
    return report


if __name__ == '__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--records',type=Path,required=True);p.add_argument('--out',type=Path,required=True);a=p.parse_args()
    result=check(a.records);a.out.mkdir(parents=True,exist_ok=True);(a.out/'summary.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
