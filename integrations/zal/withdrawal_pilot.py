"""Withdrawal temporal pilot: contract observation adapter; authority: none.

Binds the ZAL-T withdrawal statements to the withdrawal-queue contract. The
decision graph is produced by the generated V2 contract's checked Authority.
This module names the observations, checks the adapter obligations over every
decision of that graph, builds the finite observation automaton for the
bounded claim and the controller rows for the Lean model. Verdicts come from
the existing Rust temporal evaluator and the Lean kernel, never from here.
"""
import hashlib
import json
from collections import deque
from pathlib import Path
import subprocess
import tempfile

import ladder as l
from temporal_lowering import lower

ROOT=Path(__file__).resolve().parents[2]
EXAMPLES=Path(__file__).with_name('examples')/'temporal'
CONTRACT=ROOT/'crates/zeno-fcis-cli/templates/withdrawal-queue'
# The decision graph depends on exactly these contract sources.
CONTRACT_SOURCES=('project.zeno','src/v2_contract.rs','v2/policy.zcve','v2/schema.zcve')
GRAPH_PROFILE='withdrawal-queue/decision-graph/1'
# Vault fields in record order: balance, lane_a, amount_a, lane_b, amount_b,
# pause, must_serve, priority (project.zeno fields 120..127).
GENESIS=[0,180,0,180,0,0,0,170]
EMPTY,ARRIVED,PENDING=180,181,182
REQUEST,TICK=161,162
KEEPER=193
SETTLEMENT=300
LANES={'lane_a':(170,1,2),'lane_b':(171,3,4)}  # variant, status index, amount index
DOMAIN={'action':[160,161,162],'lane':[170,171],'amount':[1,2],'caller':[190,191,192,193],'alarm':[0,1]}
# k is the contract's response bound: a presented lane is paid on or before
# the eighth tick, counting the presenting tick (README, controller bound).
K=7
# Reachability depth of the tick projection (4, checked) plus the k+1 tick window.
HORIZON=12


class AdapterRefusal(l.Refusal):
    """A named adapter obligation failed on the contract's decision graph."""


def contract_identity():
    digest=hashlib.sha256(b'withdrawal-queue/contract-sources/1\0')
    for name in CONTRACT_SOURCES:
        digest.update(name.encode()+b'\0'+(CONTRACT/name).read_bytes())
    return digest.hexdigest()


def load_graph(text):
    graph=json.loads(text)
    if graph.get('profile')!=GRAPH_PROFILE or graph.get('authority')!='none':
        raise AdapterRefusal('adapter:graph-profile')
    if graph.get('complete') is not True:raise AdapterRefusal('adapter:incomplete-graph')
    if graph.get('domain')!=DOMAIN:raise AdapterRefusal('adapter:declared-domain')
    return graph


def ctrl(vault):
    """The controller part: lane_a, lane_b, pause, must_serve, priority."""
    return (vault[1],vault[3],vault[5],vault[6],vault[7])


def decisions(graph):
    for row in graph['decisions']:
        source,action,lane,amount,caller,alarm,cls,reason,target,outbox=row
        yield {'from':source,'action':action,'lane':lane,'amount':amount,'caller':caller,'alarm':alarm,
               'class':cls,'reason':reason,'to':target,'outbox':outbox}


def payouts(decision):
    return [(row[1],row[2]) for row in decision['outbox'] if row[0]==SETTLEMENT]


def accepted(decision,action):
    return decision['class']=='accept' and decision['action']==action


def request_observation(graph,decision):
    """R at a decision: its acceptance of an owner's request on that lane."""
    return {name:accepted(decision,REQUEST) and decision['lane']==variant for name,(variant,_,_) in LANES.items()}


def completion_observation(graph,decision):
    """C at a decision: a settlement payout of that lane is queued."""
    paid={lane for lane,_ in payouts(decision)}
    return {name:variant in paid for name,(variant,_,_) in LANES.items()}


