#!/usr/bin/env python3
"""Pinned actual-source law/genesis proof, custody tests and mutation coverage.

This lower-level checked engine is not transition authorization. Catalog
lowering and the complete decision/frame producer bridge remain separate.
"""
from __future__ import annotations

from qualification_controls import ControlRefused, run_controls
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import zipfile
from v2_proof_sources import execution_sources, adjust_specimen_source_lengths
from v2_native_dependencies import native_dependency_args, native_test_environment
import check_verus as verifier
from check_finite_execution import once
from check_metered_execution import UNIT_SOURCES as METER_SOURCES
from verus_coverage import profile_inventory, require_coverage

ROOT = Path(__file__).resolve().parents[1]
HARNESS = Path('verification/verus/laws.rs')
PROFILE = Path('verification/verus/laws-v2.json')
SUBJECT = Path('crates/zeno-fcis-synthesis/src/finite/execution_v2/laws.rs')
DIRECTORY = SUBJECT.parent / 'laws'
UNIT_SOURCES = (HARNESS, *METER_SOURCES[1:], SUBJECT,
                *(DIRECTORY / name for name in ('types.rs','atoms.rs','frame.rs','predicate.rs','spec.rs')))

UNIT_SOURCES = tuple(dict.fromkeys((*UNIT_SOURCES, *execution_sources(ROOT))))
SOURCES = (*UNIT_SOURCES, PROFILE, verifier.PIN, DIRECTORY/'tests.rs',
           SUBJECT.parent/'tests.rs', SUBJECT.parent/'input_view/tests.rs',SUBJECT.parent/'composition/record_tests.rs',
           Path('tools/check_laws_v2.py'),Path('tools/test_check_laws_v2.py'),
           Path('tools/check_verus.py'),Path('tools/verus_coverage.py'),Path('tools/check_finite_execution.py'),Path('tools/check_metered_execution.py'),
           Path('README.md'),Path('crates/zeno-fcis-synthesis/tests/v2_laws.rs'),
           Path('crates/zeno-fcis-synthesis/src/finite/mod.rs'),Path('Cargo.toml'),Path('rust-toolchain.toml'))

SOURCES = tuple(dict.fromkeys((*SOURCES, Path("tools/v2_proof_sources.py"),
                                Path("verification/verus/authority_v2_sources.json"))))

SOURCES = tuple(dict.fromkeys((*SOURCES, Path("tools/v2_native_dependencies.py"))))
SOURCES = tuple(dict.fromkeys((*SOURCES, *verifier.QUALIFICATION_SOURCES)))

def snapshot():
    for path in SOURCES:
        if (ROOT/path).is_symlink():
            raise RuntimeError(f'proof source is a symbolic link: {path}')
    # A new production source must not silently escape this checked profile.
    expected={SUBJECT,*(p for p in SOURCES if p.parent==DIRECTORY)}
    actual={SUBJECT,*(p.relative_to(ROOT) for p in (ROOT/DIRECTORY).rglob('*.rs'))}
    if expected != actual:
        raise RuntimeError('law source coverage changed')
    return {str(path):verifier.digest(ROOT/path) for path in SOURCES}

