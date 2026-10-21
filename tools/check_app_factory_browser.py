#!/usr/bin/env python3
"""Host-only Chrome and TCP journey against each generated Rust application."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import secrets
import shutil
import signal
import subprocess
import tempfile
import time
import urllib.error
import urllib.request

ROOT=Path(__file__).resolve().parents[1]

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--apps',type=Path,required=True,help='one completed check_app_factory run directory')
    parser.add_argument('--out',type=Path,required=True)
    parser.add_argument('--chrome',default=shutil.which('google-chrome') or shutil.which('chromium'))
    args=parser.parse_args()
    if not args.chrome:
        raise SystemExit('Chrome is required for the host evidence lane')
    args.out.mkdir(parents=True,exist_ok=True)
    out=Path(tempfile.mkdtemp(prefix='browser-',dir=args.out.resolve()))
    secret=out/'credential'
    fd=os.open(secret,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
    with os.fdopen(fd,'w') as stream:
        stream.write(secrets.token_hex(32)+'\n')
    results=[]
    for index,name in enumerate(['withdrawal-queue','inventory-reservation','wide-counter']):
        app=(args.apps/name).resolve()
        manifest=json.loads((app/'app-manifest.json').read_text())
        for relative,digest in manifest['files'].items():
            if hashlib.sha256((app/relative).read_bytes()).hexdigest()!=digest:
                raise RuntimeError(f'app manifest drift: {name}/{relative}')
        # A host run must exercise the current sign-in sources, never stale assets.
        for relative,digest in manifest['generator_sources'].items():
            if hashlib.sha256((ROOT/relative).read_bytes()).hexdigest()!=digest:
                raise RuntimeError(f'app generator drift: {name}/{relative}; regenerate applications')
        for asset in (app/'www').rglob('*'):
            if asset.is_file() and secret.read_bytes().strip() in asset.read_bytes():
                raise RuntimeError('credential in public asset')
        fixture=json.loads((app/'app-checks/journey.json').read_text())
        context=out/f'{name}-context.json';context.write_text(json.dumps(fixture['steps'][0]['context'])+'\n')
        address=f'127.0.0.1:{18260+index}';url=f'http://{address}'
        env=os.environ.copy();env.update(ZENO_APP_ADDRESS=address,ZENO_APP_SECRET_FILE=str(secret),ZENO_APP_CONTEXT=str(context),ZENO_APP_STORE=str(out/f'{name}.sqlite'),ZENO_APP_RECEIVER=str(out/f'{name}-deliveries'))
        with (out/f'{name}-server.log').open('w') as log:
            process=subprocess.Popen(['nice','-n','19','ionice','-c3','cargo','+1.97.1','run','--locked','--offline','--bin','app-server'],cwd=app,env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
            try:
                deadline=time.monotonic()+120
                while True:
                    if process.poll() is not None:
                        raise RuntimeError('server stopped before listening')
                    try:
                        with urllib.request.urlopen(url+'/',timeout=1) as response:
                            if response.status==200:
                                break
                    except urllib.error.HTTPError:
                        pass
                    except OSError:
                        pass
                    if time.monotonic()>deadline:
                        raise RuntimeError('server readiness deadline')
                    time.sleep(.1)
                result=subprocess.run(['nice','-n','19','ionice','-c3','node',str(ROOT/'site/tests/app-factory-browser.mjs'),args.chrome,str(out/f'{name}-profile'),url,str(secret),str(app/'app-checks/journey.json'),str(out/f'{name}.json')])
                results.append({'template':name,'exit':result.returncode})
                (out/'results.json').write_text(json.dumps(results,indent=2)+'\n')
                if result.returncode:
                    raise RuntimeError(f'{name}: browser journey failed')
            finally:
                os.killpg(process.pid,signal.SIGTERM)
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid,signal.SIGKILL);process.wait()
    (out/'results.json').write_text(json.dumps(results,indent=2)+'\n')
    print(f'app-browser: PASS; {out}')

if __name__ == "__main__":
    try:
        from telemetry import run_cli
    except ImportError:
        # Reporting is optional, including for a standalone copy of this tool.
        run_cli = lambda function: function()
    def _entrypoint():
        main()
    run_cli(_entrypoint)
