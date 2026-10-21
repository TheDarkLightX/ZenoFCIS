#!/usr/bin/env python3
"""Independent certificate verification. Standard library only; no search imports."""
import argparse
import hashlib
import itertools
import json
from pathlib import Path
import subprocess
from game_check import check as check_game, load_bound as load_game_bound

GRAMMAR = ('true', 'actor == 0', 'false')
ORACLE_RESPONSES = ('wait','settle','cancel')
POWERS = dict(accounts=3, signers=1, oracle_choices=3, approval_ages=3, arbitrary_order=True)
COMPOSITION = ((0,1,0,0,0),(0,1,1,0,0),(1,1,0,1,0),(1,2,0,0,0),
               (2,0,0,0,0),(2,0,0,0,1),(3,0,0,0,0),(4,0,0,0,0),(5,0,0,0,0))


def need(condition, message):
    if not condition:
        raise ValueError(message)


def valid_index(value, size):
    return type(value) is int and 0 <= value < size


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def outcome(s, reason=None, component=None, refused=False):
    return dict(kind='Refused' if refused else 'Reject' if reason is not None else 'Accept',
                reason=reason, component=component, post=list(s), events=[])


def reference(model, s, r, guard='actor == 0'):
    """Own complete finite requirement relation, including guarded native variants."""
    post=list(s); reason=None; component=None
    if model == 'withdrawal':
        b,p,v,a=s; op,actor,age,allowed=r
        permit = guard=='true' or (guard=='actor == 0' and actor==0)
        if not allowed or (op==0 and actor!=0) or (op==3 and not permit): reason=200
        elif op==3 and age>0: reason=201
        elif b+v>2 or (p and b==0): reason=203
        elif (op in (0,3) and (b==0 or p)) or (op==1 and not p): reason=202
        elif op==0: post[1]=1
        elif op in (1,3): post=[b-1,0,v+1,0]
        elif op==2: post[3]=1
        else: post[3]=0
    else:
        admin,listed,price,reserve,paid,debt=s; op,keys,age,bad,skew=r
        if op==0:
            component='governance'
            if keys<2: reason=200
            elif age: reason=201
            else: post[0]=1
        elif op==1:
            component='collateral'
            if bad: reason=203
            elif keys<2: reason=200
            else: post[1]=1
        elif op==2:
            component='oracle'
            if listed==0: reason=202
            elif skew: reason=203
            else: post[2]=0
        elif op in (3,4):
            component='vault'
            if reserve==0: reason=202
            elif op==3 and not (listed==2 and price): reason=203
            elif op==3: post[3:]=[0,paid+reserve,debt+reserve]
            else: post[3:5]=[reserve-1,paid+1]
        if reason is None and any(v>2 for v in post[3:]):
            return outcome(s, component='vault', refused=True)
    if reason is not None:
        return outcome(s, reason, component)
    if any(v<0 or v>bound for v,bound in zip(post,(2,1,2,1) if model=='withdrawal' else (1,2,1,2,2,2))):
        return outcome(s, refused=True)
    return outcome(post)


def validate_tables(packets):
    tables={}
    for name,model,guard in (('lawful','withdrawal',GRAMMAR[1]),('guard-true','withdrawal',GRAMMAR[0]),
                             ('guard-false','withdrawal',GRAMMAR[2]),('lawful-composition','composition',GRAMMAR[1])):
        packet=packets[name]; table={}
        for row in packet['table']['rows']:
            s,r=row['state'],row['request']; key=tuple(s+r)
            need(all(type(v) is int for v in s+r+row['outcome']['post']),'native integer encoding')
            need(key not in table,'duplicate native row')
            need(row['outcome']==reference(model,s,r,guard),'independent native relation')
            need(bool(row['complete_native']),'missing native outcome bytes')
            need(row['refusal_repeated'] if row['outcome']['kind']=='Refused' else row['publication_replayed'],
                 'unreplayed native outcome')
            table[key]=row['outcome']
        if model=='withdrawal':
            expected=set(itertools.product(range(3),range(2),range(3),range(2),range(6),range(3),range(3),range(2)))
            init=packet['table']['initialization']
            need(all(init.get(k) is True for k in ('replayed','wrong_genesis_refused','altered_subject_refused')),'native initialization')
        else:
            expected={s+r for s in itertools.product(range(2),range(3),range(2),range(3),range(3),range(3)) for r in COMPOSITION}
            need(packet['table']['composition_check'] is not None,'original composition obligations')
            need(packet['table']['initialization']['altered_binding_refused'] is True,'composition binding replay')
        need(set(table)==expected,'incomplete native table')
        tables[name]=table
    return tables


