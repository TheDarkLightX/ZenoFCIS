#!/usr/bin/env python3
"""Run one real page/worker/Wasm act through hashed DevTools fulfilment."""
import argparse,os,subprocess,sys,tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'site/tests'))
from deploy_check import find_chrome

def main():
 parser=argparse.ArgumentParser(description=__doc__)
 parser.add_argument('--out',type=Path,required=True)
 parser.add_argument('--chrome')
 args=parser.parse_args();args.out.mkdir(parents=True,exist_ok=True)
 chrome=find_chrome(args.chrome)
 manifest=args.out.resolve()/'assets.json'
 subprocess.run(['node',str(ROOT/'site/tests/asset-manifest.mjs'),str(ROOT/'site/public'),str(manifest)],check=True)
 with tempfile.TemporaryDirectory(prefix='zenoui-pipe-') as profile:
  command=['node',str(ROOT/'site/tests/pipe-browser.mjs'),chrome,profile,str(ROOT/'site/public'),str(manifest),str(ROOT/'site/tests/harness/factory-native.json'),str(args.out.resolve()/'browser.json')]
  result=subprocess.run(command,timeout=55)
  return result.returncode
if __name__=='__main__':
 try:sys.exit(main())
 except (OSError,ValueError,RuntimeError,subprocess.SubprocessError) as error:print(f'socket-free check: {error}',file=sys.stderr);sys.exit(1)
