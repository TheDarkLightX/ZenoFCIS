#!/usr/bin/env python3
"""Independent owner/traversal integer references. Finite witnesses, no EVM claim."""
import sys
sys.dont_write_bytecode=True
import argparse,copy,itertools,subprocess
from pathlib import Path
from position_reference import emit,rid,rp,initial,offset
Q=10**18;H=10**20;M=2**128-1;F=5*10**15
ROOT=Path(__file__).resolve().parents[1]
def ratio(c,d,p=10**8):return c*p*10**10//d if d else M
def recovery(s,p):return s['debt']>0 and ratio(s['collateral'],s['debt'],p)<15*Q//10
def system(c=10000*Q,d=4000*Q,n=3):return dict(collateral=c,debt=d,active=n,stakes=10000*Q,stakes_snapshot=0,collateral_snapshot=0,reward_c=0,reward_d=0)
def vault(i=1,c=1200*Q,d=600*Q,reserve=50*Q):return dict(parameter_version=1,id=i,owner=i,status='Active',collateral=c,debt=d,reserve=reserve,stake=c,snapshot_c=0,snapshot_d=0)
def rv(v):return emit('v::Vault',{**v,'id':rid(v['id']),'owner':rid(v['owner']),'status':'v::Status::'+v['status']})
def effective(v,s):
    v=copy.deepcopy(v);v['collateral']+=v['stake']*(s['reward_c']-v['snapshot_c'])//Q;v['debt']+=v['stake']*(s['reward_d']-v['snapshot_d'])//Q;v['snapshot_c']=s['reward_c'];v['snapshot_d']=s['reward_d'];return v

def risk(v,old,s,post,p,withdraw,borrow,opened):
    icr=ratio(v['collateral'],v['debt'],p)
    if recovery(s,p):
        if withdraw:return 1042
        if opened or borrow:
            if icr<15*Q//10 or old and icr<ratio(old['collateral'],old['debt'],p):return 1043
    elif icr<11*Q//10 or ratio(post['collateral'],post['debt'],p)<15*Q//10:return 1043

