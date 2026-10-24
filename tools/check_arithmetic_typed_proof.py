#!/usr/bin/env python3
"""Qualify typed arithmetic correspondence on exact production sources.

The native dependencies are comparison artifacts only. The Verus crate imports
production source by path and has no compiled Zeno dependencies or value stubs.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess

import check_arithmetic_proof as primitive
import check_verus as verifier

ROOT=Path(__file__).resolve().parents[1]
HARNESS=Path('verification/verus/arithmetic_typed_proof.rs')
NATIVE=Path('verification/verus/arithmetic_typed_native.rs')
PROFILE=Path('verification/verus/arithmetic-typed-proof.json')
TYPED=Path('crates/zeno-fcis-synthesis/src/finite/evaluation/typed.rs')
BOUNDARIES=(TYPED,TYPED.parent/'mod.rs',TYPED.parent.parent/'value/mod.rs',
            primitive.SCALAR,primitive.WIDE)


def require_sources(root,profile):
    for name,expected in profile['source_sha256'].items():
        path=Path(name)
        if path.is_absolute() or '..' in path.parts or (root/path).is_symlink():
            raise ValueError('unreviewed arithmetic source path')
        if primitive.digest(root/path)!=expected:
            raise ValueError('arithmetic source differs from reviewed native binding: '+name)


def audit_boundary(source):
    if re.search(r'\bcfg\s*!|cfg\s*\(\s*not\b|cfg_attr\s*\(\s*not\b',source):
        raise ValueError('native-only configuration in arithmetic boundary')
    # These exact declarations are reviewed alongside pinned full source bytes.
    # Tests and the legacy test/proof adapter do not run in the decision API.
    source=source.replace('#[cfg(test)]','')
    source=source.replace('#[cfg(any(test, verus_keep_ghost))]','')
    source=source.replace('#[cfg_attr(verus_keep_ghost, verus_verify)]','')
    primitive.audit_native_binding(source)


def accepted(report,pin,profile):
    return primitive.accepted(report,pin,profile)


def native_arguments(environment,out,work):
    target=Path(environment['CARGO_TARGET_DIR']).resolve()
    command=['cargo','+1.97.1','build','--locked','--offline','-j','4',
             '-p','zeno-fcis-codec','-p','zeno-fcis-value','-p','zeno-fcis-collections',
             '-p','zeno-fcis-crypto','--features','zeno-fcis-crypto/rustcrypto','--message-format=json']
    native_env=dict(environment);native_env.pop('RUSTUP_TOOLCHAIN',None)
    p=primitive.process(command,ROOT,native_env,out/'native-dependencies')
    verifier.require_success(p)
    names={'zeno_fcis_codec','zeno_fcis_value','zeno_fcis_collections','zeno_fcis_crypto'}
    libraries={}
    for line in p.stdout.splitlines():
        row=json.loads(line)
        name=row.get('target',{}).get('name')
        if row.get('reason')=='compiler-artifact' and name in names:
            path=next(Path(s) for s in row['filenames'] if s.endswith('.rlib'))
            if target not in path.parents:raise ValueError('native dependency outside owned target')
            libraries[name]=dict(path=str(path),sha256=primitive.digest(path),
                                 manifest=row['manifest_path'],fresh=not row['fresh'])
    if set(libraries)!=names:raise ValueError('native library inventory incomplete')
    (out/'native-libraries.json').write_text(json.dumps(libraries,indent=2)+'\n')
    args=['rustc','+1.97.1','--edition=2024','--check-cfg','cfg(verus_keep_ghost)',
          '--check-cfg','cfg(test)','-A','unused_imports','-L','dependency='+str(target/'debug/deps')]
    for name,row in sorted(libraries.items()):args+=['--extern',name+'='+row['path']]
    return args,native_env,libraries


def qualify(out,work,control):
    profile=json.loads((ROOT/PROFILE).read_text())
    pin=json.loads((ROOT/verifier.PIN).read_text())
    require_sources(ROOT,profile)
    for path in BOUNDARIES:audit_boundary((ROOT/path).read_text())
    environment={k:v for k,v in os.environ.items() if not k.startswith(('VERUS_','VARGO_'))
                 and k not in ('RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS')}
    environment.update(CARGO_PROFILE_DEV_DEBUG='0',CARGO_PROFILE_TEST_DEBUG='0',
                       CARGO_INCREMENTAL='0',CARGO_BUILD_JOBS='4')
    if not environment.get('CARGO_TARGET_DIR'):raise ValueError('owned Cargo target required')
    tool=verifier.prepare_tools(verifier.cache_directory(),pin,False)
    proof_env=dict(environment,RUSTUP_TOOLCHAIN=pin['rust_toolchain'],VERUS_Z3_PATH=str(tool/'z3'))
    command=[str(tool/'verus'),'--crate-type=lib','--edition=2024','--no-cheating',
             '--no-external-by-default','--num-threads','4','--output-json',
             '--triggers-mode','silent','-V','spinoff-all',str(HARNESS)]
    p=primitive.process(command,ROOT,proof_env,out/'proof')
    verifier.require_success(p);report=json.loads(p.stdout)
    if not accepted(report,pin,profile):raise ValueError('exact typed proof count/inventory mismatch')
    rust,native_env,libraries=native_arguments(environment,out,work)
    binary=work/'typed-native'
    verifier.require_success(primitive.process([*rust,str(NATIVE),'-o',str(binary)],ROOT,native_env,out/'native-build'))
    verifier.require_success(primitive.process([str(binary)],ROOT,native_env,out/'native-witnesses'))
    controls=[]
    if control:
        specimen=work/'truncating-cast'
        shutil.copytree(ROOT/'crates',specimen/'crates')
        for path in [HARNESS,NATIVE,primitive.SPEC,Path('verification/oracle_crypto_profile.json'),Path('verification/verus/arithmetic_typed_spec.rs'),
                     Path('verification/verus/arithmetic_vectors.rs')]:
            (specimen/path).parent.mkdir(parents=True,exist_ok=True)
            shutil.copyfile(ROOT/path,specimen/path)
        original=(specimen/TYPED).read_bytes()
        changed=primitive.once(original.decode(),'if v <= i128::MAX as u128 {','if true {')
        (specimen/TYPED).write_text(changed)
        try:
            require_sources(specimen,{'source_sha256':{str(TYPED):profile['source_sha256'][str(TYPED)]}})
        except ValueError:binding_refused=True
        else:raise ValueError('native source pin admitted a truncating cast')
        binary=specimen/'typed-native'
        verifier.require_success(primitive.process([*rust,str(NATIVE),'-o',str(binary)],specimen,native_env,out/'cast-control/build'))
        native=primitive.process([str(binary)],specimen,native_env,out/'cast-control/assertion')
        if native.returncode!=101 or 'checked_cast_refuses_truncation' not in native.stderr or 'assertion `left == right` failed' not in native.stderr:
            raise ValueError('cast control did not fail the intended native assertion')
        proof=primitive.process(command,specimen,proof_env,out/'cast-control/proof')
        bad=json.loads(proof.stdout)
        if not verifier.semantic_refusal(proof,bad,pin,TYPED,specimen,
                region=verifier.function_lines(changed,'node'),
                target_functions=['arithmetic_typed_proof::evaluation::typed::node']):
            raise ValueError('cast control did not fail the intended node postcondition')
        (specimen/TYPED).write_bytes(original)
        if primitive.digest(specimen/TYPED)!=profile['source_sha256'][str(TYPED)]:
            raise ValueError('cast restoration not byte identical')
        verifier.require_success(primitive.process([*rust,str(NATIVE),'-o',str(binary)],specimen,native_env,out/'cast-control/restored-build'))
        verifier.require_success(primitive.process([str(binary)],specimen,native_env,out/'cast-control/restored-assertion'))
        restored=primitive.process(command,specimen,proof_env,out/'cast-control/restored-proof')
        verifier.require_success(restored)
        if not accepted(json.loads(restored.stdout),pin,profile):raise ValueError('restored proof inventory differs')
        controls.append(dict(name='truncating_cast',normal=0,build=0,mutated_assertion=101,
            mutated_proof=proof.returncode,restored=0,binding_refused=binding_refused,
            restored_sha256=primitive.digest(specimen/TYPED)))
    require_sources(ROOT,profile);verifier.verify_tool_files(tool,pin)
    for row in libraries.values():
        if primitive.digest(Path(row['path']))!=row['sha256']:raise ValueError('native library changed')
    return dict(schema='zeno-fcis/arithmetic-typed-proof-evidence/1',status='passed',
        scope=profile['scope'],source_sha256=profile['source_sha256'],toolchain=pin,
        verus_report=report,controls=controls,complete_controls=control,
        native_libraries=libraries,compiled_dependencies_used_in_formal_proof=False,
        unproved=['emitter correspondence and generated-code binding','compiler/platform/standard-library contracts'])


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out',type=Path,required=True);parser.add_argument('--work',type=Path,required=True)
    parser.add_argument('--positive-only',action='store_true');a=parser.parse_args()
    a.out=a.out.resolve();a.work=a.work.resolve();a.out.mkdir(parents=True,exist_ok=False);a.work.mkdir(parents=True,exist_ok=False)
    try:r=qualify(a.out,a.work,not a.positive_only)
    except (OSError,ValueError,RuntimeError,subprocess.SubprocessError) as e:r=dict(status='failed',error=str(e))
    (a.out/'receipt.json').write_text(json.dumps(r,indent=2,sort_keys=True)+'\n')
    print(json.dumps({k:v for k,v in r.items() if k in ('status','error','scope','controls')}))
    return 0 if r['status']=='passed' else 1


if __name__=='__main__':raise SystemExit(main())
