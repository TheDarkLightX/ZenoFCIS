"""Small fixed-formula scaling series; retains resource refusals as refusals."""
import copy
import json
import time
from pathlib import Path
import run

ROOT=Path(__file__).resolve().parent
def main():
    base=json.loads((ROOT/'examples/even-graph-count.json').read_text())
    results=[]
    for n in [4,8,12,16,20,24]:
        q=copy.deepcopy(base);q['domain_size']=str(n)
        started=time.monotonic()
        result=run.count_query(q)
        elapsed=time.monotonic()-started
        expected=str(2**((n-1)*(n-2)//2))
        if result['status']=='ExactCount' and result['value']!=expected:raise AssertionError(result)
        if result['status']!='ExactCount' and 'value' in result:raise AssertionError('partial exact value')
        results.append({'vertices':n,'wall_seconds':elapsed,'expected':expected,'result':result})
        print(json.dumps({'vertices':n,'seconds':elapsed,'status':result['status'],'value':result.get('value')}),flush=True)
    record={'source_sha256':run.sha(Path(__file__).read_bytes()),'runs':results,
            'method':'one end-to-end local run per size, including process startup and source/binary hashes; not a comparative speedup benchmark'}
    (ROOT/'evidence/scaling.json').write_text(json.dumps(record,indent=2)+'\n')
if __name__=='__main__':main()
