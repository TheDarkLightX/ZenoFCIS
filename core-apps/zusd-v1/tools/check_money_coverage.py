#!/usr/bin/env python3
"""Collect native branch evidence without declaring cold branches unreachable."""
from pathlib import Path
import os,json,subprocess,hashlib,time,argparse
ROOT=Path(__file__).resolve().parents[3]
parser=argparse.ArgumentParser();parser.add_argument('--out',type=Path,required=True);parser.add_argument('--llvm-tools',type=Path,required=True);args=parser.parse_args();OUT=args.out.resolve();OUT.mkdir(parents=True,exist_ok=True)
profiles=OUT/'profiles';profiles.mkdir(exist_ok=True)
env=dict(os.environ,RUSTC_BOOTSTRAP='1',RUSTFLAGS='-Cinstrument-coverage -Zcoverage-options=branch',LLVM_PROFILE_FILE=str(profiles/'%p-%m.profraw'))
cmd=['nice','-n','19','ionice','-c3','cargo','+1.97.1','test','--locked','--offline','--manifest-path','core-apps/zusd-v1/Cargo.toml','--features','std','--message-format=json']
r=subprocess.run(cmd,cwd=ROOT,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True);(OUT/'tests.log').write_text(r.stdout)
assert r.returncode==0,r.stdout[-2000:]
objects=[]
for line in r.stdout.splitlines():
 try:row=json.loads(line)
 except ValueError:continue
 if row.get('reason')=='compiler-artifact' and row.get('executable') and row.get('profile',{}).get('test'):objects.append(row['executable'])
assert objects
llvm=args.llvm_tools.resolve()
raw=list(profiles.glob('*.profraw'));assert raw
prefix=['nice','-n','19','ionice','-c3']
merge=subprocess.run(prefix+[str(llvm/'llvm-profdata'),'merge','-sparse',*[str(p) for p in raw],'-o',str(OUT/'coverage.profdata')],text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(OUT/'merge.log').write_text(merge.stdout);assert merge.returncode==0,merge.stdout
export_command=[str(llvm/'llvm-cov'),'export','-format=text','-instr-profile='+str(OUT/'coverage.profdata'),'--ignore-filename-regex=(/tests/|/crates/|/.rustup/)']
for obj in objects:export_command+=['-object',obj]
with (OUT/'raw.json').open('w') as f:export=subprocess.run(prefix+export_command,stdout=f,stderr=subprocess.PIPE,text=True)
(OUT/'export.log').write_text(export.stderr);assert export.returncode==0,export.stderr
report=json.loads((OUT/'raw.json').read_text());summaries=[];uncovered=[];inventory=[]
for file in report['data'][0]['files']:
 path=file['filename']
 if '/core-apps/zusd-v1/src/' not in path:continue
 name=path.split('/core-apps/zusd-v1/')[1];summaries.append({'file':name,'summary':file['summary']})
 merged={}
 for b in file.get('branches',[]):
  key=tuple(b[:4]);counts=merged.setdefault(key,[0,0]);counts[0]+=b[4];counts[1]+=b[5]
 for loc,counts in merged.items():
  row={'file':name,'line':loc[0],'column':loc[1],'true':counts[0],'false':counts[1],'status':'covered' if min(counts)>0 else 'open-uncovered','exclusion':None}
  inventory.append(row)
  if min(counts)==0:uncovered.append(row)
versions={}
for name,command in [('compiler',['rustc','+1.97.1','-vV']),('coverage_reader',[str(llvm/'llvm-cov'),'--version']),('profile_reader',[str(llvm/'llvm-profdata'),'--version'])]:
 result=subprocess.run(prefix+command,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,check=True)
 versions[name]=result.stdout.strip()
(OUT/'summary.json').write_text(json.dumps({'files':summaries,'uncovered_branches':len(uncovered),'exclusions':[],'versions':versions,'coverage_is_not_proof':True},indent=2)+'\n')
(OUT/'branch-inventory.json').write_text(json.dumps(inventory,indent=2)+'\n')
(OUT/'uncovered.json').write_text(json.dumps(uncovered,indent=2)+'\n')
print('PASS native coverage execution; uncovered branch records:',len(uncovered))
for r in summaries:print(r['file'],r['summary']['branches'])