def mutation_sources():
    texts={p:(ROOT/p).read_text() for p in (SUBJECT,DIRECTORY/'frame.rs',DIRECTORY/'predicate.rs',DIRECTORY/'atoms.rs',DIRECTORY/'spec.rs')}
    mutations={}
    anchors=[
        ('skip_applicable_law',SUBJECT,'Scope::Always => true,','Scope::Always => false,'),
        ('skip_mandatory_genesis',SUBJECT,'Frame::Genesis { .. } => law.genesis,','Frame::Genesis { .. } => false,'),
        ('omit_required_initial_family',SUBJECT,'&& has_kind(laws, 9)','&& true'),
        ('ignore_required_law',SUBJECT,'if required[r] == 0 || !has_id(laws, required[r]) {','if required[r] == 0 {'),
        ('allow_duplicate_law',SUBJECT,'if laws[j].id == laws[i].id {','if false {'),
        ('change_declared_law_order',SUBJECT,'predicate::evaluate(&laws[i].program, frame, laws[i].id, meter, reads)','predicate::evaluate(&laws[0].program, frame, laws[i].id, meter, reads)'),
        ('reset_shared_meter',SUBJECT,'    if !metadata(laws, required) {','    meter.used.counters = [0; 8];\n    if !metadata(laws, required) {'),
        ('shrink_delivery_ordinals',DIRECTORY/'frame.rs','if (i > 0 && deliveries[i - 1].ordinal >= deliveries[i].ordinal)','if deliveries[i].ordinal as usize != i'),
        ('wrong_original_source',DIRECTORY/'frame.rs','Observation::Pre(id) => view_field(*pre, id),','Observation::Pre(id) => view_field(*command, id),'),
        ('wrong_complete_candidate',DIRECTORY/'frame.rs','Observation::Post(id) => view_field(c.post, id),','Observation::Post(id) => view_field(*pre, id),'),
        ('alter_class_observation',DIRECTORY/'frame.rs','Class::CommittedFailure => 2,','Class::CommittedFailure => 1,'),
        ('alter_reason_observation',DIRECTORY/'frame.rs','let id = c.reason?;\n                Some(Atom::I128(id as i128))','let id = c.reason?;\n                Some(Atom::I128(0))'),
        ('genesis_fixture_substitution',DIRECTORY/'frame.rs','view_field(*initial, id)','Some(Atom::I128(0))'),
        ('wrong_outbox_payload',DIRECTORY/'frame.rs','field(c.outbox[i].payload, id)','field(c.effects[i].payload, id)'),
        ('wrong_scalar_root_source',DIRECTORY/'frame.rs','Observation::CommandRoot => view_atom(*command),','Observation::CommandRoot => view_atom(*context),'),
        ('alias_scalar_root_with_record_field',DIRECTORY/'frame.rs','RootView::Record(fields) => field(fields, id),\n        RootView::Leaf(_) => None,','RootView::Record(fields) => field(fields, id),\n        RootView::Leaf(_) => Some(Atom::I128(0)),'),
        ('root_read_alias',DIRECTORY/'frame.rs','Selector::Root => 65536,','Selector::Root => 0,'),
        ('law_effect_lane',DIRECTORY/'frame.rs','if outbox { 3 } else { 2 }','if outbox { 2 } else { 3 }'),
        ('false_success',DIRECTORY/'predicate.rs','Atom::Bool(false) => Err(Failure::Violated),','Atom::Bool(false) => Ok(()),'),
        ('omit_step_charge',DIRECTORY/'predicate.rs','meter.charge(Resource::Step, 1)','meter.charge(Resource::Step, 0)'),
        ('omit_read_charge',DIRECTORY/'predicate.rs','meter.charge(Resource::Read, 1)','meter.charge(Resource::Read, 0)'),
        ('signed_arithmetic_substitution',DIRECTORY/'atoms.rs','match a.checked_mul(b) {','match a.checked_add(b) {'),
        ('wrong_division_rounding',DIRECTORY/'atoms.rs','signed(result, negative)','signed(result, !negative)'),
    ]
    for name,path,before,after in anchors:
        text=texts[path]
        if name=='signed_arithmetic_substitution':
            # Challenge signed multiplication only, preserving unsigned branch.
            if text.count(before)!=2: raise RuntimeError('multiplication anchors changed')
            changed=text.replace(before,after,1)
        elif name=='wrong_outbox_payload':
            # Keep indexing defensively in bounds: swap entire selected source.
            before='''Observation::OutboxPayload(i, id) => {
                if i < c.outbox.len() {
                    field(&c.outbox[i].payload, id)'''
            after='''Observation::OutboxPayload(i, id) => {
                if i < c.effects.len() {
                    field(&c.effects[i].payload, id)'''
            changed=once(text,before,after)
        else: changed=once(text,before,after)
        mutations[name]=(path,changed,'proof')
    predicate=texts[DIRECTORY/'predicate.rs']
    reordered=once(predicate,'let permission = meter.charge(Resource::Read, 1);',
        'let cached = super::frame::observe(frame, observation);\n            let permission = meter.charge(Resource::Read, 1);')
    reordered=once(reordered,'match super::frame::observe(frame, observation) {','match cached {')
    mutations['observe_before_read_permission']=(DIRECTORY/'predicate.rs',reordered,'coverage')
    source=texts[SUBJECT]
    end=source.index('pub fn evaluate<')
    start=source.rindex('#[cfg_attr(verus_keep_ghost, verus_spec(result =>',0,end)
    # Weakening the postcondition makes its existing reveals unused. Suppress
    # that specific ghost-only warning in this specimen, keeping stderr strict.
    mutations['weaken_entry_contract']=(SUBJECT,source[:start]
        +'#[cfg_attr(verus_keep_ghost, verifier::allow(dead_reveal))]\n'
        +'#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures true,))]\n'
        +source[end:],'coverage')
    mutations['narrow_entry_domain']=(SUBJECT,once(source,'ensures result.view()==law_execution(',
        'requires required@.len() == 0,\n    ensures result.view()==law_execution('),'coverage')
    mutations['uncontracted_executable']=(SUBJECT,source+'\npub fn uncontracted_law_probe() -> bool { true }\n','coverage')
    mutations['changed_specification']=(DIRECTORY/'spec.rs',once(texts[DIRECTORY/'spec.rs'],
        'if v<0{-v}else{v}','if 0>v{-v}else{v}'),'coverage')
    return mutations

