#!/usr/bin/env python3
"""Independent integer replay of the declared seed-state/command product.

The runtime record generator uses only public decision APIs. This checker uses
Python integers, never runtime Wide arithmetic, emitted legs as an oracle,
or poststate debt to choose the abstract successor's monetary delta.
Generate records with ZUSD_REFINEMENT_OUT and the command_refinement integration
test, with the std feature enabled. The exact domain is in that test's source.
"""
import argparse
from collections import Counter, defaultdict
from copy import deepcopy
import hashlib
import json
from pathlib import Path

Q = 10**18
SCALE = 10**9
MCR, CCR = 11*Q//10, 3*Q//2
FLOOR, DECAY = Q//200, 999037758833783000
ROOT = Path(__file__).resolve().parents[3]


def ident(n):
    return f'{n:064x}'


def integer(a):
    return a[0]*2**128+a[1]


def balance(s):
    return {(a, k): n for a, k, n in s['ledger']}


def effective(s, v):
    v = v.copy()
    for index, reward, snapshot in [(3, 4, 7), (4, 5, 8)]:
        v[index] += (integer(s['system'][reward])-integer(v[snapshot]))*v[6]//Q
    return v


def comp(p, initial, snap_p, epoch, scale):
    assert epoch <= p['epoch'] and 0 < snap_p <= Q
    if epoch < p['epoch'] or p['scale']-scale >= 2:
        return 0
    assert p['scale'] >= scale
    n = initial*p['p']//(snap_p*SCALE**(p['scale']-scale))
    return 0 if n < initial//SCALE else n