def presented_observation(graph,decision):
    """R at a tick: the lane is arrived before the tick (presented by it)."""
    pre=graph['vaults'][decision['from']]
    return {name:pre[status]==ARRIVED for name,(_,status,_) in LANES.items()}


def check_adapter(graph):
    """Check every adapter obligation over every decision; return counts."""
    vaults=graph['vaults'];rows=list(decisions(graph))
    def fail(obligation,detail):raise AdapterRefusal('adapter:'+obligation+': '+str(detail))
    # Initialization: the contract's own genesis, nothing pending.
    if vaults[0]!=GENESIS or not graph['genesis_commits'] or not graph['changed_genesis_refused']:
        fail('initialization',vaults[0])
    if any(vaults[0][status]!=EMPTY for _,status,_ in LANES.values()):fail('initialization','request pending at genesis')
    # Membership: the complete declared domain from every vault, closed graph.
    domain=len(DOMAIN['action'])*len(DOMAIN['lane'])*len(DOMAIN['amount'])*len(DOMAIN['caller'])*len(DOMAIN['alarm'])
    per={}
    for d in rows:
        per[d['from']]=per.get(d['from'],0)+1
        if d['class'] not in ('accept','reject'):fail('membership',d)
        if (d['class']=='reject')!=(d['to'] is None) or (d['class']=='reject' and d['outbox']):fail('membership',d)
        if d['to'] is not None and not 0<=d['to']<len(vaults):fail('membership',d)
    if sorted(per)!=list(range(len(vaults))) or set(per.values())!={domain}:fail('membership','incomplete domain')
    seen={0};queue=deque([0]);edges={}
    for d in rows:edges.setdefault(d['from'],[]).append(d)
    while queue:
        for d in edges[queue.popleft()]:
            if d['to'] is not None and d['to'] not in seen:seen.add(d['to']);queue.append(d['to'])
    if len(seen)!=len(vaults):fail('membership','unreachable vault')
    counts={'vaults':len(vaults),'decisions':len(rows),'ticks':0,'requests':0,'payouts':0,'alarm_ticks_with_request':0}
    for d in rows:
        pre=vaults[d['from']];post=vaults[d['to']] if d['to'] is not None else pre
        # Exit processing: the keeper's tick is accepted from every vault.
        if d['action']==TICK and d['caller']==KEEPER and not accepted(d,TICK):fail('tick-enabled',d)
        if d['class']=='accept' and d['action']!=TICK:
            # Continuity across hidden decisions: only the requested empty
            # lane may change, to arrived; nothing else of the controller.
            if d['outbox']:fail('continuity',d)
            expect=list(ctrl(pre))
            if d['action']==REQUEST:
                name=next(n for n,(v,_,_) in LANES.items() if v==d['lane'])
                variant,status,amount=LANES[name]
                if pre[status]!=EMPTY or post[status]!=ARRIVED or post[amount]!=d['amount']:fail('identity',d)
                expect[(0 if name=='lane_a' else 1)]=ARRIVED;counts['requests']+=1
            if list(ctrl(post))!=expect:fail('continuity',d)
            for name,(_,status,amount) in LANES.items():
                if pre[status]!=EMPTY and (post[status]!=pre[status] or post[amount]!=pre[amount]):fail('identity',d)
        if accepted(d,TICK):
            counts['ticks']+=1
            if d['alarm'] and any(pre[s]==ARRIVED for _,s,_ in LANES.values()):counts['alarm_ticks_with_request']+=1
            paid=payouts(d)
            if len(paid)>1 or len(d['outbox'])!=len(paid):fail('completion',d)
            for name,(variant,status,amount) in LANES.items():
                emptied=pre[status]!=EMPTY and post[status]==EMPTY
                # The payout is the request's own: its lane and recorded amount.
                if emptied!=((variant,pre[amount]) in paid):fail('identity',d)
                if pre[status]==EMPTY and post[status]!=EMPTY:fail('identity',d)
                if post[status]==ARRIVED:fail('continuity','tick leaves a lane arrived')
                if not emptied and pre[status]!=EMPTY and post[amount]!=pre[amount]:fail('identity',d)
            counts['payouts']+=len(paid)
    # The adapter's completion observation equals the contract's payouts.
    observed=sum(sum(completion_observation(graph,d).values()) for d in rows)
    if observed!=counts['payouts']:fail('completion','observations %d, payouts %d'%(observed,counts['payouts']))
    return counts


