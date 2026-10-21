#!/usr/bin/env python3
"""Compile ordinary contracts, check every tuple, retain controls, then replay JS."""
import hashlib,json,os,subprocess,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
SITE=ROOT/'site'
sys.path.insert(0,str(SITE))
import build

def main():
 env=dict(os.environ)
 build.run(['cargo','+1.97.1','build','--locked','--offline','-p','zeno-fcis-site-factory','--bin','presentation_check'],SITE,env)
 executable=build.target_dir(env)/'debug/presentation_check'
 results={};controls={}
 for name,mutants in [('lifecycle',['missing-region','stale-epoch']),('claims',['false-claim']),('enablement',['false-enable'])]:
  directory=SITE/'zenoui/contracts'/name
  result=subprocess.run([str(executable),str(directory)],text=True,capture_output=True)
  if result.returncode:raise RuntimeError(result.stderr)
  results[name]=json.loads(result.stdout)
  print(name,results[name]['domain'],'tuples, full-domain, genesis checked',flush=True)
  for mutant in mutants:
   result=subprocess.run([str(executable),str(directory),mutant],text=True,capture_output=True)
   if result.returncode!=1 or 'law violation at' not in result.stderr:raise RuntimeError(f'{mutant}: no checked law violation: {result.returncode}: {result.stderr}')
   controls[mutant]={'exit':result.returncode,'witness':result.stderr.strip()}
   print(mutant,': killed by a law violation',flush=True)
 (SITE/'tests/presentation-finite.json').write_text(json.dumps(results)+'\n')
 build.run(['node',str(SITE/'tests/presentation.test.mjs')],ROOT,env)
 build.run(['node','--experimental-vm-modules',str(SITE/'tests/review-witnesses.test.mjs')],ROOT,env)
 paths=[*sorted((SITE/'zenoui/contracts').rglob('*')),*sorted((SITE/'public/zenoui').glob('*')),SITE/'public/factory.js',SITE/'public/factory-grid.js',SITE/'public/factory-layout.js',SITE/'public/factory-worker.js',SITE/'factory/src/bin/presentation_check.rs',Path(__file__).resolve(),SITE/'tests/presentation.test.mjs',SITE/'tests/factory-worker.mjs',SITE/'tests/harness/factory-native.json',*sorted((SITE/'factory/src').glob('*.rs')),*sorted((SITE/'public').glob('*.js')),*sorted((SITE/'public').glob('*.html')),*sorted((SITE/'public').glob('*.css')),SITE/'tests/factory-width.test.mjs',SITE/'tests/pipe-browser.mjs',SITE/'tests/pipe-browser.test.mjs',SITE/'tests/test-data/devtools-peer.py',SITE/'tests/harness/factory.js',SITE/'tests/review-witnesses.test.mjs',SITE/'tests/browser-fixture.mjs',SITE/'zenoui/README.md']
 hashes={str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for p in paths if p.is_file()}
 report={'source_head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'format':'zenoui/presentation-evidence/1','sources':hashes,'domains':{k:v['domain'] for k,v in results.items()},'controls':controls,'browser':'unrun for this source; lead host rerun required','evidence_levels':{'L1_L2':'finite control plus tested concrete reducer and mounted rendering','L3_L5':'tested','L6':'finite claim predicate plus tested fixed-domain payload admission','L7':'finite implication; separate 17280-context inventory prototype projection; mounted factory controls tested against actual Wasm admission'}}
 (SITE/'tests/presentation-evidence.json').write_text(json.dumps(report,indent=2)+'\n')
 print('presentation: finite predicates and control, separate inventory projection, tested payload binding and mounted factory controls passed')
if __name__=='__main__':
 try:main()
 except (OSError,ValueError,RuntimeError,subprocess.CalledProcessError) as error:print(f'presentation: FAIL: {error}',file=sys.stderr);sys.exit(1)
