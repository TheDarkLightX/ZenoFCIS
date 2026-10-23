#!/usr/bin/env python3
"""Build, detect and byte-restore the three specified two-run fault controls."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--evidence',type=Path,required=True)
    parser.add_argument('--target-dir',type=Path,required=True)
    args=parser.parse_args()
    root=Path(__file__).resolve().parents[1]
    source=root/'crates/zeno-fcis-contract/src/two_run.rs'
    original=source.read_bytes(); text=original.decode()
    mutations=[('first-field','for index in observe {','for index in observe.iter().take(1) {','compares_every_observed_field','pre.100.110 stays equal; pre.100.111 is 1 versus0'),
        ('ignore-refusals','if class(&left.outcomes[i]) != class(&right.outcomes[1 - i]) {',
         'if class(&left.outcomes[i]) != class(&right.outcomes[1 - i]) && !matches!(&left.outcomes[i], Outcome::Refused(_)) && !matches!(&right.outcomes[1 - i], Outcome::Refused(_)) {',
         'refuses_in_only_one_order_is_a_difference','initial value0; set2 then increment refuses; increment then set2 accepts'),
        ('include-secret','.filter(|i| !secret.contains(i))','.filter(|_| true)',
         'secret_group_excludes_secret_and_detects_leak','secret flag0 versus1 changes public value0 versus1')]
    args.evidence.mkdir(parents=True,exist_ok=True)
    env=dict(os.environ,CARGO_TARGET_DIR=str(args.target_dir.resolve()),CARGO_BUILD_JOBS='2',CARGO_PROFILE_DEV_DEBUG='0',CARGO_PROFILE_TEST_DEBUG='0',CARGO_INCREMENTAL='0')
    results=[]
    def cargo(name,arguments):
        cmd=['nice','-n','19','ionice','-c3','cargo','+1.97.1',*arguments,'--locked','--offline','-p','zeno-fcis-contract']
        if arguments==['test']:
            cmd+=['--lib',test_name,'--','--exact','--nocapture']
        with (args.evidence/(name+'.log')).open('w') as log:
            p=subprocess.run(cmd,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
        return p.returncode
    try:
        for label,before,after,test,witness in mutations:
            test_name='two_run::tests::'+test
            assert text.count(before)==1,(label,'mutation point is not unique')
            normal=cargo(label+'-normal',['test']); assert normal==0,(label,'baseline')
            source.write_text(text.replace(before,after))
            build=cargo(label+'-build',['build']); assert build==0,(label,'must build')
            result=cargo(label+'-mutated',['test'])
            log=(args.evidence/(label+'-mutated.log')).read_text()
            assert result==101 and 'assertion `left == right` failed' in log and test_name in log,(label,'must fail on named assertion')
            source.write_bytes(original)
            assert source.read_bytes()==original
            results.append(dict(control=label,test=test_name,normal_exit=normal,build_exit=build,mutated_exit=result,witness=witness,restored_sha256=hashlib.sha256(original).hexdigest()))
            print(label, 'normal=0 build=0 mutated=101 restored=true',flush=True)
    finally:
        source.write_bytes(original)
        assert source.read_bytes()==original
        (args.evidence/'controls.json').write_text(json.dumps(results,indent=2)+'\n')
    return 0


if __name__=='__main__': raise SystemExit(main())
