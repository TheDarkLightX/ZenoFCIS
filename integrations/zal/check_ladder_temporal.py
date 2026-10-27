#!/usr/bin/env python3
"""Build and challenge the experimental temporal ladder; authority: none."""
import argparse
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ROOT=Path(__file__).resolve().parents[2]
ZAL=ROOT/'integrations/zal'


def environment():
    return dict(os.environ,CARGO_TARGET_DIR=str(Path(os.environ.get('CARGO_TARGET_DIR',ROOT/'target')).resolve()),
                CARGO_PROFILE_DEV_DEBUG='0',CARGO_PROFILE_TEST_DEBUG='0',CARGO_BUILD_JOBS='2',CARGO_NET_OFFLINE='true')


def cargo(arguments,env):
    subprocess.run(['df','-B1G','/'],check=True)
    if shutil.disk_usage('/').free<6*1024**3:raise RuntimeError('STOP: less than 6 GiB free')
    subprocess.run(['cargo','+1.97.1',*arguments,'--locked','--offline','-j','2'],cwd=ROOT,env=env,check=True)


def native_build(env):
    cargo(['build','-p','zeno-fcis-formal-tools','--example','ladder_temporal_parity'],env)
    cargo(['build','-p','zeno-fcis-cli'],env)


def planted(env,logs):
    logs.mkdir(parents=True,exist_ok=True)
    owned=[ZAL/'ladder.py',ZAL/'ladder_en.json',ROOT/'crates/zeno-fcis-formal-tools/src/model_diagnostics.rs',ZAL/'test_ladder_temporal.py']
    before={p:hashlib.sha256(p.read_bytes()).hexdigest() for p in owned}
    def challenge(label,test,work,extra=None):
        runenv=dict(env,**(extra or {}))
        result=subprocess.run([sys.executable,'-B','-m','unittest',test],cwd=work,env=runenv,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=120)
        (logs/(label+'.log')).write_text(result.stdout)
        if result.returncode==0 or ('FAIL: '+test.rsplit('.',1)[-1]) not in result.stdout:
            raise RuntimeError('planted defect survived or failed outside named check: '+label)
        print('KILLED '+label+' by '+test,flush=True)
    with tempfile.TemporaryDirectory(prefix='zal-defects-') as directory:
        work=Path(directory)
        for filename in ('ladder.py','ladder_en.json','test_ladder.py','test_ladder_temporal.py','temporal_lowering.py'):
            shutil.copyfile(ZAL/filename,work/filename)
        shutil.copytree(ZAL/'examples',work/'examples')
        original=(work/'ladder.py').read_text();tests=(work/'test_ladder_temporal.py').read_text()
        first="if len(distinct)==2:\n            raise Refusal('ambiguous at token 0; expected exactly one expanded normalized tree')"
        first_defect="if len(distinct)==2:\n            return name_implicit(next(iter(distinct.values())))"
        if original.count(first)!=1:raise RuntimeError('first-parse mutation anchor drift')
        (work/'ladder.py').write_text(original.replace(first,first_defect))
        challenge('first-parse','test_ladder_temporal.TemporalLadderChecks.test_complete_uniqueness',work)
        challenge('m0-first-parse','test_ladder.LadderChecks.test02_complete_uniqueness',work)
        (work/'ladder.py').write_text(original)
        anchor='remaining=max(remaining-1,0)'
        if tests.count(anchor)!=1:raise RuntimeError('alarm mutation anchor drift')
        (work/'test_ladder_temporal.py').write_text(tests.replace(anchor,'remaining=2 if alarm else max(remaining-1,0)'))
        challenge('resettable-alarm-budget','test_ladder_temporal.TemporalLadderChecks.test_alarm_budget_nonresettable',work)
        (work/'test_ladder_temporal.py').write_text(tests)
        anchor="return templates[tag].format(a=operand(n[1],0),b=operand(n[2],1))"
        if original.count(anchor)!=2:raise RuntimeError('comparison mutation anchor drift')
        # Replace only the final binary-render branch, leaving the parser intact.
        head,tail=original.rsplit(anchor,1)
        (work/'ladder.py').write_text(head+"return templates['>' if tag=='>=' else tag].format(a=operand(n[1],0),b=operand(n[2],1))"+tail)
        challenge('m0-comparison','test_ladder.LadderChecks.test01_corpus_roundtrip_and_goldens',work)
        (work/'ladder.py').write_text(original.replace('after == prime(before, ctx)','True'))
        challenge('m0-prime','test_ladder.LadderChecks.test06_prime_sensitive_idiom',work)
        (work/'ladder.py').write_text(original)
        evaluator=owned[2].read_text()
        start=evaluator.index('        TemporalFormula::Next(value) => {',evaluator.index('fn eval_temporal_at'))
        end=evaluator.index('        TemporalFormula::Always(value)',start)
        weak=evaluator[:start]+evaluator[start:end].replace('Ok(false)','Ok(true)')+evaluator[end:]
        start=evaluator.index('        TemporalFormula::Until(left, right) => {',evaluator.index('fn eval_temporal_at'))
        end=evaluator.index('\n    }\n}',start)
        block=evaluator[start:end];head,tail=block.rsplit('Ok(false)',1)
        missing=evaluator[:start]+head+'Ok(true)'+tail+evaluator[end:]
        harness=(ROOT/'crates/zeno-fcis-formal-tools/examples/ladder_temporal_parity.rs').read_text()
        for label,source,test in (
            ('weak-next-at-end',weak,'test_ladder_temporal.TemporalLadderChecks.test_strong_next_endpoint'),
            ('missing-until-witness',missing,'test_ladder_temporal.TemporalLadderChecks.test_until_requires_witness')):
            model=work/'model_diagnostics.rs';model.write_text(source)
            name='ladder_temporal_defect';example=ROOT/'crates/zeno-fcis-formal-tools/examples'/(''+name+'.rs')
            if example.exists():raise RuntimeError('refusing existing defect harness destination')
            try:
                example.write_text(harness.replace('#[path = "../src/model_diagnostics.rs"]','#[path = "'+str(model)+'"]'))
                cargo(['build','-p','zeno-fcis-formal-tools','--example',name],env)
                executable=Path(env['CARGO_TARGET_DIR'])/'debug/examples'/name
                challenge(label,test,work,{'ZAL_TEMPORAL_CLI':str(executable)})
            finally:
                example.unlink()
    if any(hashlib.sha256(p.read_bytes()).hexdigest()!=digest for p,digest in before.items()):
        raise RuntimeError('source changed during isolated defect controls')
    print('PASS: seven named planted controls; live source unchanged')


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command',choices=('native-build','suite','planted'))
    parser.add_argument('--logs',type=Path)
    args=parser.parse_args();env=environment()
    if args.command=='native-build':native_build(env)
    elif args.command=='suite':
        subprocess.run([sys.executable,'-B','-m','unittest','discover','-s','integrations/zal','-p','test_*.py','-v'],cwd=ROOT,env=env,check=True)
    elif args.logs is not None:planted(env,args.logs)
    else:
        with tempfile.TemporaryDirectory(prefix='zal-defect-logs-') as logs:planted(env,Path(logs))
    return 0


if __name__=='__main__':raise SystemExit(main())
