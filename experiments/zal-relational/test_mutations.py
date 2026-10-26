"""Compile deliberately wrong implementations; count only semantic failures."""
import json
import argparse
import shutil
import subprocess
from pathlib import Path
import build
import run
from test_backend import atom, count, mod, quant, unary, query, request
import oracle

ROOT=Path(__file__).resolve().parent

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--work-dir',type=Path)
    args=parser.parse_args()
    work=args.work_dir or ROOT/('.build/mutation-source-'+run.binding()[1]['binary_sha256'][:12])
    if work.exists():raise SystemExit('mutation workspace exists; preserve its evidence')
    work.mkdir()
    shutil.copytree(ROOT/'src',work/'src');shutil.copytree(ROOT/'app',work/'app')
    originals={str(p.relative_to(work)):p.read_text() for p in work.rglob('*.hs')}
    p='src/Zal/Logic/Population.hs';s='src/Zal/Logic/Syntax.hs';b='src/Zal/Logic/Budget.hs'
    r=atom('R','x','y')
    mutations=[
      ('always-zero',p,'countNormal lim (normalize checked)','runBudget lim (pure (0 * fromInteger (domainSize (checkedQuery checked))))',query({'op':'true'},2)),
      ('missing-reverse-pair',p,'&& all (eval (not . standard)) (universal normal)','',query(quant('forall','x',quant('forall','y',{'op':'implies','left':r,'right':atom('R','y','x')})),2)),
      ('missing-diagonal',p,'diagonal=map (bit . eval . rowBody) (rows normal)','diagonal=map (\\row -> 0 * bit (eval (rowBody row))) (rows normal)',query(quant('forall','x',count('y',0,r)),1)),
      ('lost-multiplicity',b,'total <- sumMass mass (M.findWithDefault 0 key table)','total <- sumMass mass 0',query({'op':'true'},2)),
      ('strict-negated-count',p,'observationFor (rowObligation r==Required) (rowTest r)','observationFor True (rowTest r)',query(quant('forall','x',unary('not',count('y',1,r))),2)),
      ('cap-instead-of-wrap',s,'Just (n `mod` m)','Just (min (m-1) n)',query(quant('forall','x',mod('y',0,2,r)),2))]
    reports=[]
    out=ROOT/'evidence/mutations';out.mkdir(exist_ok=True)
    for name,path,old,new,q in mutations:
        for file,contents in originals.items():(work/file).write_text(contents)
        source=originals[path]
        if source.count(old)!=1:raise AssertionError(('mutation anchor not unique',name))
        changed=source.replace(old,new)
        (work/path).write_text(changed)
        (out/(name+'.hs.txt')).write_text(changed)
        command=['ghc','-O1','-Wall','-Werror','-Wincomplete-patterns','-rtsopts','-i'+str(work/'src'),
                 '-package','aeson','-outputdir',str(work/'objects'),'-o',str(work/'worker'),str(work/'app/Main.hs'),'+RTS','-M768m','-RTS']
        compiled=subprocess.run(command,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=120,preexec_fn=build.limits)
        (out/(name+'-build.log')).write_bytes(compiled.stdout)
        if compiled.returncode:raise AssertionError(('compiler failure is not a semantic kill',name,compiled.stdout.decode()))
        tested=subprocess.run([str(work/'worker'),'+RTS','-M1536m','-RTS'],input=run.canonical(request(q))+b'\n',
                              stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=60,preexec_fn=run.child_limits(60))
        expected=oracle.count(q)
        if tested.returncode:raise AssertionError(('crash is not semantic kill',name,tested.stderr.decode()))
        actual=json.loads(tested.stdout)
        if actual.get('status')!='ExactCount' or actual.get('value')==expected:
            raise AssertionError(('wrong-count control not distinguished',name,expected,actual))
        reports.append({'mutation':name,'source':path,'source_sha256':run.sha(changed.encode()),
                        'binary_sha256':build.sha(work/'worker'),'query':q,'expected':expected,'actual':actual,
                        'compiler_exit':compiled.returncode,'worker_exit':tested.returncode})
        print(json.dumps({'mutation':name,'expected':expected,'wrong':actual['value']}),flush=True)
    record={'baseline':run.binding()[1]['source_sha256'],'controls':reports,'killed_by_wrong_count':len(reports),
            'script_sha256':build.sha(Path(__file__))}
    (ROOT/'evidence/mutations.json').write_text(json.dumps(record,indent=2)+'\n')

if __name__=='__main__':main()
