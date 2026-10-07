#!/usr/bin/env python3
"""Offline actual publication proof, original-wire oracles and custody controls.

Invoke under the shared heavy-check.lock. Coverage expectations are reviewed
source, never refreshed by this gate. No evidence here authorizes a shell commit.
"""
from __future__ import annotations
import argparse
import collections
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
sys.dont_write_bytecode = True
import check_authority_v2 as authority
import check_original_output_v2 as output
from v2_proof_sources import execution_sources, adjust_specimen_source_lengths
from v2_native_dependencies import native_dependency_args
import check_verus as verifier
from verus_coverage import inventory, require_coverage

ROOT = Path(__file__).resolve().parents[1]
HARNESS = Path('verification/verus/publication_v2.rs')
PROFILE = Path('verification/verus/publication-v2.json')
SUBJECT = authority.BOUND.parent / 'publication.rs'
SPEC = SUBJECT.parent / 'publication/spec.rs'
PUBLIC_TEST = Path('crates/zeno-fcis-synthesis/tests/v2_publication.rs')
STAGE = Path('docs/V2_PUBLICATION_STAGE.md')
TARGET = output.TARGET
UNIT_SOURCES = tuple(sorted({HARNESS, PUBLIC_TEST, *map(Path,
    json.loads((ROOT / authority.SOURCE_MANIFEST).read_text())['paths']),
    *(p.relative_to(ROOT) for folder in ['canonical_v2', 'evaluation', 'execution_v2']
      for p in (ROOT / authority.BASE / folder).rglob('*.rs')),
    Path('crates/zeno-fcis-cli/templates/order-fulfillment/synthesized/transition.rs')}))

UNIT_SOURCES = tuple(dict.fromkeys((*UNIT_SOURCES, *execution_sources(ROOT))))
SOURCES = tuple(sorted({*UNIT_SOURCES, *output.ORACLE_SOURCES, *output.NATIVE_SOURCES, authority.PUBLIC_TEST, authority.HARNESS,
    PROFILE, STAGE, authority.SOURCE_MANIFEST, verifier.PIN,
    Path('tools/check_publication_v2.py'), Path('tools/test_check_publication_v2.py'),
    Path('tools/check_authority_v2.py'), Path('tools/check_original_output_v2.py'),
    Path('tools/check_decision_v2.py'), Path('tools/check_metered_execution.py'),
    Path('tools/check_finite_execution.py'), Path('tools/check_verus.py'), Path('tools/verus_coverage.py'),
    Path('Cargo.toml'), Path('Cargo.lock'), Path('rust-toolchain.toml'),
    Path('crates/zeno-fcis-synthesis/Cargo.toml')}))

SOURCES = tuple(dict.fromkeys((*SOURCES, Path("tools/v2_proof_sources.py"),
                                Path("tools/v2_native_dependencies.py"),
                                Path("verification/verus/authority_v2_sources.json"))))
once = output.once


def run(command: list[str], cwd: Path, env: dict, directory: Path, name: str,
        timeout: int = 600) -> subprocess.CompletedProcess:
    result = output.run(command,cwd,env,directory,name,timeout)
    context_path=directory/(name+'.context.json')
    context=json.loads(context_path.read_text())
    if 'RUSTFLAGS' in env:
        context['environment']['RUSTFLAGS']=env['RUSTFLAGS']
        context['removed_environment'].remove('RUSTFLAGS')
    context_path.write_text(json.dumps(context,indent=2,sort_keys=True)+'\n')
    return result


def snapshot() -> dict[str, str]:
    if any((ROOT / p).is_symlink() for p in SOURCES):
        raise ValueError('symlink in complete publication source closure')
    return {str(p):verifier.digest(ROOT / p) for p in SOURCES}


