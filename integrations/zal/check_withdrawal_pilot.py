#!/usr/bin/env python3
"""Build, run and challenge the withdrawal temporal pilot; authority: none.

native-build  debug harnesses for the unit tests
run           the bounded claim through length 12 (release replay) and, with
              --lean and --lean-root, the unbounded claim's kernel check
planted       the four planted defects, each against its named test
bind-source   refresh the statements' source revision from the contract bytes
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ROOT=Path(__file__).resolve().parents[2]
ZAL=ROOT/'integrations/zal'
sys.path.insert(0,str(ZAL))
import ladder as l  # noqa: E402
import withdrawal_pilot as w  # noqa: E402

EXAMPLES_DIR=ROOT/'crates/zeno-fcis-formal-tools/examples'
TEST='test_withdrawal_pilot.WithdrawalPilotChecks.'


def environment():
    return dict(os.environ,CARGO_TARGET_DIR=str(Path(os.environ.get('CARGO_TARGET_DIR',ROOT/'target')).resolve()),
                CARGO_PROFILE_DEV_DEBUG='0',CARGO_PROFILE_TEST_DEBUG='0',CARGO_BUILD_JOBS='2',CARGO_NET_OFFLINE='true')


def cargo(arguments,env,extra=()):
    if shutil.disk_usage('/').free<6*1024**3:raise RuntimeError('STOP: less than 6 GiB free')
    subprocess.run(['cargo','+1.97.1',*arguments,'--locked','--offline','-j','2',*extra],cwd=ROOT,env=env,check=True)


def native_build(env,release=False):
    profile=['--release'] if release else []
    cargo(['build','-p','zeno-fcis-generated-code-tests','--example','withdrawal_queue_decisions'],env)
    cargo(['build',*profile,'-p','zeno-fcis-formal-tools','--example','withdrawal_temporal_pilot'],env)


def target(env,profile,name):return Path(env['CARGO_TARGET_DIR'])/profile/'examples'/name


def bind_source():
    """Refresh only the derived source binding, preserving the statements."""
    revision='withdrawal-queue contract sources, sha256 '+w.contract_identity()
    for name in ('alarm_withdrawal.json','alarm_withdrawal_bounded.json'):
        path=w.EXAMPLES/name
        data=json.loads(path.read_text())
        data['trace']['source_revision']=revision
        path.write_text(json.dumps(data,indent=2,ensure_ascii=False)+'\n')


def export_lean(env,claim,out):
    """The repository's Lean exporter, reached through its ignored unit test."""
    run=dict(env,ZENO_FCIS_LADDER_LEAN_CLAIM=str(claim),ZENO_FCIS_LADDER_LEAN_OUT=str(out))
    cargo(['test','-p','zeno-fcis-formal-tools','--lib'],run,('--','--ignored','--exact','translation_tests::ladder_claim_lean_export_from_environment'))
    return out.read_text()


def run(env,length,lean,lean_root,receipt):
    native_build(env,release=True)
    result=subprocess.run([str(target(env,'debug','withdrawal_queue_decisions'))],capture_output=True,text=True,check=True)
    graph=w.load_graph(result.stdout)
    bounded=l.load(w.EXAMPLES/'alarm_withdrawal_bounded.json');unbounded=l.load(w.EXAMPLES/'alarm_withdrawal.json')
    paths=w.statement_paths(bounded);automaton,info=w.tick_automaton(graph,paths)
    record={'authority':'none','contract_sources_sha256':w.contract_identity(),'graph_sha256':hashlib.sha256(result.stdout.encode()).hexdigest(),
            'adapter':w.check_adapter(graph),'tick_projection':info,
            'bounded':{'statement_scope':l.scope_identity(bounded),'k':w.K,'horizon':w.HORIZON,'lengths':length}}
    report=w.finite_check(target(env,'release','withdrawal_temporal_pilot'),bounded,automaton,length)
    passed=report['counterexamples']==0 and report['indeterminate']==0 and length==w.HORIZON
    record['bounded'].update(report,evidence='finite checked' if passed else 'not established')
    print('bounded: %d words of lengths 1..%d; %d counterexamples; %d indeterminate'%(report['words'],length,report['counterexamples'],report['indeterminate']),flush=True)
    name,obligation=w.run_assumption_split(unbounded)
    record['unbounded']={'statement_scope':l.scope_identity(unbounded),'premise':name,'lowering_subject':obligation.subject,'evidence':'obligation (Lean not run)'}
    if lean:
        with tempfile.TemporaryDirectory(prefix='zal-withdrawal-export-') as directory:
            claim=Path(directory)/'claim.zeno';claim.write_text(obligation.source)
            exported=export_lean(env,claim,Path(directory)/'claim.lean')
        tree=subprocess.run([str(target(env,'release','withdrawal_temporal_pilot')),'inventory-lean',str(lean_root)],capture_output=True,text=True,check=True).stdout.split()[-1]
        pinned=(ROOT/'release/lean-4.30.0-tree.sha256').read_text().strip()
        source,lean_info=w.lean_source(graph,exported)
        checked=w.run_lean(source,lean,'withdrawal_never_freezes')
        control=w.run_lean(w.refutation_source(graph,exported)[0],lean,'withdrawal_freezes')
        evidence='kernel checked' if checked['status']=='kernel-checked' and tree==pinned and control['status']!='kernel-checked' else 'not established'
        record['unbounded'].update(exported_sha256=hashlib.sha256(exported.encode()).hexdigest(),lean_tree_sha256=tree,
                                   lean_tree_pinned=tree==pinned,model=lean_info,result=checked,
                                   refutation_on_contract=control['status'],evidence=evidence)
        print('unbounded: %s; axioms %s; toolchain tree %s; refutation on the contract %s'%(evidence,checked['axioms'],'pinned' if tree==pinned else 'UNPINNED',control['status']),flush=True)
    text=json.dumps(record,indent=1,sort_keys=True)
    if receipt:Path(receipt).write_text(text+'\n')
    else:print(text)
    return 0 if passed and (not lean or record['unbounded']['evidence']=='kernel checked') else 1


