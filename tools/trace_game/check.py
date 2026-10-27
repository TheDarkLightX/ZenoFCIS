#!/usr/bin/env python3
"""Independent finite references and complete history/edge coverage replay."""
from __future__ import annotations
import argparse
import copy
import hashlib
import itertools
import json
from pathlib import Path


def require(condition, message):
    if not condition:
        raise ValueError(message)


def result(state, reason=None, component=None, kind=None):
    return dict(kind=kind or ('Reject' if reason is not None else 'Accept'),
                reason=reason, component=component if reason is not None else None,
                post=list(state), events=[])


def core(state, request, fault='lawful'):
    # The native scalar profile evaluates the complete DAG eagerly. This
    # deliberately undefined expression refuses even an unselected branch.
    if fault=='arithmetic': return result(state,kind='Refused')
    balance, pending, paid, alarm = state
    op, actor, age, allowed = request
    reason = None
    if fault == 'reject-all': reason = 203
    elif not allowed or (actor != 0 and ((op == 0 and fault != 'accounting') or (op == 3 and fault != 'unauthorized'))) or (fault=='deny-withdraw' and op==3): reason = 200
    elif op == 3 and age > 0 and fault != 'stale': reason = 201
    elif balance+paid>2 or (pending and balance==0): reason = 203
    elif ((op in (0, 3) and (balance == 0 or pending)) or (op == 1 and not pending)): reason = 202
    elif op == 1 and alarm and fault == 'alarm-reset': reason = 203
    if reason is not None: return result(state, reason)
    after = list(state)
    if fault == 'accounting' and op == 0 and actor != 0: after[0] -= 1
    elif op == 0: after[1] = 1
    elif op in (1, 3):
        if fault == 'wrong-effect': after[1] = after[3] = 0
        elif fault == 'arithmetic': return result(state, kind='Refused')
        else: after = [balance-1, 0, paid+1, 0]
    elif op == 2: after[3] = 1
    else: after[3] = 0
    if fault == 'refusal' and op == 0: return result(state, kind='Refused')
    if not all(0 <= after[i] <= bound for i, bound in enumerate((2, 1, 2, 1))):
        return result(state, kind='Refused')
    return result(after)


def composition(state, request, fault='lawful'):
    admin, listed, price, reserve, paid, debt = state
    op, keys, age, bad, skew = request
    after = list(state)
    reason = component = None
    if op == 0:
        component = 'governance'
        if keys < (2 if fault == 'lawful' else 1): reason = 200
        elif age > 0 and fault != 'stale': reason = 201
        else: after[0] = 1
    elif op == 1:
        component = 'collateral'
        if bad and (fault == 'lawful' or not admin): reason = 203
        elif not bad and keys < 2: reason = 200
        else: after[1] = 2 if bad else 1
    elif op == 2:
        component = 'oracle'
        if listed == 0: reason = 202
        elif skew and fault == 'lawful': reason = 203
        else: after[2] = skew
    elif op in (3, 4):
        component = 'vault'
        if reserve == 0: reason = 202
        elif op == 3 and not (listed == 2 and price): reason = 203
        elif op == 3: after[3:] = [0, paid+reserve, debt+reserve]
        else: after[3:5] = [reserve-1, paid+1]
    if reason is not None: return result(state, reason, component)
    if any(x > 2 for x in after[3:]):
        return dict(result(state, kind='Refused'), component='vault')
    return result(after)


# Explicit normalized capabilities: op 5 aliases tick and is excluded; allowed=1 is bound policy,
# actors 0/1; age 0 except fresh/stale withdraw. All commands for each allowed caller.
CORE_ALPHABET = [(op,actor,age) for op in range(5) for actor in (0,1) for age in ((0,1) if op==3 else (0,))]
COMPOSITION_ALPHABET = [(0,1,0,0,0), (0,1,1,0,0), (1,1,0,1,0), (1,2,0,0,0),
                        (2,0,0,0,0), (2,0,0,0,1), (3,0,0,0,0), (4,0,0,0,0), (5,0,0,0,0)]


