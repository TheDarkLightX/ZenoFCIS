#!/usr/bin/env python3
"""Archived oracle-only mint rediscovery and native rounding-prefix replay."""
import argparse
import dataclasses
import hashlib
import inspect
import itertools
import json
from pathlib import Path
import sys

ROOT=Path(__file__).resolve().parents[2]
SPOT=ROOT/'core-apps/zenodex-spot'
sys.path.insert(0,str(SPOT/'tools'))
import spot_reference as reference
from check_no_free_value import valuations, valuation_gains
from check_exact_value import endpoint, check_transition
from native_check import probe, world_json
from fixture_data import seeds, initialized, json_value


def flattened(world):
    return [world.pool.r0,world.pool.r1,world.pool.supply,world.pool.status,*world.balances,*world.shares]


def mutant():
    source=inspect.getsource(reference.step)
    old='mint = min(mf(u0, p.supply, p.r0), mf(u1, p.supply, p.r1))'
    if source.count(old)!=1 or source.count('lp(cmd.actor, mint)')!=1:
        raise ValueError('archived cross-account mint mutation no longer binds')
    source=source.replace(old,old+'\n        if cmd.actor == 0: mint *= 2')
    source=source.replace('lp(cmd.actor, mint)','lp(1 if cmd.actor == 0 else cmd.actor, mint)')
    scope=dict(vars(reference))
    exec(compile(source,'<archived-oracle-control>','exec'),scope)
    return scope['step'],hashlib.sha256(source.encode()).hexdigest()


def prepare(out):
    run,mutation=mutant()
    start=reference.World(reference.Pool(1000,1000,1000),(1000,1000,0,0,4,4,4,4),(0,0,reference.K))
    commands=tuple(command for actor in (0,1) for command in (
        reference.Command(3,a=2,b=2,actor=actor,recipient=actor),reference.Command(4,a=4,actor=actor,recipient=actor)))
    worlds=[start];ids={start:0};edges={};frontier={start}
    # The whole fixed alphabet is given to the Rust engine. No exploit path.
    for depth in range(3):
        following=set()
        for state in sorted(frontier,key=flattened):
            for i,command in enumerate(commands):
                after=run(state,command).post
                if after not in ids: ids[after]=len(worlds);worlds.append(after)
                edges[ids[state],i]=ids[after]
                following.add(after)
        frontier=following
    packet=dict(worlds=list(map(flattened,worlds)),edges=[[s,c,p] for (s,c),p in sorted(edges.items())],
        provenance=dict(oracle_only=True,mutation_sha256=mutation,reference_sha256=hashlib.sha256(inspect.getsource(reference).encode()).hexdigest(),
            valuation='liquid plus sum of individual floored burn claims; realized gain also requires closed coalition LP positions',
            loss='sum of floored wealth gain at fixed X=Y=1; no outside funding, attacker debt or costs',
            start=world_json(start),commands=[dataclasses.asdict(c) for c in commands],K=3,
            claim='synthetic oracle mutant only; no native bypass'))
    out.write_text(json.dumps(packet,indent=2)+'\n')