def validate_spec(spec):
    need(spec['model'] in ('withdrawal','composition','quantifier','oracle-withholding'),'unsupported model')
    oracle=spec['model']=='oracle-withholding'
    need(spec['guard'] in (ORACLE_RESPONSES if oracle else GRAMMAR),'guard outside grammar')
    if oracle: need(spec['reactive'] is True,'declared controller class')
    need(type(spec['reactive']) is bool and type(spec['empty']) is bool,'model flags')
    need(set(spec['powers'])==set(POWERS)
         and all(type(spec['powers'][k]) is type(v) and spec['powers'][k]==v for k,v in POWERS.items())
         and spec['unfair_delay'] is False and spec['only_objective'] is None
         and spec['disabled_objective'] is None,'frozen capabilities/objectives/environment')
    need(spec['move_order']=='environment first, controller responds','observation order')
    need(spec['observations']==(['oracle','remaining','request'] if oracle else ['actor','request'])
         and spec['strategy_grammar']==list(ORACLE_RESPONSES if oracle else GRAMMAR),'observation/strategy grammar')


def initial_states(spec):
    if spec['model']=='oracle-withholding': return [[[0,0],[0,0],1,1,2,0,False]]
    if spec['model']=='quantifier': return [[[0,h],[0,h],1,0,0,0,False] for h in (0,1)]
    starts=([1,0,0,0],[2,0,0,0]) if spec['model']=='withdrawal' else ([0,0,0,2,0,0],)
    return [[s[:],s[:],c,0,0,0,False] for s in starts for c in (1,2,3)]


def is_terminal(spec,s):
    return s[-1] or (spec['model'] in ('quantifier','oracle-withholding') and s[0][0]==1)


def permitted(spec, s):
    native,ref,coalition,pending,left,stage,bad=s
    if is_terminal(spec,s) or spec['empty']: return []
    if spec['model'] in ('quantifier','oracle-withholding'): return [(0,[0]),(0,[1])]
    if spec['model']=='withdrawal':
        if pending and left==1: return [(0,[1,0,0,1])]
        return [(a,[op,a,age,1]) for op in range(5) for a in (0,1)
                for age in ((0,1) if op==3 else (0,)) if a==0 or coalition & (1<<a)]
    return [(a,list(r)) for r in COMPOSITION for a in ([2] if r[0]<2 else [a for a in (0,1) if coalition & (1<<a)])]