SEMANTIC_FUNCTIONS = {
    'skip_applicable_law': 'applies', 'skip_mandatory_genesis': 'applies',
    'omit_required_initial_family': 'metadata', 'ignore_required_law': 'metadata',
    'allow_duplicate_law': 'metadata', 'change_declared_law_order': 'evaluate_into',
    'reset_shared_meter': 'evaluate_into', 'shrink_delivery_ordinals': 'deliveries_valid',
    'wrong_original_source': 'observe', 'wrong_complete_candidate': 'observe',
    'alter_class_observation': 'observe', 'alter_reason_observation': 'observe',
    'genesis_fixture_substitution': 'observe', 'wrong_outbox_payload': 'observe',
    'wrong_scalar_root_source': 'observe', 'root_read_alias': 'selector_id',
    'law_effect_lane': 'effect_entry', 'alias_scalar_root_with_record_field': 'view_field',
    'false_success': 'evaluate', 'omit_step_charge': 'node', 'omit_read_charge': 'node',
    'signed_arithmetic_substitution': 'binary', 'wrong_division_rounding': 'divide',
}

COVERAGE_FIELDS = {
    'observe_before_read_permission': ('laws::predicate::node', 'body_sha256'),
    'weaken_entry_contract': ('laws::evaluate', 'ensures_sha256'),
    'narrow_entry_domain': ('laws::evaluate', 'requires_sha256'),
    'uncontracted_executable': ('laws::uncontracted_law_probe', 'inventory'),
    'changed_specification': ('laws::spec::magnitude', 'body_sha256'),
}

def semantic_refusal(proc, report, name, path, changed, specimen, pin, profile):
    region = verifier.function_lines(changed, SEMANTIC_FUNCTIONS[name])
    return verifier.semantic_refusal(proc, report, pin, path, specimen,
        region=region, target_functions=profile['target_functions'])

def coverage_refusal(vir, profile, name):
    error = None
    try:
        require_coverage(vir, profile)
    except ValueError as failure:
        error = str(failure)
    actual = profile_inventory(vir, profile)
    suffix, field = COVERAGE_FIELDS[name]
    target = profile['namespace'] + 'execution_v2::' + suffix
    expected = profile['functions']
    if field == 'inventory':
        matched = (set(actual) == set(expected) | {target} and target not in expected
            and actual[target]['mode'] == 'Exec' and actual[target]['requires'] == 0
            and actual[target]['ensures'] == 0)
    else:
        matched = (set(actual) == set(expected) and target in actual and target in expected
            and actual[target][field] != expected[target][field])
        if name == 'observe_before_read_permission':
            matched = matched and all(actual[target][key] == expected[target][key]
                for key in ('signature_sha256', 'requires_sha256', 'ensures_sha256'))
    return matched and error is not None and target in error, error