def starts(model):
    return [[1,0,0,0], [2,0,0,0]] if model == 'withdrawal' else [[0,0,0,2,0,0]]


def initial(state):
    return dict(state=list(state), reference=list(state), pending=None, time=0, stage=0, monitor=0)


def moves(metadata, snapshot, coalition):
    power = metadata['powers']
    if metadata['model'] == 'withdrawal':
        p = snapshot['pending']
        if not metadata['unfair_delay'] and p and snapshot['time'] >= p['submitted']+1:
            return [dict(actor=0, request=[1,0,0,1])]
        return [dict(actor=actor, request=[op,actor,age,1]) for op,actor,age in CORE_ALPHABET
                if (power['arbitrary_order'] or op==4) and (actor == 0 or coalition & power['accounts'] & (1 << actor))
                and power['approval_ages'] & (1 << age)]
    return [dict(actor=actor, request=list(row)) for row in COMPOSITION_ALPHABET
            for actor in ([a for a in (0,1) if coalition & power['accounts'] & (1<<a)] if row[0]>=2 else [2])
            if (power['arbitrary_order'] or row[0]==5) and (row[0] != 0 or power['signers']) and power['approval_ages'] & (1 << row[2])
            and (row[0] != 2 or power['oracle_choices'] & (1 << row[4]))]


def advance(metadata, before, action, native):
    model = metadata['model']
    op = action['request'][0]
    approved = (core if model == 'withdrawal' else composition)(before['reference'], action['request'])
    after = copy.deepcopy(before)
    after.update(time=before['time']+1, state=list(native['post']), reference=approved['post'])
    observed = copy.deepcopy(native)
    violations = set()
    if approved['kind'] == 'Accept' and (native['kind'] != 'Accept' or native['post'] != approved['post'] or native['reason'] != approved['reason']):
        violations.add('legitimacy')
    complete = 0
    if model == 'withdrawal':
        if native['post'][2] > before['state'][2] and op == 3 and action['actor'] != 0: violations.add('unauthorized-value')
        if native['kind'] == 'Accept' and op == 3 and action['request'][2] > 0: violations.add('stale-approval')
        if sum(native['post'][::2]) != sum(before['state'][::2]): violations.add('accounting-loss')
        if approved['kind'] == 'Accept' and op == 0:
            after['pending'] = dict(id=after['time'], submitted=after['time'], entitlement=approved['post'][2]+1)
            after['monitor'] = 0
        elif before['pending']:
            if native['kind'] == 'Accept' and op in (1,3) and native['post'][2] >= before['pending']['entitlement']:
                after['pending'] = None
                after['monitor'] = 0
                complete = 1
            else:
                after['monitor'] = min(2, before['monitor']+1)
                if after['time'] >= before['pending']['submitted']+2: violations.add('exit-deadline')
        if native['kind'] == 'Accept': observed['events'] = [['request','exit','alarm','withdraw','tick'][op]]
    else:
        if native['kind'] == 'Accept' and op == 0 and action['request'][1] < 2: violations.add('privileged-authority')
        if native['kind'] == 'Accept' and op == 0 and action['request'][2] > 0: violations.add('stale-approval')
        if native['post'][1] == 2: violations.add('bad-collateral')
        if native['post'][2]: violations.add('oracle-integrity')
        if sum(native['post'][3:5]) != sum(before['state'][3:5]): violations.add('accounting-loss')
        if native['post'][5] > 0 and native['post'][1] != 1: violations.add('insolvency')
        if native['post'][5]>0: violations.add('coalition-gain')
        if native['kind'] == 'Accept':
            observed['events'] = [['escalated','listed','price','drain','exit','tick'][op]]
            if ((before['stage'] == 0 and op == 0) or (before['stage'] == 1 and op == 1 and action['request'][3])
                    or (before['stage'] == 2 and op == 2 and action['request'][4]) or (before['stage'] == 3 and op == 3)):
                after['stage'] += 1
            if after['stage'] == 4: violations.add('ordered-drain')
    violations = sorted(v for v in violations if (metadata['only_objective'] is None or metadata['only_objective'] == v)
                        and metadata['disabled_objective'] != v)
    return after, observed, violations, complete