def successor(spec,tables,s,event,guard):
    n,ref,c,p,left,stage,bad=s; actor,r=permitted(spec,s)[event]; op=r[0]
    if spec['model']=='oracle-withholding':
        available=int(bool(n[1] or r[0]==0))
        settled=guard=='settle' and available
        remaining=0 if settled else left-1
        bad=guard=='cancel' or (guard=='settle' and not available) or (not settled and remaining==0)
        post=[int(bool(settled)),available]
        return [post,post[:],1,0 if settled else 1,remaining,0,bool(bad)]
    if spec['model']=='quantifier':
        post=[1,n[1]]
        value=guard=='true' or (guard=='actor == 0' and actor==0)
        return [post,post[:],1,0,0,0,value != (r[0]==0)]
    table='lawful-composition' if spec['model']=='composition' else dict(zip(GRAMMAR,('guard-true','lawful','guard-false')))[guard]
    actual=tables[table][tuple(n+r)]; approved=reference(spec['model'],ref,r)
    post=actual['post']; required=approved['post']; accept=actual['kind']=='Accept'
    unsafe=approved['kind']=='Accept' and (not accept or post!=required or actual['reason']!=approved['reason'])
    if spec['model']=='withdrawal':
        unsafe |= (post[2]>n[2] and op==3 and actor!=0) or (accept and op==3 and r[2]>0) or sum(post[::2])!=sum(n[::2])
        if approved['kind']=='Accept' and op==0: p,left=required[2]+1,2
        elif p:
            if accept and op in (1,3) and post[2]>=p: p,left=0,0
            else: left=max(0,left-1); unsafe |= left==0
    else:
        unsafe |= (accept and op==0 and (r[1]<2 or r[2]>0)) or post[1]==2 or post[2]!=0
        unsafe |= sum(post[3:5])!=sum(n[3:5]) or post[5]>0
        if accept and ((stage==0 and op==0) or (stage==1 and op==1 and r[3])
                       or (stage==2 and op==2 and r[4]) or (stage==3 and op==3)): stage+=1
        unsafe |= stage==4
    return [post[:],required[:],c,p,left,stage,bool(unsafe)]


def key(s):
    return json.dumps(s,separators=(',',':'))


def checked_graph(packet,tables):
    spec=packet['spec']; validate_spec(spec); g=packet['graph']
    need(g['status']=='complete','incomplete product: UNKNOWN')
    states=g['states']; edges=g['edges']; n=len(states)
    need(0<n<=10000 and len(edges)==n and sum(map(len,edges))<=100000,'product size')
    need(all(valid_index(i,n) for i in g['roots']),'root index encoding')
    for s in states:
        need(type(s) is list and len(s)==7 and type(s[-1]) is bool,'product state encoding')
        need(all(type(v) is int for v in s[0]+s[1]+s[2:6]),'product integer encoding')
    index={key(s):i for i,s in enumerate(states)}
    need(len(index)==n,'duplicate product state')
    expected_roots=[index.get(key(s)) for s in initial_states(spec)]
    need(None not in expected_roots and g['roots']==expected_roots,'initial product membership')
    for i,s in enumerate(states):
        allowed=permitted(spec,s)
        need(allowed or is_terminal(spec,s),'non-vacuity: empty environment gives UNKNOWN')
        for event,guard,target in edges[i]:
            need(valid_index(event,len(allowed)),'edge event index encoding')
            need(valid_index(target,n),'edge target index encoding')
        expected=[]
        for event in range(len(allowed)):
            for guard in (tuple(spec['strategy_grammar']) if spec['reactive'] else (spec['guard'],)):
                target=index.get(key(successor(spec,tables,s,event,guard)))
                need(target is not None,'missing product successor')
                expected.append([event,guard,target])
        need(edges[i]==expected,'complete permitted move coverage')
    # No arbitrary extra state may change safety or losing-region claims.
    reachable=set(g['roots']); todo=list(reachable)
    for i in todo:
        for _,_,target in edges[i]:
            if target not in reachable: reachable.add(target); todo.append(target)
    need(len(reachable)==n,'unreachable product state')
    return spec,states,edges


