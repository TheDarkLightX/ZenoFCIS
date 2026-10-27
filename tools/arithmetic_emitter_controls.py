"""Native semantic controls planted in a private copy of production emit.rs."""
import json
import os
from pathlib import Path
import shutil

import check_arithmetic_proof as common
import check_verus as verifier
from arithmetic_emitted_ast import BindingMismatch,decode

EMITTER=Path('crates/zeno-fcis-synthesis/src/finite/emit.rs')
DUMPER=Path('verification/verus/arithmetic_program_dump.rs')


def build_dumper(root,environment,out,work):
    env=dict(environment);env.pop('RUSTUP_TOOLCHAIN',None)
    env.update(CARGO_PROFILE_DEV_DEBUG='0',CARGO_PROFILE_TEST_DEBUG='0',CARGO_INCREMENTAL='0',CARGO_BUILD_JOBS='4')
    command=['cargo','+1.97.1','build','--locked','--offline','-j','4','-p','zeno-fcis-synthesis',
             '-p','zeno-fcis-crypto','--features','zeno-fcis-crypto/rustcrypto','--message-format=json']
    p=common.process(command,root,env,out/'cargo',timeout=1200);verifier.require_success(p)
    rows=[json.loads(line) for line in p.stdout.splitlines()]
    row=next(r for r in rows if r.get('reason')=='compiler-artifact' and r['target']['name']=='zeno_fcis_synthesis')
    library=next(Path(p) for p in row['filenames'] if p.endswith('.rlib'))
    if Path(row['manifest_path']).resolve()!=root/'crates/zeno-fcis-synthesis/Cargo.toml':
        raise ValueError('program producer library comes from another source tree')
    target=Path(env['CARGO_TARGET_DIR'])
    binary=work/'program-dump'
    command=['rustc','+1.97.1','--edition=2024','-L','dependency='+str(target/'debug/deps'),
             '--extern','zeno_fcis_synthesis='+str(library),str(DUMPER),'-o',str(binary)]
    verifier.require_success(common.process(command,root,env,out/'producer-build'))
    return binary,dict(path=str(library),sha256=common.digest(library),manifest=row['manifest_path'],
        producer_sha256=common.digest(root/DUMPER),binary_sha256=common.digest(binary))


def emit(binary,op,root,environment,out,work):
    verifier.require_success(common.process([str(binary),'op:'+op,str(work)],root,environment,out))
    return json.loads((work/'program.json').read_text()),(work/'transition.rs').read_bytes().decode('utf-8')


def native_witness(source,op,root,environment,out,work,label=None):
    label=label or ('E1_partial_refusal_propagation' if op=='mul' else 'E1_eager_evaluation')
    values='i128::MAX,2' if op=='mul' else '1,0'
    driver=work/'witness.rs';binary=work/'witness'
    driver.write_text(source+f'\nfn main() {{ assert_eq!(transition(&[{values}]),None,"{label}"); }}\n')
    cmd=['rustc','+1.97.1','--edition=2024','--check-cfg','cfg(verus_keep_ghost)',
         str(driver),'-o',str(binary)]
    verifier.require_success(common.process(cmd,root,environment,out/'build'))
    return common.process([str(binary)],root,environment,out/'assertion')


def controls(root,environment,out,work):
    specimen=work/'source'
    shutil.copytree(root,specimen,ignore=shutil.ignore_patterns('.git','.agents','.codex','.aws','target','__pycache__'))
    env=dict(environment,CARGO_TARGET_DIR=str(Path(environment['CARGO_TARGET_DIR'])/'emitter-controls'))
    original=(specimen/EMITTER).read_bytes();before=common.digest(root/EMITTER)
    binary,baseline_binding=build_dumper(specimen,env,out/'normal',work)
    baselines={}
    for op in ('mul','eager_select'):
        data=work/('normal-'+op);model,source=emit(binary,op,specimen,env,out/('normal-'+op)/'emit',data)
        decode(source,model,root)
        verifier.require_success(native_witness(source,op,specimen,env,out/('normal-'+op)/'native',data))
        baselines[op]=(model,source)
    changes=(
        ('dropped_question_mark','mul','E1_partial_refusal_propagation',
         'format!("arithmetic::scalar::multiply(v{a},v{b})?")',
         'format!("arithmetic::scalar::multiply(v{a},v{b}).unwrap_or(0)")'),
        ('lazy_untaken_branch','eager_select','E1_eager_evaluation',
         'format!("arithmetic::scalar::divide(false,v{a},v{b})?")',
         'format!("if v2 == 1 {{ arithmetic::scalar::divide(false,v{a},v{b})? }} else {{ 0_i128 }}")'),
    )
    rows=[]
    for name,op,obligation,old,new in changes:
        try:
            (specimen/EMITTER).write_text(common.once(original.decode(),old,new))
            binary,binding=build_dumper(specimen,env,out/name/'mutated',work)
            data=work/name;model,source=emit(binary,op,specimen,env,out/name/'emit',data)
            native=native_witness(source,op,specimen,env,out/name/'native',data)
            if native.returncode!=101 or obligation not in native.stderr or 'assertion `left == right` failed' not in native.stderr:
                raise ValueError('emitter control did not fail its named assertion: '+name)
            try:decode(source,model,root)
            except BindingMismatch as error:
                if error.obligation!=obligation:raise ValueError('emitter control failed the wrong binding obligation')
            else:raise ValueError('emitter control survived source-to-AST validation')
            (out/name/'mutated-transition.rs').write_text(source)
            (out/name/'program.json').write_text(json.dumps(model,indent=2)+'\n')
        finally:
            (specimen/EMITTER).write_bytes(original)
        if common.digest(specimen/EMITTER)!=before:raise ValueError('emitter source restoration differs')
        binary,restored_binding=build_dumper(specimen,env,out/name/'restored',work)
        data=work/(name+'-restored');restored_model,restored=emit(binary,op,specimen,env,out/name/'restored-emit',data)
        if (restored_model,restored)!=baselines[op]:raise ValueError('restored emitted bytes differ')
        decode(restored,restored_model,root)
        verifier.require_success(native_witness(restored,op,specimen,env,out/name/'restored-native',data))
        rows.append(dict(name=name,normal=0,mutated_build=0,mutated_assertion=101,
            mutated_gate=1,intended_obligation=obligation,restored=0,restored_sha256=before,
            production_library=binding,restored_library=restored_binding))
    if common.digest(root/EMITTER)!=before:raise ValueError('original emitter changed')
    return rows