def mutations() -> dict[str, tuple[Path, str, str]]:
    source = (ROOT / SUBJECT).read_text()
    spec = (ROOT / SPEC).read_text()
    rows = {}
    for name, old, new, kind in [
        ('state_root', 'output::encode_envelope(binding.root,', 'output::encode_envelope(0,', 'native'),
        ('state_schema', 'output::encode_envelope(binding.root,&binding.schema,', 'output::encode_envelope(binding.root,&[0;32],', 'native'),
        ('state_exact_cap', 'output::encode_envelope(binding.root,&binding.schema,&payload,cap)', 'output::encode_envelope(binding.root,&binding.schema,&payload,cap.saturating_sub(1))', 'native'),
        ('channel_roots', 'channel:d.channel,destination_root,payload_root,', 'channel:d.channel,destination_root:payload_root,payload_root:destination_root,', 'native'),
        ('delivery_payload', 'output::encode_record(&d.payload,usize::MAX)', 'output::encode_record(&[],usize::MAX)', 'native'),
        ('delivery_idempotency', 'output::encode_atom(d.idempotency,usize::MAX)', 'output::encode_atom(d.destination,usize::MAX)', 'native'),
        ('delivery_order', 'delivery_bytes(&ds[i],links)', 'delivery_bytes(&ds[0],links)', 'native'),
        ('outbox_lane', 'delivery_list(candidate.outbox(),links)', 'delivery_list(candidate.effects(),links)', 'native'),
        ('subject_idempotency', 'parts.push(Part::Bytes(&d.idempotency))', 'parts.push(Part::Bytes(&[]))', 'native'),
        ('full_replay_comparison', 'if !canonical::exact(&artifacts.subject,wanted)', 'if false && !canonical::exact(&artifacts.subject,wanted)', 'native'),
        ('reject_has_no_capability', 'if matches!(candidate.class(),Class::Reject)', 'if false && matches!(candidate.class(),Class::Reject)', 'native'),
        ('reject_replay_comparison', 'if !canonical::exact(bytes,wanted)', 'if false && !canonical::exact(bytes,wanted)', 'native'),
        ('missing_channel_refusal', 'None=>return Err(Refusal::Channel)', 'None=>(0,0)', 'proof'),
        ('encoding_refusal_class', 'Err(e)=>return Err(Refusal::Output(e)),};\n    match output::encode_envelope', 'Err(_e)=>return Err(Refusal::Encoding),};\n    match output::encode_envelope', 'proof'),
    ]:
        rows[name] = (SUBJECT, once(source, old, new), kind)
    rows['weak_getter_contract'] = (SUBJECT, once(source,
        '#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==self.view().0,))]\n    pub fn ordinal',
        '#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures true,))]\n    pub fn ordinal'), 'coverage')
    rows['equivalent_link_spec'] = (SPEC, once(spec, 'if n==0{None}else{match link',
        'if n==0{if id==0{None}else{None}}else{match link'), 'coverage')
    rows['uncontracted_inventory'] = (SUBJECT, source + '\npub fn uncontracted_publication_probe(n:u64)->u64{n}\n', 'coverage')
    return rows


def coverage_control(vir: str, profile: dict, name: str) -> tuple[bool, str | None, dict]:
    refusal = None
    try:
        require_coverage(vir, profile)
    except ValueError as error:
        refusal = str(error)
    actual = inventory(vir, profile['namespace'], tuple(profile['body_covered_functions']))
    expected = profile['functions']
    prefix = 'publication_v2::execution_v2::authority::publication::'
    if name == 'uncontracted_inventory':
        target = prefix + 'uncontracted_publication_probe'
        field = 'inventory'
        intended = target in actual and target not in expected and target in (refusal or '')
    else:
        suffix,field = {'weak_getter_contract':('::ordinal','ensures_sha256'),
                        'equivalent_link_spec':('spec::link','body_sha256')}[name]
        names = [n for n in expected if n.startswith(prefix) and n.endswith(suffix)]
        if len(names) != 1:
            raise ValueError('ambiguous intended coverage target: ' + name)
        target = names[0]
        intended = target in actual and target in (refusal or '') and actual[target][field] != expected[target][field]
    return bool(intended and refusal),refusal,{'function':target,'field':field,'intended_refusal':bool(intended)}


def native_command(source: Path, binary: Path, libraries: dict) -> list[str]:
    command = ['rustc', '+1.97.1', '--edition=2024', '--test', '--check-cfg', 'cfg(verus_keep_ghost)',
               '--check-cfg', 'cfg(test)', '-L', 'dependency=' + str(TARGET / 'debug/deps')]
    for name,path in sorted(libraries.items()):
        command += ['--extern',name + '=' + path]
    return [*command,str(source / HARNESS),'-o',str(binary)]


