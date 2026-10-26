import hashlib
import json
import re
import subprocess
from pathlib import Path
from run import child_limits

ROOT = Path(__file__).resolve().parent

def main():
    (ROOT/'evidence').mkdir(exist_ok=True)
    (ROOT/'.build').mkdir(exist_ok=True)
    path=ROOT/'proofs/Observation.lean'
    source=path.read_text()
    if re.search(r'\b(sorry|admit|axiom|unsafe)\b',source):
        raise SystemExit('placeholder or unchecked declaration')
    command=['lean','-j1','-M1024',str(path)]
    checked=subprocess.run(command,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=60,preexec_fn=child_limits(60))
    (ROOT/'evidence/lean.log').write_bytes(checked.stdout)
    if checked.returncode:raise SystemExit(checked.stdout.decode())
    if b'sorryAx' in checked.stdout:raise SystemExit('placeholder axiom in closure')
    mutant=ROOT/'.build/WrongCutoff.lean'
    mutant.write_text(source.replace('min a (k + 1) = k ↔ a = k','min a k = k ↔ a = k'))
    negative=subprocess.run(['lean','-j1','-M1024',str(mutant)],stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=60,preexec_fn=child_limits(60))
    (ROOT/'evidence/lean-negative.log').write_bytes(negative.stdout)
    if negative.returncode==0 or b'omega' not in negative.stdout or b'counterexample' not in negative.stdout:
        raise SystemExit('expected semantic failure not established: '+negative.stdout.decode())
    report={'checker':subprocess.check_output(['lean','--version'],text=True).strip(),
            'command':command,'source_sha256':hashlib.sha256(path.read_bytes()).hexdigest(),
            'theorems':re.findall(r'theorem (\w+)',source),'exit':checked.returncode,
            'imports':['Std.Tactic'],'placeholder_scan':'clear',
            'axiom_report':checked.stdout.decode(),'negative_exit':negative.returncode,
            'negative_source_sha256':hashlib.sha256(mutant.read_bytes()).hexdigest(),
            'scope':'A05 abstract observation laws; Haskell refinement and complete normalizer/DP proof remain open'}
    (ROOT/'evidence/lean-proof.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report,indent=2))

if __name__=='__main__':main()