def statement_paths(ctx):
    """Observation paths in record order, each with its (symbol, argument)."""
    paths=[]
    for field in ctx.data['native_schema']['fields']:
        path='pre.100.'+str(field['id'])
        owners=[(name,tuple(row['args'])) for name,meta in ctx.data['symbols'].items() for row in meta.get('native',()) if row.get('before')==path]
        if len(owners)!=1:raise AdapterRefusal('adapter:observation-map: '+path)
        paths.append((path,owners[0]))
    return paths


def tick_letter(graph,decision,paths):
    values={('R',(name,)):value for name,value in presented_observation(graph,decision).items()}
    values.update({('C',(name,)):value for name,value in completion_observation(graph,decision).items()})
    values[('Tick',())]=True
    return [int(values[owner]) for _,owner in paths]


def tick_automaton(graph,paths):
    """Tick projection: one event per accepted keeper tick, in commit order.

    Decisions between ticks are hidden; their effect reaches the next tick's
    pre-vault. States are vaults at tick boundaries, quotiented by their
    controller part after checking that the quotient is a bisimulation.
    """
    vaults=graph['vaults'];edges={}
    for d in decisions(graph):edges.setdefault(d['from'],[]).append(d)
    def closure(start):
        seen={start};queue=deque([start])
        while queue:
            for d in edges[queue.popleft()]:
                if d['class']=='accept' and d['action']!=TICK and d['to'] not in seen:seen.add(d['to']);queue.append(d['to'])
        return seen
    boundary={0};queue=deque([0]);moves={}
    while queue:
        start=queue.popleft();out=set()
        for vault in closure(start):
            for d in edges[vault]:
                if accepted(d,TICK) and d['caller']==KEEPER:
                    out.add((tuple(tick_letter(graph,d,paths)),d['to']))
                    if d['to'] not in boundary:boundary.add(d['to']);queue.append(d['to'])
        moves[start]=out
    classes={}
    for vault in sorted(boundary):
        key=ctrl(vaults[vault]);image={(letter,ctrl(vaults[t])) for letter,t in moves[vault]}
        if classes.setdefault(key,image)!=image:raise AdapterRefusal('adapter:quotient: %r'%(key,))
    order=sorted(classes);number={key:i for i,key in enumerate(order)}
    automaton={'states':len(order),'initial':[number[ctrl(vaults[0])]],
               'edges':sorted([number[key],list(letter),number[target]] for key in order for letter,target in classes[key])}
    return automaton,{'boundary_vaults':len(boundary),'classes':len(order),'edges':len(automaton['edges'])}


def reach_depth(automaton):
    """Longest shortest path, in ticks, from the initial class to any class."""
    depth={state:0 for state in automaton['initial']};queue=deque(depth)
    while queue:
        state=queue.popleft()
        for source,_,target in automaton['edges']:
            if source==state and target not in depth:depth[target]=depth[state]+1;queue.append(target)
    if len(depth)!=automaton['states']:raise AdapterRefusal('adapter:unreachable-class')
    return max(depth.values())


def abstract_action(decision):
    if accepted(decision,TICK):return ('tick',bool(decision['alarm']))
    if accepted(decision,REQUEST):return ('request','lane_a' if decision['lane']==170 else 'lane_b')
    return ('idle',)


