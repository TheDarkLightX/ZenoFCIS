#!/usr/bin/env python3
"""Independent integer position relations, emitting complete immutable test records.
The bounded domains are witness coverage, not an all-state/all-history proof.
"""
import argparse,copy,itertools,subprocess
from pathlib import Path
Q=10**18;R=10**9;CAP=35_000_000*Q
ROOT=Path(__file__).resolve().parents[1]
ACCUMULATOR_FIELDS = {'error_c','error_d','error_g','error_loss','error_s','f_a','f_z','g','reward_c','reward_d','s','snapshot_a','snapshot_c','snapshot_d','snapshot_z'}
def emit(ty,d):
    fields=[]
    for k,v in d.items():
        if isinstance(v,int) and k in ACCUMULATOR_FIELDS:
            assert 0<=v<2**256,(k,v)
            v=f'Accumulator256::from_limbs({v >> 128}, {v & (2**128-1)})'
        fields.append(k+': '+str(v))
    return ty+' { '+', '.join(fields)+' }'
def rid(x):return f'id({x})'
def snap(p=Q,s=0,g=0,e=0,scale=0):return dict(p=p,s=s,g=g,epoch=e,scale=scale)
def rs(s,front=False):return emit('sp::FrontSnapshot' if front else 'sp::Snapshot',{k:v for k,v in s.items() if not(front and k=='s')})
def initial():return dict(total=100*Q,p=Q,epoch=0,scale=0,error_s=0,error_loss=0,error_g=0,collateral=0,stock=dict(remaining=CAP,scheduled=0,allocated=0,missed=0,paid=0),history=[dict(epoch=0,scale=0,s=0,g=0)],epochs=[dict(epoch=0,last_scale=0,sealed=False)])
def current(s):return snap(s['p'],s['history'][-1]['s'],s['history'][-1]['g'],s['epoch'],s['scale'])
def issue(s,i):
    s['stock']['remaining']-=i;s['stock']['scheduled']+=i
    if s['total']==0:s['stock']['missed']+=i;return
    if i:
        s['stock']['allocated']+=i;per,err=divmod(i*Q+s['error_g'],s['total']);s['error_g']=err;s['history'][-1]['g']+=per*s['p']
def offset(s,debt,coll,i):
    if s['total']==0 or debt==0:return
    issue(s,i);a,err=divmod(coll*Q+s['error_s'],s['total']);s['error_s']=err
    loss=Q if debt==s['total'] else (debt*Q-s['error_loss'])//s['total']+1
    s['error_loss']=0 if debt==s['total'] else loss*s['total']-(debt*Q-s['error_loss'])
    s['history'][-1]['s']+=a*s['p'];factor=Q-loss
    if factor==0:
        s['epochs'][-1]['sealed']=True;s['epoch']+=1;s['scale']=0;s['p']=Q
        s['epochs'].append(dict(epoch=s['epoch'],last_scale=0,sealed=False));s['history'].append(dict(epoch=s['epoch'],scale=0,s=0,g=0))
    elif s['p']*factor//Q<R:
        s['p']=s['p']*factor*R//Q;s['scale']+=1;s['epochs'][-1]['last_scale']=s['scale'];s['history'].append(dict(epoch=s['epoch'],scale=s['scale'],s=0,g=0))
    else:s['p']=s['p']*factor//Q
    s['total']-=debt;s['collateral']+=coll

