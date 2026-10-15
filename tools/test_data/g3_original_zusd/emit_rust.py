#!/usr/bin/env python3
"""Emit declarative fixture data; neither this compiler nor its output grants authority."""
from pathlib import Path
import dataclasses, hashlib, json, subprocess
from compile_zusd import Graph, MAX, U128_MAX, E8, BPS, PIN, SOURCE_SHA256, UNREACHABLE_REASONS, original, compile_model, reasons

ROOT=Path(__file__).resolve().parents[3]
DEFAULT_OUT=ROOT/'crates/zeno-fcis-synthesis/tests/fixtures/g3-original-zusd'

class LawGraph(Graph):
    def observe(self, selector): return self.put('Observe',selector)
    def guarded(self, guard, selector, default): return self.put('ObserveWhen',guard,selector,default)
    def render(self,node):
        op,*args=node
        if op in ('U128','I128','Bool'):
            value=str(args[0]).lower() if type(args[0]) is bool else str(args[0])
            return f'L::Literal(A::{op}({value}))'
        if op=='Observe':return f'L::Observe(O::{args[0]})'
        if op=='ObserveWhen':return f'L::ObserveWhen({args[0]}, O::{args[1]}, {args[2]})'
        if op=='ProductsGe':return f'L::ProductsGe({list(args[0])}, {list(args[1])})'
        return f'L::{op}({", ".join(map(str,args))})'

def state_values(g,which,names):return {name:g.observe(f'{which}({110+i})') for i,name in enumerate(names)}
def bounds(g,s):return g.both(*(g.le(v,g.u(1 if name=='oracle_seen' else MAX)) for name,v in s.items()))
def supply(g,s):return g.eq(g.add(s['free_debt_e8'],s['sp_debt_e8']),s['debt_e8'])
def debt_floor(g,s):return g.either(g.eq(s['debt_e8'],g.u(0)),g.ge(s['debt_e8'],s['min_debt_open_e8']))
def solvent(g,s):
    total=g.add(g.add(s['collateral_e8'],s['sp_coll_e8']),s['protocol_collateral_e8'])
    price=g.choose(g.lt(g.u(0),s['price_e8']),s['price_e8'],g.u(E8))
    return g.products_ge((total,price,g.u(1)),(s['debt_e8'],g.u(E8),g.u(1)))
def shape(g,s):
    z=g.u(0); seen=g.eq(s['oracle_seen'],g.u(1))
    prices=g.both(g.lt(z,s['price_e8']),g.lt(z,s['price_pending_e8']),g.le(s['price_pending_e8'],s['price_e8']))
    zero=g.both(g.eq(s['price_e8'],z),g.eq(s['price_pending_e8'],z),g.eq(s['oracle_last_update_epoch'],z))
    capped=['base_rate_bps','base_rate_decay_per_epoch_bps','base_rate_borrow_bump_bps','base_rate_redeem_bump_bps','borrow_fee_max_bps','redemption_fee_max_bps','liquidation_gas_comp_bps']
    return g.both(g.le(s['oracle_last_update_epoch'],s['now_epoch']),g.le(s['base_rate_last_epoch'],s['now_epoch']),g.choose(seen,prices,zero),g.lt(z,s['mcr_bps']),g.le(s['mcr_bps'],s['ccr_bps']),g.le(s['max_debt_e8'],s['max_debt_supply_e8']),*(g.le(s[x],g.u(BPS)) for x in capped),g.le(s['borrow_fee_floor_bps'],s['borrow_fee_max_bps']),g.le(s['redemption_fee_floor_bps'],s['redemption_fee_max_bps']))

