#!/usr/bin/env python3
"""Finite cut corpus from an independent linked-list V1 transcription."""
import itertools,json,pathlib,sys
from ordered_redemption_reference import cut

def corpus():
 states=[];rows=[]
 for n in range(4):
  for ids in itertools.combinations(range(3),n):
   for ps in itertools.product([1,2],repeat=n):
    values=dict(zip(ids,ps))
    for order in itertools.permutations(ids):
     if any(values[a]<values[b] for a,b in zip(order,order[1:])):continue
     state=dict(items=[[id,values[id],bool(id%2)] for id in ids],order=list(order));index=len(states);states.append(state)
     for op,id,p,upper,lower in itertools.product([0,2],range(3),[1,2],[None,0,1,2],[None,0,1,2]):
      payload=not bool(id%2)
      if op==2 and id not in values:expected=dict(reason=303)
      else:
       after={k:[k,priority,bool(k%2)] for k,priority in values.items()};after[id]=[id,p,bool(id%2) if op==2 else payload]
       seq=cut(dict(order=list(order),priorities=list(values.items())),id,p,upper,lower)
       expected=dict(items=[after[k] for k in sorted(after)],order=seq)
      rows.append(dict(state=index,op=op,id=id,priority=p,payload=payload,upper=upper,lower=lower,expected=expected))
 return dict(states=states,rows=rows)
if __name__=='__main__':pathlib.Path(sys.argv[1]).write_text(json.dumps(corpus(),separators=(',',':'))+'\n')