def native_checks(directory,pin,environment):
    environment = native_test_environment(ROOT, environment)
    directory.mkdir(parents=True,exist_ok=True)
    rust=['rustc',f"+{pin['runtime_rust']}",'--edition=2024','--check-cfg','cfg(verus_keep_ghost)','--check-cfg','cfg(test)']
    rust += native_dependency_args(ROOT, directory, environment)
    executable=directory/'native-laws'
    build=verifier.run([*rust,'--test',str(HARNESS),'-o',str(executable)],ROOT,environment)
    (directory/'build.stderr').write_text(build.stderr);verifier.require_success(build)
    native=verifier.run([str(executable),'--test-threads=1'],ROOT,environment)
    (directory/'tests.stdout').write_text(native.stdout);verifier.require_success(native)
    library=directory/'liblaws.rlib'
    verifier.require_success(verifier.run([*rust,'--crate-type=rlib','--crate-name=laws',str(HARNESS),'-o',str(library)],ROOT,environment))
    public=(ROOT/'crates/zeno-fcis-synthesis/tests/v2_laws.rs').read_text().replace('zeno_fcis_synthesis::finite::','::laws::')
    # This consumer executes actual library code through the external API.
    public_path=directory/'public.rs';public_path.write_text(public)
    verifier.require_success(verifier.run([*rust,'--test','--extern',f'laws={library}',str(public_path),'-o',str(directory/'public')],ROOT,environment))
    verifier.require_success(verifier.run([str(directory/'public'),'--test-threads=1'],ROOT,environment))
    specimens={
        'private_meter':('fn main(){let _=laws::execution_v2::meter::new;}','E0603'),
        'private_composition':('fn main(){let _=laws::execution_v2::laws::evaluate_into;}','E0603'),
        'forge_success':('fn main(){let _=laws::execution_v2::laws::Outcome{result:Ok(()),usage:panic!(),diagnostics:vec![],reads:vec![]};}','E0451'),
        'forge_usage':('fn main(){let _=laws::execution_v2::Usage{counters:[0;8]};}','E0451'),
    }
    results={}
    for name,(source,error) in specimens.items():
        path=directory/f'{name}.rs';path.write_text(source)
        result=verifier.run([*rust,'--extern',f'laws={library}',str(path),'-o',str(directory/name)],ROOT,environment)
        (directory/f'{name}.stderr').write_text(result.stderr)
        if result.returncode==0 or error not in result.stderr:raise RuntimeError(f'custody negative {name} did not refuse for {error}')
        results[name]={'exit_code':result.returncode,'expected_error':error}
    return {'build_exit':build.returncode,'exit_code':native.returncode,'test_output':native.stdout,'public_consumer_exit':0,'api_refusals':results}