def check(packet,tables):
    if packet['schema'] in ('zeno-fcis/game-guard-exhaustion/1','zeno-fcis/game-incomplete/1') or packet.get('spec',{}).get('model')=='contract':
        return check_game(packet,tables)
    if packet['schema']=='zeno-fcis/trace-game-guard-exhaustion/1.1':
        need(packet['grammar']==list(GRAMMAR) and len(packet['attempts'])==3,'guard grammar exhaustion')
        subject=None
        for guard,attempt in zip(GRAMMAR,packet['attempts']):
            need(attempt['spec']['guard']==guard and not attempt['spec']['reactive'],'fixed guard quantifier')
            frozen=dict(attempt['spec']); frozen.pop('guard')
            if subject is None: subject=frozen
            need(frozen==subject,'guard exhaustion subject mismatch')
            need(attempt['certificate']['kind']=='counterexample','missing guard refutation')
            check(attempt,tables)
        return dict(evidence='exhausted fixed grammar',claim='no guard in G',grammar=list(GRAMMAR),model=subject['model'])
    need(packet['schema']=='zeno-fcis/trace-game-certificate/1.1','certificate schema')
    spec,states,edges=checked_graph(packet,tables); cert=packet['certificate']
    if cert['kind']=='unrealizability':
        rows=cert['ranks']; rank={}; strategy={}
        for i,r,event in rows:
            need(valid_index(i,len(states)) and i not in rank,'ranked state encoding')
            need(type(r) is int and r>=0,'rank encoding')
            rank[i]=r; strategy[i]=event
        need(valid_index(cert['root'],len(states)),'unrealizability root index encoding')
        need(cert['root'] in packet['graph']['roots'] and cert['root'] in rank,'ranked initial state')
        for i,r in rank.items():
            if r==0:
                need(states[i][-1] and strategy[i] is None,'rank zero must be Bad')
            else:
                need(not states[i][-1],'Bad must have rank zero')
                event=strategy[i]
                need(valid_index(event,len(permitted(spec,states[i]))),'permitted environment strategy')
                targets=[t for e,_,t in edges[i] if e==event]
                need(targets and all(t in rank and rank[t]<r for t in targets),'rank step does not decrease for every response')
        return dict(evidence='certified unrealizability',
                    claim='no controller in the declared class satisfies the requirement under the declared model',
                    controller_class=spec['strategy_grammar'] if spec['reactive'] else [spec['guard']],
                    model=spec['model'],initial_rank=rank[cert['root']])
    if cert['kind']=='counterexample':
        need(valid_index(cert['root'],len(states)),'counterexample root index encoding')
        need(not spec['reactive'] and cert['root'] in packet['graph']['roots'],'fixed counterexample root')
        at=cert['root']
        for event,guard in cert['moves']:
            need(valid_index(event,len(permitted(spec,states[at]))),'counterexample event index encoding')
            need(guard==spec['guard'],'counterexample guard')
            targets=[t for e,g,t in edges[at] if (e,g)==(event,guard)]
            need(len(targets)==1,'counterexample permitted move'); at=targets[0]
        need(states[at][-1],'counterexample has no violation')
        return dict(evidence='counterexample',scope='this fixed guard',model=spec['model'])
    need((cert['kind'] in ('safety','guard') and not spec['reactive'])
         or (cert['kind']=='strategy' and spec['reactive'] and spec['model']!='oracle-withholding'),'evidence kind')
    policy={}
    if cert['kind']=='strategy':
        strategy=cert['strategy']
        need(set(strategy)=={'grammar','reads','order','rules'},'strategy grammar fields')
        need(strategy['reads']==['actor','request'],'strategy reading hidden variable')
        need(strategy['order']==spec['move_order'],'strategy observation order')
        need(strategy['grammar']=='observation-table/1','strategy outside grammar')
        # Derive the domain from independently checked permitted moves, across
        # the whole product rather than restricting it to the submitted W.
        domain={(actor,tuple(request)) for s in states for actor,request in permitted(spec,s)}
        for rule in strategy['rules']:
            need(set(rule)=={'actor','request','guard'},'strategy reading hidden variable')
            need(type(rule['actor']) is int and all(type(v) is int for v in rule['request']),'strategy observation encoding')
            obs=(rule['actor'],tuple(rule['request']))
            need(obs in domain,'strategy observation outside declared domain')
            need(obs not in policy and rule['guard'] in GRAMMAR,'strategy outside grammar')
            policy[obs]=rule['guard']
    def chosen(i,event):
        if cert['kind']!='strategy': return spec['guard']
        a,r=permitted(spec,states[i])[event]
        need((a,tuple(r)) in policy,'strategy is not total on W')
        return policy[a,tuple(r)]
    if cert['kind']=='guard':
        need(cert['guard'] in GRAMMAR and cert['guard']==spec['guard'] and cert['grammar']==list(GRAMMAR),'guard outside grammar')
    winning=cert['W']
    need(all(valid_index(i,len(states)) for i in winning),'winning set encoding')
    need(winning==sorted(set(winning)),'winning set encoding')
    W=set(winning)
    need(set(packet['graph']['roots'])<=W,'initial membership in W')
    for i in winning:
        need(not states[i][-1],'unsafe state inside W')
        need(edges[i] or is_terminal(spec,states[i]),'non-vacuity: empty environment gives UNKNOWN')
        need(all(t in W for e,g,t in edges[i] if g==chosen(i,e)),'closure under every permitted move')
    # One specified reference-corpus journey from EACH initial state inside W.
    for root in packet['graph']['roots']:
        at=root
        requests=([0],) if spec['model']=='quantifier' else ([0,0,0,1],[1,0,0,1]) if spec['model']=='withdrawal' else (
            [1,2,0,0,0],[2,0,0,0,0],[4,0,0,0,0],[4,0,0,0,0])
        for request in requests:
            options=permitted(spec,states[at]); event=next((i for i,(actor,r) in enumerate(options) if r==request and (spec['model']!='withdrawal' or actor==0)),None)
            need(event is not None,'legitimate journey not permitted: UNKNOWN')
            at=next(t for e,g,t in edges[at] if e==event and g==chosen(at,event))
            need(at in W,'legitimate journey outside W')
        need((is_terminal(spec,states[at]) if spec['model']=='quantifier' else states[at][0][2 if spec['model']=='withdrawal' else 4]>0 and states[at][3]==0),'legitimate journey incomplete: UNKNOWN')
    return dict(evidence='certified winning strategy' if cert['kind']=='strategy' else 'certified guard' if cert['kind']=='guard' else 'certified safety',
                claim='strategy sigma, in grammar Sigma, is certified winning' if cert['kind']=='strategy' else f"guard {spec['guard']}, from grammar G, is certified safe" if cert['kind']=='guard' else 'safe on every finite history of the declared model',
                model=spec['model'], states=len(states), winning=len(W), roots=len(packet['graph']['roots']))


