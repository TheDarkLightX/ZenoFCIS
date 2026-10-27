"""Independent user-game certificate checks; no product or solver imports."""
import hashlib
import json
from pathlib import Path
import resource
import subprocess
import time
from game_spec import validate, description, native_numbers, SCOPE


def need(condition,message):
    if not condition: raise ValueError(message)


def key(value):
    return json.dumps(value,separators=(',',':'),sort_keys=True)


def terminal(spec,s):
    return s[-1] or bool(s[5]) or (spec['horizon'] is not None and s[3]>=spec['horizon'])


def actions(spec,s):
    if terminal(spec,s): return []
    return [(i,a) for i,a in enumerate(spec['events']) if s[2] & (1<<a['actor'])]


def after(spec,table,s,native_event):
    row=table[tuple(s[0])+(native_event,)]
    enabled=row['guards'][spec['guard']]
    actual=row['class'] if enabled else 'Blocked'
    holds=row['holds'] if enabled else row['blocked_holds']
    holds=holds and actual in spec['objective'].get('on',['Accept','Reject','CommittedFailure','Refused','Blocked'])
    return [row['post'][:] if enabled else s[0][:],[],s[2],
            s[3]+1 if spec['horizon'] is not None else 0,0,
            int(holds and spec['objective']['kind']=='reachability'),
            bool(holds and spec['objective']['kind']=='safety')]


def graph(packet,table):
    spec=packet['spec']; validate(spec['game'])
    expected=description(spec['game'],spec,spec['guard'],spec['horizon'])
    need(spec==expected,'game description drift')
    if '__subject__' in table:
        need(dict(spec,guard=0)==table['__subject__'],'native game subject mismatch')
    need(type(spec['guard']) is int and 0<=spec['guard']<len(spec['guards']),'guard index')
    need(spec['horizon'] is None or type(spec['horizon']) is int and 0<=spec['horizon']<=1000,'horizon')
    g=packet['graph']; states=g['states']; edges=g['edges']; n=len(states)
    need(g['status']=='complete','incomplete product: UNKNOWN')
    need(0<n<=10000 and len(edges)==n and sum(map(len,edges))<=100000,'product size')
    for s in states:
        need(type(s) is list and len(s)==7 and type(s[-1]) is bool,'state encoding')
        need(type(s[0]) is list and s[1]==[] and all(type(v) is int for v in s[0]+s[2:6]),'state scalar encoding')
        need(len(s[0])==len(spec['genesis']) and s[2] in spec['coalitions'] and s[4]==0 and s[5] in (0,1),'state fields')
    index={key(s):i for i,s in enumerate(states)}
    need(len(index)==n,'duplicate product state')
    roots=[[spec['genesis'],[],c,0,0,0,False] for c in spec['coalitions']]
    expected_roots=[index.get(key(s)) for s in roots]
    need(all(type(i) is int for i in g['roots']),'root integer encoding')
    need(expected_roots and None not in expected_roots and g['roots']==expected_roots,'initial membership')
    deadline=time.monotonic()+30
    for i,s in enumerate(states):
        need(time.monotonic()<deadline,'checker time budget')
        allowed=actions(spec,s)
        need(allowed or terminal(spec,s),'empty environment: UNKNOWN')
        for edge in edges[i]:
            need(type(edge) is list and len(edge)==3 and all(type(v) is int for v in edge),'edge integer encoding')
        expected=[]
        for e,(native_event,_) in enumerate(allowed):
            target=index.get(key(after(spec,table,s,native_event)))
            need(target is not None,'missing reachable successor')
            expected.append([e,spec['guard'],target])
        need(edges[i]==expected,'complete permitted move coverage')
    seen=set(g['roots']); todo=list(seen)
    for i in todo:
        for _,_,t in edges[i]:
            if t not in seen: seen.add(t); todo.append(t)
    need(len(seen)==n,'unreachable product state')
    return spec,states,edges