def table_check(packet):
    metadata = packet['metadata']
    model, fault = metadata['model'], metadata['fault']
    function = core if model == 'withdrawal' else composition
    table = {}
    for row in packet['table']['rows']:
        require(all(type(v) is int for v in row['state']+row['request']+row['outcome']['post']),'native integer encoding')
        key = tuple(row['state'] + row['request'])
        require(key not in table, 'duplicate native table row')
        expected = function(row['state'], row['request'], fault)
        require(row['outcome'] == expected, f"independent native mismatch {model}/{fault} {key}: {row['outcome']} != {expected}")
        require(row['publication_replayed'] if row['outcome']['kind']!='Refused' else row.get('refusal_repeated') is True, 'unreplayed native publication or refusal')
        require(row['complete_native'], 'missing complete native outcome bytes')
        table[key] = row['outcome']
    if model == 'withdrawal':
        domains = (range(3),range(2),range(3),range(2),range(6),range(3),range(3),range(2))
        expected_keys = set(itertools.product(*domains))
    else:
        states = itertools.product(range(2), range(3), range(2), range(3), range(3), range(3))
        expected_keys = {tuple(s)+r for s in states for r in COMPOSITION_ALPHABET}
        require(packet['table']['composition_check'] is not None, 'missing original composition obligations')
        check_leaf_publications(packet)
    require(set(table) == expected_keys, 'incomplete independent state/move native table')
    return table


def check_leaf_publications(packet):
    publications=packet['table']['initialization']['leaf_publications']
    require(len(publications)==4,'missing leaf publication replays')
    domains=[(range(2),range(3),range(3),range(2)),(range(3),range(2),range(3),range(2)),
             (range(2),range(2),range(3)),(range(3),range(3),range(3),range(2),range(3),range(2))]
    for i,table in enumerate(publications):
        require(table['sources']==packet['table']['sources']['leaves'][i],'leaf publication source binding')
        keys=set()
        for row in table['rows']:
            s,r=row['state'],row['request'];key=tuple(s+r)
            require(key not in keys,'duplicate leaf publication');keys.add(key)
            if i==0:
                expected=composition([s[0],0,0,0,0,0],[0,r[0],r[1],0,0],packet['metadata']['fault'])
                if not r[2]:expected=result([s[0]],200)
                else:expected['post']=expected['post'][:1]
            elif i==1:
                expected=composition([r[2],s[0],0,0,0,0],[1,r[1],0,r[0],0],packet['metadata']['fault']);expected['post']=expected['post'][1:2]
            elif i==2:
                expected=composition([0,r[1],s[0],0,0,0],[2,0,0,0,r[0]],packet['metadata']['fault']);expected['post']=expected['post'][2:3]
            else:
                expected=composition([0,r[1],r[2],*s],[3 if r[0] else 4,0,0,0,0],packet['metadata']['fault']);expected['post']=expected['post'][3:]
            expected['component']=None
            require(row['outcome']==expected,'independent leaf/native publication mismatch')
            require(row['publication_replayed'] if expected['kind']!='Refused' else row['refusal_repeated'],'unreplayed leaf publication/refusal')
        require(keys==set(itertools.product(*domains[i])),'incomplete local native publication domain')
        require(table['initialization']['replayed'] and table['initialization']['altered_subject_refused'] and table['initialization']['wrong_genesis_refused'],'leaf genesis/altered-byte replay missing')


def violates(metadata, table, record, actions):
    state = initial(record['start'])
    violations = []
    for action in actions:
        if action not in moves(metadata, state, record['coalition']):return False
        native = table[tuple(state['state'] + action['request'])]
        state, _, violations, _ = advance(metadata, state, action, native)
    return record['property'] in violations


def witness(metadata,table,record,actions):
    require(violates(metadata,table,record,actions),'witness does not independently violate assigned objective')