def load_bound(directory):
    binding=json.loads((directory/'binding.json').read_text()); root=Path(__file__).resolve().parents[2]
    need(subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()==binding['revision'],'source revision drift')
    files=subprocess.check_output(['git','ls-files','-z','--cached','--others','--exclude-standard'],cwd=root).split(b'\0')
    current={p.decode():digest(root/p.decode()) for p in files if p and (root/p.decode()).is_file()}
    current['Cargo.lock']=digest(root/'Cargo.lock')
    need(current==binding['sources'],'source/model/checker drift')
    need(digest(Path(binding['binary']['path']))==binding['binary']['sha256'],'native binary drift')
    for path,sha in binding['artifacts'].items(): need(digest(directory/path)==sha,'artifact drift')
    return validate_tables({name:json.loads((directory/'tables'/f'{name}.json').read_text())
                            for name in ('lawful','guard-true','guard-false','lawful-composition')})


def main():
    try:
        from guide_help import example_help
    except ImportError:
        example_help = lambda _: ""
    parser=argparse.ArgumentParser(description=__doc__, epilog=example_help("trace-game"), formatter_class=argparse.RawDescriptionHelpFormatter); parser.add_argument('directory',type=Path)
    parser.add_argument('certificate'); args=parser.parse_args()
    try:
        user_game=(args.directory/'game-binding.json').exists()
        tables=load_game_bound(args.directory) if user_game else load_bound(args.directory)
        path=args.directory/args.certificate
        binding=json.loads((args.directory/('game-binding.json' if user_game else 'binding.json')).read_text())
        need(args.certificate in binding['artifacts'],'unbound certificate')
        print(json.dumps(check(json.loads(path.read_text()),tables),sort_keys=True))
    except (ValueError,KeyError,TypeError,IndexError,OSError,subprocess.SubprocessError) as error:
        parser.exit(2,f'certificate refused: {error}\n')


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
