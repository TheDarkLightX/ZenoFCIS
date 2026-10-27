#!/usr/bin/env python3
"""Private generated fixtures for the closed, bounded ordered scan profile."""
import json, pathlib, sys
from table_test_projects import project, budgets, predicate, case, truth, read, eq, lit

def ordered(mode,capacity,ids=2):
    committed=mode!=0
    declarations=[f'type 200 int ID in 0..={ids-1};','type 201 int Priority in 1..=2;','type 202 int Operation in 0..=3;','type 203 bool Truth;','type 204 data Entries;','type 205 data Unit;','type 206 data Order;','type 207 data MaybeID;','type 208 data Pair;','type 209 data Items;','type 210 data Item;','type 211 int Reason in 0..=1000;']
    fields=[(1,210,'priority',201),(2,210,'metadata',205),(3,210,'payload',203),(110,100,'entries',204),(120,101,'operation',202),(121,101,'id',200),(122,101,'item',210)]
    if committed:fields += [(111,100,'order',206),(123,101,'upper',207),(124,101,'lower',207)]
    names=['i128-priority/id/1','i128-priority/committed/1','zusd-v1-nicr/committed/1']
    reasons=[303,306,307,308,314,315]
    source=project(f'ordered_{mode}_{capacity}',declarations,fields,reasons)
    bindings={'204':{'kind':'map','key':200,'value':210,'capacity':capacity,'item_bytes':63,'snapshot_bytes':29+63*capacity},'205':{'kind':'unit'},'206':{'kind':'vector','element':200,'max':capacity,'bytes':5+17*capacity},'207':{'kind':'maybe','element':200},'208':{'kind':'tuple','items':[200,210]},'209':{'kind':'vector','element':208,'max':capacity,'bytes':5+52*capacity}}
    pre,cmd,post=['input','pre',[]],['input','command',[]],['input','post',[]]
    def op(kind,extra,args):return ['ordered',kind,mode,100,extra,args]
    reason=['reason'];is_accept=eq(reason,lit(0,211))
    post_or_pre=['match_bool',is_accept,post,pre]
    relation=lambda kind:op(4,kind,[pre,cmd,post_or_pre,reason])
    laws=[predicate(400,'state',op(0,0,[pre])),predicate(401,'any',relation(0)),predicate(402,'any',relation(1)),predicate(403,'any',relation(2))]
    laws += [predicate(410+i,'any',relation(r)) for i,r in enumerate(reasons)]
    assignments={'110':['project',['payload'],110]}
    genesis={'110':[]}
    if committed:assignments['111']=['project',['payload'],111];genesis['111']=[]
    rules={'schema':'zeno-fcis/tables-ordered-policy/1','ordered_view':{'comparator':names[mode],'map':110,'sequence':111 if committed else None},'bindings':bindings,'genesis':genesis,'cases':[case(truth(),['match_edit',op(1,101,[pre,cmd]),['accept',assignments],['reject_edit']])],'laws':laws,'budgets':budgets(),'frame_bytes':[67+72*capacity if committed else 60+55*capacity,250,100]}
    return source,rules

def main():
    root=pathlib.Path(sys.argv[1]);root.mkdir(parents=True,exist_ok=True)
    for mode in range(3):
      for capacity in range(4):
        source,rules=ordered(mode,capacity)
        p=root/f'ordered-{mode}-{capacity}';(p/'v2').mkdir(parents=True,exist_ok=True)
        (p/'project.zeno').write_text(source);(p/'v2/policy.json').write_text(json.dumps(rules,indent=2)+'\n')
    source,rules=ordered(2,3,3);p=root/'ordered-v1-three';(p/'v2').mkdir(parents=True,exist_ok=True);(p/'project.zeno').write_text(source);(p/'v2/policy.json').write_text(json.dumps(rules,indent=2)+'\n')
if __name__=='__main__':main()