def planted(env,logs,lean):
    logs.mkdir(parents=True,exist_ok=True)
    owned=[ZAL/'withdrawal_pilot.py',ZAL/'withdrawal_pilot.lean',ZAL/'test_withdrawal_pilot.py',
           ROOT/'crates/zeno-fcis-formal-tools/src/model_diagnostics.rs',w.EXAMPLES/'alarm_withdrawal_bounded.json']
    before={p:hashlib.sha256(p.read_bytes()).hexdigest() for p in owned}
    lean_env={}
    if lean:
        with tempfile.TemporaryDirectory(prefix='zal-withdrawal-export-') as directory:
            claim=Path(directory)/'claim.zeno';claim.write_text(w.run_assumption_split(l.load(w.EXAMPLES/'alarm_withdrawal.json'))[1].source)
            exported=export_lean(env,claim,Path(directory)/'claim.lean')
        export=logs/'exported-claim.lean';export.write_text(exported)
        lean_env={'ZENO_FCIS_LEAN':str(lean),'ZAL_WITHDRAWAL_LEAN_EXPORT':str(export)}
    def check(label,test,work,expect_fail,extra=None,reason=None):
        result=subprocess.run([sys.executable,'-B','-m','unittest',TEST+test],cwd=work,env=dict(env,**lean_env,**(extra or {})),
                              text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=3600)
        (logs/(label+'.log')).write_text(result.stdout)
        failed=result.returncode!=0 and ('FAIL: '+test) in result.stdout
        if expect_fail and (not failed or (reason and reason not in result.stdout)):
            raise RuntimeError('planted defect survived or failed outside named check: '+label)
        if not expect_fail and result.returncode:raise RuntimeError('baseline failed: '+label)
        print(('KILLED ' if expect_fail else 'BASELINE ')+label+' by '+test,flush=True)
    with tempfile.TemporaryDirectory(prefix='zal-withdrawal-defects-') as directory:
        work=Path(directory)
        for filename in ('ladder.py','ladder_en.json','temporal_lowering.py','withdrawal_pilot.py','withdrawal_pilot.lean',
                         'withdrawal_pilot_refutation.lean','test_withdrawal_pilot.py'):
            shutil.copyfile(ZAL/filename,work/filename)
        shutil.copytree(ZAL/'examples',work/'examples')
        # The copy reads the contract sources from this checkout.
        source=(work/'withdrawal_pilot.py').read_text()
        rooted=source.replace("ROOT=Path(__file__).resolve().parents[2]",'ROOT=Path(%r)'%str(ROOT))
        if rooted==source:raise RuntimeError('root anchor drift')
        (work/'withdrawal_pilot.py').write_text(rooted);source=rooted
        tests=('test_adapter_obligations','test_bounded_claim_finite_checked','test_strong_endpoint')+(('test_unbounded_kernel_checked',) if lean else ())
        for test in tests:check('baseline-'+test,test,work,False)
        # 1. A resettable alarm budget, in the contract and in the Lean restatement.
        model=(work/'withdrawal_pilot.lean').read_text()
        p2='pause := .p1, must := false }, quiet)';p1='pause := .p0, must := due c.la || due c.lb }, quiet)'
        if model.count(p2)!=1 or model.count(p1)!=1 or source.count('    return graph\n')!=1:raise RuntimeError('resettable anchor drift')
        (work/'withdrawal_pilot.py').write_text(source.replace('    return graph\n','    return resettable_budget(graph)\n'))
        (work/'withdrawal_pilot.lean').write_text(model.replace(p2,'pause := if alarm then .p2 else .p1, must := false }, quiet)')
                                                  .replace(p1,'pause := if alarm then .p2 else .p0, must := !alarm && (due c.la || due c.lb) }, quiet)'))
        check('resettable-budget-bounded','test_bounded_claim_finite_checked',work,True,reason='Tuples differ: (')
        if lean:
            check('resettable-budget-unbounded','test_unbounded_kernel_checked',work,True,reason='Tactic `decide` proved that the proposition')
            mutated=subprocess.run([sys.executable,'-B','-c','import json,sys,withdrawal_pilot as w;g=w.resettable_budget(w.load_graph(open(sys.argv[1]).read()));'
                                    'r=w.run_lean(w.refutation_source(g,open(sys.argv[2]).read())[0],sys.argv[3],"withdrawal_freezes");print(r["status"])',
                                    str(logs/'graph.json'),lean_env['ZAL_WITHDRAWAL_LEAN_EXPORT'],str(lean)],cwd=work,capture_output=True,text=True)
            if mutated.stdout.strip()!='kernel-checked':raise RuntimeError('mutant refutation was not kernel checked: '+mutated.stdout+mutated.stderr)
            print('REFUTED resettable budget: a fair frozen run is kernel checked',flush=True)
        (work/'withdrawal_pilot.py').write_text(source);(work/'withdrawal_pilot.lean').write_text(model)
        # 2. A weak next, in a copy of the Rust evaluator.
        evaluator=owned[3].read_text()
        start=evaluator.index('        TemporalFormula::Next(value) => {',evaluator.index('fn eval_temporal_at'))
        end=evaluator.index('        TemporalFormula::Always(value)',start)
        weak=work/'model_diagnostics.rs';weak.write_text(evaluator[:start]+evaluator[start:end].replace('Ok(false)','Ok(true)')+evaluator[end:])
        name='withdrawal_temporal_pilot_defect';example=EXAMPLES_DIR/(name+'.rs')
        if example.exists():raise RuntimeError('refusing existing defect harness destination')
        try:
            harness=(EXAMPLES_DIR/'withdrawal_temporal_pilot.rs').read_text()
            example.write_text(harness.replace('#[path = "../src/model_diagnostics.rs"]','#[path = "'+str(weak)+'"]'))
            cargo(['build','-p','zeno-fcis-formal-tools','--example',name],env)
            check('weak-next','test_strong_endpoint',work,True,{'ZAL_WITHDRAWAL_PILOT_CLI':str(target(env,'debug',name))},reason='!= ')
        finally:
            example.unlink()
        # 3. A k one too small.
        path=work/'examples/temporal/alarm_withdrawal_bounded.json';data=json.loads(path.read_text())
        data['formula']=data['formula'].replace('within(7,','within(6,')
        data['readings'][0]['body']=data['formula']
        data['definitions'][0]['body']='next('*6+'Tick()'+')'*6;data['definitions'][0]['clause']='at least six further keeper ticks are recorded'
        path.write_text(json.dumps(data,indent=2,ensure_ascii=False))
        check('k-one-too-small','test_bounded_claim_finite_checked',work,True,reason='Tuples differ: (')
        shutil.copyfile(w.EXAMPLES/'alarm_withdrawal_bounded.json',path)
        # 4. An adapter that drops completions (lane B's payouts).
        anchor='paid={lane for lane,_ in payouts(decision)}'
        if source.count(anchor)!=1:raise RuntimeError('completion anchor drift')
        (work/'withdrawal_pilot.py').write_text(source.replace(anchor,'paid={lane for lane,_ in payouts(decision) if lane!=171}'))
        check('drops-completions','test_adapter_obligations',work,True,reason='adapter:completion')
    if any(hashlib.sha256(p.read_bytes()).hexdigest()!=digest for p,digest in before.items()):
        raise RuntimeError('source changed during isolated defect controls')
    print('PASS: four named planted defects%s; live source unchanged'%(' with Lean controls' if lean else ' (Lean controls not run)'))


def main():
    parser=argparse.ArgumentParser(description=__doc__,formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('command',choices=('native-build','run','planted','bind-source'))
    parser.add_argument('--length',type=int,default=w.HORIZON)
    parser.add_argument('--lean',type=Path);parser.add_argument('--lean-root',type=Path)
    parser.add_argument('--receipt',type=Path);parser.add_argument('--logs',type=Path)
    args=parser.parse_args();env=environment()
    if args.command=='bind-source':bind_source();return 0
    if args.lean and not args.lean_root:parser.error('--lean needs --lean-root')
    if args.command=='native-build':native_build(env);return 0
    if args.command=='run':return run(env,args.length,args.lean,args.lean_root,args.receipt)
    def challenge(logs):
        graph=subprocess.run([str(target(env,'debug','withdrawal_queue_decisions'))],capture_output=True,text=True,check=True).stdout
        logs.mkdir(parents=True,exist_ok=True);(logs/'graph.json').write_text(graph)
        planted(env,logs,args.lean)
    if args.logs is not None:challenge(args.logs)
    else:
        with tempfile.TemporaryDirectory(prefix='zal-withdrawal-logs-') as logs:challenge(Path(logs))
    return 0


if __name__=='__main__':raise SystemExit(main())
