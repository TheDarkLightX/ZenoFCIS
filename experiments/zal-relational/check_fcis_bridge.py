import json
import subprocess
from pathlib import Path
import build
import run

ROOT=Path(__file__).resolve().parent

def main():
    out=ROOT/'.build/observation-bridge';out.mkdir(parents=True,exist_ok=True)
    (ROOT/'evidence').mkdir(exist_ok=True)
    command=['ghc','-O1','-Wall','-Werror','-Wincomplete-patterns','-i'+str(ROOT/'src'),
             '-outputdir',str(out),'-o',str(out/'bridge'),str(ROOT/'tests/ObservationBridge.hs')]
    compiled=subprocess.run(command,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=60,preexec_fn=build.limits)
    (ROOT/'evidence/observation-bridge-build.log').write_bytes(compiled.stdout)
    if compiled.returncode:raise SystemExit(compiled.stdout.decode())
    executed=subprocess.run([str(out/'bridge')],stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=60,preexec_fn=run.child_limits(60))
    if executed.returncode:raise SystemExit(executed.stderr.decode())
    examples={}
    for k in range(3):
        for m in range(1,4):
            path=ROOT/f'fcis/contracts/observation_k{k}_m{m}/tests/decision-examples.txt'
            for line in path.read_text().splitlines():
                before,command_values,_,result,_=map(str.strip,line.split('|'))
                delta,mode=map(int,command_values.split());verdict,reason,after=result.split()
                examples[(k,m,int(before),delta,mode)]=(0 if verdict=='accept' else int(reason),int(after))
    seen=set()
    for line in executed.stdout.decode().splitlines():
        k,m,a,d,mode,reason,after=map(int,line.split());key=(k,m,a,d,mode)
        if key in seen or examples.get(key)!=(reason,after):raise AssertionError((key,reason,after,examples.get(key)))
        seen.add(key)
    if seen!=examples.keys():raise AssertionError('incomplete transition coverage')
    records=[ROOT/'src/Zal/Logic/Syntax.hs',ROOT/'tests/ObservationBridge.hs',Path(__file__),
             *sorted((ROOT/'fcis/contracts').glob('observation_*/tests/decision-examples.txt'))]
    report={'inputs':len(seen),'command':command,'haskell_binary_sha256':build.sha(out/'bridge'),
            'sources':{str(p.relative_to(ROOT)):build.sha(p) for p in records},
            'scope':'same Haskell observation bodies agree with all 132 native FCIS observation examples; finite parameters only'}
    (ROOT/'evidence/observation-bridge.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report,indent=2))

if __name__=='__main__':main()