def custody(directory: Path, source: Path, env: dict) -> list[dict]:
    lib = directory / 'libpublication_v2.rlib'
    verifier.require_success(run(['rustc','+1.97.1','--edition=2024','--crate-type=rlib',
        '--check-cfg','cfg(verus_keep_ghost)','--check-cfg','cfg(test)',str(source / HARNESS),'-o',str(lib)],source,env,directory,'library'))
    prefix = 'extern crate publication_v2 as api;\nuse api::execution_v2::{authority as a,composition as c};\n'
    cases = [
        ('usable_client',"pub fn run<'a>(authority:&'a a::Authority<'_>,raw:c::Raw<'a>)->a::PublicationOutcome<'a>{authority.publish(raw)}\npub fn inspect(p:&a::Publication<'_>)->usize{p.identity().len()+p.poststate().len()+p.subject().len()+p.effects().len()+p.outbox().len()}\npub fn genesis<'a>(authority:&'a a::Authority<'_>,raw:&'a[u8])->a::PublicationOutcome<'a>{authority.publish_genesis(raw)}",'',()),
        ('forge_publication',"pub fn forge<'a>(evaluation:a::Evaluation<'a>,identity:&'a[u8])->a::Publication<'a>{a::Publication{evaluation,identity,artifacts:panic!()}}",'E0451',('private',)),
        ('forge_genesis_publication',"pub fn forge<'a>(evaluation:a::Evaluation<'a>,identity:&'a[u8])->a::Publication<'a>{a::Publication{evaluation,identity,artifacts:panic!()}}",'E0451',('private',)),
        ('forge_delivery',"pub fn forge()->a::WireDelivery{a::WireDelivery{ordinal:0,channel:0,destination_root:0,payload_root:0,destination:vec![],payload:vec![],idempotency:vec![]}}",'E0451',('private',)),
        ('replace_identity',"pub fn change(p:&mut a::Publication<'_>){p.identity=&[];}",'E0616',('identity','private')),
        ('replace_evaluation',"pub fn change<'a>(p:&mut a::Publication<'a>,e:a::Evaluation<'a>){p.evaluation=e;}",'E0616',('evaluation','private')),
        ('replace_poststate',"pub fn change(p:&mut a::Publication<'_>){p.poststate()[0]=0;}",'E0594',('cannot assign',)),
        ('replace_delivery',"pub fn change(p:&mut a::Publication<'_>){p.effects()[0].payload().get_mut(0);}",'E0596',('cannot borrow',)),
        ('clone_capability',"pub fn clone_it<'a>(p:&a::Publication<'a>)->a::Publication<'a>{p.clone()}",'E0308',('mismatched types',)),
        ('clone_genesis_capability',"pub fn clone_it<'a>(p:&a::Publication<'a>)->a::Publication<'a>{p.clone()}",'E0308',('mismatched types',)),
        ('supplied_candidate',"pub fn forge<'a>(authority:&'a a::Authority<'_>,raw:c::Raw<'a>,candidate:&c::Candidate<'a>)->a::PublicationOutcome<'a>{authority.publish(raw,candidate)}",'E0061',('takes 1 argument',)),
        ('supplied_callback',"pub fn forge<'a>(authority:&'a a::Authority<'_>,raw:c::Raw<'a>)->a::PublicationOutcome<'a>{authority.publish(raw,||vec![])}",'E0061',('takes 1 argument',)),
        ('private_finish',"pub fn forge<'a>(e:a::Evaluation<'a>,id:&'a[u8],frame:&c::FrameBinding)->a::PublicationOutcome<'a>{a::publication::finish(e,id,frame,&[],None)}",'E0603',('publication','private')),
        ('private_evaluator_module',"pub fn read_digest(){let _=a::evaluator::EVALUATOR;}",'E0603',('evaluator','private')),
    ]
    results = []
    for name,body,code,terms in cases:
        path = directory / (name + '.rs')
        path.write_text(prefix + body + '\n')
        command = ['rustc','+1.97.1','--edition=2024','--crate-type=rlib','--error-format=json',
                   '--extern','publication_v2=' + str(lib),str(path),'-o',str(directory / (name + '.rlib'))]
        proc = run(command,source,env,directory,name)
        diagnostics = [json.loads(line) for line in proc.stderr.splitlines() if line.startswith('{')]
        matched = [e for e in diagnostics if (e.get('code') or {}).get('code') == code and all(t in e.get('message','') for t in terms)]
        passed = proc.returncode == 0 if not code else proc.returncode != 0 and bool(matched)
        results.append({'name':name,'passed':passed,'exit_code':proc.returncode,'expected_code':code,
                        'source_sha256':verifier.digest(path),'matched_diagnostics':matched})
        (directory / 'receipt.json').write_text(json.dumps(results,indent=2,sort_keys=True)+'\n')
        if not passed:
            raise ValueError('custody control failed or had unrelated diagnostic: ' + name)
    return results


