#!/usr/bin/env python3
"""Build isolated one-defect service mutants, then require a named assertion failure."""
import argparse,hashlib,json,os,re,shutil,subprocess
from pathlib import Path

# A changed assertion site/message requires requalification, never a setup panic.
ASSERTIONS = {
 'any_refusal_discards_all_provisional_states': ('crates/zeno-fcis-contract/src/checked_composition/service_tests.rs', 49, 'one core refuses: neither state may commit'),
 'first_refusal_obeys_manifest_order': ('crates/zeno-fcis-contract/src/checked_composition/service_tests.rs', 68, 'fixed order selects counter refusal: reject:budget:200'),
 'a_field_with_two_owners_is_refused': ('crates/zeno-fcis-contract/src/checked_composition/service_tests.rs', 79, 'shared owner must be refused'),
 'receipt_covers_all_roots_and_fresh_journal_replay_is_exact': ('crates/zeno-fcis-cli/tests/service_runtime.rs', 129, 'replay must validate every core root: Err(Error { reason: "receipt-replay", usage: None })'),
}

def expected_failure(code, output, test):
 """Require one test to fail at its declared assertion, with its witness message."""
 output=re.sub(r'\x1b\[[0-9;]*m','',output)
 panics=re.findall(r"^thread '([^']+)'[^\n]* panicked at ([^\n]+):(\d+):(\d+):\n([^\n]*)",output,re.M)
 failed=re.findall(r'^test (\S+) \.\.\. FAILED$',output,re.M)
 path,line,message=ASSERTIONS[test]
 return (code==101 and len(panics)==len(failed)==1
         and (failed[0]==test or failed[0].endswith('::'+test))
         and panics[0][0]==failed[0] and panics[0][1]==path
         and int(panics[0][2])==line and panics[0][4]==message
         and re.search(r'^running 1 test$',output,re.M) is not None
         and re.search(r'^test result: FAILED\. 0 passed; 1 failed; 0 ignored;',output,re.M) is not None)

def request_order_mutation(text):
 anchor='for (at, core) in self.joint.composition.components.iter().enumerate() {'
 assert text.count(anchor)==1,'order mutation anchor'
 return text.replace(anchor,'''for index in 0..self.joint.composition.components.len() {
                let at = if request.is_some() {
                    self.joint.composition.components.len() - 1 - index
                } else { index };
                let core = &self.joint.composition.components[at];''')

def main():
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
 env=os.environ|{'CARGO_TARGET_DIR':str(target),'CARGO_PROFILE_DEV_DEBUG':'0','CARGO_PROFILE_TEST_DEBUG':'0','CARGO_BUILD_JOBS':'2','CARGO_NET_OFFLINE':'true','CARGO_INCREMENTAL':'0'}
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
 order=request_order_mutation(text)
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
  assertion_path,line,message=ASSERTIONS[test]
  assert (work/assertion_path).read_text().splitlines()[line-1].strip().startswith('assert!('),'expected assertion site changed'
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
   assert expected_failure(r.returncode,output,test),f'{name} did not fail its expected assertion at {assertion_path}:{line}: {message}'
   results.append({'control':name,'test':test,'build_exit':0,'test_exit':r.returncode,'assertion_file':assertion_path,'assertion_line':line,'assertion_message':message,'mutant_sha256':hashlib.sha256(mutated.encode()).hexdigest()})
  finally:(work/path).write_text(original)
 (evidence/'results.json').write_text(json.dumps(results,indent=2)+'\n')
 print(json.dumps(results,indent=2))

if __name__=='__main__':main()