def gain(p, initial, snap, front=False):
    if not initial:
        return (0, 0) if front else (0, 0, 0)
    if front:
        snap_p, snap_g, epoch, scale = snap
        snap_s = [0, 0]
    else:
        snap_p, snap_s, snap_g, epoch, scale = snap
    cells = {(e, c): (integer(s), integer(g)) for e, c, s, g in p['history']}
    first = cells[epoch, scale]
    second = cells.get((epoch, scale+1), (0, 0))
    z = (first[1]-integer(snap_g)+second[1]//SCALE)*initial//(snap_p*Q)
    n = comp(p, initial, snap_p, epoch, scale)
    if front:
        return n, z
    a = (first[0]-integer(snap_s)+second[0]//SCALE)*initial//(snap_p*Q)
    return n, a, z


def pool_claims(p):
    fronts = {f[0]: f for f in p['fronts']}
    principal = collateral = token = front_principal = 0
    for _, n, tag, snap in p['deposits']:
        x, c, g = gain(p, n, snap)
        principal += x
        collateral += c
        token += g*(fronts[tag][1] if tag else Q)//Q
    for _, k, initial, snap in p['fronts']:
        x, g = gain(p, initial, snap, True)
        front_principal += x
        token += g*(Q-k)//Q
    return principal, collateral, token, front_principal


def invariant(s):
    rows = balance(s)
    assert len(rows) == len(s['ledger'])
    assert list(rows) == sorted(rows)
    totals = [sum(n for (a, k), n in rows.items() if k == i) for i in range(3)]
    active = [v for v in s['vaults'] if v[2] == 'Active']
    assert totals[0] == s['supply'] == sum(s['debt']) == s['system'][1]
    assert totals[1:] == s['backing']
    assert s['system'][1] == sum(v[4] for v in active)+s['default'][1]
    assert s['system'][0] == sum(v[3] for v in active)+s['default'][0]
    assert s['system'][2] == len(active)
    pending = [sum(effective(s, v)[i]-v[i] for v in active) for i in [3, 4]]
    default_dust = [s['default'][i]-pending[i] for i in range(2)]
    assert min(default_dust) >= 0
    assert sum(v[5] for v in active) <= s['gas']
    p, t = s['pool'], s['staking']
    pc, ac, tc, fp = pool_claims(p)
    rem, scheduled, allocated, missed, paid = p['stock']
    assert scheduled == allocated+missed
    assert rem+scheduled == 35000000*Q and paid <= allocated
    assert fp <= p['total']
    dust = [p['total']-pc, p['collateral']-ac, allocated-paid-tc]
    assert min(dust) >= 0
    assert p['liabilities'] == [dust[0] if pc else 0, 0 if pc else dust[0], dust[1], dust[2]]
    claims = [sum((integer(t['f'][i])-integer(v[2+i]))*v[1]//Q for v in t['positions']) for i in range(2)]
    staking_dust = [t['custody'][i]-t['zero'][i]-claims[i] for i in range(2)]
    assert min(staking_dust) >= 0
    assert t['total'] == sum(v[1] for v in t['positions'])
    roles = [(90, 1, sum(v[3] for v in active)), (91, 1, s['default'][0]),
             (92, 0, s['gas']), (93, 0, p['total']), (94, 1, p['collateral']),
             (95, 2, rem+missed+allocated-paid), (96, 2, t['total']),
             (97, 0, t['custody'][0]), (97, 1, t['custody'][1]),
             (98, 1, sum(n for _, n in s['surplus']))]
    for a, k, n in roles:
        assert rows.get((ident(a), k), 0) == n
    return {'pool': dust, 'staking': staking_dust, 'zero_staker': t['zero'],
            'default': default_dust, 'gas_unassigned': s['gas']-sum(v[5] for v in active)}


def decay(s):
    x, y, n = DECAY, Q, min(s['epoch']-s['fee'][1], 525600000)
    if not n:
        power = Q
    else:
        while n > 1:
            if n % 2:
                y = (x*y+Q//2)//Q
            x = (x*x+Q//2)//Q
            n //= 2
        power = (x*y+Q//2)//Q
    return s['fee'][0]*power//Q


def successor(row):
    s, post, actor = row['pre'], row['post'], row['actor']
    kind, args = row['command']['kind'], row['command']['args']
    # Expected balance changes are constructed from prestate and command alone.
    expected = defaultdict(int)
    def leg(a, b, asset, n):
        assert n >= 0
        if a:
            expected[a, asset] -= n
        if b:
            expected[b, asset] += n
    def role(n):
        return ident(90+n)
    def same(field, expected_value):
        assert post[field] == expected_value, (kind, field, post[field], expected_value)
    total_delta, external = 0, [0, 0]
    pending = [0, 0]
    own = next((v for v in s['vaults'] if v[1] == actor and v[2] == 'Active'), None)
    v = effective(s, own) if own else None
    if kind in ('open', 'adjust', 'close'):
        if kind == 'open':
            collateral, principal = args
            fee = 0 if s['system'][1] and s['system'][0]*s['price']*10**10//s['system'][1] < CCR else principal*min(FLOOR+decay(s), Q//20)//Q
            reserve = 50*Q
            total_delta = principal+fee+reserve
            leg(actor, role(0), 1, collateral)
            leg(None, actor, 0, principal);leg(None, role(7), 0, fee);leg(None, role(2), 0, reserve)
            same('gas', s['gas']+reserve)
            assert post['system'][0] == s['system'][0]+collateral
        else:
            assert v is not None
            pending = [v[i+3]-own[i+3] for i in range(2)]
            leg(role(1), role(0), 1, pending[0])
            if kind == 'close':
                total_delta = -v[4]
                leg(actor, None, 0, v[4]-v[5]);leg(role(2), None, 0, v[5]);leg(role(0), actor, 1, v[3])
                same('gas', s['gas']-v[5])
                assert post['system'][0] == s['system'][0]-v[3]
            else:
                (cs, cn), (ds, dn) = args
                if cs > 0:leg(actor, role(0), 1, cn)
                if cs < 0:leg(role(0), actor, 1, cn)
                fee = 0
                if ds > 0:
                    fee = 0 if s['system'][0]*s['price']*10**10//s['system'][1] < CCR else dn*min(FLOOR+decay(s), Q//20)//Q
                    leg(None, actor, 0, dn);leg(None, role(7), 0, fee)
                if ds < 0:leg(actor, None, 0, dn)
                total_delta = ds*dn+fee
                same('gas', s['gas'])
                assert post['system'][0] == s['system'][0]+cs*cn
            same('default', [s['default'][i]-pending[i] for i in range(2)])
    elif kind in ('deposit', 'withdraw', 'pool_claim', 'move_gain', 'register'):
        p = deepcopy(s['pool'])
        if kind != 'register':
            issued = row['issuance']
            if p['total'] and issued:
                assert 'errors' in p
                increment = (issued*Q+integer(p['errors'][2]))//p['total']*p['p']
                value = integer(p['history'][-1][3])+increment
                p['history'][-1][3] = [value//2**128, value%2**128]
            old = next((d for d in p['deposits'] if d[0] == actor), None)
            principal, collateral, raw = gain(p, old[1], old[3]) if old else (0, 0, 0)
            tag = old[2] if old else None
            front = next((f for f in p['fronts'] if f[0] == tag), None)
            # A new tagged deposit has no historic gains; its front can have gains.
            if not old and kind == 'deposit':
                tag = args[1]
                front = next((f for f in p['fronts'] if f[0] == tag), None)
            k = front[1] if front else Q
            dep_gain = raw*k//Q
            front_gain = gain(p, front[2], front[3], True)[1]*(Q-k)//Q if front else 0
            incoming = args[0] if kind == 'deposit' else 0
            outgoing = min(args[0], principal) if kind == 'withdraw' else 0
            leg(actor, role(3), 0, incoming);leg(role(3), actor, 0, outgoing)
            leg(role(4), role(0) if kind == 'move_gain' else actor, 1, collateral)
            leg(role(5), actor, 2, dep_gain)
            if front:leg(role(5), front[0], 2, front_gain)
            if kind == 'move_gain':
                pending = [v[i+3]-own[i+3] for i in range(2)]
                leg(role(1), role(0), 1, pending[0])
                same('default', [s['default'][i]-pending[i] for i in range(2)])
                assert post['system'][0] == s['system'][0]+collateral
            assert post['pool']['total'] == p['total']+incoming-outgoing
            assert post['pool']['collateral'] == p['collateral']-collateral
            assert post['pool']['stock'][4] == p['stock'][4]+dep_gain+front_gain
    elif kind in ('stake', 'unstake', 'stake_claim'):
        t = s['staking'];old = next((v for v in t['positions'] if v[0] == actor), None)
        gains = [(integer(t['f'][i])-integer(old[2+i]))*old[1]//Q if old else 0 for i in range(2)]
        incoming = args[0] if kind == 'stake' else 0
        outgoing = min(args[0], old[1]) if kind == 'unstake' else 0
        leg(actor, role(6), 2, incoming);leg(role(6), actor, 2, outgoing)
        leg(role(7), actor, 0, gains[0]);leg(role(7), actor, 1, gains[1])
        assert post['staking']['total'] == t['total']+incoming-outgoing
        assert post['staking']['custody'] == [t['custody'][i]-gains[i] for i in range(2)]
    elif kind == 'surplus':
        leg(role(8), actor, 1, sum(n for a, n in s['surplus'] if a == actor))
    elif kind.startswith('liquidate_'):
        ids = args if kind == 'liquidate_batch' else ([args[0]] if kind == 'liquidate_direct' else list(reversed(s['order']))[:args[0]])
        c, d, active = s['system'][:3]
        mode = bool(d and c*s['price']*10**10//d < CCR);start = mode
        burned = sp_c = red_c = red_d = gas = 0
        selected = set()
        for vid in ids:
            if kind == 'liquidate_lowest' and start and vid == s['order'][0]:break
            old = next((v for v in s['vaults'] if v[0] == vid and v[2] == 'Active'), None)
            if old is None or vid in selected:continue
            entire = effective(s, old);vc, vd, reserve = entire[3:6]
            icr, tcr = vc*s['price']*10**10//vd, c*s['price']*10**10//d
            available = s['pool']['total']-burned
            if kind == 'liquidate_lowest' and ((mode and icr >= MCR and not available) or (not mode and icr >= MCR)):break
            if (mode and active <= 1) or (not mode and icr >= MCR) or (mode and icr >= MCR and (icr >= tcr or available < vd)):continue
            assert active > 1
            selected.add(vid)
            if mode and icr >= MCR:
                seized = vd*MCR//(s['price']*10**10);offset = vd;surplus = vc-seized
            else:
                seized = vc;surplus = 0
                offset = 0 if mode and icr <= Q else min(vd, available)
            keeper = seized//200;sp = (seized-keeper)*offset//vd
            redistributed_c = seized-keeper-sp;redistributed_d = vd-offset
            dp = [entire[i+3]-old[i+3] for i in range(2)]
            pending = [pending[i]+dp[i] for i in range(2)]
            leg(role(1), role(0), 1, dp[0]);leg(role(0), role(4), 1, sp)
            leg(role(0), role(1), 1, redistributed_c);leg(role(0), actor, 1, keeper)
            leg(role(0), role(8), 1, surplus);leg(role(2), actor, 0, reserve)
            burned += offset;sp_c += sp;red_c += redistributed_c;red_d += redistributed_d;gas += reserve
            c -= sp+keeper+surplus;d -= offset;active -= 1
            if mode:mode = bool(d and c*s['price']*10**10//d < CCR)
        assert selected
        leg(role(3), None, 0, burned);total_delta = -burned
        same('default', [s['default'][0]-pending[0]+red_c, s['default'][1]-pending[1]+red_d])
        same('gas', s['gas']-gas)
        assert post['system'][:3] == [c, d, active]
        assert post['pool']['total'] == s['pool']['total']-burned
        assert post['pool']['collateral'] == s['pool']['collateral']+sp_c
    elif kind == 'redeem':
        maximum, visits, hint = args
        remaining = maximum;executed = gross = reserves = surplus = visited = 0
        cap = min(visits, s['system'][2]) if visits else s['system'][2]
        for vid in reversed(s['order']):
            if remaining == 0 or visited >= cap:break
            old = next(v for v in s['vaults'] if v[0] == vid)
            entire = effective(s, old);vc, vd, reserve = entire[3:6]
            if vc*s['price']*10**10//vd < MCR:continue
            visited += 1
            dp = [entire[i+3]-old[i+3] for i in range(2)]
            pending = [pending[i]+dp[i] for i in range(2)]
            leg(role(1), role(0), 1, dp[0])
            net = vd-reserve;x = min(remaining, net);g = x*10**8//s['price']
            if x == net:
                reserves += reserve;surplus += vc-g
            elif vd-x-reserve < 500*Q or (vc-g)*10**20//(vd-x) != hint:break
            executed += x;gross += g;remaining -= x
        assert executed > 0 and gross > 0
        base = min(decay(s)+(gross*s['price']*10**10//s['system'][1])//2, Q)
        fee = min(FLOOR+base, Q)*gross//Q
        leg(actor, None, 0, executed);leg(role(2), None, 0, reserves)
        leg(role(0), actor, 1, gross-fee);leg(role(0), role(7), 1, fee);leg(role(0), role(8), 1, surplus)
        total_delta = -executed-reserves;same('gas', s['gas']-reserves)
        same('default', [s['default'][i]-pending[i] for i in range(2)])
        assert post['system'][0] == s['system'][0]-gross-surplus
    elif kind == 'fund':
        recipient, asset, amount = args
        leg(None, recipient, asset, amount);external[asset-1] = amount
    else:
        assert kind in ('clock', 'oracle', 'register')
    same('supply', s['supply']+total_delta)
    assert post['system'][1] == s['system'][1]+total_delta
    same('backing', [s['backing'][i]+external[i] for i in range(2)])
    pre_rows, post_rows = balance(s), balance(post)
    keys = pre_rows.keys() | post_rows.keys() | expected.keys()
    actual = {k: post_rows.get(k, 0)-pre_rows.get(k, 0) for k in keys}
    assert all(actual[k] == expected[k] for k in keys), (kind, 'independent balance delta', {k:(actual[k],expected[k]) for k in keys if actual[k] != expected[k]})
    # Check the emitted leg plan separately against exact wallet deltas.
    movements = defaultdict(int)
    for a, b, asset, n in row['legs']:
        if a:movements[a, asset] -= n
        if b:movements[b, asset] += n
    assert all(actual.get(k, 0) == movements[k] for k in keys | movements.keys())


def controls(path):
    accepted = [r for r in map(json.loads, path.read_text().splitlines()) if r['post'] is not None]
    mutations = {}
    def specimen(kind):
        return deepcopy(next(r for r in accepted if r['command']['kind'] == kind))
    r = specimen('open');r['post']['supply'] += 1;mutations['unmatched_supply'] = r
    r = specimen('fund');r['command']['args'][1] = 3-r['command']['args'][1];mutations['swapped_funding_asset'] = r
    r = deepcopy(next(r for r in accepted if r['command']['kind'] == 'stake_claim' and r['legs']))
    r['legs'].append(r['legs'][0]);mutations['duplicate_claim_leg'] = r
    r = specimen('close');r['post']['gas'] += 50*Q;mutations['retained_closed_reserve'] = r
    r = deepcopy(next(r for r in accepted if r['post']['pool']['liabilities'][2] > 0))
    r['post']['pool']['liabilities'][2] -= 1;mutations['dust_unaccounted'] = r
    r = deepcopy(next(r for r in accepted if r['post']['staking']['positions']))
    r['post']['staking']['f'][0] = [2**120, 0];mutations['claims_above_custody'] = r
    killed = []
    for name, row in mutations.items():
        try:
            invariant(row['pre']);invariant(row['post']);successor(row)
        except AssertionError:
            killed.append(name)
        else:
            raise AssertionError('Checker control survived: '+name)
    return killed


def check(path, out):
    counts = defaultdict(Counter);states = {};attempts = Counter();nonzero = Counter()
    for line_no, line in enumerate(path.read_text().splitlines(), 1):
        row = json.loads(line);kind = row['command']['kind']
        counts[kind]['attempted'] += 1;attempts[row['label']] += 1
        try:
            for s in [row['pre'], row['post']]:
                if s is not None:
                    key = hashlib.sha256(json.dumps(s, sort_keys=True).encode()).hexdigest()
                    if key not in states:states[key] = invariant(s)
            if row['error'] is not None:
                counts[kind]['refused'] += 1;continue
            successor(row);counts[kind]['admitted'] += 1
            if row['legs']:nonzero[kind] += 1
        except AssertionError as error:
            out.mkdir(parents=True, exist_ok=True)
            (out/'witness.json').write_text(json.dumps({'line':line_no,'error':str(error),'row':row},indent=2)+'\n')
            raise AssertionError(f'line {line_no}, {kind}: {error}') from error
    required = ['open','adjust','close','surplus','deposit','withdraw','pool_claim','move_gain','register','stake','unstake','stake_claim','liquidate_direct','liquidate_batch','liquidate_lowest','redeem','fund']
    assert all(counts[k]['admitted'] > 0 for k in required), {k:dict(counts[k]) for k in required if not counts[k]['admitted']}
    assert all(nonzero[k] > 0 for k in required if k != 'register'), dict(nonzero)
    sizes = {n for k,n in attempts.items() if k.startswith('seed-')}
    assert sizes == {316} and attempts['prefix'] == 37
    assert {k for k in attempts if k != 'prefix'} == {f'seed-{i}' for i in range(17)}
    report = {'level':'finite checked','domain':{'seed_states':len(attempts)-1,'pairs_per_seed':next(iter(sizes)),'prefix_steps':attempts['prefix'],'attempts':sum(attempts.values()),'unique_checked_states':len(states)},
              'counts':dict(counts),'nonzero_movements':dict(nonzero),'claims_and_dust':{'states':len(states),'checks':'exact pool/staking/default/gas complements; all wallet/custody rows counted once'},
              'record_sha256':hashlib.sha256(path.read_bytes()).hexdigest(),
              'checker_record_controls_killed':controls(path),
              'premises':['fixed conditional fixture policy and authenticated synthetic receipts','only the declared seed-state/command Cartesian product, not all u128 states','no backend truth, external custody publication or sequence dominance claim']}
    out.mkdir(parents=True, exist_ok=True);(out/'summary.json').write_text(json.dumps(report,indent=2)+'\n')
    (out/'dust.json').write_text(json.dumps(states,indent=2)+'\n')
    print(json.dumps(report,indent=2))
    return report


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__);parser.add_argument('--records',type=Path,required=True);parser.add_argument('--out',type=Path,required=True);args = parser.parse_args()
    check(args.records,args.out)
