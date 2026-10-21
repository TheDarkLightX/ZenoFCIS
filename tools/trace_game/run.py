#!/usr/bin/env python3
"""Offline source-bound runner for the internal v1 trace-game fixtures."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

ROOT=Path(__file__).resolve().parents[2]
TARGET=Path(os.environ.get('CARGO_TARGET_DIR',ROOT/'target/trace-game'))


def sha(path):
    h=hashlib.sha256()
    with path.open('rb') as source:
        for part in iter(lambda:source.read(1<<20),b''):h.update(part)
    return h.hexdigest()


def source_files():
    files=subprocess.check_output(['git','ls-files','-z','--cached','--others','--exclude-standard'],cwd=ROOT).split(b'\0')
    result={os.fsdecode(f):sha(ROOT/os.fsdecode(f)) for f in files if f and (ROOT/os.fsdecode(f)).is_file()}
    result['Cargo.lock']=sha(ROOT/'Cargo.lock')
    return result


def verify_binding(binding):
    if source_files()!=binding['source_files']:raise ValueError('source/model/requirements drift invalidates evidence')
    if subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()!=binding['baseline']:raise ValueError('baseline revision drift')
    for name,record in binding['binaries'].items():
        if sha(Path(record['path']))!=record['sha256']:raise ValueError(f'binary drift: {name}')


def command(argv,out,label,timeout=600):
    log=out/f'{label}.log';started=time.monotonic();status='finished'
    env=dict(os.environ,CARGO_TARGET_DIR=str(TARGET),CARGO_PROFILE_DEV_DEBUG='0',CARGO_PROFILE_TEST_DEBUG='0',
             CARGO_PROFILE_DEV_OPT_LEVEL='1',CARGO_PROFILE_TEST_OPT_LEVEL='1',CARGO_BUILD_JOBS='2',
             CARGO_NET_OFFLINE='true',CARGO_TERM_COLOR='never',PYTHONDONTWRITEBYTECODE='1')
    rss=out/f'{label}.rss-kib';measured=['/usr/bin/time','-f','%M','-o',str(rss),*map(str,argv)]
    with log.open('wb') as stream:
        try:code=subprocess.run(measured,cwd=ROOT,env=env,stdout=stream,stderr=subprocess.STDOUT,timeout=timeout,check=False).returncode
        except subprocess.TimeoutExpired:code=124;status='timeout-UNKNOWN'
    record=dict(argv=list(map(str,argv)),peak_rss_kib=int(rss.read_text().strip().splitlines()[-1]) if rss.exists() and rss.read_text().strip().splitlines()[-1].isdigit() else None,rss_log=rss.name,exit_code=code,status=status,seconds=round(time.monotonic()-started,3),log=log.name,log_sha256=sha(log))
    with (out/'commands.jsonl').open('a') as stream:stream.write(json.dumps(record)+'\n')
    print(label,code,record['seconds'],flush=True)
    if code:raise ValueError(f'{label}: exit {code}; see {log}')
    return record


def disk_guard():
    subprocess.run(['df','-B1G','/'],check=True)
    if shutil.disk_usage('/').free<6*1024**3:raise ValueError('STOP: less than 6 GiB free before build')


def replay(out,checks):
    from check import check_packet,require,scalar_relation,table_check,violates
    from spot import verify_control,verify_rounding
    binding=json.loads((out/'binding.json').read_text())
    verify_binding(binding)
    for name,digest in binding['artifacts'].items():require(sha(out/name)==digest,f'artifact drift: {name}')
    results={}
    for path in sorted(out.glob('*.json')):
        if path.name in ('binding.json','replay.json','summary.json','spot-input.json'):continue
        packet=json.loads(path.read_text())
        if packet.get('schema') in ('zeno-fcis/trace-game/1','zeno-fcis/trace-game-spot-control/1','zeno-fcis/trace-game-semantics-control/1'):
            campaign_scope(path.name,packet)
        if packet.get('schema')=='zeno-fcis/trace-game/1':results[path.name]=check_packet(packet,binding,path)
        elif packet.get('schema')=='zeno-fcis/trace-game-spot-control/1':results[path.name]=verify_control(packet)
        elif packet.get('schema')=='zeno-fcis/trace-game-semantics-control/1':results[path.name]=check_semantics(packet)
        print('replayed',path.name,flush=True)
    expected={f'core-{f}.json' for f in ('unauthorized','lawful','reject-all','refusal','wrong-effect','alarm-reset','stale','arithmetic','accounting','deny-withdraw')}
    expected|={f'composition-{f}.json' for f in ('vulnerable','stale','lawful')}
    expected|={f'{f}.json' for f in ('key-only','price-only','joined','work-stop','time-stop','wall-time-stop','frontier-stop','node-stop','unfair-delay','core-next-bound','composition-next-bound')}
    expected|={f'{p}-core-{o}.json' for p in ('control','disabled','unrelated') for o in ('unauthorized-value','stale-approval','accounting-loss')}
    expected|={f'semantics-{f}-{d}.json' for f in ('lawful','rollback-committed','commit-rejected-prefix') for d in ('enabled','composition-frame','unauthorized-commit')}
    expected|={f'spot-{d}.json' for d in ('control','coalition-gain','realized-coalition-gain','composition-frame')}
    require(set(results)==expected,'missing or unexpected campaign packet inventory')
    for name,record in results.items():
        if name.startswith('semantics-'):record['native_provenance']={k:v['sha256'] for k,v in binding['binaries'].items()}
    del packet
    for name in ('core-lawful','composition-lawful','key-only','price-only'):
        require(results[name+'.json']['status']=='checked-through-K',f'{name} not completely checked')
    for name in ('core-reject-all','core-deny-withdraw','core-refusal','core-wrong-effect','core-alarm-reset','core-arithmetic'):
        require('legitimacy' in results[name+'.json']['objectives'],f'{name} escaped X0')
    require('exit-deadline' in results['core-alarm-reset.json']['objectives'],'alarm mutant escaped deadline')
    require('exit-deadline' in results['unfair-delay.json']['objectives'],'unfair delay not accounted')
    for name in ('work-stop','time-stop','wall-time-stop','frontier-stop','node-stop','core-next-bound','composition-next-bound'):require(results[name+'.json']['status']=='UNKNOWN','incomplete credited')
    require('ordered-drain' in results['composition-vulnerable.json']['objectives'],'joint four-step path not rediscovered')
    require('coalition-gain' in results['joined.json']['objectives'],'safe profiles incorrectly closed under join')
    for name in ('core-lawful','composition-lawful'):
        require(json.loads((out/(name+'.json')).read_text())['corpus']['pass'],f'{name} corpus')
    summary=json.loads((out/'summary.json').read_text())
    gaps=json.loads((out/'gap-review.json').read_text())['gaps']
    require(gaps['admissible']['full']['determined']==gaps['admissible']['full']['tuples']==3888
            and gaps['admissible']['full']['reference_refused']==0 and gaps['frame_check']['mismatches']==0
            and gaps['mutation']['full']['admitted']==0 and 991 not in gaps['declared_laws'],'gap judge did not close declared scalar relation')
    check_monitor(summary['monitor'])
    monitor_product=json.loads((out/'core-lawful.json').read_text())
    check_monitor_product(summary['monitor'],monitor_product)
    (checks/'scalar-relation.json').write_text(json.dumps(scalar_relation(monitor_product),indent=2)+'\n')
    del monitor_product
    (checks/'rounding-replay.json').write_text(json.dumps(verify_rounding(out/'rounding',binding['binaries']['cli']['sha256']),indent=2)+'\n')
    (checks/'reviewer-probes.json').write_text(json.dumps(reviewer_probes(out),indent=2)+'\n')
    retained=[]
    for kind,lawful_name in (('withdrawal','core-lawful'),('composition','composition-lawful')):
        candidate=json.loads((out/(lawful_name+'.json')).read_text());candidate_table=table_check(candidate)
        require(candidate['metadata']['powers']==dict(accounts=3,signers=1,oracle_choices=3,approval_ages=3,arbitrary_order=True),'repair reduced powers')
        require(candidate['metadata']['only_objective'] is None and candidate['metadata']['disabled_objective'] is None
                and candidate['metadata']['unfair_delay'] is False,'repair changed objectives or environment')
        for path in sorted(out.glob('core-*.json' if kind=='withdrawal' else 'composition-*.json')):
            old=json.loads(path.read_text())
            # Each packet's complete regression inventory was checked above, before repair replay.
            for record in old['regressions']:
                require(not violates(candidate['metadata'],candidate_table,record,record['original']),'retained exploit survives candidate')
                retained.append(dict(source=path.name,property=record['property'],blocked=True))
    (checks/'permanent-regressions.json').write_text(json.dumps(retained,indent=2)+'\n')
    (checks/'capability-inclusion.json').write_text(json.dumps(capabilities(),indent=2)+'\n')
    require(summary['repair_door']['count']==3,'repair proposal grammar changed')
    require(summary['retained_regression']['candidate_blocks'],'repair did not retain original regression')
    verify_binding(binding)
    (checks/'independent-replay.json').write_text(json.dumps(results,indent=2)+'\n')
    return results


def semantics_expected(state,r):
    from check import result
    if not r[2]:return result(state,200,'counter')
    if state[0]==2:return result(state,201,'counter')
    if r[0]==1:return dict(result([state[0]+1,1,state[2],state[3]],201,'wire'),kind='CommittedFailure')
    if state[3]<r[1]:return result(state,201,'budget')
    return result([state[0]+1,state[1],min(2,state[2]+1),state[3]-r[1]])


def check_semantics(packet):
    from check import require,control_history
    import copy,itertools
    table={}
    for row in packet['table']['rows']:
        require(row['outcome']==semantics_expected(row['state'],row['request']),'native prefix/rollback relation mismatch')
        key=tuple(row['state']+row['request']);require(key not in table,'duplicate semantics row');table[key]=row['outcome']
    expected={s+r for s in itertools.product(range(3),range(2),range(3),range(3)) for r in itertools.product(range(2),(1,2),range(2))}
    require(set(table)==expected,'incomplete semantics table')
    metadata=packet['metadata'];fault=metadata['fault'];disabled=metadata['disabled']
    require(fault in ('lawful','rollback-committed','commit-rejected-prefix') and disabled in (None,'composition-frame','unauthorized-commit'),'semantics scope')
    def enabled(n):return [dict(actor=0,request=list(r)) for r in itertools.product(range(2),(1,2),range(2))]
    def step(n,action):
        before=n['snapshot'];request=action['request'];actual=copy.deepcopy(table[tuple(before['state']+request)])
        if fault=='rollback-committed' and actual['kind']=='CommittedFailure':actual['post']=list(before['state'])
        if fault=='commit-rejected-prefix' and actual['kind']=='Reject' and actual['component']=='budget':
            actual['post'][0]+=1;actual['post'][2]=min(2,actual['post'][2]+1)
        violations=[]
        if disabled!='composition-frame' and actual!=semantics_expected(before['state'],request):violations.append('composition-frame')
        if disabled!='unauthorized-commit' and not request[2] and actual['post']!=before['state']:violations.append('unauthorized-commit')
        after=copy.deepcopy(before);after['state']=actual['post'];after['time']+=1
        return after,actual,violations
    checked=control_history(packet,[0,0,0,1],2,enabled,step,lambda n:0)
    checked.update(rows=len(table),scope=metadata)
    return checked


FULL=dict(accounts=3,signers=1,oracle_choices=3,approval_ages=3,arbitrary_order=True)

def campaign_scope(name,packet):
    """Frozen per-artifact contract; metadata is not evidence for its own scope."""
    from check import require
    stem=Path(name).stem;schema=packet['schema']
    if schema=='zeno-fcis/trace-game/1':
        model='composition' if stem.startswith('composition-') or stem in ('key-only','price-only','joined') else 'withdrawal'
        fault='lawful';K=4;only=None;disabled=None;unfair=stem=='unfair-delay';power=dict(FULL)
        if stem in ('core-next-bound','composition-next-bound'):K=5
        elif stem.startswith(('core-','composition-')):fault=stem.split('-',1)[1]
        elif stem in ('key-only','price-only','joined'):
            fault='vulnerable';only='coalition-gain'
            if stem=='key-only':power['oracle_choices']=1
            if stem=='price-only':power['signers']=0
        elif stem.startswith(('control-','disabled-','unrelated-')):
            prefix,kind,objective=stem.split('-',2);require(kind=='core','unexpected control model')
            fault={'unauthorized-value':'unauthorized','stale-approval':'stale','accounting-loss':'accounting'}[objective];K=1
            disabled=objective if prefix=='disabled' else 'oracle-integrity' if prefix=='unrelated' else None
        else:require(stem in ('work-stop','time-stop','wall-time-stop','frontier-stop','node-stop','unfair-delay'),'unknown credited packet')
        actual={k:packet['metadata'][k] for k in ('model','fault','horizon','powers','only_objective','disabled_objective','unfair_delay')}
        expected=dict(model=model,fault=fault,horizon=K,powers=power,only_objective=only,disabled_objective=disabled,unfair_delay=unfair)
        require(packet['metadata']['alphabet']=='full-callers-normalized','campaign alphabet mismatch')
        require(packet['limits']['nodes']==(7 if stem=='node-stop' else 200_000) and packet['limits']['frontier']==(7 if stem=='frontier-stop' else 120_000),'campaign resource budget mismatch')
        require(actual==expected and packet['limits']['horizon']==K,f'campaign scope mismatch: {name}')
        if model=='withdrawal':require(packet['metadata']['deadline']==dict(events=2,request_start_at_most=max(0,min(4,K-2)),max_processing_postponement=1),'deadline scope mismatch')
    elif schema=='zeno-fcis/trace-game-semantics-control/1':
        expected={f'semantics-{f}-{d}.json':(f,None if d=='enabled' else d) for f in ('lawful','rollback-committed','commit-rejected-prefix') for d in ('enabled','composition-frame','unauthorized-commit')}
        require(name in expected,'unknown semantics packet');f,d=expected[name];m=packet['metadata']
        require(m['fault']==f and m['disabled']==d and m['horizon']==2 and m['model']=='failure-semantics' and m['powers']=='three coalitions; all eight commands' and m['only_objective'] is None and m['unfair_delay'] is False,'semantics declared scope mismatch')
    else:
        expected={f'spot-{d}.json':None if d=='control' else d for d in ('control','coalition-gain','realized-coalition-gain','composition-frame')}
        require(name in expected and packet['input']['only_objective'] is None and packet['input']['disabled_objective']==expected[name] and packet['input']['provenance']['K']==3,'Spot declared scope mismatch')
    return True


def reviewer_probes(out):
    """Default must-refuse probes derived from all four independent reviews."""
    from check import check_history,check_packet,require
    from spot import verify_control
    import sys
    results=[]
    def load(name):return json.loads((out/name).read_text())
    def refuse(name,packet,change,checker):
        # Mutation functions replace fields; no need to duplicate a million-node tree.
        change(packet)
        try:checker(packet)
        except (ValueError,KeyError,IndexError) as error:results.append(dict(probe=name,outcome='refused',reason=str(error)));return
        raise ValueError(f'reviewer forgery accepted: {name}')
    sem='semantics-rollback-committed-enabled.json'
    refuse('Sol/Astra empty semantics witness inventory',load(sem),lambda p:p.update(witnesses=[],regressions=[],run=dict(p['run'],witnesses={},status='checked-through-K')),check_semantics)
    refuse('Sol replayed=true empty semantics trace',load(sem),lambda p:p['witnesses'][0].update(trace=[],replayed=True),check_semantics)
    refuse('Astra rejected-prefix false clean verdict',load('semantics-commit-rejected-prefix-enabled.json'),lambda p:p.update(witnesses=[],run=dict(p['run'],witnesses={},status='checked-through-K')),check_semantics)
    refuse('Sol moved Spot indices',load('spot-control.json'),lambda p:p['run'].update(witnesses={k:0 for k in p['run']['witnesses']}),verify_control)
    refuse('Astra Spot no nodes',load('spot-control.json'),lambda p:p['run'].update(nodes=[],maximum_loss=0,maximum_loss_node=None),verify_control)
    refuse('Astra omitted original/minimized regression',load('control-core-unauthorized-value.json'),lambda p:p.update(regressions=[]),check_history)
    for name in ('core-lawful.json','key-only.json','price-only.json','joined.json'):
        refuse(f'adversarial zero-age scope {name}',load(name),lambda p:p['metadata']['powers'].update(approval_ages=0),lambda p:campaign_scope(name,p))
    refuse('adversarial K1 lawful scope',load('core-lawful.json'),lambda p:(p['metadata'].update(horizon=1),p['limits'].update(horizon=1)),lambda p:campaign_scope('core-lawful.json',p))
    p=load('work-stop.json');p['metadata']['powers']['approval_ages']=0
    p['run'].update(nodes=p['run']['nodes'][:6],witnesses={},transitions=0,completed_requests=0,maximum_loss=0,maximum_loss_node=None)
    refuse('adversarial empty alphabet history',p,lambda p:None,check_history)
    for field,value in (('fault','lawful'),('model','composition'),('only_objective','stale-approval'),('disabled_objective','unauthorized-value'),('unfair_delay',True)):
        refuse(f'lead scope coordinate {field}',load('control-core-unauthorized-value.json'),lambda p,k=field,v=value:p['metadata'].__setitem__(k,v),lambda p:campaign_scope('control-core-unauthorized-value.json',p))
    p=load('work-stop.json');p['run'].update(nodes=p['run']['nodes'][:6],status='checked-through-K',stop='exhausted-declared-prefixes',completed_depth=4,transitions=0,witnesses={})
    refuse('adversarial zero interior transitions with callable commands',p,lambda p:None,check_history)
    refuse('adversarial test specimen native claim',load('control-core-accounting-loss.json'),lambda p:None,check_packet)
    require('search_compounding' not in sys.modules,'replay imported archived search module')
    results.append(dict(probe='Sol search-module import',outcome='absent'))
    # Legitimate permissionless keeper witness: paid is the owner entitlement.
    from check import initial,advance,core,moves
    p=load('work-stop.json');state=initial([2,0,0,0]);trace=[]
    for action in (dict(actor=0,request=[0,0,0,1]),dict(actor=1,request=[1,1,0,1])):
        require(action in moves(p['metadata'],state,3),'keeper probe absent from alphabet')
        state,outcome,violations,_=advance(p['metadata'],state,action,core(state['state'],action['request']))
        require(not violations,'keeper incorrectly accused');trace.append(dict(action=action,outcome=outcome,violations=violations))
    results.append(dict(probe='adversarial permissionless keeper',outcome='owner paid; no unauthorized recipient',trace=trace))
    for objective in ('unauthorized-value','stale-approval','accounting-loss'):
        for prefix in ('control','disabled','unrelated'):
            p=load(f'{prefix}-core-{objective}.json');r=check_history(p)
            require(r['status']==('checked-through-K' if prefix=='disabled' else 'counterexample'),'non-load-bearing objective control')
            results.append(dict(probe=f'objective {objective} {prefix}',outcome=r['status'],objectives=r['objectives']))
    for objective in ('unauthorized-value','stale-approval','accounting-loss'):
        packets=[load(f'{prefix}-core-{objective}.json') for prefix in ('control','disabled','unrelated')]
        baseline=packets[0]
        require(baseline['run']['witnesses'].keys()=={objective},'control violates another objective')
        for other in packets[1:]:
            require(other['metadata']['only_objective'] is None and baseline['table']==other['table'],'control changed native relation/filter')
            require([{k:v for k,v in n.items() if k!='violations'} for n in baseline['run']['nodes']]==[{k:v for k,v in n.items() if k!='violations'} for n in other['run']['nodes']],'disabled check changed behavior')
        results.append(dict(probe=f'{objective} differential control',outcome='identical native table, actions, states, outcomes; target alone disabled'))
    for objective in ('coalition-gain','realized-coalition-gain','composition-frame'):
        r=verify_control(load(f'spot-{objective}.json'))
        require(r['status']=='counterexample','overlapping Spot objective incorrectly escaped')
        results.append(dict(probe=f'Spot disables only {objective}',outcome=r['status'],objectives=r['objectives']))
    for fault in ('rollback-committed','commit-rejected-prefix'):
        for disabled in ('composition-frame','unauthorized-commit'):
            r=check_semantics(load(f'semantics-{fault}-{disabled}.json'))
            require(r['status']==('checked-through-K' if disabled=='composition-frame' else 'counterexample'),'semantics load-bearing check')
            results.append(dict(probe=f'{fault} disables only {disabled}',outcome=r['status'],objectives=r['objectives']))
    # Powers join is checked here independently of the Rust harness.
    key=load('key-only.json')['metadata']['powers'];price=load('price-only.json')['metadata']['powers']
    joined={k:(key[k] or price[k]) if k=='arbitrary_order' else key[k]|price[k] for k in FULL}
    require(joined==FULL==load('joined.json')['metadata']['powers'],'declared profiles do not join to full powers')
    results.append(dict(probe='lead/adversarial exact capability join',outcome='full powers'))
    return results


def check_monitor(monitor):
    from check import require
    import itertools
    require(monitor['adopted'] is False and monitor['states']==2,'monitor adoption/minimality')
    # Independently exclude every smaller total DFA with Boolean output.
    counts=[]
    words=[word for n in range(7) for word in itertools.product(range(2),repeat=n)]
    for states in range(1,monitor['states']):
        tested=0
        for transition in itertools.product(range(states),repeat=2*states):
            for output in itertools.product((False,True),repeat=states):
                tested+=1;fits=True
                for word in words:
                    q=0
                    for e in word:q=transition[2*q+e]
                    required=len(tuple(itertools.takewhile(lambda e:e==0,reversed(word))))<1
                    if output[q]!=required:fits=False;break
                require(not fits,'smaller monitor survived')
        counts.append(tested)
    require([r['total_machines'] for r in monitor['excluded_smaller']]==counts,'monitor exclusion counts')
    for word in words:
        q=0
        for e in word:q=monitor['transition'][2*q+e]
        require(monitor['allow_delay_outputs'][q]==(len(tuple(itertools.takewhile(lambda e:e==0,reversed(word))))<1),'monitor product')


def check_monitor_product(monitor,packet):
    from check import require
    states=[]
    for node in packet['run']['nodes']:
        q=0
        if node['parent'] is not None:
            parent=packet['run']['nodes'][node['parent']]
            q=states[node['parent']]
            if node['snapshot']['pending'] is None or (parent['snapshot']['pending'] is None and node['snapshot']['pending'] is not None):q=monitor['transition'][2*q+1]
            elif parent['snapshot']['pending'] is not None:q=monitor['transition'][2*q]
        states.append(q)
        pending=node['snapshot']['pending']
        expected=pending is None or node['snapshot']['time']==pending['submitted']
        require(monitor['allow_delay_outputs'][q]==expected,'core/monitor full product mismatch')
    require(packet['run']['status']=='checked-through-K' and not packet['run']['witnesses'],'monitor product did not pass same game checks')


def capabilities():
    from check import require,initial,starts,moves,advance,core,composition
    import itertools
    profiles=[]
    for accounts,signers,oracle,ages,ordering in itertools.product((1,3),(0,1),(1,3),(1,3),(False,True)):
        profiles.append(dict(accounts=accounts,signers=signers,oracle_choices=oracle,approval_ages=ages,arbitrary_order=ordering))
    def includes(a,b):
        return all(a[k]&b[k]==b[k] for k in ('accounts','signers','oracle_choices','approval_ages')) and (a['arbitrary_order'] or not b['arbitrary_order'])
    checked=0;languages={}
    for model in ('withdrawal','composition'):
        for i,power in enumerate(profiles):
            metadata=dict(model=model,powers=power,unfair_delay=False,only_objective=None,disabled_objective=None)
            language=set();frontier=[(initial(s),c,(("start",tuple(s)),)) for s in starts(model) for c in (1,2,3)]
            for depth in range(4):
                following=[]
                for state,coalition,trace in frontier:
                    language.add((coalition,trace))
                    if depth==3:continue
                    for action in moves(metadata,state,coalition):
                        native=(core if model=='withdrawal' else composition)(state['state'],action['request'])
                        successor,_,_,_=advance(metadata,state,action,native)
                        following.append((successor,coalition,trace+((action['actor'],tuple(action['request'])),)))
                frontier=following
            languages[model,i]=language
        for i,a in enumerate(profiles):
            for j,b in enumerate(profiles):
                if includes(a,b):
                    require(languages[model,j] <= languages[model,i],'capability trace languages are not nested')
                    checked+=1
    return dict(profiles=len(profiles),models=2,complete_prefix_horizon=3,subset_comparisons=checked,
                semantics='same transition/objectives/environment; no safe-join inference; explicit safe-profiles/unsafe-join run retained')


def main():
    from guide_help import example_help
    p=argparse.ArgumentParser(description=__doc__, epilog=example_help("trace-game"), formatter_class=argparse.RawDescriptionHelpFormatter);p.add_argument('--out',required=True,type=Path);p.add_argument('--replay-only',action='store_true');a=p.parse_args()
    out=a.out.resolve()
    if not a.replay_only and out.exists() and any(out.iterdir()):raise ValueError('retain prior attempts; choose a new empty output directory')
    out.mkdir(parents=True,exist_ok=True);checks=out/'checks';checks.mkdir(exist_ok=True)
    if a.replay_only:replay(out,checks);return
    source=source_files();disk_guard()
    command(['cargo','+1.97.1','build','--locked','--offline','-p','zeno-fcis-cli','--example','trace_game','--bin','zeno-fcis','-j','2'],checks,'build')
    if source_files()!=source:raise ValueError('source changed during build')
    binaries={name:dict(path=str(path),sha256=sha(path)) for name,path in (
        ('harness',TARGET/'debug/examples/trace_game'),('cli',TARGET/'debug/zeno-fcis'))}
    command([sys.executable,'-B','tools/trace_game/spot.py','prepare',out/'spot-input.json'],checks,'spot-prepare')
    command([binaries['harness']['path'],out],checks,'native-games',timeout=1800)
    sources=json.loads((out/'core-lawful.json').read_text())['table']['sources']
    app=out/'gap-app';(app/'v2').mkdir(parents=True,exist_ok=True)
    (app/'project.zeno').write_text(sources['project']);(app/'v2/policy.json').write_text(sources['rules'])
    command([binaries['cli']['path'],'--no-kb','contract','review',app,'--gaps','--max-tuples','3888','--out',out/'gap-review.json'],checks,'gap-judge',timeout=120)
    command([sys.executable,'-B','tools/trace_game/spot.py','rounding',out/'rounding','--cli',binaries['cli']['path']],checks,'rounding-native',timeout=120)
    binding=dict(schema='zeno-fcis/trace-game-binding/1',baseline=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
        source_files=source,binaries=binaries,build_profile=dict(toolchain='1.97.1',locked=True,offline=True,debug=0,jobs=2,opt_level=1),artifacts={str(path.relative_to(out)):sha(path) for path in out.rglob('*') if path.is_file() and 'checks' not in path.parts and path.name!='binding.json'})
    (out/'binding.json').write_text(json.dumps(binding,indent=2)+'\n')
    replay(out,checks)
    print('checked up to K under the declared capabilities: core K=4, composition K=4, Spot oracle K=3; core deadlines L=2 for submissions through K-2, later requests open; blind spots: extra identities/keys, normalized argument exclusions, real scheduler/markets, external funding, native recipient/delivery and composition caller authority, no-value unauthorized Accepts, unbounded safety/progress. Generic drain objectives use no path hint; ordered-drain encodes the path. Both native models hit a retained resource stop at K=5. Earlier K=6 results apply only to the separately retained narrow alphabet excluding actor-1 Process, Alarm, Tick and stale Withdraw.')


if __name__ == "__main__":
    import sys
    from pathlib import Path
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
    try:
        from telemetry import run_cli
    except ImportError:
        # Reporting is optional, including for a standalone copy of this tool.
        run_cli = lambda function: function()
    def _entrypoint():
        main()
    run_cli(_entrypoint)
