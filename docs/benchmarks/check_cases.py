#!/usr/bin/env python3
"""Development-only reference fixture check. Not a production checker/receipt.
Python big integers are range-checked immediately after EVERY Add/Sub.
No optimizer, native execution, resource meter, authority or canonical codec.
"""
import argparse, hashlib, itertools, json, re
from pathlib import Path
ROOT=Path(__file__).resolve().parent
MIN=-(1<<63); MAX=(1<<63)-1
DEC=re.compile(r'(?:0|-[1-9][0-9]*|[1-9][0-9]*)\Z')
def integer(x):
    assert type(x) is str and DEC.fullmatch(x), ('decimal-string',x)
    v=int(x); assert MIN<=v<=MAX, ('i64',x); return v
def domain(d):
    if d=={'kind':'Bool'}: return ('Bool',0,1)
    assert set(d)=={'kind','min','max'} and d['kind']=='Int'
    lo=integer(d['min']); hi=integer(d['max']); assert lo<=hi
    return ('Int',lo,hi)
def validate(c,p):
    assert set(p)=={'nodes','roots'}
    inputs=list(map(domain,c['input_domains'])); outputs=list(map(domain,c['output_domains']))
    assert len(inputs)<=32 and 1<=len(outputs)<=16
    assert 1<=len(p['nodes'])<=256 and len(p['roots'])==len(outputs)
    ks=[]
    arity={'Input':1,'Int':1,'Bool':1,'Add':2,'Sub':2,'Eq':2,'Lt':2,'And':2,'Not':1,'Select':3}
    for n in p['nodes']:
        op,*a=n; assert op in arity and len(a)==arity[op]
        if c['profile']=='FunctionalBoolV1': assert op in {'Input','Bool','And','Not','Select'}
        if op=='Input':
            assert type(a[0]) is int and 0<=a[0]<len(inputs); k=inputs[a[0]][0]
        elif op=='Int': integer(a[0]); k='Int'
        elif op=='Bool': assert type(a[0]) is bool; k='Bool'
        else:
            assert all(type(i) is int and 0<=i<len(ks) for i in a)
            t=[ks[i] for i in a]
            if op in {'Add','Sub','Lt'}: assert t==['Int','Int']; k='Bool' if op=='Lt' else 'Int'
            elif op=='Eq': assert t[0]==t[1]; k='Bool'
            elif op=='And': assert t==['Bool','Bool']; k='Bool'
            elif op=='Not': assert t==['Bool']; k='Bool'
            else: assert t[0]=='Bool' and t[1]==t[2]; k=t[1]
        ks.append(k)
    for d,r in zip(outputs,p['roots']): assert type(r) is int and 0<=r<len(ks) and ks[r]==d[0]
    if c['profile']=='FunctionalBoolV1': assert len(inputs)<=6 and all(d[0]=='Bool' for d in inputs+outputs)
    return inputs,outputs

def scalar(c,p,x):
    ids=list(map(domain,c['input_domains'])); ods=list(map(domain,c['output_domains']))
    if len(ids)!=len(x) or any(not lo<=v<=hi for (_,lo,hi),v in zip(ids,x)): return {'error':'InputDomain'}
    v=[]
    for n in p['nodes']:
        op,*a=n
        if op=='Input': z=x[a[0]]
        elif op=='Int': z=int(a[0])
        elif op=='Bool': z=int(a[0])
        elif op=='Not': z=int(v[a[0]]==0)
        elif op=='Select': z=v[a[1]] if v[a[0]]==1 else v[a[2]]
        elif op=='Add':
            z=v[a[0]]+v[a[1]]
            if not MIN<=z<=MAX: return {'error':'Arithmetic'}
        elif op=='Sub':
            z=v[a[0]]-v[a[1]]
            if not MIN<=z<=MAX: return {'error':'Arithmetic'}
        elif op=='Eq': z=int(v[a[0]]==v[a[1]])
        elif op=='Lt': z=int(v[a[0]]<v[a[1]])
        elif op=='And': z=int(v[a[0]]==1 and v[a[1]]==1)
        else: raise AssertionError(op)
        v.append(z)
    out=[v[r] for r in p['roots']]
    if any(not lo<=z<=hi for (_,lo,hi),z in zip(ods,out)): return {'error':'OutputDomain'}
    return {'ok':[bool(z) if d[0]=='Bool' else str(z) for d,z in zip(ods,out)]}

def main():
    ap=argparse.ArgumentParser();ap.add_argument('--write-witnesses',action='store_true');a=ap.parse_args()
    path=ROOT/'cases.json'; data=json.loads(path.read_text()); assert data['schema_version']=='zenofcis-benchmark-seeds-v1'
    cases=data['cases']; assert len(cases)==32 and len({c['id'] for c in cases})==32
    report=[]
    for c in cases:
        ins,outs=validate(c,c['original']);validate(c,c['candidate'])
        assert len(c['abi']['inputs'])==len(ins) and len(c['abi']['outputs'])==len(outs)
        assert len(set(c['abi']['inputs']))==len(ins) and len(set(c['abi']['outputs']))==len(outs)
        count=1
        for _,lo,hi in ins: count*=hi-lo+1
        assert 1<=count<=(64 if c['profile']=='FunctionalBoolV1' else 65536)
        first=None; different=0; errors={'original':{},'candidate':{}}
        for ordinal,x in enumerate(itertools.product(*(range(lo,hi+1) for _,lo,hi in ins))):
            p=scalar(c,c['original'],x); q=scalar(c,c['candidate'],x)
            for side,o in [('original',p),('candidate',q)]:
                if 'error' in o: errors[side][o['error']]=errors[side].get(o['error'],0)+1
            if p!=q:
                different+=1
                if first is None: first={'ordinal':ordinal,'tuple':[bool(z) if d[0]=='Bool' else str(z) for d,z in zip(ins,x)],'original':p,'candidate':q}
        assert ordinal+1==count
        assert (different==0)==(c['expected_relation']=='Equivalent'),c['id']
        if a.write_witnesses: c['witness']=first
        else: assert c['witness']==first,('stale-witness',c['id'])
        if c['role']=='positive': assert len(c['candidate']['nodes'])<len(c['original']['nodes']),('node-target',c['id'])
        report.append({'id':c['id'],'tuples':count,'different_tuples':different,'original_nodes':len(c['original']['nodes']),'candidate_nodes':len(c['candidate']['nodes']),'errors':errors})
    if a.write_witnesses: path.write_text(json.dumps(data,indent=2)+'\n')
    result={'status':'development-fixtures-checked','production_checker_qualified':False,'cases':len(cases),'boolean':sum(c['profile']=='FunctionalBoolV1' for c in cases),'checked_i64':sum(c['profile']=='CheckedI64V1-proposed' for c in cases),'tuple_pairs':sum(r['tuples'] for r in report),'equivalent_pairs':sum(r['different_tuples']==0 for r in report),'different_pairs':sum(r['different_tuples']>0 for r in report),'cases_sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'rows':report}
    (ROOT/'fixture-check.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps({k:v for k,v in result.items() if k!='rows'},sort_keys=True))
if __name__=='__main__': main()
