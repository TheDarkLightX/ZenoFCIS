#!/usr/bin/env python3
"""Run an explicitly declared finite contract game and retain replayable evidence."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import resource
import shutil
import subprocess
import time

from certificate_check import check
from game_spec import load, description, expand, native_numbers, SCOPE
from solve import certify, Meter

ROOT=Path(__file__).resolve().parents[2]


def write(path,value):
    path.write_text(json.dumps(value,sort_keys=True,indent=2)+'\n')


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def sources():
    files=subprocess.check_output(['git','ls-files','-z','--cached','--others','--exclude-standard'],cwd=ROOT).split(b'\0')
    return {p.decode():digest(ROOT/p.decode()) for p in files if p and (ROOT/p.decode()).is_file()}


def native(binary,request):
    completed=subprocess.run(['nice','-n','19','ionice','-c3',str(binary)],
                             input=json.dumps(request).encode(),capture_output=True,timeout=30,
                             preexec_fn=lambda: resource.setrlimit(resource.RLIMIT_AS,(1<<30,1<<30)))
    if completed.returncode:
        raise ValueError(completed.stderr.decode().strip()[:1000] or 'native process refused')
    return native_numbers(json.loads(completed.stdout))


def driver(args):
    if args.native: return args.native.resolve()
    target=Path(os.environ.get('CARGO_TARGET_DIR',ROOT/'target')).resolve()
    env=dict(os.environ,CARGO_TARGET_DIR=str(target),CARGO_PROFILE_DEV_DEBUG='0',CARGO_PROFILE_TEST_DEBUG='0')
    command=['nice','-n','19','ionice','-c3','cargo','+1.97.1','build','--locked','--offline',
             '-p','zeno-fcis-contract','--example','trace_game_input','-j','3']
    with (args.out/'build.log').open('wb') as log:
        completed=subprocess.run(command,cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT)
    if completed.returncode: raise ValueError('native build failed; see build.log')
    return target/'debug/examples/trace_game_input'


def bind(args,horizon=None):
    binary=args.out/'native-driver'
    artifacts={p.name:digest(p) for p in args.out.iterdir() if p.is_file() and p.name not in ('native-driver','game-binding.json')}
    binding=dict(schema='zeno-fcis/game-binding/1',sources=sources(),artifacts=artifacts,
                 binary=dict(path=str(binary.resolve()),sha256=digest(binary)) if binary.is_file() else None,
                 mode=args.mode,horizon=horizon)
    write(args.out/'game-binding.json',binding)


def run(args):
    if args.out.resolve().is_relative_to(ROOT): raise ValueError('evidence directory must be outside repository')
    args.out.mkdir(parents=True,exist_ok=False)
    args.created=True
    game,contract=load(args.game)
    write(args.out/'game.json',game)
    binary=args.out/'native-driver'
    shutil.copy2(driver(args),binary)
    binary=binary.resolve()
    request={}
    for key,name in (('project','project.zeno'),('policy','v2/policy.json')):
        with (contract/name).open('rb') as stream: raw=stream.read(1048577)
        if len(raw)>1048576: raise ValueError('contract source byte budget')
        request[key]=raw.decode()
    # Take one owned source snapshot before any native invocation. No later
    # filesystem acquisition can change the contract between table rows.
    metadata=native(binary,request)
    events=expand(game,metadata['positions'])
    request.update(events=events,formula=game['objective']['formula'],guards=game.get('guards',['true']))
    write(args.out/'native-request.json',request)
    table_packet=native(binary,request)
    write(args.out/'native-table.json',table_packet)
    table={tuple(row['pre'])+(row['event'],):row for row in table_packet['rows']}
    horizon=game.get('K',6) if args.mode=='check' else None
    spec=description(game,table_packet,horizon=horizon)
    candidates=range(len(spec['guards'])) if args.mode=='select-guard' else (0,)
    attempts=[]; meter=Meter(); product_end=time.monotonic()+30
    states=0; edges=0
    for guard in candidates:
        candidate=dict(spec,guard=guard)
        remaining=max(0,product_end-time.monotonic())
        packet=certify(candidate,table,solver_meter=meter,
                       max_states=10000-states,max_edges=100000-edges,seconds=remaining)
        if packet.get('status')=='UNKNOWN':
            write(args.out/'attempt.json',packet)
            raise ValueError(meter.stop or packet.get('stop','product limit'))
        states+=len(packet['graph']['states']); edges+=sum(map(len,packet['graph']['edges']))
        verdict=check(packet,table)
        if verdict['evidence']!='counterexample' or args.mode!='select-guard': break
        attempts.append(packet)
    else:
        packet=dict(schema='zeno-fcis/game-guard-exhaustion/1',attempts=attempts)
        verdict=check(packet,table)
    write(args.out/'certificate.json',packet)
    verdict.update(guard=spec['guards'][guard],solver_work=meter.work)
    write(args.out/'verdict.json',verdict)
    bind(args,horizon)
    return verdict


def main():
    parser=argparse.ArgumentParser(description=__doc__,formatter_class=argparse.RawDescriptionHelpFormatter,epilog=(
        'Example: nice -n 19 ionice -c3 python3 -B tools/trace_game/game.py certify --game '
        'test-data/trace-games/escrow-safe.json --out ../escrow-game-evidence\n\n'
        'Input: schema="zeno-fcis/game/1"; contract=directory relative to the game file.\n'
        'actors[] = {id:string, compromised:boolean, command:{position:[integer,...]},\n'
        '            context:{position:[integer,...]}}; specify every input position.\n'
        'coalition_bound and compromised_actor_bound bound active actor subsets.\n'
        'objective = {kind:"safety"|"reachability", formula:law-expression,\n'
        '             on?:["Accept","Reject","CommittedFailure","Refused","Blocked"]}.\n'
        'Safety forbids the formula; reachability must force it against every event.\n'
        'All actor events are environment choices. Optional guards are a finite list\n'
        'of pre/command/context law expressions: false blocks into a stutter.\n'
        'certify/check fix guard 0; select-guard chooses a fixed guard before play.\n'
        'pre/post include rejection/refusal stutters; decision_reason is 0 if absent.\n'
        'Optional K=0..1000 bounds check (default 6); observation must be "perfect".\n'
        'Limits: 64 KiB input; 16 actors; 32 guards; 10000 product states; 100000\n'
        'edges; 30 s construction; 1000000 solver work units and 30 s across guards.\n'
        'Retained lineages and unsigned scalars return UNKNOWN. Evidence directories\n'
        'must be fresh and outside the repository. Check evidence independently:\n'
        'nice -n 19 ionice -c3 python3 -B tools/trace_game/certificate_check.py '
        '../escrow-game-evidence certificate.json\n\n'+SCOPE))
    parser.add_argument('mode',choices=('check','certify','select-guard'))
    parser.add_argument('--game',type=Path,required=True)
    parser.add_argument('--out',type=Path,required=True,help='new evidence directory')
    parser.add_argument('--native',type=Path,help='prebuilt trace_game_input executable; otherwise build offline')
    args=parser.parse_args()
    try:
        verdict=run(args); code=0
    except (ValueError,KeyError,TypeError,IndexError,OverflowError,RecursionError,OSError,subprocess.SubprocessError) as error:
        verdict=dict(evidence='UNKNOWN',limit=str(error),claim='no certificate established',scope=SCOPE)
        if getattr(args,'created',False):
            write(args.out/'verdict.json',verdict)
            write(args.out/'certificate.json',dict(schema='zeno-fcis/game-incomplete/1',**verdict))
            bind(args)
        code=2
    print(json.dumps(verdict,sort_keys=True))
    return code


if __name__=='__main__': raise SystemExit(main())