def verify_control(packet):
    from check import require,control_history,result
    run,mutation=mutant();data=packet['input'];provenance=data['provenance']
    require(provenance['K']==3 and data['only_objective'] is None,'Spot scope changed')
    require(provenance['mutation_sha256']==mutation and provenance['reference_sha256']==hashlib.sha256(inspect.getsource(reference).encode()).hexdigest(),'Spot oracle source drift')
    require(data['worlds'] and len(data['worlds'])<=4096 and all(len(w)==15 and all(type(v) is int and 0<=v<=1_000_000_000 for v in w) and w[2]>0 for w in data['worlds']),'Spot world domain')
    worlds=[reference.World(reference.Pool(*w[:4]),tuple(w[4:12]),tuple(w[12:])) for w in data['worlds']]
    start=reference.World(reference.Pool(1000,1000,1000),(1000,1000,0,0,4,4,4,4),(0,0,reference.K))
    require(worlds and worlds[0]==start,'Spot start changed')
    commands=tuple(c for a in (0,1) for c in (reference.Command(3,a=2,b=2,actor=a,recipient=a),reference.Command(4,a=4,actor=a,recipient=a)))
    require(provenance['commands']==[dataclasses.asdict(c) for c in commands],'Spot alphabet changed')
    edges={}
    for s,c,p in data['edges']:
        require(all(type(v) is int for v in (s,c,p)) and 0<=s<len(worlds) and 0<=p<len(worlds) and 0<=c<4,'Spot edge domain')
        require((s,c) not in edges,'duplicate Spot oracle edge');edges[s,c]=p
        require(run(worlds[s],commands[c]).post==worlds[p],'Spot oracle edge mismatch')
    disabled=data['disabled_objective']
    require(disabled in (None,'coalition-gain','realized-coalition-gain','composition-frame'),'Spot objective scope')
    def enabled(n):return [dict(actor=i//2,request=[i]) for i in range(4) if n['coalition']&(1<<(i//2))]
    def step(n,action):
        import copy
        p=edges[n['snapshot']['state'][0],action['request'][0]];members=tuple(a for a in (0,1) if n['coalition']&(1<<a))
        violations=[]
        if valuation_gains(worlds[0],worlds[p],members)['floored_burn_payout']:violations.append('coalition-gain')
        before=tuple(sum(worlds[0].balances[4+2*a+b] for a in members) for b in (0,1))
        after=tuple(sum(worlds[p].balances[4+2*a+b] for a in members) for b in (0,1))
        if not any(worlds[p].shares[a] for a in members) and all(a>=b for a,b in zip(after,before)) and after!=before:violations.append('realized-coalition-gain')
        violations=[v for v in violations if v!=disabled]
        require(n['coalition']==3 or not violations,'Spot singleton not clean')
        snapshot=copy.deepcopy(n['snapshot']);snapshot['state']=[p];snapshot['time']+=1
        return snapshot,result([p],kind='oracle-only'),violations
    def loss(n):
        members=tuple(a for a in (0,1) if n['coalition']&(1<<a));p=n['snapshot']['state'][0]
        return max(0,sum(valuations(worlds[p],members)[0])-sum(valuations(worlds[0],members)[0]))
    checked=control_history(packet,[0],3,enabled,step,loss)
    checked.update(edges=len(edges),singleton_controls='clean',oracle_only=True,scope=dict(model='Spot-oracle',fault='cross-account-mint',horizon=3,powers='all four commands for coalition actors 0/1',only_objective=None,disabled_objective=disabled,unfair_delay=False))
    return checked


def rounding(cli,out):
    # Original false one-atom claim, its exact reachable start and full native prefix.
    genesis,prefix,command=seeds()[1]
    current=genesis;replays=[]
    for i,operation in enumerate((*prefix,command)):
        decision=reference.step(current,operation)
        if decision.reason is not None:raise ValueError('rounding prefix refused')
        probe(current,operation,decision,out,f'prefix-{i}',cli=cli)
        replays.append(dict(before=current,command=operation,decision=decision))
        current=decision.post
    before=initialized(seeds()[1]);accounting=endpoint(before,current,(0,1))
    if str(accounting['rounding_excess'][1])!='170371/154371':raise ValueError('original one-atom refutation changed')
    check_transition(before,command,reference.step(before,command),(0,1))
    packet=json_value(dict(classification='specification-refutation; not a drain',
        false_claim='coalition rounding excess is less than one atom per asset',corrected_bound='less than coalition size per asset',
        coalition=[0,1],genesis=genesis,prefix=prefix,baseline=before,command=command,accounting=accounting,native_prefix=replays,
        native_coverage='full three-command prefix, complete post-states and effects',cli_sha256=hashlib.sha256(cli.read_bytes()).hexdigest()))
    (out/'rounding.json').write_text(json.dumps(packet,indent=2)+'\n')


def verify_rounding(out,cli_sha256):
    from check import require
    from native_check import parse_observation,expected_observation
    packet=json.loads((out/'rounding.json').read_text());genesis,prefix,command=seeds()[1]
    current=genesis;replays=[]
    for i,operation in enumerate((*prefix,command)):
        decision=reference.step(current,operation);require(decision.reason is None,'rounding prefix refused')
        native=json.loads((out/f'prefix-{i}.json').read_text())
        require(parse_observation(native['result']['authority']['example'])==expected_observation(decision),'rounding native outcome mismatch')
        record=json.loads((out/f'prefix-{i}.command.json').read_text())
        require(record['exit_code']==0,'rounding native command failed')
        replays.append(dict(before=current,command=operation,decision=decision));current=decision.post
    before=initialized(seeds()[1]);accounting=endpoint(before,current,(0,1))
    require(packet['native_prefix']==json_value(replays) and packet['accounting']==json_value(accounting),'rounding record mismatch')
    require(packet['genesis']==json_value(genesis) and packet['prefix']==json_value(prefix) and packet['command']==json_value(command) and packet['baseline']==json_value(before),'rounding seed mismatch')
    require(packet['cli_sha256']==cli_sha256,'rounding binary provenance')
    require(str(accounting['rounding_excess'][1])=='170371/154371','rounding refutation changed')
    check_transition(before,command,reference.step(before,command),(0,1))
    return dict(classification='specification-refutation; not a drain',native_prefixes=3,Y_excess=str(accounting['rounding_excess'][1]),corrected_bound='less than coalition size per asset')


def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('mode',choices=['prepare','replay','rounding']);p.add_argument('path',type=Path);p.add_argument('--cli',type=Path);a=p.parse_args()
    if a.mode=='prepare':prepare(a.path)
    elif a.mode=='replay':print(json.dumps(verify_control(json.loads(a.path.read_text()))))
    else:
        if a.cli is None:p.error('--cli required')
        a.path.mkdir(parents=True,exist_ok=True);rounding(a.cli,a.path)


if __name__=='__main__':main()
