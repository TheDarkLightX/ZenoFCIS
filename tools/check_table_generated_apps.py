#!/usr/bin/env python3
"""Compile and execute emitted table bindings against an independent native runner.
Uses exact Cargo JSON artifacts from a preceding locked offline CLI build.
"""
import argparse,json,os,shutil,subprocess
from pathlib import Path

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--artifacts',type=Path,required=True);p.add_argument('--out',type=Path,required=True);a=p.parse_args()
    root=Path(__file__).resolve().parents[1];a.out.mkdir(parents=True,exist_ok=False)
    artifacts={}
    for line in a.artifacts.read_text().splitlines():
        try:r=json.loads(line)
        except ValueError:continue
        if r.get('reason')=='compiler-artifact':artifacts[r['target']['name']]=r
    synthesis=next(p for p in artifacts['zeno_fcis_synthesis']['filenames'] if p.endswith('.rlib'))
    cli=artifacts['zeno-fcis']['executable'];deps=str(Path(synthesis).parent)
    commands=[]
    def run(args,build=False):
        if build:
            disk=subprocess.run(['df','-B1G','/'],capture_output=True,text=True,check=True)
            if os.statvfs('/').f_bavail*os.statvfs('/').f_frsize<6*1024**3:raise RuntimeError('disk below 6 GiB')
            (a.out/'disk.log').open('a').write(disk.stdout)
        r=subprocess.run([str(x) for x in args],cwd=root,capture_output=True,text=True)
        commands.append({'command':[str(x) for x in args],'exit_code':r.returncode,'stdout':r.stdout,'stderr':r.stderr})
        (a.out/'commands.json').write_text(json.dumps(commands,indent=2)+'\n')
        if r.returncode:raise RuntimeError(r.stderr or r.stdout)
        return r.stdout
    run(['python3',root/'tools/table_test_projects.py',a.out/'test-projects'])
    results=[]
    for kind in ['map','set']:
        for capacity in range(4):
            app=a.out/'test-projects'/f'{kind}-{capacity}'
            run([cli,'generate','contract',app])
            shutil.copyfile(root/'tools/table_generated_app.rs',app/'src/main.rs')
            run(['rustup','run','1.97.1','rustc','--edition=2024','-C','debuginfo=0','-L','dependency='+deps,'--extern','zeno_fcis_synthesis='+synthesis,app/'src/main.rs','-o',app/'application'],True)
            results.append(run([app/'application']).strip())
    # Native CLI finite judge uses the same reviewed canonical domain, never scalar projection.
    review=json.loads(run([cli,'contract','review',a.out/'test-projects/map-1','--gaps','--max-tuples','1000','--max-work','1000000','--format','json']))
    assert review['status']=='finite-checked' and review['gaps']['determined']==review['tuples']
    (a.out/'result.json').write_text(json.dumps({'status':'passed','generated_applications':results,'cli_review':review,'authority':'none'},indent=2)+'\n')
    print('\n'.join(results));print('CLI finite review/gaps/lemma: passed')
if __name__ == "__main__":
    try:
        from telemetry import run_cli
    except ImportError:
        # Reporting is optional, including for a standalone copy of this tool.
        run_cli = lambda function: function()
    def _entrypoint():
        main()
    run_cli(_entrypoint)