def check_history(packet):
    require(packet['schema'] == 'zeno-fcis/trace-game/1', 'packet schema')
    metadata, run, limits = packet['metadata'], packet['run'], packet['limits']
    require(1 <= limits['horizon'] <= 6, 'invalid K')
    require(type(limits['horizon']) is int and metadata['horizon']==limits['horizon'],'model/check horizon mismatch')
    table = table_check(packet)
    nodes = run['nodes']
    require(len(nodes)<=limits.get('nodes',len(nodes)), 'node budget exceeded')
    require(run['transitions']<=limits.get('transitions',run['transitions']), 'work budget exceeded')
    roots = [(i,c) for i in range(len(starts(metadata['model']))) for c in (1,2,3)]
    require(len(nodes) >= len(roots), 'missing initial states or coalitions')
    for i, (start,c) in enumerate(roots):
        require(nodes[i] == dict(parent=None,action=None,start=start,coalition=c,
                snapshot=initial(starts(metadata['model'])[start]),outcome=None,violations=[]), 'initial state/coalition mismatch')
    children = [[] for _ in nodes]
    witnesses = {}
    completed = 0
    maximum_loss = 0
    maximum_loss_node = None
    for i, node in enumerate(nodes[len(roots):], len(roots)):
        parent = node['parent']
        require(type(parent) is int and 0 <= parent < i, 'noncausal parent')
        pre = nodes[parent]
        require((node['start'],node['coalition']) == (pre['start'],pre['coalition']), 'changed start/coalition')
        require(node['action'] in moves(metadata, pre['snapshot'], pre['coalition']), 'disabled edge')
        native = table[tuple(pre['snapshot']['state'] + node['action']['request'])]
        next_state, observed, violations, count = advance(metadata, pre['snapshot'], node['action'], native)
        require((node['snapshot'],node['outcome'],node['violations']) == (next_state,observed,violations), f'edge replay mismatch at {i}')
        require(next_state['time'] <= limits['horizon'], 'node past K')
        completed += count
        loss = next_state['state'][5] if metadata['model']=='composition' else max(0,next_state['state'][2]-next_state['reference'][2])
        if loss>maximum_loss:maximum_loss,maximum_loss_node=loss,i
        children[parent].append(node['action'])
        for prop in violations: witnesses.setdefault(prop, i)
    complete_depth = limits['horizon']
    for i, node in enumerate(nodes):
        depth = node['snapshot']['time']
        if depth == limits['horizon']:
            require(not children[i], 'children past K')
            continue
        expected = moves(metadata, node['snapshot'], node['coalition'])
        require(expected, 'unsupported empty alphabet at interior node')
        require(children[i] or run['status']=='UNKNOWN' or run['stop']!='exhausted-declared-prefixes', 'zero transitions at interior node')
        require(children[i] == expected[:len(children[i])], 'noncanonical, duplicate or missing interior edge')
        if len(children[i]) != len(expected): complete_depth = min(complete_depth, depth)
    require(run['completed_depth'] == complete_depth, 'incorrect completed depth')
    require(run['witnesses'] == witnesses, 'false/missing witness index')
    require(run['completed_requests'] == completed, 'lost progress accounting')
    require((run['maximum_loss'],run['maximum_loss_node']) == (maximum_loss,maximum_loss_node),'incorrect worst loss')
    require(run['transitions'] == len(nodes)-len(roots), 'lost transition/refusal work')
    full = complete_depth == limits['horizon']
    expected_status = 'counterexample' if witnesses else ('checked-through-K' if full else 'UNKNOWN')
    require(run['status'] == expected_status, 'false complete verdict')
    if full:
        frontier = [i for i,n in enumerate(nodes) if n['snapshot']['time'] == limits['horizon']]
        require(run['frontier'] == frontier, 'incomplete horizon frontier')
        pending = [nodes[i]['snapshot']['pending'] for i in frontier if nodes[i]['snapshot']['pending']]
        require(run['open_requests'] == len(pending), 'truncated request marked complete')
        require(run['late_requests'] == sum(p['submitted']+2 > limits['horizon'] for p in pending), 'late request scope')
    else:
        require(run['stop'] != 'exhausted-declared-prefixes' and run['frontier'], 'missing incomplete frontier')
        current=[i for i,n in enumerate(nodes) if n['snapshot']['time']==complete_depth]
        first=next((at for at,i in enumerate(current) if len(children[i])<len(moves(metadata,nodes[i]['snapshot'],nodes[i]['coalition']))),None)
        require(first is not None,'incomplete stop without an unexpanded parent')
        following=[i for i,n in enumerate(nodes) if n['snapshot']['time']==complete_depth+1]
        require(run['frontier']==current[first:]+following,'incomplete frontier lost or invented work')
    regression_inventory(packet, lambda record,actions: violates(metadata,table,record,actions))
    return dict(status=expected_status,native_rows=len(table),transitions=run['transitions'],completed_depth=complete_depth,
                objectives=sorted(witnesses),open_requests=run['open_requests'],
                scope={key:metadata[key] for key in ('model','fault','horizon','powers','only_objective','disabled_objective','unfair_delay')},
                alphabet=metadata.get('alphabet'),deadline=metadata.get('deadline'),blind_spots=metadata.get('blind_spots',[]))


