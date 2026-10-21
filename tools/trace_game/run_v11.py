#!/usr/bin/env python3
"""Build, retain and independently check offline finite trace-game certificates."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import resource
import shutil
import subprocess
import sys
import time

ROOT=Path(__file__).resolve().parents[2]
TABLES=('lawful','guard-true','guard-false','lawful-composition')
CERTIFICATES=('withdrawal-safety','composition-safety','withdrawal-guard','quantifier-no-guard',
              'oracle-unrealizability','withdrawal-strategy','quantifier-strategy')


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def sources():
    files=subprocess.check_output(['git','ls-files','-z','--cached','--others','--exclude-standard'],cwd=ROOT).split(b'\0')
    result={p.decode():sha(ROOT/p.decode()) for p in files if p and (ROOT/p.decode()).is_file()}
    result['Cargo.lock']=sha(ROOT/'Cargo.lock')
    return result


def write(path,value):
    path.write_text(json.dumps(value,indent=2,sort_keys=True)+'\n')


def measure(argv,out,label,env,cap=1024,timeout=180,expected=0):
    def limits():
        resource.setrlimit(resource.RLIMIT_AS,(cap*1024**2,cap*1024**2))
    start=time.monotonic(); log=out/f'{label}.log'; rss=out/f'{label}.rss'
    with log.open('wb') as stream:
        try:
            code=subprocess.run(['/usr/bin/time','-f','%M','-o',str(rss),*map(str,argv)],cwd=ROOT,
                                env=env,preexec_fn=limits,stdout=stream,stderr=subprocess.STDOUT,
                                timeout=timeout,check=False).returncode
        except subprocess.TimeoutExpired:
            code=124
    lines=rss.read_text().splitlines() if rss.exists() else []
    record=dict(label=label,argv=list(map(str,argv)),exit_code=code,cap_mib=cap,
                peak_rss_kib=int(lines[-1]) if lines and lines[-1].isdigit() else None,
                seconds=round(time.monotonic()-start,3))
    with (out/'commands.jsonl').open('a') as stream: stream.write(json.dumps(record)+'\n')
    print(label,code,record['peak_rss_kib'],flush=True)
    if code!=expected: raise ValueError(f'{label}: exit {code}; inspect {log}')
    return record


def worker(out):
    # The solver may call references; the standalone acceptance process imports none.
    from certificate_check import validate_tables
    from product import description, build
    from solve import certify, fixed_guard, unrealizable, reactive
    tables=validate_tables({n:json.loads((out/'tables'/f'{n}.json').read_text()) for n in TABLES})
    packets=(certify(description('withdrawal'),tables),certify(description('composition'),tables),
             fixed_guard(description('withdrawal'),tables),fixed_guard(description('quantifier'),{}),
             unrealizable(description('oracle-withholding'),{}),reactive(description('withdrawal'),tables),
             reactive(description('quantifier'),{}))
    for name,packet in zip(CERTIFICATES,packets):
        if 'schema' not in packet: raise ValueError(f'{name}: {packet}')
        write(out/f'{name}.json',packet)
    stops={}
    for name,budget in (('state',dict(max_states=6)),('edge',dict(max_edges=1)),('time',dict(seconds=0))):
        graph=build(description('withdrawal'),tables,**budget)
        if graph.status!='UNKNOWN': raise ValueError('budget did not stop')
        stops[name]=graph.wire()
    empty=build(description('withdrawal',empty=True),tables)
    if empty.status!='UNKNOWN': raise ValueError('empty environment did not stop')
    stops['empty']=empty.wire(); write(out/'unknown-runs.json',stops)


def main():
    from guide_help import example_help
    parser=argparse.ArgumentParser(description=__doc__, epilog=example_help("trace-game"), formatter_class=argparse.RawDescriptionHelpFormatter); parser.add_argument('--out',type=Path,required=True)
    parser.add_argument('--certificates-only', action='store_true', help='Build and check native fixture certificates and tests; omit repository-wide documentation/style checks, not release qualification')
    parser.add_argument('--worker',action='store_true'); args=parser.parse_args(); out=args.out.resolve()
    if args.worker: worker(out); return
    subprocess.run(['df','-B1G','/'],check=True)
    if shutil.disk_usage('/').free<6*1024**3: parser.error('STOP: below 6 GiB free')
    if out.is_relative_to(ROOT): parser.error('retain evidence outside the source tree')
    if out.exists() and any(out.iterdir()): parser.error('retain previous attempts; choose a new directory')
    if subprocess.check_output(['git','status','--porcelain'],cwd=ROOT): parser.error('commit final source before binding evidence')
    out.mkdir(parents=True,exist_ok=True)
    scratch=ROOT.parent
    target=Path(os.environ.get('CARGO_TARGET_DIR', scratch/'tmp/build')).resolve()
    temporary=Path(os.environ.get('TMPDIR', scratch/'tmp')).resolve(); temporary.mkdir(parents=True,exist_ok=True)
    env=dict(os.environ,CARGO_TARGET_DIR=str(target),TMPDIR=str(temporary),CARGO_PROFILE_DEV_DEBUG='0',
             CARGO_PROFILE_TEST_DEBUG='0',CARGO_PROFILE_DEV_OPT_LEVEL='1',CARGO_PROFILE_TEST_OPT_LEVEL='1',
             CARGO_BUILD_JOBS='2',CARGO_NET_OFFLINE='true',CARGO_TERM_COLOR='never',
             PYTHONDONTWRITEBYTECODE='1',PYTHONPATH=str(ROOT/'tools/trace_game'))
    before=sources(); revision=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
    measure(['cargo','+1.97.1','build','--locked','--offline','-p','zeno-fcis-cli','--example','trace_game','-j','2'],
            out,'build',env,cap=3072,timeout=600)
    binary=target/'debug/examples/trace_game'
    measure([binary,out/'tables','--tables-only'],out,'native-tables',env)
    measure([sys.executable,'-B',__file__,'--worker','--out',out],out,'solve',env)
    if sources()!=before: raise ValueError('source drift during build/solve')
    artifacts={str(p.relative_to(out)):sha(p) for p in out.rglob('*.json')}
    write(out/'binding.json',dict(schema='zeno-fcis/trace-game-binding/1.1',revision=revision,sources=before,
                                 binary=dict(path=str(binary),sha256=sha(binary)),artifacts=artifacts,
                                 profile=dict(toolchain='1.97.1',locked=True,offline=True,debug=0,jobs=2,repository_checks='not-run' if args.certificates_only else 'required')))
    results={}
    for name in CERTIFICATES:
        measure([sys.executable,'-B','tools/trace_game/certificate_check.py',out,f'{name}.json'],out,f'check-{name}',env)
        results[name]=json.loads((out/f'check-{name}.log').read_text())
    tests=['test_product','test_certificate','test_guards','test_unrealizable','test_strategy','test_admission',
           'test_check.Check','test_check.Standalone.test_standalone_refuses_specimen']
    measure([sys.executable,'-B','-m','unittest','-v',*tests],out,'tests',env)
    measure(['cargo','+1.97.1','test','--locked','--offline','-p','zeno-fcis-cli','--example','trace_game','-j','2'],
            out,'rust-tests',env,cap=3072,timeout=600)
    if not args.certificates_only:
        for tool in ('check_public_docs.py','check_library_docs.py'):
            measure([sys.executable,'-B',f'tools/{tool}'],out,tool[:-3],env)
        measure([sys.executable,'-B','tools/check_release_privacy.py','check','docs/TRACE_GAME.md','tools/trace_game',
                 'crates/zeno-fcis-cli/examples/trace_game.rs'],out,'privacy',env)
        measure(['cargo','+1.97.1','fmt','--check','-p','zeno-fcis-cli'],out,'format',env)
        measure(['git','diff','--check'],out,'diff-check',env)
    if sources()!=before: raise ValueError('source drift during final checks')
    write(out/'results.json',results)
    print(json.dumps(results,indent=2))


if __name__ == "__main__":
    import sys
    from pathlib import Path
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
    try:
        from telemetry import run_cli
    except ImportError:
        # Reporting is optional, including for a standalone copy of this tool.
        run_cli = lambda function: function()
    def _entrypoint():
        main()
    run_cli(_entrypoint)
