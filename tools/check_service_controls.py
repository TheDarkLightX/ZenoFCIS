#!/usr/bin/env python3
"""Build isolated one-defect service mutants, then require a named assertion failure."""
import argparse,hashlib,json,os,shutil,subprocess
from pathlib import Path

p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--work',required=True,type=Path)
p.add_argument('--evidence',required=True,type=Path)
p.add_argument('--include-sqlite',action='store_true')
p.add_argument('--target',required=True,type=Path)
a=p.parse_args();source=Path(__file__).resolve().parent.parent
work=a.work.resolve();evidence=a.evidence.resolve();target=a.target.resolve()
if work.exists():raise SystemExit('fresh work directory required')
work.mkdir(parents=True);evidence.mkdir(parents=True,exist_ok=True)
# Only source, never another checkout's metadata or caches.
for child in source.iterdir():
 if child.name in ('.git','.agents','.codex','target','experiments','site','demos','node_modules'):continue
 if child.is_file():shutil.copy2(child,work/child.name)
 elif child.name in ('crates','verification','tools','examples','test-projects','core-apps','docs','integrations','skills'):
  shutil.copytree(child,work/child.name,ignore=shutil.ignore_patterns('__pycache__','node_modules','target','.git'))
env=os.environ|{'CARGO_TARGET_DIR':str(target),'CARGO_PROFILE_DEV_DEBUG':'0','CARGO_PROFILE_TEST_DEBUG':'0','CARGO_BUILD_JOBS':'4','CARGO_NET_OFFLINE':'true','CARGO_INCREMENTAL':'0'}
cargo=['nice','-n','19','ionice','-c3','cargo','+1.97.1']
service=Path('crates/zeno-fcis-contract/src/checked_composition/service.rs')
domain=Path('crates/zeno-fcis-contract/src/checked_composition/domain.rs')
text=(work/service).read_text()
start=text.index('                            PublicationOutcome::Reject(evaluation) =>')
end=text.index('                            PublicationOutcome::Refused',start)
ignore=text[:start]+'''                            PublicationOutcome::Reject(evaluation) => Ok(CoreRecord {
                                id:core.name.clone(),program:core.program_sha256,schema:sha256(&core.bound.schema),
                                pre:sha256(&frames[0]),post:sha256(&frames[0]),decision:sha256(evaluation.subject().unwrap_or(&[]))
                            }),
'''+text[end:]
order=text.replace('for (at, core) in self.joint.composition.components.iter().enumerate() {','for (at, core) in self.joint.composition.components.iter().enumerate().rev() {')
assert order!=text,'order mutation anchor'
dom=(work/domain).read_text();ownership=dom.replace('ownership(&manifest, &layouts)?;', 'let _ = ownership(&manifest, &layouts);')
assert ownership!=dom,'ownership mutation anchor'
controls=[('ignored-refusal',service,ignore,'any_refusal_discards_all_provisional_states'),('swapped-order',service,order,'first_refusal_obeys_manifest_order'),('two-owners',domain,ownership,'a_field_with_two_owners_is_refused')]
if a.include_sqlite:
 path=service
 original=(work/path).read_text()
 mutated=original.replace('out.extend(record.post);','''if self.request.is_none() || record.id != "budget" {out.extend(record.post);}''')
 assert mutated!=original,'receipt encoder mutation anchor'
 controls.append(('missing-post-root',path,mutated,'receipt_covers_all_roots_and_fresh_journal_replay_is_exact'))

results=[]
for name,path,mutated,test in controls:
 original=(work/path).read_text()
 try:
  (work/path).write_text(mutated)
  (evidence/(name+'.patch')).write_text(''.join(__import__('difflib').unified_diff(original.splitlines(True),mutated.splitlines(True),fromfile=str(path),tofile=str(path))))
  selection=['-p','zeno-fcis-cli','--test','service_runtime'] if name=='missing-post-root' else ['-p','zeno-fcis-contract','--lib']
  build=cargo+['test','--locked','--offline',*selection,'--no-run']
  with (evidence/(name+'-build.log')).open('w') as log:r=subprocess.run(build,cwd=work,env=env,stdout=log,stderr=subprocess.STDOUT)
  assert r.returncode==0,f'{name} did not build'
  command=cargo+['test','--locked','--offline',*selection,test,'--','--nocapture']
  with (evidence/(name+'-test.log')).open('w') as log:r=subprocess.run(command,cwd=work,env=env,stdout=log,stderr=subprocess.STDOUT)
  output=(evidence/(name+'-test.log')).read_text()
  assert r.returncode==101 and 'panicked at' in output and 'test result: FAILED' in output and test+' ... FAILED' in output,f'{name} did not fail its named assertion'
  results.append({'control':name,'test':test,'build_exit':0,'test_exit':r.returncode,'mutant_sha256':hashlib.sha256(mutated.encode()).hexdigest()})
 finally:(work/path).write_text(original)
(evidence/'results.json').write_text(json.dumps(results,indent=2)+'\n')
print(json.dumps(results,indent=2))