def check(directory: Path, positive_only: bool = False) -> dict:
    generation = authority.check_generated_sources()
    before = snapshot()
    source = directory / 'source'
    for p in SOURCES:
        target = source / p; target.parent.mkdir(parents=True,exist_ok=True)
        target.write_bytes((ROOT / p).read_bytes())
    (directory / 'source_sha256.json').write_text(json.dumps(before,indent=2,sort_keys=True)+'\n')
    pin = json.loads((ROOT / verifier.PIN).read_text())
    profile = json.loads((ROOT / PROFILE).read_text())
    tool = verifier.prepare_tools(Path.home() / '.cache/zeno-fcis/verus',pin,False)
    env = {k:v for k,v in os.environ.items() if not k.startswith(('VERUS_','VARGO_')) and k not in ('RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS')}
    env.update(RUSTUP_TOOLCHAIN=pin['rust_toolchain'],VERUS_Z3_PATH=str(tool/'z3'),
               CARGO_TARGET_DIR=str(TARGET),CARGO_BUILD_JOBS='1',RUST_TEST_THREADS='1',CARGO_NET_OFFLINE='true',CARGO_INCREMENTAL='0')
    command = [str(tool/'verus'),'--crate-type=lib','--edition=2024','--no-cheating','--no-external-by-default',
               '--num-threads','2','-V','spinoff-all','--output-json','--log','vir','--log','vir-option=no_span+no_type+no_fn_details']
    proof = run([*command,'--log-dir',str(directory/'positive/logs'),str(source/HARNESS)],source,env,directory/'positive','verus',900)
    verifier.require_success(proof)
    report = output.parsed_report(proof)
    if not verifier.accepted(report,{**pin,'expected_verified':profile['expected_verified'],'target_functions':profile['target_functions']}):
        raise ValueError('whole publication closure not proved by exact pinned toolchain')
    coverage = require_coverage((directory/'positive/logs/crate.vir').read_text(),profile)
    print('Publication: whole-closure proof and raw translated coverage passed',flush=True)
    native_dependency_args(ROOT, directory / 'native', env)
    dependency_receipt=json.loads((directory/'native/native-dependencies/result.json').read_text())
    libraries={name:record['path'] for name,record in dependency_receipt['libraries'].items()}
    verifier.require_success(run(native_command(source,directory/'native/tests',libraries),source,env,directory/'native','build'))
    native = run([str(directory/'native/tests'),'public_publication::','--test-threads=1'],source,env,directory/'native','tests')
    verifier.require_success(native)
    if '6 passed; 0 failed' not in native.stdout:
        raise ValueError('complete public native oracle corpus did not run')
    custody_results = custody(directory/'custody',source,env)
    no_std = run(['rustc','+1.97.1','--edition=2024','--crate-type=lib','--emit=metadata',
        '--check-cfg','cfg(verus_keep_ghost)','--check-cfg','cfg(test)',str(source/HARNESS),'-o',str(directory/'native/no-std.rmeta')],source,env,directory/'native','no-std')
    verifier.require_success(no_std)
    public = run(['cargo','+1.97.1','test','--locked','--offline','-vv','-p','zeno-fcis-synthesis','--test','v2_publication','--','--test-threads=1'],ROOT,env,directory/'native','public-test')
    verifier.require_success(public)
    if '6 passed; 0 failed' not in public.stdout:
        raise ValueError('registered Cargo publication corpus did not run')
    compatibility = run(['cargo','+1.97.1','test','--locked','--offline','-vv','-p','zeno-fcis-synthesis','--test','v2_authority','--','--test-threads=1'],ROOT,env,directory/'native','authority-compatibility')
    verifier.require_success(compatibility)
    cargo_no_std = run(['cargo','+1.97.1','check','--locked','--offline','-vv','-p','zeno-fcis-synthesis','--lib','--no-default-features'],ROOT,env,directory/'native','cargo-no-std')
    verifier.require_success(cargo_no_std)
    clippy = run(['cargo','+1.97.1','clippy','--locked','--offline','-vv','-p','zeno-fcis-synthesis','--test','v2_publication','--','-D','warnings'],ROOT,env,directory/'native','clippy')
    verifier.require_success(clippy)
    controls = []
    if not positive_only:
        for name,(path,changed,kind) in mutations().items():
            specimen = directory/'mutations'/name
            for p in UNIT_SOURCES:
                dest=specimen/p;dest.parent.mkdir(parents=True,exist_ok=True)
                dest.write_bytes(changed.encode() if p==path else (source/p).read_bytes())
            adjustments=adjust_specimen_source_lengths(specimen)
            refusal=None;intended=None
            if kind=='native':
                verifier.require_success(run(native_command(specimen,specimen/'tests',libraries),specimen,env,specimen,'build'))
                result=run([str(specimen/'tests'),'public_publication::','--test-threads=1'],specimen,env,specimen,'native')
                killed=result.returncode==101 and 'test result: FAILED' in result.stdout and ('assertion' in result.stdout or 'panicked at' in result.stdout)
                details={'stdout':str(specimen/'native.stdout')}
            else:
                result=run([*command,'--log-dir',str(specimen/'logs'),str(specimen/HARNESS)],specimen,env,specimen,'verus',900)
                parsed=output.parsed_report(result);details=parsed.get('verification-results',{})
                if kind=='proof':
                    errors=re.findall(r'(?m)^error:.*(?:\n(?!error:|note:).*)*',result.stderr)
                    killed=result.returncode!=0 and details.get('errors',0)>0 and details.get('encountered-vir-error') is False and not re.search(r'(?i)resource limit|timed out|timeout|out of memory',result.stderr) and any('authority/publication.rs' in e for e in errors)
                else:
                    matched=False
                    if result.returncode==0:
                        matched,refusal,intended=coverage_control((specimen/'logs/crate.vir').read_text(),profile,name)
                    killed=result.returncode==0 and details.get('success') is True and details.get('errors')==0 and matched
            row={'name':name,'kind':kind,'killed':bool(killed),'exit_code':result.returncode,'source':str(path),
                 'source_sha256':verifier.digest(specimen/path),'source_array_length_adjustments':adjustments,
                 'coverage_refusal':refusal,'intended_coverage_control':intended,'result':details,'logs':str(specimen)}
            controls.append(row)
            (directory/'mutations.json').write_text(json.dumps(controls,indent=2,sort_keys=True)+'\n')
            print(f'Publication: {name}: {"caught" if killed else "FAILED"} ({kind})',flush=True)
            if not killed:
                raise ValueError('semantic/coverage control survived or failed for unrelated reason: '+name)
    verifier.verify_tool_files(tool,pin)
    if snapshot()!=before:
        raise ValueError('publication source changed during qualification')
    return {'schema':'zeno-fcis/publication-v2-evidence/1','status':'positive_only' if positive_only else 'passed',
        'source_sha256':before,'source_generation':generation,'toolchain':pin,'verus_report':report,
        'translated_function_coverage':coverage,'coverage_modes':dict(collections.Counter(r['mode'] for r in coverage.values())),
        'native':{'exit_code':native.returncode,'output':native.stdout,'oracle_artifacts':libraries,
                  'oracle_artifact_sha256':{k:verifier.digest(Path(p)) for k,p in libraries.items()}},
        'custody_controls':custody_results,'public_test_exit':public.returncode,'authority_compatibility_exit':compatibility.returncode,'no_std_exit':no_std.returncode,
        'cargo_no_std_exit':cargo_no_std.returncode,'strict_clippy_exit':clippy.returncode,'mutations':controls,
        'trusted_base':['reviewed exact original-wire specification and complete coverage manifest','pinned Verus/vstd/Z3',
                        'Rust compiler and allocator','host platform'],
        'unproved':['legacy Value/Envelope oracle implementation','cryptographic schema hash recomputation',
                    'physical output allocation or serialization cost','host authentication','root shell atomic CAS and durable delivery',
                    'whole V2 release, six-template registration, combined Miri/ATDD/CI']}


def main() -> int:
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out',type=Path,required=True)
    parser.add_argument('--positive-only',action='store_true',help='development evidence only')
    args=parser.parse_args();args.out.parent.mkdir(parents=True,exist_ok=True)
    directory=Path(tempfile.mkdtemp(prefix='publication-run-',dir=args.out.resolve().parent))
    try:
        receipt=check(directory,args.positive_only)
    except (OSError,ValueError,KeyError,RuntimeError,subprocess.SubprocessError) as error:
        receipt={'schema':'zeno-fcis/publication-v2-evidence/1','status':'failed','error':str(error)}
    receipt['logs']=str(directory)
    args.out.write_text(json.dumps(receipt,indent=2,sort_keys=True)+'\n')
    print(f'Publication: {receipt["status"]}; receipt {args.out}')
    if receipt['status']=='failed':print(receipt['error'])
    return int(receipt['status']=='failed')

if __name__=='__main__':
    raise SystemExit(main())
