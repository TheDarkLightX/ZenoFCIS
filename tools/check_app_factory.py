#!/usr/bin/env python3
"""Build and check the three supported app journeys and the multi-party escrow
example; TCP is an explicit host lane."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT=Path(__file__).resolve().parents[1]

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out',type=Path)
    parser.add_argument('--tcp',action='store_true')
    args=parser.parse_args()
    target=Path(os.environ.get('CARGO_TARGET_DIR',ROOT/'target')).resolve()
    parent=args.out.resolve() if args.out else target/'app-factory-checks'
    parent.mkdir(parents=True,exist_ok=True)
    out=Path(tempfile.mkdtemp(prefix='run-',dir=parent))
    environment=os.environ.copy()
    if args.tcp:
        environment.pop('INGRESS_TEST_MEMORY',None)
        environment['APP_JOURNEY_TCP']='1'
    else:
        environment['INGRESS_TEST_MEMORY']='1'
        environment.pop('APP_JOURNEY_TCP',None)
    checks=[]
    def run(command,*,cwd=ROOT,env=environment):
        path=out/f'{len(checks):02d}.log'
        print('check:', ' '.join(map(str,command)),flush=True)
        with path.open('w') as log:
            result=subprocess.run(['nice','-n','19','ionice','-c3',*map(str,command)],cwd=cwd,env=env,stdout=log,stderr=subprocess.STDOUT)
        checks.append({'command':list(map(str,command)),'cwd':str(cwd),'exit':result.returncode,'log':path.name})
        (out/'checks.json').write_text(json.dumps({'transport':'tcp' if args.tcp else 'memory-http','checks':checks},indent=2)+'\n')
        if result.returncode:
            print(path.read_text()[-12000:]);raise SystemExit(f'failed: {path}')
    cargo=['cargo','+1.97.1']
    run([*cargo,'build','--locked','--offline','-p','zeno-fcis-cli'])
    cli=target/'debug/zeno-fcis'
    for template,source in [('wide-counter','site/zenoui/test-projects/wide-counter'),('inventory-reservation','crates/zeno-fcis-cli/templates/inventory-reservation'),('withdrawal-queue','crates/zeno-fcis-cli/templates/withdrawal-queue')]:
        app=out/template
        run([cli,'new',app,'--contract',ROOT/source/'project.zeno','--app'])
        receipt=json.loads((app/'app-manifest.json').read_text())
        for name,digest in receipt['files'].items():
            if hashlib.sha256((app/name).read_bytes()).hexdigest()!=digest:
                raise RuntimeError(f'manifest drift: {template}/{name}')
        run([*cargo,'test','--locked','--offline','--no-default-features'],cwd=app)
        local=environment.copy();local['APP_JOURNEY_OUT']=str(out/f'{template}.json')
        run([*cargo,'test','--locked','--offline','--all-features'],cwd=app,env=local)
        run([*cargo,'clippy','--locked','--offline','--all-features','--all-targets','--','-D','warnings'],cwd=app)
        run(['node','--experimental-vm-modules',ROOT/'site/tests/app-durable.test.mjs',out/f'{template}.json'])
    # One server for three parties: buyer, seller and operator each act with their own credential.
    app = out/'escrow-deal'
    run([cli,'new',app,'--contract',ROOT/'examples/apps/escrow-deal/project.zeno','--app'])
    receipt = json.loads((app/'app-manifest.json').read_text())
    for name,digest in receipt['files'].items():
        if hashlib.sha256((app/name).read_bytes()).hexdigest()!=digest:
            raise RuntimeError(f'manifest drift: escrow-deal/{name}')
    run([*cargo,'test','--locked','--offline','--all-features','--test','app_journey'],cwd=app)
    if 'test parties_journey ... ok' not in (out/checks[-1]['log']).read_text():
        raise SystemExit('parties journey did not run')
    if args.tcp:
        # Hostile raw HTTP against the escrow server itself, on loopback only.
        built=subprocess.run([*cargo,'build','--locked','--offline','--bin','app-server','--message-format','json'],cwd=app,env=environment,capture_output=True,text=True,check=True)
        server=[m['executable'] for m in map(json.loads,built.stdout.splitlines()) if m.get('reason')=='compiler-artifact' and m['target']['name']=='app-server'][0]
        run(['python3','tools/check_http_wire.py','--app',app,'--server',server,'--out',out/'http-wire.json'])
        # The admission rules as a contract: every input reviewed, then sent to this server.
        run(['python3','tools/check_admission_model.py','--cli',cli,'--app',app,'--server',server,'--out',out/'admission'])
    run([*cargo,'test','--locked','--offline','-p','zeno-fcis-ingress'])
    run(['python3','tools/check_ingress.py'])
    print(f'app-factory: PASS; {out}',flush=True)

if __name__=='__main__':
    main()
