#!/usr/bin/env python3
"""Translation-validate the pinned shipped Rust arithmetic AST inventory.

Trusted additions to the arithmetic proof base: the reviewed closed Rust grammar
decoder, production-Program data printer, this orchestration and Python runtime.
The theorem concerns the decoded AST, not a direct Verus proof of emitted bodies.
"""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess

import check_arithmetic_proof as primitive
import check_arithmetic_typed_proof as typed
import check_verus as verifier
from arithmetic_emitted_ast import decode,BindingMismatch
from arithmetic_emission_cases import OPERATIONS,discover,checks_source
from arithmetic_emitter_controls import build_dumper,controls,native_witness

ROOT=Path(__file__).resolve().parents[1]
PROFILE=Path('verification/verus/arithmetic-emission.json')
INVENTORY=Path('verification/verus/arithmetic-emission-inventory.json')
EMITTER=Path('crates/zeno-fcis-synthesis/src/finite/emit.rs')


def helper_path_binding(emitter,harness):
    """Check the two literal include sites resolve to the verified files.

    Full source pins bind these reviewed include forms; this is an additional
    explicit check of the emitter-to-proof path relationship, not a Rust parser.
    """
    included=re.findall(r'\binclude_str!\(\s*"([^"]+)"\s*\)',emitter)
    declarations=re.findall(r'#\[path\s*=\s*"([^"]+)"\]\s*pub\(crate\)\s+mod\s+(scalar|wide)\s*;',harness)
    if len(included)!=2 or len(declarations)!=2 or {name for _,name in declarations}!={'scalar','wide'}:
        raise ValueError('E2_helper_paths: expected exactly two reviewed include sites')
    bindings=[]
    for name,expected in [('scalar',primitive.SCALAR),('wide',primitive.WIDE)]:
        matching=[p for p in included if (ROOT/EMITTER.parent/p).resolve()==(ROOT/expected).resolve()]
        proof_path=next(p for p,n in declarations if n==name)
        if len(matching)!=1 or (ROOT/primitive.HARNESS.parent/proof_path).resolve()!=(ROOT/expected).resolve():
            raise ValueError('E2_helper_paths: emitter and proof differ for '+name)
        bindings.append(dict(module=name,emitter_include=matching[0],proof_include=proof_path,
                             verified_source=str(expected)))
    return bindings