def trace_to(nodes,index):
    actions=[]
    while nodes[index]['parent'] is not None:
        actions.append(nodes[index]['action']);index=nodes[index]['parent']
    return list(reversed(actions)),nodes[index]


def regression_inventory(packet,replays):
    records=packet['regressions'];nodes=packet['run']['nodes'];witnesses=packet['run']['witnesses']
    require(len(records)==len(witnesses) and {r['property'] for r in records}==set(witnesses),'missing or duplicate regression inventory')
    for record in records:
        original,root=trace_to(nodes,witnesses[record['property']])
        require(record['original']==original and record['start']==root['snapshot']['state'] and record['coalition']==root['coalition'],'regression not bound to discovered witness')
        for which in ('original','minimized'):
            require(replays(record,record[which]),'witness does not independently violate assigned objective')
        minimized=record['minimized']
        for width in range(1,len(minimized)+1):
            for at in range(len(minimized)-width+1):
                require(not replays(record,minimized[:at]+minimized[at+width:]),'witness still has a removable chunk')
        for i,action in enumerate(minimized):
            for field,value in enumerate(action['request']):
                if value==0:continue
                proposal=copy.deepcopy(minimized);proposal[i]['request'][field]=0
                require(not replays(record,proposal),'witness still has a shrinkable argument')


def control_history(packet,start,horizon,enabled,step,loss):
    """Independent complete tree checker, including all fields and witness traces."""
    run=packet['run'];nodes=run['nodes'];require(len(nodes)>=3,'missing control roots')
    for i in range(3):
        require(nodes[i]==dict(parent=None,action=None,start=0,coalition=i+1,snapshot=initial(start),outcome=None,violations=[]),'control root mismatch')
    children=[[] for _ in nodes];witnesses={};maximum=0;maximum_node=None;edges={}
    for i,n in enumerate(nodes[3:],3):
        parent=n['parent'];require(type(parent) is int and 0<=parent<i,'control noncausal parent')
        pre=nodes[parent];require(n['start']==0 and n['coalition']==pre['coalition'],'control coalition/start')
        require(n['action'] in enabled(pre),'control disabled action')
        snapshot,outcome,violations=step(pre,n['action'])
        require(n['snapshot']==snapshot and n['outcome']==outcome and n['violations']==violations,'control edge replay')
        require(snapshot['time']<=horizon,'control past K');children[parent].append(n['action'])
        key=(parent,n['action']['actor'],tuple(n['action']['request']));require(key not in edges,'control duplicate edge');edges[key]=i
        for prop in violations:witnesses.setdefault(prop,i)
        value=loss(n)
        if value>maximum:maximum,maximum_node=value,i
    for n,cs in zip(nodes,children):
        require(cs==(enabled(n) if n['snapshot']['time']<horizon else []),'control incomplete history coverage')
    status='counterexample' if witnesses else 'checked-through-K'
    require(run['witnesses']==witnesses,'control false/missing witness index')
    require(run['status']==status and run['completed_depth']==horizon and run['stop']=='exhausted-declared-prefixes','control false verdict')
    require(run['transitions']==len(nodes)-3 and run['frontier']==[i for i,n in enumerate(nodes) if n['snapshot']['time']==horizon],'control incomplete frontier/work')
    require((run['maximum_loss'],run['maximum_loss_node'])==(maximum,maximum_node),'control worst loss')
    require(run['completed_requests']==run['open_requests']==run['late_requests']==0,'control invented progress')
    def replay(record,actions):
        node=record['coalition']-1
        for action in actions:
            node=edges.get((node,action['actor'],tuple(action['request'])))
            if node is None:return False
        return record['property'] in nodes[node]['violations']
    # Legacy claims remain checked as traces, never as replay booleans.
    if 'witnesses' in packet:
        require(len(packet['witnesses'])==len(witnesses),'control missing witness traces')
        for w in packet['witnesses']:
            require(w['property'] in witnesses and replay(w,w['trace']),'control invalid claimed witness trace')
    regression_inventory(packet,replay)
    return dict(status=status,objectives=sorted(witnesses),nodes=len(nodes),transitions=len(nodes)-3,horizon=horizon)