def check(cache,install,evidence):
    before=snapshot();pin=json.loads((ROOT/verifier.PIN).read_text());profile=json.loads((ROOT/PROFILE).read_text())
    tools=verifier.prepare_tools(cache,pin,install)
    env={k:v for k,v in os.environ.items() if not k.startswith(('VERUS_','VARGO_')) and k not in ('RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS')}
    env.update(RUSTUP_TOOLCHAIN=pin['rust_toolchain'],VERUS_Z3_PATH=str(tools/'z3'),CARGO_BUILD_JOBS='1',RUST_TEST_THREADS='1')
    command=[str(tools/'verus'),'--crate-type=lib','--edition=2024','--no-cheating','--no-external-by-default','--num-threads','2', "-V", "spinoff-all",'--output-json','--triggers-mode','silent','--log','vir','--log','vir-option=no_span+no_type+no_fn_details']
    positive_command=[*command,'--log-dir',str(evidence/'positive'),str(HARNESS)]
    positive=verifier.run(positive_command,ROOT,env)
    (evidence/'positive.stdout.json').write_text(positive.stdout);(evidence/'positive.stderr').write_text(positive.stderr)
    verifier.require_success(positive);report=json.loads(positive.stdout)
    report_pin={**pin,**{key:profile[key] for key in ('expected_verified','target_functions')}}
    if not verifier.accepted(report,report_pin):raise RuntimeError('whole-source pinned verification report did not qualify')
    coverage=require_coverage((evidence/'positive/crate.vir').read_text(),profile)
    native=native_checks(evidence/'native',pin,env)
    print('laws: complete proof/coverage and native/custody passed',flush=True)
    mutations=[]
    def run_control(control_item):
        (name, (path, changed, expected)) = control_item
        specimen=evidence/name
        for unit in UNIT_SOURCES:
            target=specimen/unit;target.parent.mkdir(parents=True,exist_ok=True)
            target.write_text(changed) if unit==path else target.write_bytes((ROOT/unit).read_bytes())
        adjust_specimen_source_lengths(specimen)
        budget_args=['--rlimit','80'] if name=='shrink_delivery_ordinals' else []
        result=verifier.run([*command,*budget_args,'--log-dir',str(specimen/'logs'),str(HARNESS)],specimen,env)
        (specimen/'stdout.json').write_text(result.stdout);(specimen/'stderr').write_text(result.stderr)
        parsed=json.loads(result.stdout);actual=parsed.get('verification-results',{});coverage_error=None
        if expected=='proof':
            killed=semantic_refusal(result,parsed,name,path,changed,specimen,pin,profile)
        else:
            matched,coverage_error=coverage_refusal((specimen/'logs/crate.vir').read_text(),profile,name)
            killed=result.returncode==0 and verifier.proof_succeeded(parsed,pin) and not result.stderr.strip() and matched
        entry={'name':name,'expected_failure':expected,'killed':killed,'exit_code':result.returncode,'verification_results':actual,'coverage_refusal':coverage_error,'log_directory':str(specimen)}
        control_result = entry
        if not killed:raise ControlRefused(control_result, f'mutation {name} survived or failed for an unrelated reason; inspect {specimen}')
        print(f'laws: caught {name} ({expected})',flush=True)
        return control_result
    def publish_control(control_result):
        mutations.append(control_result)
        (evidence/'mutations.json').write_text(json.dumps(mutations,indent=2)+'\n')
    run_controls(run_control, mutation_sources().items(), timings=evidence / 'control-timings.json', publish=publish_control)
    verifier.verify_tool_files(tools,pin)
    if before!=snapshot():raise RuntimeError('source changed during law verification')
    return {'schema':'zeno-fcis/laws-v2-evidence/1','status':'passed','source_sha256':before, "source_bindings": verifier.evidence_bindings(before, HARNESS),'command':positive_command,'exit_code':positive.returncode,
            'verus_report':report,'translated_function_coverage':coverage,'native':native,'mutations':mutations,
            'evidence_directory':str(evidence),'revision':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
            'operational_order_evidence':'complete reviewed executable-body inventory; verifying observe-before-charge control refused by coverage',
            'trusted_base':['reviewed specification','Verus translation/erasure and bundled vstd/Z3','Rust compiler','allocation/platform'],
            'unproved':['catalog and .zeno lowering','complete candidate-to-frame producer correspondence','mandatory authority and replay','canonical hashing','physical-cost and shell behavior']}

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--cache',type=Path,default=verifier.cache_directory());p.add_argument('--install',action='store_true');p.add_argument('--out',required=True,type=Path);p.add_argument('--evidence-dir',type=Path)
    args=p.parse_args();args.out.parent.mkdir(parents=True,exist_ok=True)
    evidence=args.evidence_dir or Path(tempfile.mkdtemp(prefix='laws-gate-',dir=args.out.parent));evidence.mkdir(parents=True,exist_ok=True)
    try:receipt=check(args.cache.resolve(),args.install,evidence.resolve())
    except (OSError,ValueError,KeyError,RuntimeError,subprocess.SubprocessError,zipfile.BadZipFile) as error:
        receipt={'schema':'zeno-fcis/laws-v2-evidence/1','status':'failed','error':str(error),'evidence_directory':str(evidence)}
    args.out.write_text(json.dumps(receipt,indent=2,sort_keys=True)+'\n');print(f"laws: {receipt['status']}; {args.out}")
    if receipt['status']!='passed':print(receipt['error']);return 1
    return 0
if __name__=='__main__':raise SystemExit(main())
