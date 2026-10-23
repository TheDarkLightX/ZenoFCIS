#!/usr/bin/env python3
"""Generate one local audit HTTP app from an exact scalar service manifest.

No decision rules are merged or copied into the shell. The app uses ServiceAuthority
and its nominal publication, with one SQLite transaction per joint commit.
"""
import argparse,hashlib,json,os,subprocess,tomllib,shutil
from pathlib import Path
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--service',required=True,type=Path);p.add_argument('--out',required=True,type=Path)
a=p.parse_args();repo=Path(__file__).resolve().parent.parent;out=a.out.resolve();service=a.service.resolve()
if out.exists():raise SystemExit('fresh output directory required')
manifest=json.loads(service.read_text())
if manifest['schema']!='zeno-fcis/service/1':raise SystemExit('unsupported service profile')
source_files=[Path(__file__).resolve(),repo/'crates/zeno-fcis-cli/service-app/main.rs.in',repo/'Cargo.toml',repo/'Cargo.lock',repo/'rust-toolchain.toml']+[x for x in (repo/'crates').rglob('*') if x.is_file() and x.suffix in ('.rs','.toml','.json','.txt')]
def source_pins():return {str(x.relative_to(repo)):hashlib.sha256(x.read_bytes()).hexdigest() for x in sorted(source_files)}
before=source_pins();(out/'src').mkdir(parents=True);(out/'contract').mkdir()
(out/'contract/service.json').write_bytes(service.read_bytes());sources=[]
for at,core in enumerate(manifest['composition']['components']):
 directory=service.parent/core['path']
 for file,label in [('project.zeno','project'),('v2/policy.json','policy')]:
  data=(directory/file).read_bytes()
  if hashlib.sha256(data).hexdigest()!=core[('project' if label=='project' else 'policy')+'_sha256']:raise SystemExit('source pin mismatch')
  (out/'contract'/f'core-{at}-{label}.txt').write_bytes(data)
 sources.append(f'LeafSources{{project:include_str!("../contract/core-{at}-project.txt").into(),rules:include_str!("../contract/core-{at}-policy.txt").into()}}')
template=repo/'crates/zeno-fcis-cli/service-app';(out/'src/main.rs').write_text((template/'main.rs.in').read_text().replace('__SOURCES__',','.join(sources)))
(out/'src/store.rs').write_bytes((template/'store.rs').read_bytes())
version=json.loads(subprocess.run(['cargo','+1.97.1','metadata','--locked','--offline','--no-deps','--format-version','1'],cwd=repo,capture_output=True,text=True,check=True).stdout)['packages'][0]['version']
lines=['[package]','name = "composition-service-app"','version = "0.1.0"','edition = "2024"','','[workspace]','','[dependencies]','serde = {version="=1.0.229",features=["derive"]}','serde_json = "=1.0.151"']
for crate in ('zeno-fcis-contract','zeno-fcis-shell-sqlite','zeno-fcis-codec','zeno-fcis-crypto'):
 lines.append(crate+' = { version='+json.dumps('='+version)+', path='+json.dumps(str(repo/'crates'/crate))+' }')
(out/'Cargo.toml').write_text('\n'.join(lines)+'\n')
env=os.environ|{'CARGO_PROFILE_DEV_DEBUG':'0','CARGO_PROFILE_TEST_DEBUG':'0','CARGO_BUILD_JOBS':'4','CARGO_NET_OFFLINE':'true','CARGO_INCREMENTAL':'0'}
shutil.copy2(repo/'Cargo.lock',out/'Cargo.lock')
# Resolve only the generated root while retaining existing locked versions.
subprocess.run(['nice','-n','19','ionice','-c3','cargo','+1.97.1','metadata','--offline','--format-version','1'],cwd=out,env=env,stdout=subprocess.DEVNULL,check=True)
root_packages={(x['name'],x['version'],x.get('source')) for x in tomllib.loads((repo/'Cargo.lock').read_text())['package']}
external={(x['name'],x['version'],x.get('source')) for x in tomllib.loads((out/'Cargo.lock').read_text())['package'] if 'source' in x}
if not external <= root_packages:raise SystemExit('generated app added an external dependency')
subprocess.run(['nice','-n','19','ionice','-c3','cargo','+1.97.1','build','--locked','--offline'],cwd=out,env=env,check=True)
if source_pins()!=before:raise SystemExit('build source changed while generating app')
pins={'schema':'zeno-fcis/service-app-build/1','service_sha256':hashlib.sha256(service.read_bytes()).hexdigest(),'source_files':before,'generated_files':{str(x.relative_to(out)):hashlib.sha256(x.read_bytes()).hexdigest() for x in sorted(out.rglob('*')) if x.is_file()}}
(out/'app-manifest.json').write_text(json.dumps(pins,indent=2)+'\n');print(json.dumps({'status':'generated','out':str(out),'profile':'local-scalar-audit'}))