def controller_rows(graph):
    """Distinct (controller, abstract action) -> (controller, observation) rows.

    Refuses a contract whose decisions differ on one abstract row: the Lean
    model's step is a function, so every row must be one.
    """
    vaults=graph['vaults'];rows={}
    for d in decisions(graph):
        pre=ctrl(vaults[d['from']]);post=ctrl(vaults[d['to']]) if d['to'] is not None else pre
        observed=(request_observation(graph,d),completion_observation(graph,d))
        value=(post,tuple(observed[0].values()),tuple(observed[1].values()))
        if rows.setdefault((pre,abstract_action(d)),value)!=value:raise AdapterRefusal('adapter:controller-row: %r'%((pre,abstract_action(d)),))
    return rows


def run_assumption_split(ctx):
    """Split `forall sigma: A(sigma) => body` into the named premise and body.

    The unbounded run assumption cannot be lowered (it is not an event
    observation); the Lean obligation states it as an explicit premise over
    the model's runs instead, and the body lowers through the existing route.
    """
    tree=l.parse(ctx.data['formula'],'typed',ctx)
    expanded=l.expand(tree,ctx,derived=False)
    if expanded[0]!='forall' or expanded[3][0]!='=>' or ctx.data['trace']['mode']!='unbounded':
        raise AdapterRefusal('adapter:run-assumption-shape')
    premise,body=expanded[3][1],expanded[3][2]
    if premise[0]!='call' or ctx.symbol(premise[1])['observation']!='trace':
        raise AdapterRefusal('adapter:run-assumption-shape')
    data=l.plain_data(ctx.data)
    data['assumptions']={'formula':'true','definition':'Consequent of '+premise[1]+'(sigma); '+premise[1]+' is an explicit premise of the Lean theorem.','version':ctx.data['assumptions']['version']}
    inner=l.Context.from_data(data)
    consequent=('forall',expanded[1],expanded[2],body)
    obligation=lower(l.parse(l.render(consequent,'typed',inner),'typed',inner),inner)
    if obligation.source is None:raise AdapterRefusal('adapter:consequent-lowering: '+'; '.join(obligation.obligations))
    return premise[1],obligation


LEAN_TEMPLATE=Path(__file__).with_name('withdrawal_pilot.lean')
LEAN_HEADER='-- zeno-fcis/lean-obligation/1\n-- claim-id 2\n'
LEAN_EXPORTED_THEOREM='theorem claim_2_checked'
LEAN_LANE={EMPTY:'.empty',ARRIVED:'.arrived',PENDING:'.pending'}
LEAN_SIDE={170:'.a',171:'.b','lane_a':'.a','lane_b':'.b'}
LEAN_PAUSE={0:'.p0',1:'.p1',2:'.p2'}
SIDES=('lane_a','lane_b')


def lean_ctrl(c):
    return '⟨%s, %s, %s, %s, %s⟩'%(LEAN_LANE[c[0]],LEAN_LANE[c[1]],LEAN_PAUSE[c[2]],str(bool(c[3])).lower(),LEAN_SIDE[c[4]])


def lean_act(a):
    if a[0]=='tick':return '.tick '+str(a[1]).lower()
    return '.request '+LEAN_SIDE[a[1]] if a[0]=='request' else '.idle'


def lean_list(items):
    return '[\n  '+',\n  '.join(items)+']'


def occupied(c,side):return c[SIDES.index(side)]!=EMPTY


def model_step(rows,c,act):
    """The contract row, or the model's no-op for a request on a held lane."""
    if (c,act) in rows:return rows[c,act]
    if act[0]=='request' and occupied(c,act[1]):return (c,(False,False),(False,False))
    raise AdapterRefusal('adapter:controller-row-missing: %r'%((c,act),))


