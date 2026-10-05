#!/usr/bin/env python3
"""Check actual-source complete typed decisions; byte/authority composition is separate."""
from __future__ import annotations
import argparse
import json
import os
import re
from pathlib import Path
import subprocess
import tempfile
from v2_proof_sources import execution_sources, adjust_specimen_source_lengths
from v2_native_dependencies import native_dependency_args
import check_verus as verifier
from check_metered_execution import UNIT_SOURCES as BASE
from verus_coverage import require_coverage

ROOT=Path(__file__).resolve().parents[1]
HARNESS=Path('verification/verus/decision.rs')
PROFILE=Path('verification/verus/decision.json')
SUBJECT=Path('crates/zeno-fcis-synthesis/src/finite/execution_v2/decision.rs')
SPEC=SUBJECT.parent/'decision/spec.rs'
TESTS=SUBJECT.parent/'decision/tests.rs'
UNIT_SOURCES=(HARNESS,*BASE[1:],SUBJECT,SPEC)

UNIT_SOURCES = tuple(dict.fromkeys((*UNIT_SOURCES, *execution_sources(ROOT))))
TEST_SOURCES=(SUBJECT.parent/'tests.rs',SUBJECT.parent/'input_view/tests.rs',SUBJECT.parent/'composition/record_tests.rs',TESTS,Path('crates/zeno-fcis-cli/templates/order-fulfillment/synthesized/transition.rs'))
SOURCES=(*UNIT_SOURCES,*TEST_SOURCES,PROFILE,verifier.PIN,Path('tools/check_decision_v2.py'),
    Path('tools/test_check_decision_v2.py'),Path('tools/check_metered_execution.py'),Path('tools/check_finite_execution.py'),Path('tools/check_verus.py'),Path('tools/verus_coverage.py'),
    Path('docs/V2_COMPLETE_DECISION_STAGE.md'))

SOURCES = tuple(dict.fromkeys((*SOURCES, Path("tools/v2_proof_sources.py"),
                                Path("verification/verus/authority_v2_sources.json"))))

SOURCES = tuple(dict.fromkeys((*SOURCES, Path("tools/v2_native_dependencies.py"))))

def once(source,before,after):
    tokens=re.findall(r'\w+|[^\w\s]',before)
    parts=[]
    for i,token in enumerate(tokens):
        if token=='}' and i and tokens[i-1]!=',':parts.append(r',?')
        parts.append(re.escape(token))
    pattern=r'\s*'.join(parts)
    matches=list(re.finditer(pattern,source))
    if len(matches)!=1:raise ValueError(f'expected one mutation anchor, found {len(matches)}: {before}')
    found=matches[0]
    return source[:found.start()]+after+source[found.end():]

def snapshot():
    for p in SOURCES:
        if (ROOT/p).is_symlink():raise RuntimeError(f'symlink proof source: {p}')
    return {str(p):verifier.digest(ROOT/p) for p in SOURCES}

