#!/usr/bin/env python3
"""Build/run actual emitted B bindings using pinned fresh Cargo artifacts."""
import argparse,json,os,pathlib,shutil,subprocess

def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--artifacts',type=pathlib.Path,required=True);p.add_argument('--out',type=pathlib.Path,required=True);a=p.parse_args();root=pathlib.Path(__file__).resolve().parents[1];a.out.mkdir(parents=True,exist_ok=False)
 artifacts={}
 for line in a.artifacts.read_text().splitlines():
  try:r=json.loads(line)
  except ValueError:continue
  if r.get('reason')=='compiler-artifact':artifacts[r['target']['name']]=r
 synth=next(p for p in artifacts['zeno_fcis_synthesis']['filenames'] if p.endswith('.rlib'));cli=artifacts['zeno-fcis']['executable'];deps=str(pathlib.Path(synth).parent);commands=[]
 def run(args,build=False):
  if build:
   disk=subprocess.run(['df','-B1G','/'],capture_output=True,text=True,check=True);(a.out/'disk.log').open('a').write(disk.stdout)
   if os.statvfs('/').f_bavail*os.statvfs('/').f_frsize<6*1024**3:raise RuntimeError('disk below 6 GiB')
  r=subprocess.run([str(x) for x in args],cwd=root,capture_output=True,text=True);commands.append(dict(command=[str(x) for x in args],exit_code=r.returncode,stdout=r.stdout,stderr=r.stderr));(a.out/'commands.json').write_text(json.dumps(commands,indent=2)+'\n')
  if r.returncode:raise RuntimeError(r.stdout+r.stderr)
  return r.stdout
 run(['python3',root/'tools/ordered_test_projects.py',a.out/'test-projects']);results=[]
 for mode in range(3):
  for capacity in range(4):
   app=a.out/'test-projects'/f'ordered-{mode}-{capacity}';run([cli,'generate','contract',app]);shutil.copyfile(root/'tools/ordered_generated_app.rs',app/'src/main.rs');run(['rustup','run','1.97.1','rustc','--edition=2024','-C','debuginfo=0','-L','dependency='+deps,'--extern','zeno_fcis_synthesis='+synth,app/'src/main.rs','-o',app/'application'],True);results.append(run([app/'application']).strip())
 reviews=[]
 for mode in range(3):
  review=json.loads(run([cli,'contract','review',a.out/'test-projects'/f'ordered-{mode}-1','--gaps','--max-tuples','10000','--max-work','10000000','--format','json']));assert review['status']=='finite-checked' and review['gaps']['determined']==review['tuples'];reviews.append(review)
 (a.out/'result.json').write_text(json.dumps(dict(status='passed',generated_applications=results,reviews=reviews,authority='none'),indent=2)+'\n');print('\n'.join(results));print('CLI B review/gaps/lemma: passed')
if __name__=='__main__':main()