def build(out):
    native=original();names=native.ZUSD_STATE_FIELD_ORDER;g,roots,commands=compile_model(native)
    extra=[]
    for _,_,_,fields in commands:
        for name,node in fields.items():
            if name!='event' and node not in roots+extra:extra.append(node)
    numeric_roots=roots+extra
    root_index={node:i for i,node in enumerate(numeric_roots)}
    assert len(numeric_roots)<=64
    # Admission requires a branch for every code in the decision domain.
    # Preserve the original graph/reason registry for the conformance law and
    # native oracle; only its private branch selector uses contiguous codes.
    assert [tag for tag,_,_,_ in commands] == list(range(1,13))
    assert sorted(reasons().values()) == list(range(200,246))
    reason_shift=200-(len(commands)+1)
    accepting_node=g.index[('Lt',roots[0],g.index[('I128',200)])]
    numeric_nodes=[g.rust(node) for node in g.nodes]
    shift_node=len(numeric_nodes)
    numeric_nodes += [f'N::I128({reason_shift})',
                      f'N::Sub(Width::Signed128, {roots[0]}, {shift_node})',
                      f'N::Select({accepting_node}, {roots[0]}, {shift_node+1})']
    numeric_roots[0]=shift_node+2
    assert len(numeric_nodes)<=4096
    laws=[]
    def law(number,kind,scope,genesis,graph,root):laws.append((number,kind,scope,genesis,graph,root))
    for number,formula in [(500,bounds),(501,supply),(502,shape),(503,debt_floor),(504,solvent)]:
        p=LawGraph();post=state_values(p,'Post',names);law(number,'StateInvariant','Committing',True,p,formula(p,post))
    p=LawGraph();pre=state_values(p,'Pre',names);post=state_values(p,'Post',names)
    action=p.observe('Command(150)');amt=p.observe('Command(151)');deposit=p.eq(action,p.u(5));safe=p.choose(deposit,amt,p.u(0))
    tests=[p.eq(post['collateral_e8'],p.add(pre['collateral_e8'],safe))]+[p.eq(post[n],pre[n]) for n in names if n!='collateral_e8']
    law(510,'DebitCreditEffectEquality','Accept',False,p,p.choose(deposit,p.both(*tests),p.put('Bool',True)))
    p=LawGraph();pre=state_values(p,'Pre',names);post=state_values(p,'Post',names);action=p.observe('Command(150)');liquidation=p.eq(action,p.u(12))
    total=lambda s:p.add(p.add(s['collateral_e8'],s['sp_coll_e8']),s['liquidator_compensation_collateral_cum_e8'])
    law(511,'AssetConservation','Accept',False,p,p.choose(liquidation,p.eq(total(pre),total(post)),p.put('Bool',True)))
    p=LawGraph();pre=state_values(p,'Pre',names);post=state_values(p,'Post',names);action=p.observe('Command(150)')
    conserved=p.eq(p.add(pre['debt_e8'],post['sp_debt_e8']),p.add(post['debt_e8'],pre['sp_debt_e8']))
    law(512,'AssetConservation','Accept',False,p,p.choose(p.eq(action,p.u(12)),conserved,p.put('Bool',True)))
    # The original guarded deposit requirement: compute its prospective state,
    # then require the corresponding class and complete successor if enabled.
    p=LawGraph();pre=state_values(p,'Pre',names);action=p.observe('Command(150)');amt=p.observe('Command(151)')
    deposit=p.eq(action,p.u(5));bounded_amt=p.minimum(amt,p.u(MAX));prospective=pre|{'collateral_e8':p.add(pre['collateral_e8'],bounded_amt)}
    enabled=p.both(deposit,p.lt(p.u(0),amt),p.le(amt,p.u(MAX)),bounds(p,pre),shape(p,pre),bounds(p,prospective),supply(p,prospective),debt_floor(p,prospective),solvent(p,prospective))
    requirements=[p.eq(p.observe('Class'),p.i(0))]
    for i,name in enumerate(names):requirements.append(p.eq(p.guarded(enabled,f'Post({110+i})','A::U128(0)'),p.choose(enabled,prospective[name],p.u(0))))
    law(513,'DecisionConformance','Always',False,p,p.choose(enabled,p.both(*requirements),p.put('Bool',True)))
    p=LawGraph();zero=p.u(0);checks=[p.eq(p.observe(x),zero) for x in ['PostLength','PatchLength','EffectLength','OutboxLength']]
    law(900,'RejectNoAuthority','Reject',False,p,p.both(*checks))
    p=LawGraph();law(901,'CommittedFailureEffects','CommittedFailure',False,p,p.put('Bool',False))
    p=LawGraph();initial=state_values(p,'Initial',names);defaults=[int(getattr(native.ZUSDState(),n)) for n in names]
    law(902,'InitialCondition','Always',True,p,p.both(*(p.eq(initial[n],p.u(value)) for n,value in zip(names,defaults))))
    # Closed law graph: same declared command expressions, observed independently
    # from the complete frame, never a decision callback or a supplied verdict.
    p=LawGraph();p.nodes=[]
    for op,*args in g.nodes:
        if op=='Input':
            i=args[0];p.nodes.append(('Observe',f'Pre({110+i})' if i<32 else f'Command({150+i-32})'))
        else:p.nodes.append((op,*args))
    p.index={tuple(node):i for i,node in enumerate(p.nodes)}
    accepting=p.lt(roots[0],p.i(200));rejecting=p.neg(accepting)
    checks=[p.eq(p.observe('Class'),p.choose(accepting,p.i(0),p.i(1))),p.eq(p.observe('HasReason'),rejecting),p.eq(p.guarded(rejecting,'Reason','A::I128(0)'),p.choose(rejecting,roots[0],p.i(0))),p.eq(p.observe('EffectLength'),p.choose(accepting,p.u(1),p.u(0))),p.eq(p.observe('OutboxLength'),p.u(0))]
    for i in range(32):checks.append(p.eq(p.guarded(accepting,f'Post({110+i})','A::U128(0)'),p.choose(accepting,roots[i+1],p.u(0))))
    for tag,_,_,effect in commands:
        active=p.both(accepting,p.eq(p.nodes.index(('Observe','Command(150)')),p.u(tag)))
        checks.extend([p.eq(p.guarded(active,'EffectChannel(0)','A::I128(0)'),p.choose(active,p.i(400+tag),p.i(0))),p.eq(p.guarded(active,'EffectOrdinal(0)','A::I128(0)'),p.i(0)),p.eq(p.guarded(active,'EffectDestination(0)','A::Text(b"none")'),p.put('Select',active,p.put('LiteralText','audit'),p.put('LiteralText','none')))])
        for index,(name,value) in enumerate(effect.items()):
            field=1000+index
            if name=='event':
                expected=p.put('EventText',value);absent=p.put('EventText','none')
                observed=p.guarded(active,f'EffectPayload(0, {field})','A::Text(b"none")')
                checks.append(p.eq(observed,p.choose(active,expected,absent)))
            else:
                checks.append(p.eq(p.guarded(active,f'EffectPayload(0, {field})','A::U128(0)'),p.choose(active,value,p.u(0))))
    law(903,'DecisionConformance','Always',False,p,p.both(*checks))
    def render_law(graph,node):
        if node[0] in ['LiteralText','EventText']:return f'L::Literal(A::Text(b"{node[1]}"))'
        return graph.render(node)
    body=['// Generated data-only fixture. No application decision or law callback.']
    body+=['pub(crate) const NODE_DATA: &[N] = &[']+['    '+node+',' for node in numeric_nodes]+['];']
    body+=['pub(crate) const ROOTS: &[u16] = &['+', '.join(map(str,numeric_roots))+'];']
    fields=['    V2InputField { id: '+str(110+i)+', leaf: '+('BIT' if n=='oracle_seen' else 'AMOUNT')+' },' for i,n in enumerate(names)]
    body+=['pub(crate) const STATE: &[V2InputField<c::Domain<\'static>>] = &[']+fields+['];']
    body+=['pub(crate) const COMMAND: &[V2InputField<c::Domain<\'static>>] = &[V2InputField { id:150,leaf:RAW },V2InputField { id:151,leaf:RAW },V2InputField { id:152,leaf:BIT }];']
    domains=['BIT' if n=='oracle_seen' else 'AMOUNT' for n in names]
    # Context is part of the complete admitted invocation, even though this
    # model's decisions do not depend on its Boolean value.
    body+=['pub(crate) const INPUTS: &[c::Domain<\'static>] = &['+', '.join(domains+['RAW','RAW','BIT','c::Domain::Bool'])+'];']
    body+=['pub(crate) const OUTPUTS: &[c::Domain<\'static>] = &['+', '.join(['c::Domain::I128 { min:1,max:58 }']+domains+['RAW']*len(extra))+'];']
    body+=['pub(crate) const BINDINGS: &[c::Binding] = &[']+[f'    c::Binding {{ source:c::Source::{"State" if i<32 else "Command"}, selector:c::Selector::Field({110+i if i<32 else 150+i-32}) }},' for i in range(35)]+['    c::Binding { source:c::Source::Context, selector:c::Selector::Root },','];']
    body+=['pub(crate) const ASSIGNMENTS: &[c::Assignment<\'static>] = &[']+[f'    c::Assignment {{ field:{110+i}, value:c::Expr::Output({i+1}), domain:{domains[i]} }},' for i in range(32)]+['];']
    body+=['pub(crate) const CHANNELS: &[c::Channel<\'static>] = &[']
    for tag,_,_,effect in commands:
        payload=', '.join(f'c::TypedField {{ field:{1000+i}, domain:c::Domain::{"Text" if name=="event" else "U128 { min:0,max:u128::MAX }"} }}' for i,name in enumerate(effect))
        body.append(f'    c::Channel {{ id:{400+tag}, destination:c::Domain::Text, payload:&[{payload}], idempotency:c::Domain::Bytes }},')
    body+= ['];','pub(crate) const BRANCHES: &[c::Branch<\'static>] = &[']
    for tag,_,_,effect in commands:
        payload=', '.join(f'c::PayloadField {{ field:{1000+i}, value:'+ (f'c::Expr::Constant(A::Text(b"{value}"))' if name=='event' else f'c::Expr::Output({root_index[value]})')+' }' for i,(name,value) in enumerate(effect.items()))
        body.append(f'    c::Branch {{ code:{tag}, class:c::Class::Accept, reason:None, assignments:ASSIGNMENTS, effects:&[c::DeliveryPlan {{ ordinal:0, channel:{400+tag}, when:c::Expr::Constant(A::Bool(true)), destination:c::Expr::Constant(A::Text(b"audit")), payload:&[{payload}], idempotency:c::Expr::Constant(A::Bytes(b"native-event")) }}], outbox:&[] }},')
    for number in range(200,246):body.append(f'    c::Branch {{ code:{number-reason_shift},class:c::Class::Reject,reason:Some({number}),assignments:&[],effects:&[],outbox:&[] }},')
    body+= ['];','pub(crate) const REASONS: &[c::Reason] = &[']+[f'    c::Reason {{ id:{i},class:c::Class::Reject }},' for i in range(200,246)]+['];']
    for number,_,_,_,p,root in laws:
        body += [f'const LAW_{number}: &[L<\'static>] = &[']+['    '+render_law(p,node)+',' for node in p.nodes]+['];']
    body += ['pub(crate) const LAWS: &[l::Law<\'static>] = &[']+[f'    l::Law {{ id:{number},kind:l::Kind::{kind},scope:l::Scope::{scope},genesis:{str(genesis).lower()},program:l::Program {{ nodes:LAW_{number},root:{root} }} }},' for number,kind,scope,genesis,_,root in laws]+['];']
    body += ['pub(crate) const REQUIRED: &[u32] = &['+', '.join(str(x[0]) for x in laws)+'];']
    body += ['pub(crate) const GENESIS: &[u128] = &['+', '.join(map(str,defaults))+'];']
    descriptions=[]
    for typeid,title,entries in [(100,'ZusdState',[(110+i,n,105 if n=='oracle_seen' else 103) for i,n in enumerate(names)]),(101,'ZusdCommand',[(150,'action',104),(151,'amount_e8',104),(152,'auth_ok',105)])]+[(300+tag, 'Payload'+str(tag),[(1000+i,name,106 if name=='event' else 104) for i,name in enumerate(effect)]) for tag,_,_,effect in commands]:
        fields=', '.join(f's::Field {{ id:{field},name:b"{name}",type_id:{leaf} }}' for field,name,leaf in entries)
        descriptions.append((typeid,f's::Definition {{ id:{typeid},name:b"{title}",kind:s::Kind::Record(&[{fields}]) }}'))
    descriptions += [(102,'s::Definition { id:102,name:b"ZusdContext",kind:s::Kind::Bool }'),(103,f's::Definition {{ id:103,name:b"Amount",kind:s::Kind::U128 {{ min:0,max:{MAX} }} }}'),(104,'s::Definition { id:104,name:b"RawAmount",kind:s::Kind::U128 { min:0,max:u128::MAX } }'),(105,'s::Definition { id:105,name:b"Bit",kind:s::Kind::U128 { min:0,max:1 } }'),(106,'s::Definition { id:106,name:b"Text",kind:s::Kind::Text { min:0,max:128 } }')]
    body+= ['pub(crate) const DESCRIPTION: s::Description<\'static> = s::Description { profile:b"OriginalZusd",version:1,root:100,definitions:&[']+['    '+row+',' for _,row in sorted(descriptions)]+['] };']
    body+= ['pub(crate) const CHANNEL_ROOTS: &[(u32,u32,u32)] = &['+', '.join(f'({400+tag},106,{300+tag})' for tag,_,_,_ in commands)+'];']
    out.mkdir(exist_ok=True)
    data='\n'.join(body)+'\n';(out/'data.rs').write_text(data)
    # The checked-in file is rustfmt's layout of the generated text; both
    # digests are recorded, so SOURCE.json is exactly this generator's output.
    subprocess.run(['rustfmt','+1.97.1','--edition','2024',str(out/'data.rs')],check=True)
    formatted=(out/'data.rs').read_bytes()
    from check_original import characterize
    corpus_rows, characterization = characterize()
    records=[]
    registry=reasons()
    for row in corpus_rows:
        code=int(row['inputs'][32]) if row['accept'] else registry[row['reason']]
        fields=[] if not row['accept'] else [str(value) for name,value in row['effects'].items() if name!='event']
        event='' if not row['accept'] else row['effects']['event']
        records.append(' '.join(row['inputs'])+'|'+str(code)+'|'+' '.join(row['post'])+'|'+' '.join(fields)+'|'+event)
    native_cases='\n'.join(records)+'\n'
    (out/'native-cases.txt').write_text(native_cases)
    manifest={'case_format':'inputs|original_code|post|effect_numbers|event','native_revision':PIN,'native_source_sha256':SOURCE_SHA256,'numeric_inputs':36,'numeric_nodes':len(numeric_nodes),'numeric_roots':len(numeric_roots),'commands':12,'registered_reasons':46,'branch_codes':{'accept':[1,12],'reject':[13,58],'reason_shift':reason_shift},'corpus_rows':len(records),'native_corpus_sha256':characterization['corpus_sha256'],'rust_data_sha256':hashlib.sha256(data.encode()).hexdigest(),'formatted_rust_data_sha256':hashlib.sha256(formatted).hexdigest(),'exercised_reasons':sorted(registry[name] for name in characterization['observed_reasons']),'unreachable_reasons':characterization['unreachable_reasons'],'native_cases_sha256':hashlib.sha256(native_cases.encode()).hexdigest(),'characterization_only':True,'formal_native_equivalence':False,'law_nodes':{str(n):len(p.nodes) for n,_,_,_,p,_ in laws},'native_argument_boundary':'nonnegative U128; Python arguments outside this ABI are not claimed'}
    (out/'SOURCE.json').write_text(json.dumps(manifest,indent=2,sort_keys=True)+'\n')
    print(json.dumps(manifest))

if __name__=='__main__':
    import argparse
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out',type=Path,default=DEFAULT_OUT)
    args=parser.parse_args()
    build(args.out)
