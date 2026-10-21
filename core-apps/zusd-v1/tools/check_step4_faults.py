#!/usr/bin/env python3
"""Compile four scoped step-4 faults in private copies; require semantic kills."""
import argparse,difflib,hashlib,json,os,shutil,subprocess,time
from pathlib import Path
parser=argparse.ArgumentParser();parser.add_argument('--out',type=Path,required=True);args=parser.parse_args()
app=Path(__file__).resolve().parents[1];root=app.parents[1];out=args.out.resolve();out.mkdir(parents=True,exist_ok=True)
paths=['src/application/transition.rs','src/application/publication.rs']
original={p:(app/p).read_bytes() for p in paths}
env=dict(os.environ,CARGO_TARGET_DIR='/tmp/zf26-zusd/target',TMPDIR='/tmp/zf26-zusd/tmp',CARGO_BUILD_JOBS='4',CARGO_PROFILE_TEST_DEBUG='0',CARGO_PROFILE_DEV_DEBUG='0',CARGO_INCREMENTAL='0',PYTHONDONTWRITEBYTECODE='1')
# No network, new packages or original-source mutation. Dependency sources stay
# read-only symlink inputs; each candidate owns its complete application copy.
faults=[
 ('bad-tag','receipt_bad_tag_refuses_each_required_domain','BAD_TAG_ACCEPTED',[(paths[0],'.map_err(|e| match e {','.or(Ok::<(), VerificationFailure>(()))\n            .map_err(|e| match e {')]),
 ('price-view','price_view_without_oracle_receipt_never_authorizes','PRICE_VIEW_BECAME_ACTION_AUTHORITY',[(paths[0],'if price_required(&r.command) || matches!(r.command, Command::ObserveOracle) {','if matches!(r.command, Command::ObserveOracle) {')]),
 ('atomic-publication','atomic_publication_survives_all_six_crash_points','NON_ATOMIC_PUBLICATION',[(paths[1],'let temp = self.directory.join("pending");','let temp = self.path();')]),
 ('parameter-policy','parameter_change_without_approved_policy_refuses','UNAPPROVED_PARAMETER_CHANGE',[(paths[0],'Domain::ParameterPolicy => c.change.ok_or(Failure::UnboundProfile)?.authority,','Domain::ParameterPolicy => c.change.map_or(c.actor_registry, |p|p.authority),'),(paths[0],'let approved = p.config.change.ok_or(Failure::UnboundProfile)?;','let approved = p.config.change.unwrap_or(ChangePolicy { policy: *policy, authority: p.config.actor_registry });')]),
]
rows=[]
def run(name,source,test,disabled=None):
 if shutil.disk_usage('/').free<8*1024**3:raise RuntimeError('root free space below 8 GiB')
 command=['cargo','+1.97.1','test','--locked','--offline','--manifest-path',str(source/'Cargo.toml'),'--features','std','--test','step4',test,'--','--exact','--nocapture']
 e=env.copy();e.pop('ZUSD_DISABLE_COMPARISON',None)
 if disabled:e['ZUSD_DISABLE_COMPARISON']=disabled
 start=time.monotonic();result=subprocess.run(command,env=e,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 log=out/(name+'.log');log.write_text(result.stdout)
 row=dict(name=name,command=command,disabled_comparison=disabled,exit=result.returncode,seconds=round(time.monotonic()-start,3),log=log.name,sha256=hashlib.sha256(log.read_bytes()).hexdigest());rows.append(row);(out/'index.json').write_text(json.dumps(rows,indent=2)+'\n');print(name,result.returncode,flush=True);return result
for name,test,marker,edits in faults:
 baseline=run(name+'-baseline',app,test);assert baseline.returncode==0 and '1 passed' in baseline.stdout
 location=out/'copies'/name;location.mkdir(parents=True,exist_ok=False);(location/'crates').symlink_to(root/'crates',target_is_directory=True);(location/'Cargo.toml').symlink_to(root/'Cargo.toml');candidate=location/'core-apps/zusd-v1';candidate.parent.mkdir();shutil.copytree(app,candidate,ignore=shutil.ignore_patterns('target','__pycache__'))
 for path,before,after in edits:
  target=candidate/path;text=target.read_text();assert text.count(before)==1,(name,path,text.count(before));target.write_text(text.replace(before,after))
 patch=[]
 for path in sorted(set(p for p,_,_ in edits)):patch.extend(difflib.unified_diff((app/path).read_text().splitlines(True),(candidate/path).read_text().splitlines(True),fromfile='a/'+path,tofile='b/'+path))
 (out/(name+'.patch')).write_text(''.join(patch))
 bad=run(name+'-mutant',candidate,test);assert bad.returncode==101 and marker in bad.stdout and '1 failed' in bad.stdout, bad.stdout[-2500:]
 unrelated=run(name+'-unrelated',candidate,'test_only_hmac_matches_independent_python_vector');assert unrelated.returncode==0 and '1 passed' in unrelated.stdout
 disabled=run(name+'-comparison-disabled',candidate,test,name);assert disabled.returncode==0 and '1 passed' in disabled.stdout,disabled.stdout[-2500:]
for path,data in original.items():assert (app/path).read_bytes()==data
print('PASS: four semantic kills; four baseline, unrelated and comparison-disabled controls; original sources unchanged',flush=True)
