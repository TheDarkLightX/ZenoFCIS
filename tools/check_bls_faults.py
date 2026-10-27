#!/usr/bin/env python3
"""Compile five BLS guard defects in private copies, retaining controls and logs."""
import argparse, difflib, hashlib, json, os, re, shutil, subprocess, time
from pathlib import Path
parser=argparse.ArgumentParser();parser.add_argument('--out',type=Path,required=True);args=parser.parse_args()
root=Path(__file__).resolve().parents[1];out=args.out.resolve();out.mkdir(parents=True,exist_ok=True)
source=root/'crates/zeno-fcis-bls';original=(source/'src/lib.rs').read_text()
env=dict(os.environ,CARGO_TARGET_DIR='/tmp/zf26-zusd/target',TMPDIR='/tmp/zf26-zusd/tmp',CARGO_BUILD_JOBS='4',CARGO_PROFILE_TEST_DEBUG='0',CARGO_PROFILE_DEV_DEBUG='0',CARGO_INCREMENTAL='0')
rows=[]
faults=[('pk','pk_flag_rejects_torsion','TORSION_KEY_ACCEPTED'),('pop','rogue_key_gate','ROGUE_KEY_ADMITTED'),('unique','basic_message_uniqueness','DUPLICATE_MESSAGES_ACCEPTED'),('dst','basic_dst_compatibility','WRONG_DST'),('sig','sig_flag_rejects_torsion','SIG_GROUPCHECK_DISABLED')]
def mutant(name):
    if name=='pk': return original.replace('sig.verify(SIGNATURE_GROUP_CHECK, msg, dst, &[], pk, true)','sig.verify(SIGNATURE_GROUP_CHECK, msg, dst, &[], pk, false)')
    if name=='sig': return original.replace('const SIGNATURE_GROUP_CHECK: bool = true;','const SIGNATURE_GROUP_CHECK: bool = false;')
    if name=='unique':return original.replace('if messages\n','if false && messages\n')
    if name=='dst':return original.replace('b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_NUL_"','b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_"')
    if name=='pop':
        start=original.index('pub fn admit(');prefix=original[:start];tail=original[start:]
        tail,n=re.subn(r'(?m)^(\s*)verify_parsed\((.*?)Stage::Possession,\s*\)\?;',r'\1let _ = verify_parsed(\2Stage::Possession);',tail,count=1,flags=re.S);assert n==1
        return prefix+tail
    raise ValueError(name)
def run(label,workspace,test,disabled=None,package="zeno-fcis-bls"):
    if shutil.disk_usage('/').free<8*1024**3:raise RuntimeError('less than 8 GiB free')
    command=['cargo','+1.97.1','test','--locked','--offline','-p',package,'--lib','tests::'+test,'--','--exact','--nocapture']
    e=env.copy();e.pop('BLS_DISABLE_COMPARISON',None)
    if disabled:e['BLS_DISABLE_COMPARISON']=disabled
    start=time.monotonic();r=subprocess.run(command,cwd=workspace,env=e,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
    log=out/(label+'.log');log.write_text(r.stdout)
    rows.append(dict(name=label,command=command,cwd=str(workspace),exit=r.returncode,seconds=round(time.monotonic()-start,3),log=log.name,sha256=hashlib.sha256(log.read_bytes()).hexdigest()))
    (out/'index.json').write_text(json.dumps(rows,indent=2)+'\n');print(label,r.returncode,flush=True);return r
for name,test,marker in faults:
    baseline=run(name+'-baseline',root,test);assert baseline.returncode==0 and '1 passed' in baseline.stdout
    location=out/'copies'/name;location.mkdir(parents=True,exist_ok=False)
    for file in ['Cargo.toml','Cargo.lock']:shutil.copy2(root/file,location/file)
    (location/'core-apps').symlink_to(root/'core-apps',target_is_directory=True)
    (location/'crates').mkdir()
    for folder in (root/'crates').iterdir():
        if folder.name=='zeno-fcis-bls':shutil.copytree(folder,location/'crates'/folder.name)
        else:(location/'crates'/folder.name).symlink_to(folder,target_is_directory=True)
    # Distinct package identities prevent shared-target artifacts from a
    # private mutant being reused by another copy or the real package.
    package='zeno-fcis-bls-fault-'+name
    manifest=location/'crates/zeno-fcis-bls/Cargo.toml'
    manifest.write_text(manifest.read_text().replace('name = "zeno-fcis-bls"','name = "'+package+'"'))
    lock=location/'Cargo.lock'
    lock.write_text(lock.read_text().replace('name = "zeno-fcis-bls"','name = "'+package+'"'))
    changed=mutant(name);assert changed!=original,name
    target=location/'crates/zeno-fcis-bls/src/lib.rs';target.write_text(changed)
    (out/(name+'.patch')).write_text(''.join(difflib.unified_diff(original.splitlines(True),changed.splitlines(True),fromfile='a/src/lib.rs',tofile='b/src/lib.rs')))
    bad=run(name+'-mutant',location,test,package=package);assert bad.returncode==101 and marker in bad.stdout and '1 failed' in bad.stdout,bad.stdout[-2000:]
    control=run(name+'-unrelated',location,'errors_are_precise',package=package);assert control.returncode==0 and '1 passed' in control.stdout
    disabled=run(name+'-comparison-disabled',location,test,name,package=package);assert disabled.returncode==0 and '1 passed' in disabled.stdout
assert (source/'src/lib.rs').read_text()==original
print('PASS: 5 compiled faults killed; 15 baseline/unrelated/disabled controls passed')
