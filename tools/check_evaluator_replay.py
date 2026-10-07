#!/usr/bin/env python3
"""Retain unmutated subjects, then challenge a separately compiled digest flip.

Run under the shared heavy lock with pinned dependency artifacts from the native
harness build. This control does not regenerate a subject under the mutant.
"""
from pathlib import Path
import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import check_authority_v2 as gate

PROBE = r'''
#[test]
fn retained_evaluator_subjects() {
    let directory=std::env::var("ZENO_EVALUATOR_FIXTURES").unwrap_or_else(|e| panic!("fixture directory: {e}"));
    let mode=std::env::var("ZENO_EVALUATOR_MODE").unwrap_or_else(|e| panic!("fixture mode: {e}"));
    with_catalog(b"Authority",u64::MAX,true,|catalog,_|{
        let authority=authority::bind(catalog).unwrap_or_else(|e| panic!("bind: {e:?}"));
        let values=originals(0,true);
        let transition=std::path::Path::new(&directory).join("transition.bin");
        let genesis=std::path::Path::new(&directory).join("genesis.bin");
        if mode=="save" {
            std::fs::write(transition,authority.evaluate(raw(&values)).subject().unwrap_or_else(|e| panic!("transition: {e:?}"))).unwrap_or_else(|e| panic!("save: {e}"));
            std::fs::write(genesis,authority.genesis(&values.0).subject().unwrap_or_else(|e| panic!("genesis: {e:?}"))).unwrap_or_else(|e| panic!("save: {e}"));
        } else {
            assert!(mode=="accept" || mode=="refuse");
            let t=std::fs::read(transition).unwrap_or_else(|e| panic!("read: {e}"));
            let g=std::fs::read(genesis).unwrap_or_else(|e| panic!("read: {e}"));
            let tr=authority.replay(raw(&values),&t);
            let gr=authority.replay_genesis(&values.0,&g);
            if mode=="accept" { assert!(tr.result().is_ok()); assert_eq!(gr.result(),Ok(())); }
            else { assert!(matches!(tr.result(),Err(authority::Refusal::ReplayMismatch))); assert_eq!(gr.result(),Err(authority::Refusal::ReplayMismatch)); }
        }
    });
}
'''


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out',required=True,type=Path)
    parser.add_argument('--artifacts',required=True,type=Path)
    args=parser.parse_args()
    gate.check_generated_sources()
    out=args.out.resolve();out.mkdir(parents=True,exist_ok=False)
    specimen=out/'source'
    for relative in gate.sources():
        target=specimen/relative;target.parent.mkdir(parents=True,exist_ok=True)
        shutil.copyfile(gate.ROOT/relative,target)
    public=specimen/gate.PUBLIC_TEST;public.write_text(public.read_text()+PROBE)
    artifacts=json.loads(args.artifacts.read_text())
    assert set(artifacts)=={'zeno_fcis_codec','zeno_fcis_value'}
    deps=Path(artifacts['zeno_fcis_codec']).parent
    if deps.name != 'deps':deps=deps/'deps'
    rust=['rustc','+1.97.1','--edition=2024','--check-cfg','cfg(verus_keep_ghost)',
          '--check-cfg','cfg(test)','-C','debuginfo=0','-L','dependency='+str(deps)]
    for name,path in sorted(artifacts.items()):rust+=['--extern',name+'='+path]
    env={**os.environ,'CARGO_MANIFEST_DIR':str(specimen/'crates/zeno-fcis-synthesis'),
         'ZENO_EVALUATOR_FIXTURES':str(out)}
    records=[]
    def run(name,cmd,mode='',expected=0):
        result=subprocess.run(cmd,cwd=specimen,env={**env,'ZENO_EVALUATOR_MODE':mode},
                              capture_output=True,text=True,timeout=180)
        (out/(name+'.stdout')).write_text(result.stdout);(out/(name+'.stderr')).write_text(result.stderr)
        records.append({'name':name,'exit':result.returncode,'command':cmd,'mode':mode})
        (out/'progress.json').write_text(json.dumps(records,indent=2)+'\n')
        if result.returncode!=expected:raise RuntimeError(f'{name}: unexpected exit {result.returncode}: {result.stderr[-1000:]}')
        return result
    baseline=out/'baseline-native'
    run('baseline-build',rust+['--test',str(gate.HARNESS),'-o',str(baseline)])
    filter_args=['public_authority::retained_evaluator_subjects','--exact','--nocapture']
    run('retain-subjects',[str(baseline),*filter_args],mode='save')
    accepted=run('baseline-replay',[str(baseline),*filter_args],mode='accept')
    assert '1 passed; 0 failed' in accepted.stdout
    retained={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in [out/'transition.bin',out/'genesis.bin']}
    generated=specimen/gate.EVALUATOR
    original=generated.read_text()
    changed=re.sub(r'(= \[)(\d+)u8',lambda m:m[1]+str(int(m[2])^1)+'u8',original,count=1)
    assert changed!=original;generated.write_text(changed)
    mutant=out/'mutant-native'
    run('mutant-build',rust+['--test',str(gate.HARNESS),'-o',str(mutant)])
    refused=run('mutant-replay',[str(mutant),*filter_args],mode='refuse')
    assert '1 passed; 0 failed' in refused.stdout
    library=out/'libzeno_fcis_synthesis.rlib'
    run('mutant-library',rust+['--crate-type=rlib','--crate-name','zeno_fcis_synthesis',str(gate.HARNESS),'-o',str(library)])
    independent=out/'independent-digest-test'
    run('independent-build',rust+['--extern','zeno_fcis_synthesis='+str(library),'--test',
        'crates/zeno-fcis-synthesis/tests/v2_evaluator_identity.rs','-o',str(independent)])
    negative=run('independent-refusal',[str(independent),'--nocapture'],expected=101)
    assert 'workspace_digest_equals_compiled_authority ... FAILED' in negative.stdout
    assert 'fixed_encoding_vector ... ok' in negative.stdout
    assert retained=={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in [out/'transition.bin',out/'genesis.bin']}
    result={'status':'passed','retained_subject_sha256':retained,'checks':records,
            'original_evaluator_sha256':hashlib.sha256(original.encode()).hexdigest(),
            'mutated_evaluator_sha256':hashlib.sha256(changed.encode()).hexdigest()}
    (out/'receipt.json').write_text(json.dumps(result,indent=2,sort_keys=True)+'\n')
    print(json.dumps({'status':'passed','retained_subject_sha256':retained}))

if __name__=='__main__':main()