def mutations(source,specification):
    rows={}
    for name,before,after in (
        ('branch','found = Some(i);','found = Some(0);'),
        ('class','Ok(Candidate{class:branch.class,reason:branch.reason,pre:copy_fields(inputs.state),post,patch,effects,outbox})','Ok(Candidate{class:Class::Reject,reason:branch.reason,pre:copy_fields(inputs.state),post,patch,effects,outbox})'),
        ('reason','reason:branch.reason,pre:copy_fields(inputs.state),post:Vec::new()','reason:None,pre:copy_fields(inputs.state),post:Vec::new()'),
        ('post_state','post.push(Field{id:plan[i].field,value});','post.push(Field{id:plan[i].field,value:inputs.state[i].value});'),
        ('patch','if !equal(inputs.state[i].value,value) {','if false {'),
        ('effect_order','result.push(Delivery{ordinal:entry.ordinal,channel:entry.channel','result.push(Delivery{ordinal:0,channel:entry.channel'),
        ('outbox','deliveries(inputs,output,branch.outbox,true,meter,attempts)?','deliveries(inputs,output,branch.effects,true,meter,attempts)?'),
        ('footprint','Attempt::Write(plan[i].field,charge.is_ok())','Attempt::Write(plan[i].field,true)'),
        ('meter','meter.charge(Resource::Candidate,1)','meter.charge(Resource::Candidate,0)'),
        ('domain','min <= v && v <= max','min <= v'),
    ):
        if name=='domain':
            changed=source.replace(before,after,1)
        else: changed=once(source,before,after)
        rows[name]=(SUBJECT,changed,'proof',True)
    header=source.index("pub(super) fn construct<'a>")
    start=source.rindex('#[cfg_attr(verus_keep_ghost, verus_spec(result =>',0,header)
    end=source.index('#[cfg_attr(verus_keep_ghost, verifier::rlimit',start)
    rows['weaken_construct_contract']=(SUBJECT,source[:start]+'#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures true,))]\n'+source[end:],'coverage',False)
    rows['narrow_construct_domain']=(SUBJECT,source[:start]+source[start:].replace('    ensures final(meter).limits','    requires code >= 0,\n    ensures final(meter).limits',1),'coverage',False)
    rows['uncontracted_helper']=(SUBJECT,source+'\npub fn uncontracted_decision_probe() -> u64 { 42 }\n','coverage',False)
    rows['specification_body']=(SPEC,once(specification,'let c = charge(limits,used,Resource::Candidate,1);','let c = { let first = charge(limits,used,Resource::Candidate,1); first };'),'coverage',False)
    cached=once(source,'        let charge = meter.charge(Resource::Write,1);','        let cached = resolve(inputs,output,plan[i].value);\n        let charge = meter.charge(Resource::Write,1);')
    cached=once(cached,'let Some(value) = resolve(inputs,output,plan[i].value) else {','let Some(value) = cached else {')
    rows['resolve_before_write_charge']=(SUBJECT,cached,'coverage',False)
    return rows

def native(directory,environment,root,filtered=False):
    binary=directory/'native-tests'
    args=['rustc','+1.97.1','--edition=2024','--test','--check-cfg','cfg(verus_keep_ghost)','--check-cfg','cfg(test)',str(HARNESS),'-o',str(binary)]
    args += native_dependency_args(ROOT, directory, environment)
    build=verifier.run(args,root,environment,timeout=180)
    verifier.require_success(build)
    command=[str(binary),'--test-threads=1']
    if filtered: command.append('execution_v2::decision::tests')
    return verifier.run(command,root,environment,timeout=180)