def gains(s,z,sn):
    if not z:return (0,0,0)
    cells={(x['epoch'],x['scale']):x for x in s['history']};a=cells[(sn['epoch'],sn['scale'])];n=cells.get((sn['epoch'],sn['scale']+1),dict(s=0,g=0))
    c=z*((a['s']-sn.get('s',0))+n['s']//R)//sn['p']//Q
    g=z*((a['g']-sn['g'])+n['g']//R)//sn['p']//Q
    diff=s['scale']-sn['scale'];p=0 if sn['epoch']<s['epoch'] or diff>=2 else z*s['p']//(sn['p']*(R if diff==1 else 1))
    if p<z//R:p=0
    return (p,c,g)
def rp(s):
    fields={k:v for k,v in s.items() if k not in ('stock','history','epochs')}
    fields.update(stock=emit('sp::Stock',s['stock']),history='vec!['+','.join(emit('sp::Cell',h) for h in s['history'])+']',epochs='vec!['+','.join(emit('sp::Epoch',{k:(str(v).lower() if isinstance(v,bool) else v) for k,v in h.items()}) for h in s['epochs'])+']')
    return emit('sp::State',fields)
def rb(b):
    return emit('sp::Book',dict(pool=rp(b['pool']),deposits='vec!['+','.join(emit('sp::Deposit',dict(owner=rid(d['owner']),initial=d['initial'],tag='Some('+rid(d['tag'])+')' if d['tag'] else 'None',snapshot=rs(d['snapshot']))) for d in b['deposits'])+']',front_ends='vec!['+','.join(emit('sp::FrontEnd',dict(owner=rid(d['owner']),kickback=d['kickback'],initial=d['initial'],snapshot=rs(d['snapshot'],True))) for d in b['front_ends'])+']'))
def sp_change(pre,actor,op,amount,tag,wallet,i):
    b=copy.deepcopy(pre);s=b['pool'];d=next((d for d in b['deposits'] if d['owner']==actor),None);f=next((f for f in b['front_ends'] if f['owner']==actor),None)
    if op=='Register':
        if d or f:return 1011
        if amount>Q:return 1012
        b['front_ends'].append(dict(owner=actor,kickback=amount,initial=0,snapshot=current(s)));b['front_ends'].sort(key=lambda x:x['owner'])
        return (b,(0,0,0,0),0,0,None,False)
    if op=='Deposit':
        if amount==0:return 1010
        if f:return 1011
        if tag and not any(f['owner']==tag for f in b['front_ends']):return 1012
        if wallet<amount:return 1033
    elif not d:return 1011
    if op=='Move':
        if actor not in (1,2):return 1011
        if gains(s,d['initial'],d['snapshot'])[1]==0:return 1045
    if i is None:return 'unbound'
    issue(s,i);p,c,g=gains(s,d['initial'],d['snapshot']) if d else (0,0,0)
    kept=d['tag'] if d else tag;front=next((f for f in b['front_ends'] if f['owner']==kept),None)
    k=front['kickback'] if front else Q;dep_gain=g*k//Q
    debit=amount if op=='Deposit' else 0;withdraw=min(amount,p) if op=='Withdraw' else 0;fe_gain=0
    if front:
        fp,_,fg=gains(s,front['initial'],front['snapshot']);fe_gain=fg*(Q-k)//Q;front['initial']=fp+debit-withdraw;front['snapshot']=current(s)
    s['total']+=debit-withdraw;s['collateral']-=c;s['stock']['paid']+=dep_gain+fe_gain
    new=p+debit-withdraw;b['deposits']=[d for d in b['deposits'] if d['owner']!=actor]
    if new:
        if len(b['deposits'])>=2:return 1061
        b['deposits'].append(dict(owner=actor,initial=new,tag=kept,snapshot=current(s)));b['deposits'].sort(key=lambda d:d['owner'])
    return (b,(p,c,dep_gain,fe_gain),debit,withdraw,kept,op=='Move')
def sp_cases():
    rows=[]
    commands=[(1,'Deposit',0,None,0,0),(1,'Deposit',Q,5,Q,0),(3,'Deposit',Q,None,Q,0),(1,'Deposit',Q,None,0,0),(1,'Deposit',Q,None,Q,0),(2,'Deposit',Q,3,Q,0),(4,'Deposit',Q,None,Q,0),(1,'Withdraw',0,None,0,0),(1,'Withdraw',100*Q,None,0,0),(2,'Claim',0,None,0,0),(4,'Claim',0,None,0,0),(1,'Move',0,None,0,0),(1,'Claim',0,None,0,None),(1,'Claim',0,None,0,100*Q),(3,'Register',Q//2,None,0,None),(4,'Register',Q+1,None,0,None),(4,'Register',Q,None,0,None)]
    for kick,debt in itertools.product((0,Q//2,Q),(0,30*Q,100*Q)):
        s=initial();offset(s,debt,100*Q,100*Q)
        b=dict(pool=s,deposits=[dict(owner=1,initial=60*Q,tag=3,snapshot=snap()),dict(owner=2,initial=40*Q,tag=None,snapshot=snap())],front_ends=[dict(owner=3,kickback=kick,initial=60*Q,snapshot=snap())])
        for actor,op,amount,tag,wallet,i in commands:
            result=sp_change(b,actor,op,amount,tag,wallet,i)
            command={'Deposit':f'sp::Command::Deposit {{amount:{amount},tag:'+('Some('+rid(tag)+')' if tag else 'None')+'}', 'Withdraw':f'sp::Command::Withdraw({amount})','Claim':'sp::Command::Claim','Move':'sp::Command::MoveGainToVault','Register':f'sp::Command::RegisterFrontEnd({amount})'}[op]
            if result=='unbound':expected='Err(Failure::UnboundProfile)'
            elif isinstance(result,int):expected=f'Err(Failure::Reject({result}))'
            else:
                out,g,inp,wd,fe,mv=result
                expected='Ok('+emit('sp::Outcome',dict(book=rb(out),gains=emit('sp::Gains',dict(zip(('principal','collateral','depositor_zdex','front_end_zdex'),g))),principal_in=inp,principal_out=wd,front_end='Some('+rid(fe)+')' if fe else 'None',gain_to_vault=str(mv).lower()))+')'
            rows.append('('+','.join((rb(b),rid(actor),command,str(wallet),'None' if i is None else f'Some({i})',expected))+')')
    return 'type SpPositionCase=(sp::Book,Id,sp::Command,u128,Option<u128>,Result<sp::Outcome,Failure>);\nfn sp_cases()->Vec<SpPositionCase> { vec!['+',\n'.join(rows)+'] }\n',len(rows)

def staking_cases():
    rows=[]
    for n,fz,fa,actor,op,amount,wallet in itertools.product((0,1,2),(0,Q//2,Q),(0,Q), (1,3),('Stake','Unstake','Claim'),(0,2),(0,2)):
        positions=[dict(owner=i,stake=1,snapshot_z=0,snapshot_a=0) for i in range(1,n+1)]
        pre=dict(total=n,f_z=fz,f_a=fa,custody_z=n*(fz//Q)+3,custody_a=n*(fa//Q)+5,zero_z=3,zero_a=5,positions=positions)
        post=copy.deepcopy(pre);old=next((p for p in positions if p['owner']==actor),None);reason=None
        if op=='Stake' and amount==0:reason=1010
        elif not old and op!='Stake':reason=1011
        elif op=='Stake' and amount>wallet:reason=1033
        inp=amount if op=='Stake' else 0;wd=min(amount,old['stake']) if op=='Unstake' and old else 0
        z=old['stake']*fz//Q if old else 0;a=old['stake']*fa//Q if old else 0;next_stake=(old['stake'] if old else 0)+inp-wd
        post['positions']=[p for p in positions if p['owner']!=actor]
        if next_stake:
            if len(post['positions'])>=2 and reason is None:reason=1061
            post['positions'].append(dict(owner=actor,stake=next_stake,snapshot_z=fz,snapshot_a=fa));post['positions'].sort(key=lambda p:p['owner'])
        post['total']+=inp-wd;post['custody_z']-=z;post['custody_a']-=a
        def state(s):return emit('st::State',{**{k:v for k,v in s.items() if k!='positions'},'positions':'vec!['+','.join(emit('st::Position',{**p,'owner':rid(p['owner'])}) for p in s['positions'])+']'})
        expected=f'Err(Failure::Reject({reason}))' if reason else 'Ok('+emit('st::Outcome',dict(state=state(post),stake_in=inp,stake_out=wd,zusd_gain=z,agrs_gain=a))+')'
        command='st::Command::Claim' if op=='Claim' else f'st::Command::{op}({amount})'
        rows.append('('+','.join((state(pre),rid(actor),command,str(wallet),expected))+')')
    return 'type StakingPositionCase=(st::State,Id,st::Command,u128,Result<st::Outcome,Failure>);\nfn staking_cases()->Vec<StakingPositionCase> {vec!['+',\n'.join(rows)+']}\n',len(rows)

def main():
    p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');args=p.parse_args()
    for name,fn in [('sp_positions',sp_cases),('staking_positions',staking_cases)]:
        text,count=fn();text='// Independent bounded reference rows. Regenerate with tools/position_reference.py.\n'+text
        text=subprocess.run(['rustfmt','+1.97.1','--edition','2024'],input=text,text=True,capture_output=True,check=True).stdout
        path=ROOT/'tests/test-data'/f'{name}.rs'
        if args.check:assert path.read_text()==text,name
        else:path.write_text(text)
        print(f'PASS {name}: {count} complete outcome/ordered-reason rows')
if __name__=='__main__':main()
