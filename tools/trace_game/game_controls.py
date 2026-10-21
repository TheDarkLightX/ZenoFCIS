#!/usr/bin/env python3
"""Build single-defect mutations and require named assertions, never setup errors."""
import argparse
import hashlib
import importlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import unittest

ROOT=Path(__file__).resolve().parents[2]


def exercise(module,name):
    module.OBSERVATIONS.clear()
    log=io.StringIO()
    result=unittest.TextTestRunner(stream=log,verbosity=2).run(
        unittest.TestSuite([module.NativeGame(name)]))
    return dict(passed=result.wasSuccessful(),failures=len(result.failures),errors=len(result.errors),
                observations=dict(module.OBSERVATIONS),log=log.getvalue())


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--native',required=True,type=Path)
    parser.add_argument('--out',required=True,type=Path)
    parser.add_argument('--target',required=True,type=Path)
    args=parser.parse_args(); args.out.mkdir(parents=True,exist_ok=False)
    os.environ['ZENO_TRACE_GAME_NATIVE']=str(args.native.resolve())
    tests=importlib.import_module('test_game_input')
    tests.NativeGame.setUpClass()
    tests.NativeGame.setUpClass=classmethod(lambda cls: None)
    records=[]
    for label,name,module_name,old,new in (
        ('capabilities','test_capabilities_bound_events','game_spec',
         "lists = [values[p['name']] for p in inputs]",
         "lists = [([0,1] if p['domain']['kind']=='bool' else list(range(int(p['domain']['min']),int(p['domain']['max'])+1)) if p['domain']['kind']=='int' else p['domain']['variants']) for p in inputs]"),
        ('reachable-state','test_certificate_contains_all_reachable_states','solve','W=W))','W=W[:-1]))'),
        ('product-limit','test_product_limit_enforced','product','if len(states) >= max_states:','if False:'),
    ):
        normal=exercise(tests,name)
        module=importlib.import_module(module_name); original=Path(module.__file__).read_text()
        if original.count(old)!=1: raise RuntimeError('mutation anchor is not unique: '+label)
        mutated=original.replace(old,new)
        specimen=args.out/(label+'.py'); specimen.write_text(mutated)
        compiled=compile(mutated,str(specimen),'exec')
        try:
            exec(compiled,module.__dict__)
            tests.build=importlib.import_module('product').build
            tests.certify=importlib.import_module('solve').certify
            mutant=exercise(tests,name)
        finally:
            exec(compile(original,module.__file__,'exec'),module.__dict__)
            tests.build=importlib.import_module('product').build
            tests.certify=importlib.import_module('solve').certify
        records.append(dict(control=label,test=name,build='compiled',normal=normal,mutated=mutant,
                            source_sha256=hashlib.sha256(mutated.encode()).hexdigest()))
    name='test_safety_observes_post'; normal=exercise(tests,name)
    path=ROOT/'crates/zeno-fcis-contract/examples/trace_game_input.rs'
    original=path.read_text(); old='&tuple, &post, outbox'; new='&tuple, &pre, outbox'
    if original.count(old)!=1: raise RuntimeError('native mutation anchor is not unique')
    mutated=original.replace(old,new); (args.out/'pre-state.rs').write_text(mutated)
    env=dict(os.environ,CARGO_TARGET_DIR=str(args.target.resolve()),CARGO_PROFILE_DEV_DEBUG='0',CARGO_PROFILE_TEST_DEBUG='0')
    command=['nice','-n','19','ionice','-c3','cargo','+1.97.1','build','--locked','--offline','-p','zeno-fcis-contract','--example','trace_game_input','-j','3']
    try:
        path.write_text(mutated)
        with (args.out/'pre-state-build.log').open('wb') as log:
            subprocess.run(command,cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
        binary=args.out/'pre-state-driver'; shutil.copy2(args.target/'debug/examples/trace_game_input',binary)
        tests.NativeGame.planted=tests.prepare(ROOT/'test-data/trace-games/escrow-planted.json',binary.resolve())
        mutant=exercise(tests,name)
    finally:
        path.write_text(original)
        with (args.out/'restored-build.log').open('wb') as log:
            subprocess.run(command,cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
    records.append(dict(control='pre-state',test=name,build='cargo build passed',normal=normal,mutated=mutant,
                        source_sha256=hashlib.sha256(mutated.encode()).hexdigest()))
    (args.out/'controls.json').write_text(json.dumps(records,indent=2)+'\n')
    valid=all(r['normal']['passed'] and r['mutated']['failures']==1 and r['mutated']['errors']==0 for r in records)
    print(json.dumps(dict(controls=len(records),all_built_and_failed_named_assertions=valid)))
    return 0 if valid else 1


if __name__=='__main__': raise SystemExit(main())