def check(out:Path,positive_only=False):
    pin=json.loads((ROOT/verifier.PIN).read_text());profile=json.loads((ROOT/PROFILE).read_text())
    report_pin={**pin,'expected_verified':profile['expected_verified'],'target_functions':profile['target_functions']}
    before=snapshot(); tool=verifier.prepare_tools(Path.home()/'.cache/zeno-fcis/verus',pin,False)
    env={k:v for k,v in os.environ.items() if not k.startswith(('VERUS_','VARGO_')) and k not in ('RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS')}
    env.update(RUSTUP_TOOLCHAIN=pin['rust_toolchain'],VERUS_Z3_PATH=str(tool/'z3'),CARGO_BUILD_JOBS='1',RUST_TEST_THREADS='1')
    command=[str(tool/'verus'),'--crate-type=lib','--edition=2024','--no-cheating','--no-external-by-default','--num-threads','2', "-V", "spinoff-all",'--output-json','--log','vir','--log','vir-option=no_span+no_type+no_fn_details']
    evidence=out.parent; evidence.mkdir(parents=True,exist_ok=True)
    positive=verifier.run([*command,'--log-dir',str(evidence/'positive'),str(HARNESS)],ROOT,env,timeout=300)
    (evidence/'positive.stdout').write_text(positive.stdout);(evidence/'positive.stderr').write_text(positive.stderr)
    verifier.require_success(positive);report=json.loads(positive.stdout)
    if not verifier.accepted(report,report_pin):raise RuntimeError('unaccepted whole-crate proof/toolchain/inventory count')
    coverage=require_coverage((evidence/'positive/crate.vir').read_text(),profile)
    base_native=native(evidence,env,ROOT)
    (evidence/'native.stdout').write_text(base_native.stdout);(evidence/'native.stderr').write_text(base_native.stderr)
    verifier.require_success(base_native)
    print('Complete decision positive proof, coverage and native tests passed',flush=True)
    results=[]
    if not positive_only:
        with tempfile.TemporaryDirectory(prefix='zeno-decision-mutations-') as temporary:
            for name,(path,changed,expected,native_control) in mutations((ROOT/SUBJECT).read_text(),(ROOT/SPEC).read_text()).items():
                specimen=Path(temporary)/name
                for unit in (*UNIT_SOURCES,*TEST_SOURCES):
                    target=specimen/unit;target.parent.mkdir(parents=True,exist_ok=True)
                    target.write_text(changed) if unit==path else target.write_bytes((ROOT/unit).read_bytes())
                adjust_specimen_source_lengths(specimen)
                proof=verifier.run([*command,'--log-dir',str(specimen/'logs'),str(HARNESS)],specimen,env,timeout=300)
                (evidence/f'{name}.stdout').write_text(proof.stdout);(evidence/f'{name}.stderr').write_text(proof.stderr)
                parsed=json.loads(proof.stdout).get('verification-results',{})
                refusal=None
                if expected=='proof':
                    killed=proof.returncode!=0 and parsed.get('success') is False and parsed.get('errors',0)>0 and parsed.get('encountered-vir-error') is False
                else:
                    try:require_coverage((specimen/'logs/crate.vir').read_text(),profile)
                    except ValueError as e:refusal=str(e)
                    killed=proof.returncode==0 and parsed.get('success') is True and refusal is not None
                native_status=None
                if native_control:
                    n=native(specimen,env,specimen,True);native_status=n.returncode
                    (evidence/f'{name}.native.stdout').write_text(n.stdout);(evidence/f'{name}.native.stderr').write_text(n.stderr)
                    killed=killed and n.returncode!=0 and 'test result: FAILED' in n.stdout
                results.append(dict(name=name,expected=expected,killed=killed,exit_code=proof.returncode,verification_results=parsed,coverage_refusal=refusal,native_exit_code=native_status))
                if not killed:raise RuntimeError(f'mutation {name} survived or failed for unrelated reason: {results[-1]}')
                print(f'Caught {name} ({expected}; native={native_status})',flush=True)
    if before!=snapshot():raise RuntimeError('source drift during gate')
    verifier.verify_tool_files(tool,pin)
    return dict(schema='zeno-fcis/decision-v2-evidence/1',status='passed',revision=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),source_sha256=before,verus_report=report,translated_function_coverage=coverage,mutations=results,native_exit_code=base_native.returncode,proof_exit_code=positive.returncode,positive_only=positive_only,
        unproved=['exact raw admission/execution-to-typed constructor bridge','canonical encoding/hash correspondence','catalog-bound closed plan admission','authority and replay sealing','law/genesis composition','compiler/allocator/platform and shell'])

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--out',required=True,type=Path);parser.add_argument('--positive-only',action='store_true');args=parser.parse_args()
    try:receipt=check(args.out.resolve(),args.positive_only)
    except (OSError,ValueError,KeyError,RuntimeError,subprocess.SubprocessError) as e:receipt=dict(schema='zeno-fcis/decision-v2-evidence/1',status='failed',error=str(e))
    args.out.parent.mkdir(parents=True,exist_ok=True);args.out.write_text(json.dumps(receipt,sort_keys=True,indent=2)+'\n');print(receipt['status'])
    if receipt['status']!='passed':print(receipt['error']);return 1
    return 0
if __name__=='__main__':raise SystemExit(main())