def qualify(out,work,run_controls):
    profile=json.loads((ROOT/PROFILE).read_text())
    pin=json.loads((ROOT/verifier.PIN).read_text())
    inventory=json.loads((ROOT/INVENTORY).read_text())
    if discover(ROOT)!=inventory:raise ValueError('shipped program/problem inventory differs')
    typed.require_sources(ROOT,profile)
    helper_bindings=helper_path_binding((ROOT/EMITTER).read_text(),(ROOT/primitive.HARNESS).read_text())
    primitive.audit_native_binding((ROOT/primitive.SCALAR).read_text())
    primitive.audit_native_binding((ROOT/primitive.WIDE).read_text())
    env={k:v for k,v in os.environ.items() if not k.startswith(('VERUS_','VARGO_'))
         and k not in ('RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS','RUSTUP_TOOLCHAIN')}
    if not env.get('CARGO_TARGET_DIR'):raise ValueError('owned Cargo target required')
    env.update(CARGO_PROFILE_DEV_DEBUG='0',CARGO_PROFILE_TEST_DEBUG='0',CARGO_INCREMENTAL='0',CARGO_BUILD_JOBS='4')
    producer,binding=build_dumper(ROOT,env,out/'producer',work)
    cases=[];subjects=[]
    entries=[(Path(p).parent.parent.name.replace('-','_'),str(ROOT/p),Path(p)) for p in inventory['programs']]
    entries += [('probe_'+op,'op:'+op,None) for op in OPERATIONS]
    for name,argument,path in entries:
        directory=out/'subjects'/name
        p=primitive.process([str(producer),argument,str(directory)],ROOT,env,out/'capture'/name)
        verifier.require_success(p)
        original=json.loads((directory/'program.json').read_text())
        source=(directory/'transition.rs').read_bytes().decode('utf-8')
        decoded,helpers=decode(source,original,ROOT)
        if path is not None and (directory/'transition.rs').read_bytes()!=(ROOT/path.with_name('transition.rs')).read_bytes():
            raise ValueError('current emitter bytes differ from shipped artifact: '+name)
        (directory/'ast.json').write_text(json.dumps(decoded,indent=2)+'\n')
        cases.append((name,original,decoded))
        subjects.append(dict(name=name,program=None if path is None else str(path),
            nodes=len(original['nodes']),helpers=helpers,
            source_sha256=primitive.digest(directory/'transition.rs'),
            original_ir_sha256=primitive.digest(directory/'program.json'),
            decoded_ast_sha256=primitive.digest(directory/'ast.json')))
    harness=work/'arithmetic-emission-checks.rs'
    harness.write_text(checks_source(ROOT,cases))
    proof_sha256=primitive.digest(harness)
    shutil.copyfile(harness,out/'arithmetic-emission-checks.rs')
    tool=verifier.prepare_tools(verifier.cache_directory(),pin,False)
    proof_env=dict(env,RUSTUP_TOOLCHAIN=pin['rust_toolchain'],VERUS_Z3_PATH=str(tool/'z3'))
    command=[str(tool/'verus'),'--crate-type=lib','--crate-name','arithmetic_emission','--edition=2024',
        '--no-cheating','--no-external-by-default','--num-threads','4','--output-json',
        '--triggers-mode','silent','-V','spinoff-all',str(harness)]
    proof=primitive.process(command,ROOT,proof_env,out/'proof',timeout=1200)
    verifier.require_success(proof);report=json.loads(proof.stdout)
    if not primitive.accepted(report,pin,profile):raise ValueError('emission proof count or exact function inventory differs')
    required={'arithmetic_emission::checked_programs::check_'+name for name,_,_ in cases}
    if not required.issubset(report['func-details']):raise ValueError('a concrete translation check was omitted')
    mutations=[]
    if run_controls:
        control_work=work/'controls';control_work.mkdir()
        mutations=controls(ROOT,env,out/'controls',control_work)
        # Native-only code inside a generated verified helper must fail E2,
        # even when the same source could verify after ghost configuration.
        subject=out/'subjects/probe_div_floor';source=(subject/'transition.rs').read_bytes().decode('utf-8')
        model=json.loads((subject/'program.json').read_text())
        anchor='pub(crate) fn divide(up: bool, a: i128, b: i128) -> Option<i128> {'
        injected=primitive.once(source,anchor,anchor+'\n    #[cfg(not(verus_keep_ghost))]\n    if a == 1 && b == 0 { return Some(0); }')
        data=work/'native-only-helper';data.mkdir()
        baseline=native_witness(source,'div_floor',ROOT,env,out/'native-only/normal',data,'E2_native_configuration')
        verifier.require_success(baseline)
        native=native_witness(injected,'div_floor',ROOT,env,out/'native-only/mutated',data,'E2_native_configuration')
        if native.returncode!=101 or 'E2_native_configuration' not in native.stderr:
            raise ValueError('native-only helper control did not fail its assertion')
        try:decode(injected,model,ROOT)
        except BindingMismatch as error:
            if error.obligation!='E2_helper_identity':raise ValueError('native helper failed wrong binding obligation')
        else:raise ValueError('native-only generated helper was admitted')
        verifier.require_success(native_witness(source,'div_floor',ROOT,env,out/'native-only/restored',data,'E2_native_configuration'))
        (out/'native-only/mutated.rs').write_text(injected)
        mutations.append(dict(name='native_only_helper',normal=0,mutated_build=0,mutated_assertion=101,
            mutated_gate=1,intended_obligation='E2_helper_identity',restored=0,
            restored_sha256=primitive.digest(subject/'transition.rs')))
    typed.require_sources(ROOT,profile);verifier.verify_tool_files(tool,pin)
    if primitive.digest(harness)!=proof_sha256:raise ValueError('generated proof changed during qualification')
    for row in subjects:
        directory=out/'subjects'/row['name']
        for filename,key in [('transition.rs','source_sha256'),('program.json','original_ir_sha256'),('ast.json','decoded_ast_sha256')]:
            if primitive.digest(directory/filename)!=row[key]:raise ValueError('captured program changed during qualification')
    from arithmetic_reference64_vectors import source as reference64_source
    if (ROOT/'verification/verus/arithmetic_reference64.rs').read_text()!=reference64_source():
        raise ValueError('64-bit abstract reference proofs differ from their producer')
    return dict(schema='zeno-fcis/arithmetic-emission-evidence/1',status='passed',
        source_sha256=profile['source_sha256'],inventory=inventory,subjects=subjects,
        generated_proof_sha256=proof_sha256,toolchain=pin,verus_report=report,
        helper_bindings=helper_bindings,
        native_producer=binding,controls=mutations,complete_controls=run_controls,
        method='per-program translation validation against mathematical emitted-AST semantics',
        trusted=['reviewed closed Rust grammar decoder','production-Program data printer',
                 'gate orchestration and Python','rustc/native code generation and std contracts',
                 'pinned Verus/vstd/Z3 and Linux x86-64'],
        unproved=['universal emitter/formatter correctness','direct emitted-native-body Verus correspondence',
                  'Rust compiler/binary correctness','shell, resource metering, application policy, release authority'])


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out',type=Path,required=True);parser.add_argument('--work',type=Path,required=True)
    parser.add_argument('--positive-only',action='store_true');a=parser.parse_args()
    a.out=a.out.resolve();a.work=a.work.resolve();a.out.mkdir(parents=True,exist_ok=False);a.work.mkdir(parents=True,exist_ok=False)
    try:r=qualify(a.out,a.work,not a.positive_only)
    except (OSError,ValueError,RuntimeError,subprocess.SubprocessError,BindingMismatch) as e:r=dict(status='failed',error=str(e))
    (a.out/'receipt.json').write_text(json.dumps(r,indent=2,sort_keys=True)+'\n')
    print(json.dumps({k:v for k,v in r.items() if k in ('status','error','method','controls')}))
    return 0 if r['status']=='passed' else 1


if __name__=='__main__':raise SystemExit(main())
