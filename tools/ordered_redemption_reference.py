#!/usr/bin/env python3
"""Independent exact-integer reference for pinned V1 traversal plus ROUND_B2 fixes.
Solidity pins and intentional departures are recorded in the private evidence.
This oracle neither calls Rust nor supplies native decision authority.
"""
import copy,itertools,json,pathlib,sys
Q=10**18;NQ=10**20;MCR=11*10**17;DECAY=999037758833783000

def effective(s,v):return (v['collateral']+v['stake']*(s['reward_c']-v['snapshot_c'])//Q,v['debt']+v['stake']*(s['reward_d']-v['snapshot_d'])//Q)
def priority(s,v):c,d=effective(s,v);return c*NQ//d
def nicr(c,d):return c*NQ//d
def eligible(s,v):c,d=effective(s,v);return c*s['price_e8']*10**10//d>=MCR

def cut(s,id,p,upper,lower):
    ids=[x for x in s['order'] if x!=id];ps=dict(s['priorities']);prev={x:ids[i-1] if i else None for i,x in enumerate(ids)};nxt={x:ids[i+1] if i+1<len(ids) else None for i,x in enumerate(ids)}
    def valid(u,l):
        if u is None and l is None:return not ids
        if u is None:return bool(ids) and l==ids[0] and p>=ps[l]
        if l is None:return bool(ids) and u==ids[-1] and p<=ps[u]
        return u in nxt and nxt[u]==l and ps[u]>=p>=ps[l]
    def descend(start):
        if not ids:return None,None
        if start==ids[0] and p>=ps[start]:return None,start
        u=start;l=nxt[u]
        while u is not None and not valid(u,l):u=nxt[u];l=nxt.get(u)
        return u,l
    def ascend(start):
        if start==ids[-1] and p<=ps[start]:return start,None
        l=start;u=prev[l]
        while l is not None and not valid(u,l):l=prev[l];u=prev.get(l)
        return u,l
    u,l=upper,lower
    if not valid(u,l):
        if u is not None and (u not in ids or p>ps[u]):u=None
        if l is not None and (l not in ids or p<ps[l]):l=None
        u,l=descend(ids[0] if ids else None) if u is None and l is None else ascend(l) if u is None else descend(u)
    assert valid(u,l)
    ids.insert(0 if u is None else ids.index(u)+1,id);return ids

def decmul(a,b):return (a*b+Q//2)//Q
def decay(base,minutes):
    n=min(minutes,525600000)
    if n==0:return base
    x,y=DECAY,Q
    while n>1:
        if n%2==0:x=decmul(x,x);n//=2
        else:y=decmul(x,y);x=decmul(x,x);n=(n-1)//2
    return base*decmul(x,y)//Q

def reference(pre,c,p,x,v1=False,*,mode=None):
    """V1 rules with declared units/constants, or all adopted native corrections.

    The historical v1=True argument aliases mode='v1'; False aliases
    'v1-corrected'. Solidity source is transcribed, not executed in an EVM.
    """
    mode=('v1' if v1 else 'v1-corrected') if mode is None else mode
    if mode not in ('v1','v1-corrected'):raise ValueError('unknown reference mode')
    v1=mode=='v1'
    s=copy.deepcopy(pre)
    for fails,reason in [(c['actor']!=x['actor'] or c['actor']!=s['holder'],'Unauthorized'),(c['nonce']!=s['nonce'],'Replay'),(s['nonce']==2**64-1,'NonceExhausted'),(s['price_e8']<=0 or s['price_epoch']>x['epoch'] or x['epoch']-s['price_epoch']>p['max_price_age'],'Oracle'),(c['maximum']>p['redemption_capacity'],'Capacity'),(not Q//200<=c['max_fee']<=Q,'FeeDomain'),(x['epoch']<p['bootstrap_epoch'],'Bootstrap'),((s['active_collateral']+s['default_collateral'])*s['price_e8']*10**10//(s['active_debt']+s['default_debt'])<MCR,'Tcr'),(c['maximum']<=0,'Amount'),(s['holder_balance']<c['maximum'],'Balance')]:
        if fails:return {'reason':reason}
    vs={v['id']:v for v in s['vaults']};eligible_ids=[id for id in reversed(s['order']) if eligible(s,vs[id])];first=eligible_ids[0] if eligible_ids else None
    hint_used=c['first'] is not None and c['first']==first
    if v1 and c['first'] in s['order'] and eligible(s,vs[c['first']]):
        j=s['order'].index(c['first']);nextid=s['order'][j+1] if j+1<len(s['order']) else None
        if nextid is None or not eligible(s,vs[nextid]):first=c['first'];hint_used=True
    tr=dict(visited=[],closed=[],first=first,first_hint_used=hint_used,stop='End',executed=0,unfilled=c['maximum'],gross_collateral=0,reserve_burn=0,surplus=0,fee=0,settled=[])
    cap=len(s['order']) if c['max_visits']==0 else min(c['max_visits'],len(s['order']))
    traversal=list(reversed(s['order'][:s['order'].index(first)+1])) if first is not None else []
    for id in traversal:
        if not tr['unfilled']:tr['stop']='Filled';break
        if len(tr['visited'])==cap:tr['stop']='VisitCap';break
        tr['visited'].append(id);v=vs[id]
        if not v1 and not eligible(s,v):continue
        coll,debt=effective(s,v)
        # TroveManager.hasPendingRewards checks only the collateral snapshot.
        # The corrected profile adopts the OR test, including debt-only rewards.
        pending=v['snapshot_c']<s['reward_c'] or (not v1 and v['snapshot_d']<s['reward_d'])
        if pending:
            dc,dd=coll-v['collateral'],debt-v['debt'];s['default_collateral']-=dc;s['default_debt']-=dd;s['active_collateral']+=dc;s['active_debt']+=dd;v.update(collateral=coll,debt=debt,snapshot_c=s['reward_c'],snapshot_d=s['reward_d']);tr['settled'].append(id)
        # _redeemCollateralFromTrove uses STORED amounts after _applyPendingRewards.
        # Effective quantities still govern ICR/start selection even when V1's
        # collateral-only gate leaves pending debt unapplied.
        coll,debt=v['collateral'],v['debt']
        net=debt-p['reserve'];lot=min(net,tr['unfilled']);drawn=lot*10**8//s['price_e8'];nc,nd=coll-drawn,debt-lot
        if lot==net:
            if len(s['order'])<=1:return {'reason':'LastVault'}
            tr['closed'].append(id);tr['reserve_burn']+=p['reserve'];tr['surplus']+=nc;s['active_collateral']-=drawn+nc;s['active_debt']-=lot+p['reserve'];surplus=dict(s['surplus']);surplus[v['owner']]=surplus.get(v['owner'],0)+nc;s['surplus']=sorted(surplus.items());v.update(active=False,collateral=0,debt=0,stake=0,snapshot_c=0,snapshot_d=0);s['order'].remove(id);s['priorities']=[(k,r) for k,r in s['priorities'] if k!=id]
        else:
            newp=nicr(nc,nd)
            if newp!=c['partial_nicr'] or nd-p['reserve']<p['minimum_net_debt']:tr['stop']='CancelledPartial';break
            s['order']=cut(s,id,newp,c['upper'],c['lower']);s['priorities']=[(k,newp if k==id else r) for k,r in s['priorities']];v.update(collateral=nc,debt=nd,stake=nc if not s['collateral_snapshot'] else nc*s['stakes_snapshot']//s['collateral_snapshot']);s['active_collateral']-=drawn;s['active_debt']-=lot
        tr['executed']+=lot;tr['unfilled']-=lot;tr['gross_collateral']+=drawn
        if not tr['unfilled']:tr['stop']='Filled';break
    if not tr['executed'] or not tr['gross_collateral']:return {'reason':'NoPositiveExecution'}
    seconds=(x['epoch']-pre['last_fee_epoch'])*p['epoch_seconds'];fraction=tr['gross_collateral']*pre['price_e8']*10**10//pre['supply'];s['base_rate']=min(Q,decay(pre['base_rate'],seconds//60)+fraction//2)
    if seconds>=60:s['last_fee_epoch']=x['epoch']
    rate=min(Q,Q//200+s['base_rate']);tr['fee']=rate*tr['gross_collateral']//Q
    if tr['fee']>=tr['gross_collateral']:return {'reason':'FeeConsumesAll'}
    if tr['fee']*Q//tr['gross_collateral']>c['max_fee']:return {'reason':'FeeExceeded'}
    s['holder_balance']-=tr['executed'];s['reserve_balance']-=tr['reserve_burn'];s['supply']-=tr['executed']+tr['reserve_burn'];s['holder_collateral']+=tr['gross_collateral']-tr['fee'];s['staking_collateral']+=tr['fee'];s['nonce']+=1
    return {'state':s,'trace':tr}

def base():
    vs=[dict(id=i,owner=i+10,active=True,collateral=c*Q,debt=d*Q,stake=100*Q,snapshot_c=0,snapshot_d=0) for i,c,d in [(0,3000,1550),(1,1800,1050),(2,5000,2050)]]
    s=dict(vaults=vs,order=[2,0,1],priorities=[],reward_c=0,reward_d=0,stakes_snapshot=0,collateral_snapshot=0,active_collateral=sum(v['collateral'] for v in vs),default_collateral=0,active_debt=sum(v['debt'] for v in vs),default_debt=0,holder=99,holder_balance=4500*Q,reserve_balance=150*Q,supply=sum(v['debt'] for v in vs),holder_collateral=0,staking_collateral=0,surplus=[],nonce=0,price_e8=10**8,price_epoch=100,base_rate=0,last_fee_epoch=100)
    s['priorities']=[(v['id'],priority(s,v)) for v in vs]
    p=dict(capacity=3,inventory_capacity=4,retained_bytes=10000,minimum_net_debt=500*Q,reserve=50*Q,bootstrap_epoch=0,epoch_seconds=60,max_price_age=100,redemption_capacity=10**38)
    c=dict(actor=99,nonce=0,maximum=1100*Q,max_visits=0,max_fee=Q,first=None,partial_nicr=nicr(2900*Q,1450*Q),upper=None,lower=None)
    x=dict(actor=99,epoch=100)
    return s,c,p,x

def cases():
    rows=[]
    def add(name,s,c,p,x):rows.append(dict(name=name,pre=copy.deepcopy(s),command=copy.deepcopy(c),policy=copy.deepcopy(p),context=copy.deepcopy(x),expected=reference(s,c,p,x,mode='v1-corrected')))
    def departure():
        row=rows[-1];row['v1_expected']=reference(s,c,p,x,v1=True);row['v1_corrected_expected']=reference(s,c,p,x,mode='v1-corrected')
        assert row['v1_expected']==reference(s,c,p,x,mode='v1')
        assert row['expected']==row['v1_corrected_expected'] and row['expected']!=row['v1_expected']
    s,c,p,x=base();add('partial',s,c,p,x)
    for k in [0,1,3,4]:c['max_visits']=k;add(f'cap-{k}',s,c,p,x)
    c['max_visits']=0;c['partial_nicr']=0;add('cancelled-positive-prefix',s,c,p,x)
    c['maximum']=100*Q;add('cancelled-no-prefix',s,c,p,x)
    s,c,p,x=base();c['maximum']=2000*Q;c['partial_nicr']=nicr(2000*Q,550*Q);add('debt-floor-positive-prefix',s,c,p,x)
    c['maximum']=2200*Q;add('debt-floor-refusal',s,c,p,x)
    s,c,p,x=base();s.update(reward_c=Q//100,reward_d=Q//10,default_collateral=3*Q,default_debt=30*Q,supply=s['supply']+30*Q);s['priorities']=[(v['id'],priority(s,v)) for v in s['vaults']];c['partial_nicr']=0;add('cancelled-partial-settles-pending',s,c,p,x)
    for field,reason,value in [('actor','Unauthorized',98),('nonce','Replay',1),('max_fee','FeeDomain',0),('maximum','Amount',0),('maximum','Balance',4500*Q+1)]:
        s,c,p,x=base();c[field]=value;add(reason,s,c,p,x)
    s,c,p,x=base();c['maximum']=4500*Q;add('last-vault-rolls-back-prefix',s,c,p,x)
    s,c,p,x=base();c['max_fee']=Q//200;add('fee-late-rollback',s,c,p,x)
    s['base_rate']=Q;c['max_fee']=Q;add('fee-consumes-all',s,c,p,x)
    for epoch in [101,819,820,821,1000000]:
        s,c,p,x=base();x['epoch']=epoch;s['price_epoch']=epoch;s['base_rate']=Q//10;add(f'epoch-decay-{epoch}',s,c,p,x)
    s,c,p,x=base();c['maximum']=1;s['price_e8']=10**30;c['partial_nicr']=nicr(s['vaults'][1]['collateral'],s['vaults'][1]['debt']-1);add('zero-draw',s,c,p,x)
    s,c,p,x=base()
    for v in s['vaults']:v.update(collateral=11*10**20+(-1 if v['id']==1 else 1),debt=30*10**20)
    s.update(active_collateral=sum(v['collateral'] for v in s['vaults']),active_debt=90*10**20,supply=90*10**20,holder_balance=8500*Q,price_e8=3*10**8,order=[0,1,2]);s['priorities']=[(v['id'],priority(s,v)) for v in s['vaults']];c.update(maximum=3000*Q,partial_nicr=nicr(s['vaults'][0]['collateral']-50*Q//3,2950*Q));add('tie-eligibility-departure',s,c,p,x);departure()
    c['first']=0;add('first-hint-cannot-skip-lower-eligible',s,c,p,x);departure()
    for name in ['NonceExhausted','Oracle','Capacity','Bootstrap','Tcr']:
        s,c,p,x=base()
        if name=='NonceExhausted':s['nonce']=2**64-1;c['nonce']=s['nonce']
        elif name=='Oracle':s['price_epoch']=x['epoch']+1
        elif name=='Capacity':p['redemption_capacity']=1000*Q
        elif name=='Bootstrap':p['bootstrap_epoch']=101
        elif name=='Tcr':s['price_e8']=10**6
        add(name,s,c,p,x)
    s,c,p,x=base();maximum=2**127-1
    for v in s['vaults']:v.update(collateral=maximum//20,debt=maximum//12)
    s.update(active_collateral=sum(v['collateral'] for v in s['vaults']),active_debt=sum(v['debt'] for v in s['vaults']),holder_balance=maximum//5,price_e8=4*10**8,order=[0,1,2]);s['supply']=s['active_debt'];s['priorities']=[(v['id'],priority(s,v)) for v in s['vaults']]
    c['maximum']=maximum//100;c['partial_nicr']=nicr(s['vaults'][2]['collateral']-c['maximum']//4,s['vaults'][2]['debt']-c['maximum']);add('i128-wide-boundary',s,c,p,x)
    s,c,p,x=base()
    v=s['vaults'][0];v.update(collateral=2**127+102,debt=2**127+101)
    s.update(active_collateral=sum(v['collateral'] for v in s['vaults']),active_debt=sum(v['debt'] for v in s['vaults']),price_e8=4*10**8,base_rate=Q//1000)
    s['supply']=s['active_debt'];s['priorities']=[(v['id'],priority(s,v)) for v in s['vaults']]
    s['order']=sorted(s['order'],key=lambda k:-dict(s['priorities'])[k])
    c.update(maximum=100*Q,partial_nicr=nicr(v['collateral']-25*Q,v['debt']-100*Q))
    add('u128-high-bit-vault',s,c,p,x)
    assert 'reason' not in rows[-1]['expected']
    s,c,p,x=base()
    for v in s['vaults']:v['collateral']*=2;v['debt']*=2
    s['active_collateral']*=2;s['active_debt']*=2;s['supply']*=2
    s.update(reward_d=Q//10,default_debt=30*Q,supply=s['supply']+30*Q);s['priorities']=[(v['id'],priority(s,v)) for v in s['vaults']]
    c.update(maximum=100*Q,partial_nicr=nicr(3500*Q,2010*Q))
    add('debt-only-reward-departure',s,c,p,x);departure()
    assert rows[-1]['v1_expected']==dict(reason='NoPositiveExecution')
    assert rows[-1]['expected']['state']['vaults'][1]['debt']==2010*Q
    p.update(reserve=200*Q,minimum_net_debt=1800*Q)
    add('debt-only-reward-v1-constant-control',s,c,p,x);departure()
    assert rows[-1]['v1_expected']==dict(reason='NoPositiveExecution')
    # Exhaust all collateral/debt snapshot-lag classes through the full traversal.
    # These healthy, no-first-hint cases isolate the pending-detection departure.
    for collateral_lag,debt_lag in itertools.product([False,True],repeat=2):
        s,c,p,x=base()
        for v in s['vaults']:v['collateral']*=2;v['debt']*=2
        s['active_collateral']*=2;s['active_debt']*=2;s['supply']*=2
        s.update(reward_c=Q//10 if collateral_lag else 0,reward_d=Q//10 if debt_lag else 0,default_collateral=30*Q if collateral_lag else 0,default_debt=30*Q if debt_lag else 0)
        s['supply']+=s['default_debt'];s['priorities']=[(v['id'],priority(s,v)) for v in s['vaults']]
        ec,ed=effective(s,s['vaults'][1]);c.update(maximum=100*Q,partial_nicr=nicr(ec-100*Q,ed-100*Q))
        add(f'pending-gate-{int(collateral_lag)}{int(debt_lag)}',s,c,p,x)
        row=rows[-1];row['v1_expected']=reference(s,c,p,x,v1=True);row['v1_corrected_expected']=reference(s,c,p,x,mode='v1-corrected')
        assert row['expected']==row['v1_corrected_expected']
        assert (row['expected']!=row['v1_expected'])==(debt_lag and not collateral_lag)
    s,c,p,x=base();c.update(actor=98,nonce=1,maximum=0,max_fee=0);p['bootstrap_epoch']=101;s['price_epoch']=x['epoch']+1;add('guard-precedence-many-failures',s,c,p,x)
    return rows

def redistribution_cases():
    rows=[]
    full=[(2500*Q+5,2000*Q+4),(2500*Q+6,2000*Q+5)]
    scenarios=[('Q2_SWAP_SMALL',[(3,2),(4,3)],[0,1],2,5),('Q2_TIE_SMALL',[(3,2),(4,3)],[0,1],2,4),('Q2_SWAP_V1_SCALE',full,[0,1],900*Q+2,7800*Q+18)]
    for order in itertools.permutations(range(3)):
        for debt in range(6):scenarios.append((f'tie-history-{order}-{debt}',[(3,2),(6,4),(9,6)],list(order),2,debt))
    for name,amounts,order,coll,debt in scenarios:
        s,c,p,x=base();p.update(capacity=len(amounts),inventory_capacity=len(amounts),reserve=1,minimum_net_debt=1)
        s['vaults']=[dict(id=i,owner=10+i,active=True,collateral=a,debt=b,stake=a,snapshot_c=0,snapshot_d=0) for i,(a,b) in enumerate(amounts)]
        s.update(order=order,active_collateral=sum(a for a,b in amounts),active_debt=sum(b for a,b in amounts),default_collateral=coll,default_debt=debt,holder_balance=0,reserve_balance=0)
        s['supply']=s['active_debt']+debt;s['priorities']=[(v['id'],priority(s,v)) for v in s['vaults']]
        total=sum(a for a,b in amounts);post=copy.deepcopy(s);post.update(reward_c=coll*Q//total,reward_d=debt*Q//total)
        post['priorities']=[(v['id'],priority(post,v)) for v in post['vaults']]
        keys=dict(post['priorities']);post['order']=sorted(s['order'],key=lambda id:-keys[id])
        rows.append(dict(name=name,pre=s,policy=p,expected=post))
    assert rows[0]['expected']['order']==[1,0]
    assert rows[1]['expected']['order']==[0,1]
    assert rows[1]['expected']['priorities']==[(0,NQ),(1,NQ)]
    assert rows[2]['expected']['order']==[1,0]
    return rows

def stringify(x):
    if isinstance(x,bool) or x is None:return x
    if isinstance(x,int):return str(x)
    if isinstance(x,(list,tuple)):return [stringify(a) for a in x]
    if isinstance(x,dict):return {k:stringify(v) for k,v in x.items()}
    return x
if __name__=='__main__':pathlib.Path(sys.argv[1]).write_text(json.dumps(stringify(redistribution_cases() if '--redistribution' in sys.argv[2:] else cases()),indent=2)+'\n')