def potentials(rows,reach,side,limit=64):
    """Worst-case ticks until `side` is paid, by monotone iteration.

    Returns the last iterate and whether it converged; the Lean kernel checks
    the progress properties either way, so an unconverged table fails there.
    """
    index=SIDES.index(side);table={c:0 for c in reach}
    for _ in range(limit):
        new={}
        for c in reach:
            value=0
            if occupied(c,side):
                for alarm in (False,True):
                    post,_,paid=model_step(rows,c,('tick',alarm))
                    value=max(value,1 if paid[index] else 1+table[post])
                for act in (('request','lane_a'),('request','lane_b'),('idle',)):
                    value=max(value,table[model_step(rows,c,act)[0]])
            new[c]=value
        if new==table:return table,True
        table=new
    return table,False


def lean_source(graph,exported,template=None):
    """The exported claim, unchanged up to its unproved theorem, plus the model."""
    if not exported.startswith(LEAN_HEADER) or exported.count(LEAN_EXPORTED_THEOREM)!=1:
        raise AdapterRefusal('adapter:lean-export-shape')
    prefix=exported.split(LEAN_EXPORTED_THEOREM)[0]
    template=LEAN_TEMPLATE.read_text() if template is None else template
    rows=controller_rows(graph);reach=sorted({ctrl(v) for v in graph['vaults']})
    tables={};converged=True
    for side in SIDES:
        table,done=potentials(rows,reach,side);converged&=done
        tables[side]=[(c,table[c]) for c in reach if occupied(c,side)]
    def obs(r,c):return '⟨%s, %s, %s, %s⟩'%tuple(str(v).lower() for v in (r[0],c[0],r[1],c[1]))
    fills={'@@ROWS@@':lean_list('(%s, %s, %s, %s)'%(lean_ctrl(pre),lean_act(act),lean_ctrl(post),obs(r,c)) for (pre,act),(post,r,c) in sorted(rows.items(),key=repr)),
           '@@REACH@@':lean_list(lean_ctrl(c) for c in reach),
           '@@POT_A@@':lean_list('(%s, %d)'%(lean_ctrl(c),v) for c,v in tables['lane_a']),
           '@@POT_B@@':lean_list('(%s, %d)'%(lean_ctrl(c),v) for c,v in tables['lane_b'])}
    for marker,text in fills.items():
        if template.count(marker)!=1:raise AdapterRefusal('adapter:lean-template-marker: '+marker)
        template=template.replace(marker,text)
    info={'rows':len(rows),'reach':len(reach),'potential_converged':converged,
          'bound':max(v for side in SIDES for _,v in tables[side])}
    return prefix+template,info


def lean_axioms(text):
    """The single axiom report, as the repository's Lean adapter reads it."""
    reports=[line.split('depends on axioms:',1)[1].strip() for line in text.splitlines() if 'depends on axioms:' in line]
    if 'does not depend on any axioms' in text and not reports:return []
    if len(reports)!=1 or not reports[0].startswith('[') or not reports[0].endswith(']'):return None
    return sorted({item.strip() for item in reports[0][1:-1].split(',') if item.strip()})


LEAN_REFUTATION=Path(__file__).with_name('withdrawal_pilot_refutation.lean')
LIVENESS='-- @@LIVENESS@@'
# Lean's standard foundational axioms; the report must name exactly these.
ALLOWED_AXIOMS=['Classical.choice','Quot.sound','propext']


def refutation_source(graph,exported,template=None):
    """The model section followed by the frozen-run refutation."""
    full,info=lean_source(graph,exported,template)
    if full.count(LIVENESS)!=1:raise AdapterRefusal('adapter:lean-template-marker: '+LIVENESS)
    return full.split(LIVENESS)[0]+LEAN_REFUTATION.read_text(),info


