#!/usr/bin/env python3
"""Verify the production reachable probe, shared fixed points and semantic controls."""
from __future__ import annotations
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

import check_lemma_kernel as lemma
import check_verus as verifier
from verus_coverage import inventory

ROOT = Path(__file__).resolve().parents[1]
HARNESS = Path('verification/verus/probe_kernel.rs')
SUBJECT = Path('crates/zeno-fcis-cli/src/contract/review/probe_kernel.rs')
SEARCH = lemma.SUBJECT
PROFILE = Path('verification/verus/reachable-probe.json')
TESTS = Path('verification/verus/probe_kernel_tests.rs')
SPEC = lemma.SPEC
PROOFS = Path('verification/verus/finite_machine_proofs.rs')
VERDICTS = Path('verification/verus/lemma_fixed_point.rs')
UNIT = (HARNESS, SUBJECT, SEARCH, SPEC, PROOFS, VERDICTS, TESTS)
NAMESPACE = 'probe_kernel::'
SOURCES = tuple(dict.fromkeys((*UNIT, PROFILE, lemma.PROFILE, verifier.PIN,
    Path('tools/check_reachable_probe.py'), Path('tools/test_check_reachable_probe.py'),
    Path('tools/check_lemma_kernel.py'), Path('tools/check_verus.py'), Path('tools/verus_coverage.py'),
    *verifier.QUALIFICATION_SOURCES,
    SUBJECT.with_name('probe.rs'), SUBJECT.with_name('lemma.rs'), SUBJECT.with_name('machine.rs'),
    SUBJECT.parents[1] / 'review.rs', Path('crates/zeno-fcis-cli/src/judge_command.rs'),
    Path('Cargo.toml'), Path('Cargo.lock'), Path('rust-toolchain.toml'), Path('.github/workflows/verus.yml'))))
CONTROLS = {
    'noncommitting_target': (SUBJECT,
        'let search = search_kernel::reachable(n, k, g, after, commits);',
        'let search = search_kernel::reachable(n, k, g, after, &alloc::vec![true; after.len()]);',
        'decide_reachable', 'noncommitting_target_is_unreachable'),
    'early_frontier': (SEARCH, 'while at < queue.len() {',
        'while at < queue.len() && at != queue.len() - 1 {',
        'reachable', 'last_frontier_target_is_reachable'),
}


def snapshot() -> dict[str, str]:
    result = {}
    for p in SOURCES:
        if not (ROOT/p).is_file() or (ROOT/p).is_symlink():
            raise RuntimeError(f'missing or symbolic-link input: {p}')
        result[str(p)] = verifier.digest(ROOT/p)
    return result


def configuration(root: Path, profile: dict):
    for path in (SUBJECT, SEARCH, SPEC, PROOFS, VERDICTS):
        source = (root/path).read_text()
        if lemma.FORBIDDEN.search(re.sub(r'//[^\n]*', '', source)):
            raise RuntimeError(f'untrusted proof escape in {path}')
        verifier.check_native_configuration(source, path,
            profile['native_configuration_exceptions'] if path == SEARCH else ())


def check_inventory(vir: str, profile: dict) -> dict:
    if profile.get('namespace') != NAMESPACE:
        raise ValueError('unexpected probe namespace')
    actual = inventory(vir, NAMESPACE, tuple(profile['body_covered_functions']))
    if actual != profile['functions']:
        changed = sorted(n for n in set(actual) | set(profile['functions'])
            if actual.get(n) != profile['functions'].get(n))
        raise ValueError(f'probe translated inventory/contracts changed: {changed}')
    for name in ('within_limit', 'decide_reachable'):
        record = actual[NAMESPACE+'probe_kernel::'+name]
        if record['requires'] or not record['ensures']:
            raise ValueError(f'unconditional probe boundary lost: {name}')
    return actual


def source_controls(root: Path):
    result = {}
    for name, (path, anchor, replacement, function, test) in CONTROLS.items():
        source = (root/path).read_text()
        if source.count(anchor) != 1:
            raise RuntimeError(f'control anchor is not unique: {name}')
        result[name] = (path, source.replace(anchor, replacement), function, test)
    return result


def native_command(pin, root, binary):
    return ['rustc', f"+{pin['runtime_rust']}", '--edition=2024', '--test',
        '--check-cfg', 'cfg(verus_keep_ghost)', '--check-cfg', 'cfg(test)',
        str(root/HARNESS), '-o', str(binary)]