def check_packet(packet,binding=None,artifact=None):
    # Native credit comes only from a verified source/binary/artifact binding.
    require(binding is not None and artifact is not None,'native source binding required; test specimens are not native evidence')
    artifact=Path(artifact)
    require(hashlib.sha256(artifact.read_bytes()).hexdigest()==binding['artifacts'].get(artifact.name),'native artifact binding')
    require(json.loads(artifact.read_text())==packet,'native packet differs from bound artifact')
    for name in ('harness','cli'):
        binary=binding['binaries'][name]
        require(hashlib.sha256(Path(binary['path']).read_bytes()).hexdigest()==binary['sha256'],'native binary binding')
    return dict(check_history(packet),native_provenance={name:binding['binaries'][name]['sha256'] for name in ('harness','cli')})


def scalar_relation(packet):
    """Independent ordered reason partition and complete frame enumeration."""
    counts=dict(raw_inputs=0,candidate_outcomes=0,zero=0,ambiguous=0,excluded_framework_law=991)
    posts=list(map(list,itertools.product(range(3),range(2),range(3),range(2))))
    for row in packet['table']['rows']:
        balance,pending,paid,alarm=row['state'];op,actor,age,allowed=row['request']
        unauthorized=not allowed or (actor!=0 and op in (0,3))
        stale=not unauthorized and op==3 and age>0
        invalid=not unauthorized and not stale and (balance+paid>2 or (pending and balance==0))
        unavailable=not unauthorized and not stale and not invalid and (((op in (0,3)) and (balance==0 or pending)) or (op==1 and not pending))
        reject_conditions={200:unauthorized,201:stale,203:invalid,202:unavailable}
        eligible=not any(reject_conditions.values())
        lawful=sum(bool(v) for v in reject_conditions.values())
        for post in posts:
            exact=(post[0]==balance-int(op in (1,3)) and post[1]==(1 if op==0 else 0 if op in (1,3) else pending)
                   and post[2]==paid+int(op in (1,3)) and post[3]==(alarm if op==0 else 1 if op==2 else 0))
            lawful+=int(eligible and exact)
        counts['raw_inputs']+=1;counts['candidate_outcomes']+=len(posts)+len(reject_conditions)
        counts['zero']+=int(lawful==0);counts['ambiguous']+=int(lawful>1)
    require(counts['zero']==counts['ambiguous']==0,'independent complete relation has a gap')
    certificate=packet['table']['initialization']['gap_relation']
    require(certificate['excluded_framework_law']==991 and certificate['zero_lawful_outcomes']==certificate['ambiguous_outcomes']==0
            and certificate['raw_inputs']==counts['raw_inputs'],'native gap relation mismatch')
    return counts


def main():
    from run import replay
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory',type=Path)
    args=parser.parse_args()
    directory=args.directory.resolve()
    if not (directory/'binding.json').is_file():
        parser.error('native source binding required; test specimens are not native evidence')
    checks=directory/'checks';checks.mkdir(exist_ok=True)
    results=replay(directory,checks)
    (directory/'replay.json').write_text(json.dumps(results,indent=2)+'\n')


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
