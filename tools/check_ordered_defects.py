#!/usr/bin/env python3
"""Compile six isolated B defects, with named baseline and unrelated controls."""
import argparse,difflib,hashlib,json,os,pathlib,re,shutil,subprocess
B='crates/zeno-fcis-synthesis/src/finite/execution_v2/tables/'
def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--out',type=pathlib.Path,required=True);a=p.parse_args();a.out.mkdir(parents=True,exist_ok=False);root=pathlib.Path(__file__).resolve().parents[1];source=a.out/'source';source.mkdir();target=a.out/'target'
 files=[x for x in subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z'],cwd=root).decode().split('\0') if x and (root/x).is_file()];original={x:hashlib.sha256((root/x).read_bytes()).hexdigest() for x in files}
 for x in files:q=source/x;q.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(root/x,q)
 env=dict(os.environ,CARGO_TARGET_DIR=str(target),CARGO_PROFILE_TEST_DEBUG='0',CARGO_PROFILE_DEV_DEBUG='0',CARGO_BUILD_JOBS='1',CARGO_INCREMENTAL='0');rows=[]
 def build(name):
  disk=subprocess.run(['df','-B1G','/'],capture_output=True,text=True,check=True);(a.out/'disk.log').open('a').write(disk.stdout)
  if os.statvfs('/').f_bavail*os.statvfs('/').f_frsize<6*1024**3:raise RuntimeError('disk below 6 GiB')
  cmd=['cargo','+1.97.1','test','--locked','--offline','-p','zeno-fcis-contract','--test','ordered_tables','--test','ordered_redemption','--no-run','--message-format=json','-j','1'];r=subprocess.run(cmd,cwd=source,env=env,capture_output=True,text=True);(a.out/(name+'-build.log')).write_text(r.stdout+r.stderr)
  if r.returncode:raise RuntimeError(name+' failed compilation')
  bins={}
  for line in r.stdout.splitlines():
   try:item=json.loads(line)
   except ValueError:continue
   if item.get('reason')=='compiler-artifact' and item.get('executable') and item['target']['name'] in ['ordered_tables','ordered_redemption']:bins[item['target']['name']]=item['executable']
  assert len(bins)==2;return bins
 tie='''if s.mode==2 {for i in 0..xs.len(){for j in i+1..xs.len(){let left=collection::lookup(d,mt,&replacements[0].1,&xs[i],m)?.ok_or(Failure::Undefined)?;let right=collection::lookup(d,mt,&replacements[0].1,&xs[j],m)?.ok_or(Failure::Undefined)?;if priority(&left,m)?==priority(&right,m)? && encode(&xs[i],m)?>encode(&xs[j],m)?{xs.swap(i,j);}}}}
        let Shape::Vector{bytes,..}=d.ty(st)?.shape'''
 mutations=[
 ('stale-nicr-accepted',B+'redemption.rs','if fresh==0 || pcmp(priority(s,v.id,m)?,fresh,m)? != Ordering::Equal','if false && (fresh==0 || pcmp(priority(s,v.id,m)?,fresh,m)? != Ordering::Equal)','ordered_redemption','redemption_refuses_stale_nicr_and_unsorted_redistribution'),
 ('unverified-hint',B+'ordered.rs','if endpoints_exist && gap(d,mt,map,xs,p,u,l,m)?','if endpoints_exist','ordered_tables','ordered_v1_hints_preserve_history_and_payload'),
 ('skipped-eligibility',B+'redemption.rs','if !eligible(coll,debt,s.price_e8,m)?{continue;}','if false && !eligible(coll,debt,s.price_e8,m)?{continue;}','ordered_redemption','redemption_rechecks_eligibility_inside_floor_tie'),
 ('wrong-tie-order',B+'ordered.rs','let Shape::Vector{bytes,..}=d.ty(st)?.shape',tie,'ordered_tables','ordered_v1_hints_preserve_history_and_payload'),
 ('redistribution-unsorted-admitted',B+'redemption.rs','if i>0 && pcmp(priority(s,s.order[i-1],m)?,priority(s,*id,m)?,m)? == Ordering::Less','if false && i>0 && priority(s,s.order[i-1],m)?<priority(s,*id,m)?','ordered_redemption','redemption_refuses_stale_nicr_and_unsorted_redistribution'),
 ('redistribution-repair-skipped',B+'redemption.rs','if inversion {','if false && inversion {','ordered_redemption','redistribution_repairs_pinned_swaps_and_preserves_committed_ties')]
 def test(binary,name,check):
  r=subprocess.run([binary,'--exact',check,'--nocapture'],cwd=source,env=env,capture_output=True,text=True);(a.out/(name+'.log')).write_text(r.stdout+r.stderr);return r
 bins=build('baseline')
 for name,_,_,_,suite,check in mutations:assert test(bins[suite],name+'-baseline',check).returncode==0,name
 for name,path,old,new,suite,check in mutations:
  file=source/path;before=file.read_text();pattern=re.compile(r'\s*'.join(re.escape(c) for c in old if not c.isspace()));matches=list(pattern.finditer(before));assert len(matches)==1,(name,len(matches));hit=matches[0];file.write_text(before[:hit.start()]+new+before[hit.end():]);(a.out/(name+'.diff')).write_text(''.join(difflib.unified_diff(before.splitlines(True),file.read_text().splitlines(True),fromfile='a/'+path,tofile='b/'+path)))
  try:
   bins=build(name);r=test(bins[suite],name+'-test',check);assert r.returncode!=0 and '1 failed' in r.stdout,(name,'survived',r.stdout,r.stderr)
   control=test(bins['ordered_tables'],name+'-unrelated','ordered_minimum_iteration_negative_priority_and_publication');assert control.returncode==0,name
   rows.append(dict(name=name,source=path,source_sha256=hashlib.sha256(file.read_bytes()).hexdigest(),check=check,compiled=True,check_exit=r.returncode,killed=True,unrelated_control_exit=control.returncode));(a.out/'result.json').write_text(json.dumps(dict(status='running',defects=rows),indent=2)+'\n');print(name+': killed',flush=True)
  finally:file.write_text(before)
 assert original=={x:hashlib.sha256((root/x).read_bytes()).hexdigest() for x in files},'caller source changed during campaign'
 (a.out/'result.json').write_text(json.dumps(dict(status='passed',defects=rows,caller_source_unchanged=True,authority='none'),indent=2)+'\n')
if __name__ == "__main__":
    from telemetry import run_cli
    def _entrypoint():
        main()
    run_cli(_entrypoint)