def run_lean(source,lean,theorem):
    """Run one Lean file; kernel checked only with the exact axiom report."""
    with tempfile.TemporaryDirectory(prefix='zal-withdrawal-lean-') as directory:
        path=Path(directory)/'WithdrawalPilot.lean';path.write_text(source)
        result=subprocess.run([str(lean),str(path)],capture_output=True,text=True,timeout=1800)
    text=result.stdout+result.stderr
    axioms=lean_axioms(text) if "'ZenoFCIS.WithdrawalPilot."+theorem+"'" in text else None
    errors=[line for line in text.splitlines() if ': error:' in line]
    status='kernel-checked' if result.returncode==0 and not errors and axioms==ALLOWED_AXIOMS else 'not-checked'
    return {'status':status,'exit':result.returncode,'axioms':axioms,'errors':errors,
            'source_sha256':hashlib.sha256(source.encode()).hexdigest()}


def resettable_budget(graph):
    """Planted defect: an alarm during a pause restarts the pause."""
    vaults=graph['vaults'];index={tuple(v):i for i,v in enumerate(vaults)};rows=[]
    for row in graph['decisions']:
        d=dict(zip(('from','action','lane','amount','caller','alarm','class','reason','to','outbox'),row))
        if accepted(d,TICK) and d['alarm'] and vaults[d['from']][5]>0:
            post=list(vaults[d['to']]);post[5]=2;post[6]=0
            if tuple(post) not in index:raise AdapterRefusal('adapter:mutant-vault')
            row=row[:8]+[index[tuple(post)]]+row[9:]
        rows.append(row)
    return {**graph,'decisions':rows}


def native(executable,request):
    result=subprocess.run([str(executable)],input=json.dumps(request),capture_output=True,text=True,timeout=3600)
    if result.returncode:raise AdapterRefusal('adapter:native-exit: '+result.stderr[-2000:])
    output=json.loads(result.stdout)
    if output.get('authority')!='none' or 'refusal' in output:raise AdapterRefusal('adapter:native: '+str(output.get('refusal')))
    return output


def finite_check(executable,ctx,automaton,length,show=3):
    """Replay every word of lengths 1..length through the Rust evaluator."""
    obligation=lower(l.parse(ctx.data['formula'],'typed',ctx),ctx)
    if obligation.source is None:raise AdapterRefusal('adapter:lowering: '+'; '.join(obligation.obligations))
    paths=[path for path,_ in statement_paths(ctx)]
    report=native(executable,{'source':obligation.source,'paths':paths,'automaton':automaton,'max_length':length,'show':show})
    report['lowering_subject']=obligation.subject
    return report


def variant(ctx,formula,definitions=None):
    """The same scope with another formula; used by tightness and endpoint checks."""
    data=l.plain_data(ctx.data);data['formula']=formula;data['readings']=[]
    if definitions is not None:data['definitions']=definitions
    return l.Context.from_data(data)


def words(automaton,length):
    """Every distinct word of lengths 1..length, by lazy determinization."""
    edges={}
    for source,letter,target in automaton['edges']:edges.setdefault(source,[]).append((tuple(letter),target))
    stack=[((),frozenset(automaton['initial']))]
    while stack:
        word,states=stack.pop()
        if word:yield word
        if len(word)==length:continue
        nxt={}
        for state in states:
            for letter,target in edges.get(state,()):nxt.setdefault(letter,set()).add(target)
        stack.extend((word+(letter,),frozenset(targets)) for letter,targets in nxt.items())


def in_language(automaton,word):
    states=set(automaton['initial'])
    for letter in word:
        states={t for s,a,t in automaton['edges'] if s in states and tuple(a)==tuple(letter)}
        if not states:return False
    return True


def window_oracle(word,k,guarded,columns):
    """Independent reading: each presented request is paid within k ticks.

    `columns` gives the (request, completion) letter positions per lane.
    Guarded: only windows with k further recorded ticks are judged.
    Unguarded: a window cut short by the end of the word must still pay.
    """
    n=len(word)
    for request,completion in columns:
        for p in range(n):
            if not word[p][request]:continue
            if guarded and p+k>=n:continue
            if not any(word[q][completion] for q in range(p,min(p+k,n-1)+1)):return False
    return True


if __name__=='__main__':
    print(contract_identity())