def replay(spec,states,edges,cert,table):
    root=cert['root']
    need(type(root) is int and 0<=root<len(states),'counterexample root')
    at=root; visited=[at]; history=[]
    for move in cert['moves']:
        need(type(move) is list and len(move)==2 and all(type(v) is int for v in move),'counterexample move')
        e,g=move
        options=actions(spec,states[at])
        need(0<=e<len(options) and g==spec['guard'],'counterexample permitted event')
        native_event,action=options[e]
        target=edges[at][e][2]
        row=table[tuple(states[at][0])+(native_event,)]
        enabled=row['guards'][g]
        inputs=[p['name'] for p in spec['positions'] if not p['name'].startswith('pre.')]
        history.append(dict(actor=spec['game']['actors'][action['actor']]['id'],
                            request=action['request'],pre=states[at][0],post=states[target][0],
                            command={k:v for k,v in zip(inputs,action['request']) if k.startswith('command.')},
                            context={k:v for k,v in zip(inputs,action['request']) if k.startswith('context.')},
                            decision=dict(kind=row['class'] if enabled else 'Blocked',reason=row.get('reason') if enabled else None),
                            native_event=native_event,guard=g))
        at=target; visited.append(at)
    return at,visited,history


def check(packet,table):
    if packet['schema']=='zeno-fcis/game-incomplete/1':
        need(table.get('__incomplete__') is True and packet['evidence']=='UNKNOWN'
             and type(packet['limit']) is str and packet['limit'],'incomplete diagnostic')
        return dict(evidence='UNKNOWN',limit=packet['limit'],claim='source-bound diagnostic only; no game certificate established',scope=SCOPE)
    if packet['schema']=='zeno-fcis/game-guard-exhaustion/1':
        attempts=packet['attempts']; need(attempts,'empty guard class')
        need(sum(len(a['graph']['states']) for a in attempts)<=10000 and
             sum(len(row) for a in attempts for row in a['graph']['edges'])<=100000,'guard class product budget')
        subject=None
        for guard,attempt in enumerate(attempts):
            spec=attempt['spec']; frozen=dict(spec,guard=0)
            if subject is None: subject=frozen
            need(frozen==subject and spec['guard']==guard,'fixed guard subject')
            need(check(attempt,table)['evidence']=='counterexample','missing guard refutation')
        need(len(attempts)==len(subject['guards']),'incomplete guard class')
        return dict(evidence='no guard in class',claim='no guard in the declared finite candidate class',scope=SCOPE)
    need(packet['schema']=='zeno-fcis/trace-game-certificate/1.1','game certificate schema')
    spec,states,edges=graph(packet,table); cert=packet['certificate']; kind=cert['kind']
    roots=set(packet['graph']['roots']); objective=spec['objective']['kind']
    if kind=='counterexample':
        need(objective=='safety' and cert['root'] in roots,'safety counterexample subject')
        at,_,history=replay(spec,states,edges,cert,table)
        need(states[at][-1],'history has no violation')
        return dict(evidence='counterexample',history=history,claim='replayed violation in the declared game',scope=SCOPE)
    if kind=='bounded':
        need(spec['horizon'] is not None,'bounded horizon')
        if objective=='safety': need(not any(s[-1] for s in states),'bounded safety violation')
        return dict(evidence='checked up to K',K=spec['horizon'],
                    claim='complete histories through K only; no unbounded claim',scope=SCOPE)
    need(spec['horizon'] is None,'unbounded certificate with cutoff')
    if kind=='reachability-counterexample':
        need(objective=='reachability' and cert['root'] in roots,'reachability counterexample subject')
        losing={}
        for i,e,g in cert['avoidance']:
            need(type(i) is int and 0<=i<len(states) and i not in losing,'avoidance state')
            need(not states[i][5] and type(e) is int and 0<=e<len(edges[i]) and g==spec['guard'],'avoidance event')
            losing[i]=edges[i][e][2]
        need(cert['root'] in losing and all(t in losing for t in losing.values()),'avoidance closure')
        at,visited,history=replay(spec,states,edges,cert,table)
        repeat=cert['repeat_at']
        need(type(repeat) is int and 0<=repeat<len(history) and visited[repeat]==at,'non-reaching cycle replay')
        need(all(i in losing for i in visited),'cycle leaves avoidance set')
        return dict(evidence='counterexample',history=history,repeat_at=repeat,
                    claim='replayed infinite non-reaching history; repeat the cycle',scope=SCOPE)
    if kind=='reachability':
        need(objective=='reachability','reachability objective')
        ranks={}
        for i,r in cert['ranks']:
            need(type(i) is int and 0<=i<len(states) and i not in ranks and type(r) is int and 0<=r<=len(states),'rank encoding')
            ranks[i]=r
        need(roots<=ranks.keys(),'ranked initial states')
        for i,r in ranks.items():
            if r==0: need(bool(states[i][5]),'rank zero target')
            else: need(edges[i] and all(t in ranks and ranks[t]<r for _,_,t in edges[i]),'universal rank decrease')
        return dict(evidence='certified reachability',claim='the system forces the target in the declared finite game',scope=SCOPE)
    need(kind=='safety' and objective=='safety','evidence kind')
    W=cert['W']; need(W==sorted(set(W)) and all(type(i) is int and 0<=i<len(states) for i in W),'winning set encoding')
    need(roots<=set(W),'initial membership in W')
    for i in W:
        need(not states[i][-1],'unsafe state in W')
        need(all(t in W for _,_,t in edges[i]),'closure under permitted moves')
    return dict(evidence='certified safety',claim=SCOPE,states=len(states),roots=len(roots))


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_bound(directory):
    root=Path(__file__).resolve().parents[2]
    binding=json.loads((directory/'game-binding.json').read_text())
    need(binding['schema']=='zeno-fcis/game-binding/1','binding schema')
    files=subprocess.check_output(['git','ls-files','-z','--cached','--others','--exclude-standard'],cwd=root).split(b'\0')
    inventory={p.decode() for p in files if p and (root/p.decode()).is_file()}
    need(set(binding['sources'])==inventory,'source inventory drift')
    for name,sha in binding['sources'].items():
        need(digest(root/name)==sha,'source drift: '+name)
    for name,sha in binding['artifacts'].items():
        need(digest(directory/name)==sha,'artifact drift: '+name)
    certificate=json.loads((directory/'certificate.json').read_text())
    if certificate['schema']=='zeno-fcis/game-incomplete/1': return {'__incomplete__':True}
    binary=Path(binding['binary']['path']); need(digest(binary)==binding['binary']['sha256'],'native binary drift')
    request=json.loads((directory/'native-request.json').read_text())
    completed=subprocess.run(['nice','-n','19','ionice','-c3',str(binary)],input=json.dumps(request).encode(),
        capture_output=True,timeout=30,preexec_fn=lambda: resource.setrlimit(resource.RLIMIT_AS,(1<<30,1<<30)))
    need(completed.returncode==0,'native replay refused: '+completed.stderr.decode()[:200])
    native=json.loads((directory/'native-table.json').read_text())
    need(native_numbers(json.loads(completed.stdout))==native,'native transition/objective replay mismatch')
    need(native['genesis_replayed'] is True,'native genesis replay')
    need(all(r['publication_replayed'] is True or r['refusal_repeated'] is True for r in native['rows']),'native row replay')
    game=validate(json.loads((directory/'game.json').read_text()))
    subject=description(game,native,horizon=binding['horizon'])
    need(request['events']==subject['events'] and request['formula']==game['objective']['formula']
         and request['guards']==subject['guards'],'native request/game mismatch')
    need(binding['mode'] in ('check','certify','select-guard'),'invocation mode')
    need(binding['horizon']==(game.get('K',6) if binding['mode']=='check' else None),'invocation horizon')
    table={tuple(row['pre'])+(row['event'],):row for row in native['rows']}
    table['__subject__']=subject
    return table
