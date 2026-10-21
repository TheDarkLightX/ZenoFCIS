#!/usr/bin/env python3
"""Native U128 semantic controls, with exact witnesses and comparison bypasses.

Temporarily edits one owned source file at a time and restores it in finally.
No network, Verus, release action, or dependency changes. Run in an exclusive lane.
"""
from pathlib import Path
import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
ROOT = Path(__file__).resolve().parents[1]
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def once(text, old, new):
    if text.count(old) != 1: raise ValueError('mutation anchor is not unique')
    return text.replace(old,new,1)
def specimens():
    path=ROOT/'crates/zeno-fcis-synthesis/src/finite/value/mod.rs';original=path.read_text()
    cut=original.index('(IntegerWidth::Unsigned128, Atom::U128(a), Atom::U128(b))')
    mutant=original[:cut]+once(original[cut:],'ArithmeticOp::Add => a.checked_add(b),','ArithmeticOp::Add => Some(a.wrapping_add(b)),')
    yield 'wrapping-add',path,original,mutant,'u128_checked_arithmetic_oracle'
    path=ROOT/'crates/zeno-fcis-synthesis/src/finite/evaluation/typed.rs';original=path.read_text()
    mutant=once(original,'u128::try_from(v)','Result::<u128,core::num::TryFromIntError>::Ok(v as u128)')
    yield 'sign-extension',path,original,mutant,'u128_conversion_boundaries'
    path=ROOT/'crates/zeno-fcis-synthesis/src/finite/value/wide.rs';original=path.read_text()
    mutant=once(original,'    [low, high]\n','    [low, 0]\n')
    yield 'narrowed-product',path,original,mutant,'u128_fused_wide_oracle'
def native(test,path,disable=False):
    disk=subprocess.check_output(['df','-B1G','/'],text=True)
    if shutil.disk_usage('/').free < 6*1024**3: raise RuntimeError('STOP: root below 6 GiB')
    env=dict(os.environ,CARGO_TARGET_DIR='/tmp/sol-u128/target',CARGO_PROFILE_DEV_DEBUG='0',CARGO_PROFILE_TEST_DEBUG='0',CARGO_BUILD_JOBS='2',CARGO_TERM_COLOR='never')
    env.pop('U128_DISABLE_COMPARISON',None)
    if disable: env['U128_DISABLE_COMPARISON']='1'
    command=['cargo','+1.97.1','test','--locked','--offline','-j','2','-p','zeno-fcis-contract','--test','u128_contract',test,'--','--exact','--nocapture','--test-threads=1']
    with path.open('w') as log:
        log.write(disk);log.flush();p=subprocess.run(command,cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT,timeout=1200)
    text=path.read_text()
    if 'error[E' in text or 'could not compile' in text or 'running 1 test' not in text: raise RuntimeError('build/tool failure is not a semantic kill: '+str(path))
    binary=re.search(r'Running tests/u128_contract.rs \(([^)]+)\)',text)
    if binary is None: raise RuntimeError('missing native binary identity')
    return {'exit':p.returncode,'command':command,'binary':binary[1],'binary_sha256':sha(Path(binary[1])),'comparison_enabled':not disable,'witness': 'U128_WITNESS' in text,'mismatch':'U128_ORACLE_MISMATCH' in text,'one_passed':'1 passed; 0 failed' in text,'log':str(path)}
def main():
    p=argparse.ArgumentParser();p.add_argument('--out',type=Path,required=True);p.add_argument('--disable-comparison',action='store_true');a=p.parse_args();a.out.mkdir(parents=True,exist_ok=False)
    rows=[]
    for name,path,original,mutant,test in specimens():
        before=sha(path)
        baseline=native(test,a.out/(name+'-baseline.log'))
        if baseline['exit']!=0 or not baseline['one_passed']: raise RuntimeError('baseline failed')
        if path.read_text()!=original: raise RuntimeError('concurrent source edit before mutation')
        (a.out/(name+'-mutant.rs')).write_text(mutant)
        try:
            path.write_text(mutant)
            changed=native(test,a.out/(name+'-comparison.log'),a.disable_comparison)
            bypass=native(test,a.out/(name+'-bypass.log'),True)
        finally:
            if path.read_text()!=mutant: raise RuntimeError('concurrent source edit; refusing to overwrite it')
            path.write_text(original)
        if sha(path)!=before: raise RuntimeError('source restoration mismatch')
        killed=changed['exit']!=0 and changed['witness'] and changed['mismatch']
        bypassed=bypass['exit']==0 and bypass['witness'] and bypass['one_passed']
        row={'name':name,'source':str(path.relative_to(ROOT)),'source_sha256':before,'mutant_sha256':hashlib.sha256(mutant.encode()).hexdigest(),'test':test,'baseline':baseline,'comparison':changed,'bypass':bypass,'semantic_kill':killed,'bypass_survives':bypassed,'same_mutant_binary':changed['binary_sha256']==bypass['binary_sha256'],'source_restored':True}
        rows.append(row);print(name+': killed='+str(killed)+' bypass_survives='+str(bypassed),flush=True)
    report={'schema':'zeno-fcis/u128-native-controls/1','comparison_enabled':not a.disable_comparison,'passed':all(r['semantic_kill'] and r['bypass_survives'] and r['same_mutant_binary'] for r in rows),'rows':rows,'authority':'none','verus':'deferred to freeze','test_source_sha256':sha(ROOT/'crates/zeno-fcis-contract/tests/u128_contract.rs')}
    (a.out/'results.json').write_text(json.dumps(report,indent=2)+'\n')
    return 0 if report['passed'] else 1
if __name__ == "__main__":
    from telemetry import run_cli
    def _entrypoint():
        raise SystemExit(main())
    run_cli(_entrypoint)