def owner_out(pre,s,base,price,pending,status,op,cx,dx,wallet,maxfee):
    s=copy.deepcopy(s);post=copy.deepcopy(s);fee=dict(base=base,last_epoch=0)
    bad=lambda delta:delta[0]!='None' and delta[1]==0
    if op=='Open':
        if not cx[1] or not dx[1]:return 1010
        if pre and pre['status']=='Active':return 1011
    else:
        if op=='Adjust' and (cx[0]==dx[0]=='None' or bad(cx) or bad(dx)):return 1010
        if pre['status']!='Active':return 1011
    old=effective(pre,s) if pre and op!='Open' else None
    borrowing=op=='Open' or op=='Adjust' and dx[0]=='Add';withdraw=op=='Adjust' and cx[0]=='Remove'
    frozen=status!='Usable'
    if op!='Adjust' or borrowing or withdraw or not frozen:
        if frozen:return dict(Unavailable=1020,Stale=1021,Disputed=1022,Uncertain=1023,OverBudget=1024)[status]
    rec=recovery(s,price) if borrowing else False
    if borrowing and (maxfee>Q or (not rec and maxfee<F)):return 1030
    f=0;mint=burn=cin=cout=rm=rb=pc=pd=0
    def stake(c):return c*s['stakes_snapshot']//s['collateral_snapshot'] if s['collateral_snapshot'] else c
    if op=='Open':
        c,x=cx[1],dx[1]
        if c>wallet[0]:return 1033
        f=0 if rec else min(F+base,Q//20)*x//Q
        if x+f<500*Q:return 1040
        out=vault(1,c,x+f+50*Q);out.update(stake=stake(c),snapshot_c=s['reward_c'],snapshot_d=s['reward_d'])
        post.update(collateral=s['collateral']+c,debt=s['debt']+out['debt'],active=s['active']+1,stakes=s['stakes']+out['stake'])
        for p in [price]+([pending] if pending else []):
            r=risk(out,None,s,post,p,False,True,True)
            if r:return r
        if f*Q//x>maxfee:return 1052
        mint=x;cin=c;rm=50*Q
    elif op=='Close':
        net=old['debt']-old['reserve']
        if wallet[1]<net:return 1033
        if s['active']<=1:return 1041
        post.update(collateral=s['collateral']-old['collateral'],debt=s['debt']-old['debt'],active=s['active']-1,stakes=s['stakes']-old['stake'])
        for p in [price]+([pending] if pending else []):
            if recovery(s,p):return 1042
            if ratio(post['collateral'],post['debt'],p)<15*Q//10:return 1043
        out=copy.deepcopy(old);out['status']='ClosedOwner'
        for k in ('collateral','debt','reserve','stake','snapshot_c','snapshot_d'):out[k]=0
        burn=net;cout=old['collateral'];rb=old['reserve']
    else:
        out=copy.deepcopy(old)
        if cx[0]=='Add':
            if cx[1]>wallet[0]:return 1033
            cin=cx[1];out['collateral']+=cin
        elif cx[0]=='Remove':
            if cx[1]>old['collateral']:return 1033
            cout=cx[1];out['collateral']-=cout
        if dx[0]=='Add':
            mint=dx[1];f=0 if rec else min(F+base,Q//20)*mint//Q;out['debt']+=mint+f
        elif dx[0]=='Remove':
            burn=dx[1]
            if burn>wallet[1]:return 1033
            if burn>old['debt']-old['reserve']:return 1040
            out['debt']-=burn
        if out['debt']-out['reserve']<500*Q:return 1040
        out['stake']=stake(out['collateral']);post.update(collateral=s['collateral']+cin-cout,debt=s['debt']+mint+f-burn,stakes=s['stakes']-pre['stake']+out['stake'])
        if not frozen:
            for p in [price]+([pending] if pending and (borrowing or withdraw) else []):
                r=risk(out,old,s,post,p,withdraw,borrowing,False)
                if r:return r
        if borrowing and f*Q//mint>maxfee:return 1052
    if old:pc=old['collateral']-pre['collateral'];pd=old['debt']-pre['debt']
    return dict(vault=rv(out),system=emit('v::System',post),fee=emit('fees::State',fee),minted=mint,burned=burn,fee_paid=f,collateral_in=cin,collateral_out=cout,reserve_minted=rm,reserve_burned=rb,pending_collateral=pc,pending_debt=pd,nicr=out['collateral']*H//out['debt'] if out['debt'] else 0)

def owner_cases():
    scenarios=[(system(),0,None,'Usable'),(system(),Q,None,'Usable'),(system(2800*Q,2000*Q),0,None,'Usable'),(system(),0,50_000_000,'Usable')]
    for status in ('Unavailable','Stale','Disputed','Uncertain','OverBudget'):scenarios.append((system(),0,None,status))
    for key in ('reward_c','reward_d'):
        s=system();s[key]=Q//100;scenarios.append((s,0,None,'Usable'))
    s=system();s.update(stakes_snapshot=2000*Q,collateral_snapshot=1000*Q);scenarios.append((s,0,None,'Usable'))
    scenarios.append((system(n=1),0,None,'Usable'))
    none=('None',0);add=lambda n:('Add',n);rem=lambda n:('Remove',n)
    commands=[('Open',add(2000*Q),add(500*Q),Q),('Open',add(500*Q),add(500*Q),Q),('Open',add(2000*Q),add(100*Q),F),('Open',add(0),add(500*Q),Q),('Open',add(2000*Q),add(498*Q),Q),('Open',add(2000*Q),add(500*Q),0),('Open',add(2000*Q),add(500*Q),F),('Adjust',add(200*Q),none,Q),('Adjust',none,rem(50*Q),Q),('Adjust',none,rem(51*Q),Q),('Adjust',add(300*Q),add(100*Q),Q),('Adjust',none,add(100*Q),Q),('Adjust',rem(10*Q),none,Q),('Adjust',rem(10000*Q),none,Q),('Adjust',none,none,Q),('Adjust',add(0),none,Q),('Adjust',none,rem(600*Q),Q),('Close',none,none,Q)]
    rows=[]
    for s,base,pending,status in scenarios:
        for op,cx,dx,maxfee in commands:
            for wallet in ((10000*Q,10000*Q),(0,0)):
                pre=None if op=='Open' else vault();res=owner_out(pre,s,base,10**8,pending,status,op,cx,dx,wallet,maxfee)
                expected=f'Err(Failure::Reject({res}))' if isinstance(res,int) else 'Ok('+emit('v::Outcome',res)+')'
                delta=lambda d:'v::Delta::None' if d[0]=='None' else f'v::Delta::{d[0]}({d[1]})'
                operation={'Open':f'OwnerCommand::Open({cx[1]}, {dx[1]})','Adjust':f'OwnerCommand::Adjust({delta(cx)}, {delta(dx)})','Close':'OwnerCommand::Close'}[op]
                rows.append('('+','.join(('None' if pre is None else 'Some('+rv(pre)+')',emit('v::System',s),emit('fees::State',dict(base=base,last_epoch=0)),emit('v::Prices',dict(committed=10**8,pending='None' if pending is None else f'Some({pending})',status='v::PriceStatus::'+status,pending_blocked='false')),f'({wallet[0]},{wallet[1]})',str(maxfee),operation,expected))+')')
    closed=close_record(vault(),'ClosedOwner')
    for pre,op,cx,dx,operation in [
        (closed,'Open',('Add',2000*Q),('Add',500*Q),f'OwnerCommand::Open({2000*Q},{500*Q})'),
        (vault(),'Open',('Add',2000*Q),('Add',500*Q),f'OwnerCommand::Open({2000*Q},{500*Q})'),
        (closed,'Adjust',('Add',Q),('None',0),f'OwnerCommand::Adjust(v::Delta::Add({Q}),v::Delta::None)')]:
        s=system();wallet=(10000*Q,10000*Q);res=owner_out(pre,s,0,10**8,None,'Usable',op,cx,dx,wallet,Q)
        expected=f'Err(Failure::Reject({res}))' if isinstance(res,int) else 'Ok('+emit('v::Outcome',res)+')'
        rows.append('('+','.join(('Some('+rv(pre)+')',emit('v::System',s),emit('fees::State',dict(base=0,last_epoch=0)),emit('v::Prices',dict(committed=10**8,pending='None',status='v::PriceStatus::Usable',pending_blocked='false')),f'({wallet[0]},{wallet[1]})',str(Q),operation,expected))+')')
    return 'fn owner_cases()->Vec<(Option<v::Vault>,v::System,fees::State,v::Prices,(u128,u128),u128,OwnerCommand,Result<v::Outcome,Failure>)>{vec!['+',\n'.join(rows)+']}\n',len(rows)

def book(vs,default_c=0,default_d=0):
    s=system(sum(v['collateral'] for v in vs)+default_c,sum(v['debt'] for v in vs)+default_d,len(vs));s['stakes']=sum(v['stake'] for v in vs)
    pool=initial();pool['total']=0
    b=dict(vaults=vs,system=s,default_collateral=default_c,default_debt=default_d,error_c=0,error_d=0,gas_balance=sum(v['reserve'] for v in vs),surplus=[],pool=pool)
    b['order']=sorted((v['id'] for v in vs),key=lambda i:-(next(v for v in vs if v['id']==i)['collateral']*H//next(v for v in vs if v['id']==i)['debt']))
    freshen(b);return b

def fresh(b,i):
    v=effective(next(v for v in b['vaults'] if v['id']==i),b['system']);return v['collateral']*H//v['debt']
def freshen(b):b['priorities']=[(v['id'],fresh(b,v['id'])) for v in b['vaults'] if v['status']=='Active']
def rbk(b):
    fields=dict(vaults='vec!['+','.join(rv(v) for v in b['vaults'])+']',order='vec!['+','.join(rid(i) for i in b['order'])+']',priorities='vec!['+','.join('('+rid(i)+','+str(p)+')' for i,p in b['priorities'])+']',system=emit('v::System',b['system']))
    fields.update({k:b[k] for k in ('default_collateral','default_debt','error_c','error_d','gas_balance')})
    fields['surplus']='vec!['+','.join('('+rid(i)+','+str(n)+')' for i,n in b['surplus'])+']';fields['pool']=rp(b['pool']);return emit('l::Book',fields)
def close_record(v,status):
    v=copy.deepcopy(v);v['status']=status
    for k in ('collateral','debt','reserve','stake','snapshot_c','snapshot_d'):v[k]=0
    return v

def partition(v,s,available,mode):
    c,d=v['collateral'],v['debt'];icr=ratio(c,d);tcr=ratio(s['collateral'],s['debt'])
    if mode and s['active']<=1:return None
    if not mode and icr>=11*Q//10 or mode and icr>=11*Q//10 and (icr>=tcr or available<d):return None
    cap=d*11//10 if mode and icr>=11*Q//10 else c;keeper=cap//200;surplus=c-cap
    offset_d=d if surplus or mode and icr>=11*Q//10 else (0 if mode and icr<=Q else min(d,available))
    sp=cap-keeper if mode and icr>=11*Q//10 else (cap-keeper)*offset_d//d
    return dict(debt=d,collateral=c,offset=offset_d,redistributed_debt=d-offset_d,sp_collateral=sp,redistributed_collateral=cap-keeper-sp,keeper_collateral=keeper,surplus=surplus,reserve=v['reserve'])
def liquidate(pre,op,arg,issuance):
    if op=='Lowest' and arg==0 or op=='Batch' and not arg:return 1010
    if op=='Direct':
        row=next((v for v in pre['vaults'] if v['id']==arg),None)
        if not row or row['status']!='Active':return 1011
        if pre['system']['active']<=1:return 1041
    b=copy.deepcopy(pre);running=copy.deepcopy(pre['system']);mode=recovery(running,10**8);initial_mode=mode
    ids=[arg] if op=='Direct' else arg if op=='Batch' else list(reversed(pre['order']))[:arg]
    selected=[];parts=[];off=spc=rd=rc=kc=kz=0
    for i in ids:
        if op=='Lowest' and initial_mode and i==pre['order'][0]:break
        raw=next((v for v in b['vaults'] if v['id']==i),None)
        if not raw or raw['status']!='Active':continue
        v=effective(raw,b['system']);available=pre['pool']['total']-off;icr=ratio(v['collateral'],v['debt'])
        if op=='Lowest' and ((mode and icr>=11*Q//10 and available==0) or (not mode and icr>=11*Q//10)):break
        p=partition(v,running,available,mode)
        if not p:continue
        if running['active']<=1:return 1041
        b['default_collateral']-=v['collateral']-raw['collateral'];b['default_debt']-=v['debt']-raw['debt'];b['system']['stakes']-=v['stake']
        b['vaults'][b['vaults'].index(raw)]=close_record(v,'ClosedLiquidation');b['order'].remove(i)
        if p['surplus']:
            values=dict(b['surplus']);values[v['owner']]=values.get(v['owner'],0)+p['surplus'];b['surplus']=sorted(values.items())
        off+=p['offset'];spc+=p['sp_collateral'];rd+=p['redistributed_debt'];rc+=p['redistributed_collateral'];kc+=p['keeper_collateral'];kz+=p['reserve']
        running['collateral']-=p['sp_collateral']+p['keeper_collateral']+p['surplus'];running['debt']-=p['offset'];running['active']-=1
        if mode:mode=recovery(running,10**8)
        selected.append(i);parts.append(p)
    if not selected:return 1044 if op=='Direct' else 1045
    if off and pre['pool']['total'] and issuance is None:return 'unbound'
    offset(b['pool'],off,spc,issuance or 0);b['gas_balance']-=kz;b['default_collateral']+=rc;b['default_debt']+=rd
    if rd:
        for key,amount in [('c',rc),('d',rd)]:
            delta,err=divmod(amount*Q+b['error_'+key],b['system']['stakes']);b['error_'+key]=err;b['system']['reward_'+key]+=delta
    for k in ('collateral','debt','active'):b['system'][k]=running[k]
    b['system']['stakes_snapshot']=b['system']['stakes'];b['system']['collateral_snapshot']=running['collateral']
    b['order'].sort(key=lambda i:-fresh(b,i));freshen(b)
    return dict(book=rbk(b),selected='vec!['+','.join(rid(i) for i in selected)+']',partitions='vec!['+','.join(emit('l::Partition',p) for p in parts)+']',keeper_zusd=kz,keeper_collateral=kc,burned=off,redistributed_debt=rd,redistributed_collateral=rc)

def liquidation_cases():
    shapes=[[(300,100),(105,100),(90,100)],[(189,100),(110,100),(210,200)],[(94,50),(55,50),(120,100)],[(3,2),(4,3),(2,5)],[(90,100),(90,100)],[(90,100)]]
    rows=[]
    commands=[('Direct',1),('Direct',2),('Direct',3),('Direct',9),('Lowest',0),('Lowest',1),('Lowest',3),('Batch',[]),('Batch',[3,2,1]),('Batch',[9,3,3,2,1])]
    for shape,deposit,issuance in itertools.product(shapes,(0,300),(0,None)):
        pre=book([vault(i+1,c,d,1) for i,(c,d) in enumerate(shape)]);pre['pool']['total']=deposit
        for op,arg in commands:
            result=liquidate(pre,op,arg,issuance)
            expected='Err(Failure::UnboundProfile)' if result=='unbound' else f'Err(Failure::Reject({result}))' if isinstance(result,int) else 'Ok('+emit('l::Outcome',result)+')'
            command=f'l::Command::{op}('+('vec!['+','.join(rid(i) for i in arg)+']' if op=='Batch' else rid(arg) if op=='Direct' else str(arg))+')'
            rows.append('('+','.join((rbk(pre),command,'None' if issuance is None else f'Some({issuance})',expected))+')')
    return 'fn liquidation_cases()->Vec<(l::Book,l::Command,Option<u128>,Result<l::Outcome,Failure>)>{vec!['+',\n'.join(rows)+']}\n',len(rows)


def insert_cut(b,i,priority,upper,lower):
    ids=[x for x in b['order'] if x!=i];ps=[fresh(b,x) for x in ids];n=len(ids)
    gaps=[j for j in range(n+1) if (j==0 or ps[j-1]>=priority) and (j==n or priority>=ps[j])]
    u=ids.index(upper) if upper in ids else None;l=ids.index(lower) if lower in ids else None
    hinted=0 if n==0 and u is None and l is None else (0 if upper is None and l==0 else n if lower is None and u==n-1 else l if u is not None and l==u+1 else None)
    if hinted in gaps:at=hinted
    else:
        if u is not None and ps[u]<priority:u=None
        if l is not None and ps[l]>priority:l=None
        if u is None and l is not None:at=n if l==n-1 and priority<=ps[-1] else max(g for g in gaps if g<=l)
        else:
            start=u or 0;at=0 if start==0 and priority>=ps[0] else min(g for g in gaps if g>=start+1)
    ids.insert(at,i);b['order']=ids

def redeem(pre,c,base=0,price=10**8,balance=10000*Q,age=20160,clock=True):
    if not c['maximum']:return 1010
    if not F<=c['max_fee']<=Q:return 1030
    if age is None:return 'unbound'
    if age<20160:return 1031
    if ratio(pre['system']['collateral'],pre['system']['debt'],price)<11*Q//10:return 1032
    if balance<c['maximum']:return 1033
    first=None
    for i in reversed(pre['order']):
        v=effective(next(v for v in pre['vaults'] if v['id']==i),pre['system'])
        if ratio(v['collateral'],v['debt'],price)>=11*Q//10:
            first=i;break

    b=copy.deepcopy(pre);remaining=c['maximum'];executed=gross=reserves=surplus=0;visited=[];closed=[];settled=[];cursor=first;stop='End';cap=pre['system']['active'] if c['visits']==0 else min(c['visits'],pre['system']['active'])
    while cursor is not None:
        if remaining==0:stop='Filled';break
        if len(visited)>=cap:stop='VisitCap';break
        i=cursor;at=b['order'].index(i);cursor=b['order'][at-1] if at else None;visited.append(i)
        raw=next(v for v in b['vaults'] if v['id']==i);v=effective(raw,b['system'])
        if ratio(v['collateral'],v['debt'],price)<11*Q//10:continue
        b['default_collateral']-=v['collateral']-raw['collateral'];b['default_debt']-=v['debt']-raw['debt'];b['vaults'][b['vaults'].index(raw)]=v;settled.append(i)
        x=min(remaining,v['debt']-v['reserve']);g=x*10**8//price;nc=v['collateral']-g;nd=v['debt']-x
        if x==v['debt']-v['reserve']:
            if b['system']['active']<=1:return 1041
            b['vaults'][b['vaults'].index(v)]=close_record(v,'ClosedRedemption');b['order'].remove(i);b['system']['active']-=1;b['system']['stakes']-=v['stake'];b['system']['collateral']-=v['collateral'];b['system']['debt']-=v['debt'];b['gas_balance']-=v['reserve'];reserves+=v['reserve'];surplus+=nc
            values=dict(b['surplus'])
            if nc:
                if v['owner'] not in values and len(values)>=5:return 1061
                values[v['owner']]=values.get(v['owner'],0)+nc;b['surplus']=sorted(values.items())
            closed.append(i)
        else:
            priority=nc*H//nd
            if nd-v['reserve']<500*Q or priority!=c['partial_nicr']:stop='CancelledPartial';break
            stake=nc if not b['system']['collateral_snapshot'] else nc*b['system']['stakes_snapshot']//b['system']['collateral_snapshot']
            new=copy.deepcopy(v);new.update(collateral=nc,debt=nd,stake=stake);b['vaults'][b['vaults'].index(v)]=new;b['system']['stakes']+=stake-v['stake'];b['system']['collateral']-=g;b['system']['debt']-=x;insert_cut(b,i,priority,c['upper'],c['lower'])
        gross+=g;executed+=x;remaining-=x
        if remaining==0:stop='Filled';break
    if executed==0 or gross==0:return 1045
    if not clock:return 'unbound'
    newbase=min(base+(gross*price*10**10//pre['system']['debt'])//2,Q)
    if not newbase:return 1050
    fee=min(F+newbase,Q)*gross//Q
    if fee>=gross:return 1051
    if fee*Q//gross>c['max_fee']:return 1052
    if gross-fee<c['min_out']:return 1053
    freshen(b)
    trace=dict(visited='vec!['+','.join(rid(i) for i in visited)+']',closed='vec!['+','.join(rid(i) for i in closed)+']',settled='vec!['+','.join(rid(i) for i in settled)+']',executed=executed,unfilled=remaining,gross=gross,reserve_burn=reserves,surplus=surplus,fee=fee,stop='r::Stop::'+stop)
    return dict(book=rbk(b),holder=emit('r::Holder',dict(zusd=balance-executed,agrs=gross-fee,staking_agrs=fee,fee=emit('fees::State',dict(base=newbase,last_epoch=0)))),trace=emit('r::Trace',trace))

def redemption_cases():
    pre=book([vault(1,2000*Q,1000*Q,50*Q),vault(2,1500*Q,1000*Q,50*Q),vault(3,1200*Q,1000*Q,50*Q)])
    books=[pre];lag=copy.deepcopy(pre);lag['system']['reward_d']=Q//100;lag['default_debt']=sum(v['stake']//100 for v in lag['vaults']);lag['system']['debt']+=lag['default_debt'];freshen(lag);lag['order'].sort(key=lambda i:-fresh(lag,i));books.append(lag)
    rows=[]
    def append(b,c,base=0,price=10**8,balance=10000*Q,age=20160,clock=True):
        result=redeem(b,c,base,price,balance,age,clock)
        expected='Err(Failure::UnboundProfile)' if result=='unbound' else f'Err(Failure::Reject({result}))' if isinstance(result,int) else 'Ok('+emit('r::Outcome',result)+')'
        command=emit('r::Command',{**c,'first':'Some('+rid(b['order'][0])+')','upper':'None' if c['upper'] is None else 'Some('+rid(c['upper'])+')','lower':'None' if c['lower'] is None else 'Some('+rid(c['lower'])+')'})
        rows.append('('+','.join((rbk(b),command,str(base),str(price),str(balance),'None' if age is None else f'Some({age})',str(clock).lower(),expected))+')')
    for b,amount,visits,good in itertools.product(books,(0,1,100*Q,450*Q,950*Q,1000*Q,1900*Q,2850*Q),(0,1,2),(False,True)):
        remaining=amount;partial=0
        for i in reversed(b['order']):
            v=effective(next(v for v in b['vaults'] if v['id']==i),b['system']);net=v['debt']-v['reserve']
            if remaining<net:
                partial=(v['collateral']-remaining)*H//(v['debt']-remaining);break
            remaining-=net
        c=dict(maximum=amount,visits=visits,first=None,partial_nicr=partial if good else 0,upper=None,lower=None,max_fee=Q,min_out=0)
        append(b,c)
    c=dict(maximum=950*Q,visits=1,first=None,partial_nicr=0,upper=None,lower=None,max_fee=Q,min_out=0)
    for changes in [dict(age=20159),dict(age=None),dict(balance=0),dict(price=50_000_000),dict(base=Q),dict(clock=False)]:append(pre,c,**changes)
    for key,value in [('max_fee',F),('max_fee',0),('min_out',M)]:append(pre,{**c,key:value})
    full=copy.deepcopy(pre);full['surplus']=[(i,1) for i in range(10,15)];append(full,c)
    tied=book([vault(1,11*H-1,30*H,1),vault(2,11*H+1,30*H,1),vault(3,11*H+1,30*H,1)])
    append(tied,{**c,'maximum':2*(30*H-1)+1,'visits':0},price=300_000_000)
    eq=book([vault(i,1500*Q,1000*Q,50*Q) for i in (1,2,3)])
    partial={**c,'maximum':100*Q,'visits':0,'partial_nicr':1400*H//900}
    for upper,lower in [(None,None),(1,None),(None,1),(None,2),(1,2),(99,99),(3,3)]:append(eq,{**partial,'upper':upper,'lower':lower})
    single=book([vault(1,1500*Q,1000*Q,50*Q)]);append(single,partial)
    eq['system']['stakes_snapshot']=2000*Q;eq['system']['collateral_snapshot']=4000*Q;append(eq,partial)
    return 'fn redemption_cases()->Vec<(l::Book,r::Command,u128,u128,u128,Option<u64>,bool,Result<r::Outcome,Failure>)>{vec!['+',\n'.join(rows)+']}\n',len(rows)


def main():
    p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');args=p.parse_args()
    for name,fn in [('owner',owner_cases),('liquidation',liquidation_cases),('redemption',redemption_cases)]:
        text,count=fn();text='// Independently generated complete native record witnesses.\n'+text
        text=subprocess.run(['rustfmt','+1.97.1','--edition','2024'],input=text,text=True,capture_output=True,check=True).stdout
        path=ROOT/'tests/test-data'/f'{name}_reference.rs'
        if args.check:assert path.read_text()==text,name
        else:path.write_text(text)
        print(f'PASS {name}: {count} complete outcome/reason reference rows')
if __name__=='__main__':main()