def check(cache: Path, install: bool, directory: Path) -> dict:
    before = snapshot()
    pin = json.loads((ROOT/verifier.PIN).read_text())
    profile = json.loads((ROOT/PROFILE).read_text())
    configuration(ROOT, profile)
    tools = verifier.prepare_tools(cache, pin, install)
    env = {k:v for k,v in os.environ.items() if not k.startswith(('VERUS_', 'VARGO_'))
        and k not in ('RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS')}
    env.update(RUSTUP_TOOLCHAIN=pin['rust_toolchain'], VERUS_Z3_PATH=str(tools/'z3'), RUST_BACKTRACE='0')
    command = [str(tools/'verus'), '--crate-type=lib', '--edition=2024', '--no-cheating',
        '--no-external-by-default', '--num-threads', '2', '--output-json',
        '--log', 'vir', '--log', 'vir-option=no_span+no_type+no_fn_details']
    def prove(root, prefix):
        p = lemma.run([*command, '--log-dir', str(directory/(prefix+'-vir')), str(root/HARNESS)],
            ROOT, env, directory/prefix)
        return p, json.loads(p.stdout)
    positive, report = prove(ROOT, 'positive')
    if not lemma.positive_accepted(positive, report, pin, profile):
        raise RuntimeError('probe lacks pinned whole-crate proof/count admission')
    coverage = check_inventory((directory/'positive-vir/crate.vir').read_text(),profile)
    binary = directory/'native-tests'
    verifier.require_success(lemma.run(native_command(pin,ROOT,binary), ROOT,env,directory/'native-build'))
    native = lemma.run([str(binary),'--nocapture'],ROOT,env,directory/'native-tests')
    verifier.require_success(native)
    controls = []
    for name,(path,changed,function,test) in source_controls(ROOT).items():
        specimen = directory/name
        for source in UNIT:
            target=specimen/source;target.parent.mkdir(parents=True,exist_ok=True)
            target.write_bytes((ROOT/source).read_bytes())
        target = specimen/path
        original = target.read_bytes()
        executable = specimen/'native-tests'
        try:
            target.write_text(changed)
            (specimen/'mutated-source.rs.txt').write_text(changed)
            configuration(specimen,profile)
            build = lemma.run(native_command(pin,specimen,executable),ROOT,env,specimen/'native-build')
            verifier.require_success(build)
            assertion = lemma.run([str(executable),'--exact','tests::'+test,'--nocapture'],ROOT,env,specimen/'native-control')
            if not lemma.native_failed(assertion,test):
                raise RuntimeError(f'control lacks named native assertion: {name}')
            proof, mutant = prove(specimen,name+'-mutated')
            if not verifier.semantic_refusal(proof,mutant,pin,path,specimen,
                    region=lemma.function_region(changed,function),target_functions=profile['target_functions']):
                raise RuntimeError(f'control lacks intended semantic obligation: {name}')
            headers = list(verifier.ERROR_HEADER.finditer(proof.stderr)); obligations=[]
            for i,header in enumerate(headers):
                if header[1].startswith('aborting due to '):continue
                end=headers[i+1].start() if i+1<len(headers) else len(proof.stderr)
                primary=re.search(r'^\s*-->\s+(.+):(\d+):(\d+)\s*$',proof.stderr[header.end():end],re.M)
                line=int(primary[2])
                obligations.append({'kind':header[1],'line':line,'source':changed.splitlines()[line-1].strip()})
            item={'name':name,'subject':str(path),'function':function,'native_test':test,
                'native_build_exit':build.returncode,'native_assertion_exit':assertion.returncode,
                'verus_exit':proof.returncode,'obligations':obligations,
                'normal_sha256':before[str(path)],'mutant_sha256':verifier.digest(target)}
        finally:
            target.write_bytes(original)
        if verifier.digest(target)!=before[str(path)]:
            raise RuntimeError(f'control source restoration failed: {name}')
        configuration(specimen,profile)
        verifier.require_success(lemma.run(native_command(pin,specimen,executable),ROOT,env,specimen/'restored-build'))
        verifier.require_success(lemma.run([str(executable),'--exact','tests::'+test,'--nocapture'],ROOT,env,specimen/'restored-native'))
        restored, restored_report = prove(specimen,name+'-restored')
        if not lemma.positive_accepted(restored,restored_report,pin,profile):
            raise RuntimeError(f'restored control does not verify: {name}')
        check_inventory((directory/(name+'-restored-vir/crate.vir')).read_text(),profile)
        item.update(restored_sha256=verifier.digest(target),restored_build_exit=0,
            restored_assertion_exit=0,restored_verus_exit=0,restored_verified=restored_report['verification-results']['verified'])
        controls.append(item)
        print(f'probe: {name}: native assertion and intended obligation rejected; restored checks passed',flush=True)
        (directory/'controls.json').write_text(json.dumps(controls,indent=2)+'\n')
    verifier.verify_tool_files(tools,pin)
    if snapshot()!=before:raise RuntimeError('probe source changed during gate')
    return {'schema':'zeno-fcis/reachable-probe-evidence/1','status':'passed',
        'revision':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
        'working_tree_dirty':bool(subprocess.check_output(['git','status','--porcelain'],cwd=ROOT,text=True).strip()),
        'source_sha256':before,'toolchain':pin,'verus_report':report,'translated_inventory':coverage,
        'native_exit':native.returncode,'controls':controls,
        'scope':'complete finite-table predicate reachability, effective witness traces, tuple-bound refusal and Park verdict forms',
        'trusted_base':['pinned Verus/vstd/Z3','Rust/erasure/macro correspondence','library contracts','table/predicate abstraction','source-admission tooling','allocator/host'],
        'unproved':['whole-CLI/JSON adapter theorem','predicate evaluator','domain enumeration','shortest-trace minimality','physical time/OOM','symbolic/unsigned routes']}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cache',type=Path,default=verifier.cache_directory())
    parser.add_argument('--install',action='store_true')
    parser.add_argument('--out',type=Path,required=True)
    args=parser.parse_args();args.out.parent.mkdir(parents=True,exist_ok=True)
    directory=Path(tempfile.mkdtemp(prefix='reachable-probe-',dir=args.out.parent)).resolve()
    try:receipt=check(args.cache.resolve(),args.install,directory)
    except (OSError,ValueError,KeyError,RuntimeError,subprocess.SubprocessError) as error:
        receipt={'schema':'zeno-fcis/reachable-probe-evidence/1','status':'failed','error':str(error)}
    receipt['logs']=str(directory);args.out.write_text(json.dumps(receipt,indent=2,sort_keys=True)+'\n')
    print(f"reachable probe: {receipt['status']}; receipt: {args.out}")
    if receipt['status']!='passed':print(receipt['error']);return 1
    print(f"{receipt['verus_report']['verification-results']['verified']} obligations; {len(receipt['controls'])} controls")
    return 0

if __name__=='__main__':raise SystemExit(main())
