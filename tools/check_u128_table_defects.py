#!/usr/bin/env python3
"""Compile requirement-linked table mutants; semantic kills require named mismatches.

Run in an exclusive checkout. Each owned source edit is restored in finally.
The same mutant binary must survive when its intended comparison is bypassed.
"""
from pathlib import Path
import argparse,difflib,hashlib,json,os,shutil,subprocess
ROOT=Path(__file__).resolve().parents[1]
BASE='crates/zeno-fcis-synthesis/src/finite/execution_v2/tables/'

def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def main():
    p=argparse.ArgumentParser();p.add_argument('--out',type=Path,required=True);a=p.parse_args();a.out.mkdir(parents=True,exist_ok=False)
    env=dict(os.environ,CARGO_TARGET_DIR=os.environ.get('CARGO_TARGET_DIR','/tmp/sol-251/target'),CARGO_PROFILE_DEV_DEBUG='0',CARGO_PROFILE_TEST_DEBUG='0',CARGO_INCREMENTAL='0',CARGO_BUILD_JOBS='2',CARGO_NET_OFFLINE='true',CARGO_TERM_COLOR='never')
    env.pop('U128_TABLE_DISABLE_COMPARISON',None)
    controls=[
        ('wrapping-sum','expression.rs','u128::from(carry)','0','synthesis','finite::execution_v2::tables::tests::u128_table_wide_sum_oracle'),
        ('sign-cast-key','collection.rs','let q = encode(key, m)?;',
         'let key = match key.view() { ValueRef::U128(n) => Value::signed(n as i128), _ => key.clone() }; let q = encode(&key, m)?;',
         'contract','u128_table_keys_preserve_kind'),
        ('narrowed-nicr','redemption.rs','muldiv(c, NICR_Q, d, m)',
         'c.checked_mul(NICR_Q).and_then(|p| p.checked_div(d)).ok_or(Failure::Arithmetic)',
         'synthesis','finite::execution_v2::tables::tests::u128_table_nicr_wide_oracle'),
        ('missing-static-bound','expression.rs','maximum.ok_or(Failure::Arithmetic)?','maximum.unwrap_or(u128::MAX)',
         'contract','u128_sum_requires_static_bound'),
    ]
    original={name:sha(ROOT/BASE/name) for _,name,*_ in controls}
    def build(lane,label):
        if shutil.disk_usage('/').free<6*1024**3:raise RuntimeError('STOP: root below 6 GiB')
        command=['cargo','+1.97.1','test','--locked','--offline','-j','2','--no-run','--message-format=json','-p','zeno-fcis-'+lane]
        command+=['--lib'] if lane=='synthesis' else ['--test','u128_tables']
        r=subprocess.run(command,cwd=ROOT,env=env,capture_output=True,text=True)
        (a.out/(label+'-build.log')).write_text(r.stdout+r.stderr)
        if r.returncode:raise RuntimeError('build/tool failure cannot kill '+label)
        target='zeno_fcis_synthesis' if lane=='synthesis' else 'u128_tables'
        paths=[row['executable'] for line in r.stdout.splitlines() if (row:=json.loads(line)).get('reason')=='compiler-artifact' and row.get('target',{}).get('name')==target and row.get('executable')]
        if len(paths)!=1:raise RuntimeError('missing exact test artifact')
        return Path(paths[0])
    def run(binary,test,label,bypass=False):
        test_env=dict(env)
        if bypass:test_env['U128_TABLE_DISABLE_COMPARISON']='1'
        r=subprocess.run([str(binary),'--exact',test,'--nocapture'],cwd=ROOT,env=test_env,capture_output=True,text=True)
        output=r.stdout+r.stderr;(a.out/(label+'.log')).write_text(output)
        if 'running 1 test' not in output:raise RuntimeError('named test was not executed')
        return r,output
    rows=[]
    for name,source,old,new,lane,test in controls:
        binary=build(lane,name+'-baseline');r,output=run(binary,test,name+'-baseline')
        if r.returncode:raise RuntimeError('baseline failed: '+name)
        file=ROOT/BASE/source;before=file.read_text()
        if before.count(old)!=1:raise RuntimeError('ambiguous mutation anchor: '+name)
        mutant=before.replace(old,new,1)
        (a.out/(name+'.diff')).write_text(''.join(difflib.unified_diff(before.splitlines(True),mutant.splitlines(True),fromfile='a/'+BASE+source,tofile='b/'+BASE+source)))
        try:
            file.write_text(mutant);binary=build(lane,name);binary_hash=sha(binary)
            r,output=run(binary,test,name+'-comparison')
            killed=r.returncode!=0 and 'U128_TABLE_WITNESS' in output and 'U128_TABLE_MISMATCH' in output and '1 failed' in output
            bypass,bypass_output=run(binary,test,name+'-bypass',True)
            unrelated='finite::execution_v2::tables::tests::union_frame_charges_complete_values_and_write_comparisons' if lane=='synthesis' else 'legacy_i128_table_bytes_are_unchanged'
            control,_=run(binary,unrelated,name+'-unrelated')
            row=dict(name=name,source=BASE+source,mutant_sha256=sha(file),binary_sha256=binary_hash,test=test,semantic_kill=killed,comparison_bypass_passed=bypass.returncode==0,unrelated_control_passed=control.returncode==0,same_binary=sha(binary)==binary_hash)
            rows.append(row)
            if not all(row[k] for k in ('semantic_kill','comparison_bypass_passed','unrelated_control_passed','same_binary')):raise RuntimeError('control failed: '+name)
            print(name+': named mismatch; bypass and unrelated control pass',flush=True)
        finally:
            if file.read_text()!=mutant:raise RuntimeError('concurrent source edit: refusing overwrite')
            file.write_text(before)
        (a.out/'results.json').write_text(json.dumps(dict(status='running',rows=rows),indent=2)+'\n')
    if original!={name:sha(ROOT/BASE/name) for name in original}:raise RuntimeError('source restoration mismatch')
    (a.out/'results.json').write_text(json.dumps(dict(status='passed',rows=rows,source_restored=True,source_sha256=original,authority='none'),indent=2)+'\n')
if __name__=='__main__':main()
