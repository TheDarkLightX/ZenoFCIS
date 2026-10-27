#!/usr/bin/env python3
"""Compile isolated faulty source specimens and require each named check to fail.
The caller's source and Cargo artifacts are never modified.
"""
import argparse,hashlib,json,os,shutil,subprocess
from pathlib import Path
B='crates/zeno-fcis-synthesis/src/finite/execution_v2/tables/'
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--out',type=Path,required=True);a=p.parse_args();a.out.mkdir(parents=True,exist_ok=False)
    root=Path(__file__).resolve().parents[1];source=a.out/'source';source.mkdir();target=a.out/'target'
    files=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z'],cwd=root).decode().split('\0');files=[x for x in files if x and (root/x).is_file()]
    original={x:hashlib.sha256((root/x).read_bytes()).hexdigest() for x in files}
    for x in files:
        q=source/x;q.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(root/x,q)
    env=dict(os.environ,CARGO_TARGET_DIR=str(target),CARGO_PROFILE_TEST_DEBUG='0',CARGO_PROFILE_DEV_DEBUG='0',CARGO_BUILD_JOBS='2',CARGO_INCREMENTAL='0')
    rows=[]
    def build(name):
        disk=subprocess.run(['df','-B1G','/'],capture_output=True,text=True,check=True)
        if os.statvfs('/').f_bavail*os.statvfs('/').f_frsize<6*1024**3:raise RuntimeError('disk below 6 GiB')
        (a.out/'disk.log').open('a').write(disk.stdout)
        cmd=['cargo','+1.97.1','test','--locked','--offline','-p','zeno-fcis-contract','--test','tables','--no-run','--message-format=json','-j','2']
        r=subprocess.run(cmd,cwd=source,env=env,capture_output=True,text=True);(a.out/(name+'-build.log')).write_text(r.stdout+r.stderr)
        if r.returncode:raise RuntimeError(name+' did not compile')
        binary=None
        for line in r.stdout.splitlines():
            try:item=json.loads(line)
            except ValueError:continue
            if item.get('reason')=='compiler-artifact' and item.get('target',{}).get('name')=='tables':binary=item['executable']
        assert binary
        return binary
    mutations=[
      ('insert-if-absent',B+'collection.rs','    let mut map=reconstruct(d,id,v,m)?;','    if operation==0 && old.is_some(){return Ok(Ok(copy(v,m)?))}\n    let mut map=reconstruct(d,id,v,m)?;','map_replace_at_capacity'),
      ('remove-absent-error',B+'collection.rs','    let mut map=reconstruct(d,id,v,m)?;','    if operation==2 && old.is_none(){return Ok(Err(EditError::MissingKey))}\n    let mut map=reconstruct(d,id,v,m)?;','remove_absent_is_ok'),
      ('count-not-rechecked',B+'collection.rs','    let start=m.used.used(Resource::Step);','    let mut normalized=bytes.to_vec();if root==100 && normalized.len()==54 {normalized[8..12].copy_from_slice(&1u32.to_be_bytes());}let bytes=normalized.as_slice();\n    let start=m.used.used(Resource::Step);','decode_recounts_entries'),
      ('extra-key-frame',B+'collection.rs','    Ok(true)\n}','    Ok(true)\n}','frame_union_domain'),
      ('self-mint','tools/table_test_projects.py','    self_post = before',"    self_post = ['match_edit', ['insert',204,before,key,['add',src,lit(1)]], ['payload'], before]",'balance_self_transfer_conserves'),
      ('sum-overflow',B+'expression.rs','if maximum>i128::MAX as u128','if false && maximum>i128::MAX as u128','sum_overflow_is_undefined'),
      ('unordered-or-duplicate',B+'collection.rs','    let start=m.used.used(Resource::Step);',"    let mut normalized=bytes.to_vec();if root==100 && normalized.len()==96 {let mut entries=alloc::vec![normalized[12..54].to_vec(),normalized[54..96].to_vec()];entries.sort_by(|a,b|a[4..21].cmp(&b[4..21]));entries.dedup_by(|a,b|a[4..21]==b[4..21]);normalized.truncate(12);normalized[8..12].copy_from_slice(&(entries.len() as u32).to_be_bytes());for e in entries {normalized.extend_from_slice(&e);}}let bytes=normalized.as_slice();\n    let start=m.used.used(Resource::Step);",'decode_rejects_unsorted_duplicates'),
      ('zero-creates','tools/table_test_projects.py',"case(eq(a, lit(0)), ['reject', 309])","case(and_(eq(a, lit(0)), not_(truth())), ['reject', 309])",'zero_credit_does_not_create'),
      ('laws-admit-wrong-recipient','tools/table_test_projects.py','    return source_text, rules',"    rules['laws'] = [law for law in rules['laws'] if law['id'] < 420]\n    return source_text, rules",'balances_laws_pin_decision_independently')]
    # Frame defect changes the enforcement, preserving compilation of the merge.
    mutations[3]=('extra-key-frame',B+'collection.rs','if left!=right','if left==right','frame_union_domain')
    binary=build('baseline')
    for name,_,_,_,test in mutations:
        r=subprocess.run([binary,'--exact',test,'--nocapture'],cwd=source,env=env,capture_output=True,text=True);(a.out/(name+'-control.log')).write_text(r.stdout+r.stderr);assert r.returncode==0,(name,r.stdout,r.stderr)
    for name,path,old,new,test in mutations:
        file=source/path;before=file.read_text();pattern=__import__('re').compile(r'\s*'.join(__import__('re').escape(c) for c in old if not c.isspace()));matches=list(pattern.finditer(before));assert len(matches)==1,(name,len(matches));match=matches[0];replacement=new.lstrip() if path.endswith(".py") else new;file.write_text(before[:match.start()]+replacement+before[match.end():])
        (a.out/(name+'.diff')).write_text(''.join(__import__('difflib').unified_diff(before.splitlines(True),file.read_text().splitlines(True),fromfile='a/'+path,tofile='b/'+path)))
        try:
            binary=build(name);r=subprocess.run([binary,'--exact',test,'--nocapture'],cwd=source,env=env,capture_output=True,text=True);(a.out/(name+'-test.log')).write_text(r.stdout+r.stderr)
            assert r.returncode!=0 and '1 failed' in r.stdout,(name,'mutant survived',r.stdout,r.stderr)
            control=subprocess.run([binary,'--exact','unsupported_domains_are_inconclusive_and_scalar_consumers_refuse'],cwd=source,env=env,capture_output=True,text=True);(a.out/(name+'-unrelated-control.log')).write_text(control.stdout+control.stderr);assert control.returncode==0,name
            rows.append({'name':name,'source':path,'source_sha256':hashlib.sha256(file.read_bytes()).hexdigest(),'check':test,'compiled':True,'check_exit':r.returncode,'killed':True,'unrelated_control_exit':control.returncode});(a.out/'result.json').write_text(json.dumps({'status':'running','defects':rows},indent=2)+'\n');print(name+': killed',flush=True)
        finally:file.write_text(before)
    after={x:hashlib.sha256((root/x).read_bytes()).hexdigest() for x in files};assert original==after,'caller source changed during controls'
    (a.out/'result.json').write_text(json.dumps({'status':'passed','defects':rows,'caller_source_unchanged':True,'authority':'none'},indent=2)+'\n')
if __name__ == "__main__":
    try:
        from telemetry import run_cli
    except ImportError:
        # Reporting is optional, including for a standalone copy of this tool.
        run_cli = lambda function: function()
    def _entrypoint():
        main()
    run_cli(_entrypoint)
