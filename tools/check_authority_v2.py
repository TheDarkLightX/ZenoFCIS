#!/usr/bin/env python3
"""Check exact canonical artifacts on their complete registered source closure."""
from __future__ import annotations

from qualification_controls import ControlRefused, run_controls
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import check_verus as verifier
import evaluator_identity as evaluator
from verus_coverage import descriptor_inventory, inventory, require_coverage
from verus_coverage import observed_inventory
from v2_native_dependencies import native_dependency_args, prepare_native_dependencies, native_test_environment
from v2_proof_sources import NATIVE_TEST_DATA
ROOT=Path(__file__).resolve().parents[1]
HARNESS=Path('verification/verus/authority_v2.rs')
PROFILE=Path('verification/verus/authority_v2.json')
BASE=Path('crates/zeno-fcis-synthesis/src/finite')
SUBJECT=BASE/'execution_v2/authority.rs'
CANONICAL=SUBJECT.parent/'authority/canonical.rs'
UTIL=SUBJECT.parent/'util.rs'
CANDIDATE=SUBJECT.parent/'authority/candidate.rs'
OBSERVATIONS=SUBJECT.parent/'authority/observations.rs'
FRAMING=SUBJECT.parent/'authority/framing.rs'
SPEC=SUBJECT.parent/'authority/spec.rs'
METADATA=SUBJECT.parent/'authority/metadata.rs'
BOUND=SUBJECT.parent/'authority/bound.rs'
OUTCOME=SUBJECT.parent/'authority/outcome.rs'
POLICY=SUBJECT.parent/'authority/policy.rs'
DESCRIPTOR=SUBJECT.parent/'authority/descriptor.rs'
EVALUATOR=SUBJECT.parent/'authority/evaluator.rs'
SOURCE_MANIFEST=Path('verification/verus/authority_v2_sources.json')
PUBLIC_TEST=Path('crates/zeno-fcis-synthesis/tests/v2_authority.rs')
APPROVED_SOURCE_CLOSURE_SHA256='3c42a44638e78b8a8d3bb88374b79900a0a961fb28cf3a494ece661c2626b167'
# These command-substitution controls need more than the default
# solver budget. Exhaustion still refuses qualification; it is not a kill.
CONTROL_RLIMITS={'sealed_command':80,'stage_usage':80,'replay_original':80}

SEMANTIC_TARGETS = {
    'word_byte_order': 'word', 'length_header': 'encoded_size',
    'byte_mismatch': 'bytes_equal', 'signed_value': 'atom',
    'decision_class': 'class_tag', 'reason': 'candidate_parts',
    'pre_state': 'candidate_parts', 'patch_before': 'patches',
    'effect_outbox_order': 'candidate_parts', 'usage_counter': 'append_usage',
    'law_observation': 'append_observation', 'invocation_state': 'subject_bytes',
    'genesis_tag': 'subject_bytes', 'metadata_variant_code': 'variants',
    'metadata_branch_reason': 'branches', 'metadata_delivery_idempotency': 'deliveries',
    'metadata_law_scope': 'law_list', 'metadata_law_rounding': 'division',
    'metadata_graph_roots': 'program', 'metadata_limits': 'limits',
    'replay_comparison': 'compare', 'candidate_after_refusal': 'result',
    'sealed_command': 'seal', 'sealed_initial': 'genesis',
    'stage_usage': 'seal', 'root_read_selector': 'append_reads',
    'policy_channel_destination': 'policy_bytes', 'policy_channel_count': 'policy_bytes',
    'frame_limit': 'framing_bytes', 'unframed_evaluation': 'evaluate',
    'replay_original': 'replay', 'replay_genesis_original': 'replay_genesis',
    'sealed_law_reports': 'seal', 'core_refusal_order': 'seal',
    'constructor_policy_substitution': 'bind', 'constructor_source_omission': 'bind',
    'constructor_framing_substitution': 'bind',
}

def source_manifest(path):
    if verifier.digest(path)!=APPROVED_SOURCE_CLOSURE_SHA256:
        raise ValueError('source closure differs from root-approved paths and fixed pin bytes')
    return evaluator.manifest(ROOT,path)

def generate_evaluator(path):
    raw=source_manifest(path)
    target=ROOT/EVALUATOR
    target.write_text(evaluator.render(ROOT,raw))
    return {'generated_file':str(EVALUATOR),'approved_manifest_sha256':APPROVED_SOURCE_CLOSURE_SHA256}

def released_24_source_extension(raw, copies):
    """The sole historical extension: retain every approved path and build pin."""
    if copies.get('subject') != '8c444487f96d6acd14c9f7fa55d9ba88ca780fb6' or copies.get('tag') != 'v2.4.0':
        raise ValueError('only the released 2.4.0 source closure is supported')
    additions = [row['copy'] for row in copies['files']]
    additions += [row['path'] for row in copies['binding_sources']]
    additions += [
        'release/historical-24-copy.json',
        'crates/zeno-fcis-synthesis/src/finite/historical_24_sources.rs',
        *[f'crates/zeno-fcis-contract/src/internals/{name}.rs' for name in ('model', 'graph', 'policy')],
    ]
    if not all(evaluator.safe_name(path) for path in additions):
        raise ValueError('unsafe historical source path')
    return {**raw, 'paths': sorted(set(raw['paths']) | set(additions))}

VALUE_DECODER_SOURCES = (
    'crates/zeno-fcis-codec/src/decoder_bytes.rs',
    'crates/zeno-fcis-codec/src/decoder_induction.rs',
    'crates/zeno-fcis-codec/src/decoder_refinement.rs',
    'crates/zeno-fcis-codec/src/decoder_spec.rs',
    'crates/zeno-fcis-value/src/decoder_bytes.rs',
    'crates/zeno-fcis-value/src/decoder_model.rs',
    'crates/zeno-fcis-value/src/decoder_refinement.rs',
)

def value_decoder_source_extension(raw):
    """Add exactly the seven decoder modules; retain every source and fixed pin."""
    return {**raw, 'paths': sorted(set(raw['paths']) | set(VALUE_DECODER_SOURCES))}

def include_value_decoder_sources():
    return _include_source_extension(value_decoder_source_extension(
        source_manifest(ROOT/SOURCE_MANIFEST)))

def include_released_24_sources():
    """Generate both membership guards from the checked additive release copy.

    This records a closed membership change for review. It cannot omit an
    approved source or alter any runtime, verifier or core-profile pin bytes.
    The usual source preflight still refuses any subsequent omission.
    """
    import check_historical_24_copy as historical
    path = ROOT/SOURCE_MANIFEST
    raw = source_manifest(path)
    copies = json.loads(historical.RECORD.read_text())
    historical.check(copies)
    return _include_source_extension(released_24_source_extension(raw, copies))

def _include_source_extension(extended):
    """Generate both guards only after retaining all old membership and pins."""
    path = ROOT/SOURCE_MANIFEST
    raw = source_manifest(path)
    if (set(extended) != set(raw) or extended['pins'] != raw['pins']
            or extended['schema'] != raw['schema']
            or not set(raw['paths']) <= set(extended['paths'])
            or extended['paths'] != sorted(set(extended['paths']))
            or not all(evaluator.safe_name(name) for name in extended['paths'])):
        raise ValueError('source extension must preserve approved paths and fixed pin bytes')
    import hashlib
    test = ROOT/'crates/zeno-fcis-synthesis/tests/v2_evaluator_identity.rs'
    text = test.read_text()
    pattern = r'(let approved_manifest = \[)(.*?)(\n    \];)'
    match = re.search(pattern, text, re.DOTALL)
    if match is None:
        raise ValueError('missing independent Rust membership guard')
    old = bytes(int(value) for value in re.findall(r'(\d+)_u8', match[2]))
    if old.hex() != APPROVED_SOURCE_CLOSURE_SHA256:
        raise ValueError('independent membership guard differs before generation')
    for name in extended['paths']: evaluator.read_regular(ROOT/name)
    encoded = (json.dumps(extended, indent=2, sort_keys=True)+'\n').encode()
    value = hashlib.sha256(encoded).digest()
    rows = ''.join('\n        '+', '.join(f'{byte}_u8' for byte in value[start:start+12])+','
                   for start in range(0, 32, 12))
    rust = text[:match.start(2)]+rows+text[match.end(2):]
    own_path = Path(__file__)
    own = own_path.read_text()
    before = "APPROVED_SOURCE_CLOSURE_SHA256='"+APPROVED_SOURCE_CLOSURE_SHA256+"'"
    if own.count(before) != 1:
        raise ValueError('membership generator guard is ambiguous')
    own = own.replace(before, "APPROVED_SOURCE_CLOSURE_SHA256='"+value.hex()+"'")
    path.write_bytes(encoded)
    test.write_text(rust)
    own_path.write_text(own)
    return {'previous_manifest_sha256': old.hex(), 'manifest_sha256': value.hex(),
            'added_paths': sorted(set(extended['paths'])-set(raw['paths'])),
            'removed_paths': [], 'build_pins_unchanged': extended['pins']==raw['pins'],
            'review_required': True}

def check_generated_sources():
    raw=source_manifest(ROOT/SOURCE_MANIFEST)
    if evaluator.read_regular(ROOT/EVALUATOR).decode()!=evaluator.render(ROOT,raw):
        raise ValueError('compiled evaluator digest differs from approved source generation')
    return {'approved_manifest_sha256':APPROVED_SOURCE_CLOSURE_SHA256,'generated_sha256':verifier.digest(ROOT/EVALUATOR),'paths':len(raw['paths']),'pins':len(raw['pins'])}

def adjust_specimen_source_lengths(specimen):
    # Evaluator is deliberately frozen in behavioral mutation specimens.
    return []

def sources():
    closure=sorted(p.relative_to(ROOT) for d in ('evaluation','canonical_v2','execution_v2') for p in (ROOT/BASE/d).rglob('*.rs'))
    paths=[Path("tools/v2_native_dependencies.py"),Path("tools/v2_proof_sources.py"),*NATIVE_TEST_DATA,HARNESS,PUBLIC_TEST,Path("crates/zeno-fcis-synthesis/tests/v2_evaluator_identity.rs"),Path("verification/verus/evaluator_encoding_vector.txt"),*closure,Path('crates/zeno-fcis-cli/templates/order-fulfillment/synthesized/transition.rs'),PROFILE,verifier.PIN,
        Path('tools/check_authority_v2.py'),Path('tools/test_check_authority_v2.py'),Path('tools/evaluator_identity.py'),Path('tools/test_evaluator_identity.py'),Path('tools/check_evaluator_replay.py'),Path('tools/check_verus.py'),Path('tools/verus_coverage.py'),Path('tools/check_decision_v2.py'),Path('tools/check_metered_execution.py'),Path('tools/check_finite_execution.py'),Path('README.md')]
    if (ROOT/SOURCE_MANIFEST).exists():
        names=source_manifest(ROOT/SOURCE_MANIFEST)['paths']
        test_manifest=Path('verification/verus/authority_v2_test_sources.json')
        tests=json.loads(evaluator.read_regular(ROOT/test_manifest))
        if (set(tests)!={'schema','paths'} or tests['schema']!='zeno-fcis/authority-test-sources/1'
                or not isinstance(tests['paths'],list) or not tests['paths']
                or not all(evaluator.safe_name(n) for n in tests['paths'])
                or tests['paths']!=sorted(set(tests['paths']))):
            raise ValueError('invalid native authority test source closure')
        for name in tests['paths']:evaluator.read_regular(ROOT/name)
        paths.extend([SOURCE_MANIFEST,EVALUATOR,test_manifest,*map(Path,names),*map(Path,tests['paths'])])
    return list(dict.fromkeys([*paths, *verifier.QUALIFICATION_SOURCES]))

def snapshot(skip_profile=False):
    paths=[p for p in sources() if not(skip_profile and p==PROFILE)]
    if any((ROOT/p).is_symlink() for p in paths):raise RuntimeError('symlink source')
    return {str(p):verifier.digest(ROOT/p) for p in paths}

def mutations():
    from check_decision_v2 import once
    controls={}
    for name,path,before,after,native in [
        ('word_byte_order',CANONICAL,'(value >> 120u32) as u8,','(value >> 0u32) as u8,',True),
        ('length_header',CANONICAL,'size.checked_add(17)','size.checked_add(16)',True),
        ('byte_mismatch',UTIL,'if left[i]!=right[i] {return false;}','if left[i]!=right[i] {return true;}',True),
        ('signed_value',CANDIDATE,'Atom::I128(v)=>{parts.push(Part::Word(1));parts.push(Part::Word(v as u128));}','Atom::I128(v)=>{parts.push(Part::Word(1));parts.push(Part::Word(0));}',True),
        ('decision_class',CANDIDATE,'Class::CommittedFailure=>2','Class::CommittedFailure=>1',True),
        ('reason',CANDIDATE,'parts.push(Part::Word(reason as u128));','parts.push(Part::Word(0));',True),
        ('pre_state',CANDIDATE,'fields(&mut parts,candidate.pre());','fields(&mut parts,candidate.post());',True),
        ('patch_before',CANDIDATE,'atom(parts,values[i].before);','atom(parts,values[i].after);',True),
        ('effect_outbox_order',CANDIDATE,'deliveries(&mut parts,candidate.outbox());','deliveries(&mut parts,candidate.effects());',True),
        ('usage_counter',OBSERVATIONS,'usage.used(Resource::Step)','usage.used(Resource::Read)',True),
        ('law_observation',OBSERVATIONS,'laws::Observation::OutboxPayload(a,b)=>{parts.push(Part::Word(28));','laws::Observation::OutboxPayload(a,b)=>{parts.push(Part::Word(27));',True),
        ('invocation_state',FRAMING,'Part::Bytes(identity),Part::Bytes(state),','Part::Bytes(identity),Part::Bytes(command),',True),
        ('genesis_tag',FRAMING,'Kind::Genesis=>0,Kind::Transition=>1','Kind::Genesis=>1,Kind::Transition=>1',True),
        ('metadata_variant_code',METADATA,'parts.push(Part::Word(values[i].id as u128));parts.push(Part::Word(values[i].code as u128));','parts.push(Part::Word(values[i].id as u128));parts.push(Part::Word(0));',True),
        ('metadata_branch_reason',METADATA,'reason(parts,&values[i].reason);','reason(parts,&None);',True),
        ('metadata_delivery_idempotency',METADATA,'expr(parts,&values[i].idempotency);','expr(parts,&values[i].destination);',True),
        ('metadata_law_scope',METADATA,'law_scope(parts,&values[i].scope);','law_scope(parts,&laws::Scope::Always);',True),
        ('metadata_law_rounding',METADATA,'laws::Division::Ceil=>{parts.push(Part::Word(2));}','laws::Division::Ceil=>{parts.push(Part::Word(1));}',True),
        ('metadata_graph_roots',METADATA,'roots(parts,value.roots);','roots(parts,&[]);',True),
        ('metadata_limits',METADATA,'value.limit(Resource::Step)','value.limit(Resource::Read)',True),
        ('replay_comparison',BOUND,'if canonical::exact(&bytes,expected)','if true',True),
        ('candidate_after_refusal',OUTCOME,'match self.subject{Err(e)=>Err(e),Ok(_)=>match self.outcome.result(){Ok(c)=>Ok(c),Err(e)=>Err(Refusal::Core(e))}}','match self.outcome.result(){Ok(c)=>Ok(c),Err(e)=>Err(Refusal::Core(e))}',True),
        # genesis_after_refusal retired in S5: genesis and transition share Evaluation::result, covered by candidate_after_refusal.
        ('sealed_command',OUTCOME,'seal_bytes(kind,identity,raw.state,raw.command,raw.context,&artifact,)','seal_bytes(kind,identity,raw.state,raw.state,raw.context,&artifact,)',True),
        ('sealed_initial',BOUND,'Kind::Genesis,Raw{state:original,command:&[],context:&[]},&self.framing','Kind::Genesis,Raw{state:&[],command:&[],context:original},&self.framing',True),
        ('stage_usage',OUTCOME,'observations::append_optional_usage(&mut parts,outcome.decision_usage());','observations::append_optional_usage(&mut parts,outcome.ingress_usage());',True),
        ('root_read_selector',OUTCOME,'Selector::Root=>parts.push(Part::Word(0))','Selector::Root=>parts.push(Part::Word(1))',True),
        ('policy_channel_destination',POLICY,'parts.push(Part::Word(v.1 as u128));','parts.push(Part::Word(v.2 as u128));',True),
        ('policy_channel_count',POLICY,'Part::Word(channel_roots.len() as u128)];','Part::Word(0)];',True),
        ('frame_limit',BOUND,'Part::Word(config.state.max_bytes as u128),','Part::Word(config.context.max_bytes as u128),',True),
        ('unframed_evaluation',BOUND,'self.core.frame_family(Kind::Transition,original,&self.framing)','self.core.execute_family(Kind::Transition,original)',True),
        ('replay_original',BOUND,'let evaluated=self.evaluate(original);\n        let subject=compare(evaluated.subject,expected);','let evaluated=self.evaluate(Raw{state:original.state,command:original.state,context:original.context});\n        let subject=compare(evaluated.subject,expected);',True),
        ('replay_genesis_original',BOUND,'let evaluated=self.genesis(original);\n        let subject=compare(evaluated.subject,expected);','let evaluated=self.genesis(&[]);\n        let subject=compare(evaluated.subject,expected);',True),
        ('sealed_law_reports',OUTCOME,'observations::append_decision_attempts(&mut parts,outcome.decision_attempts());\n    observations::append_diagnostics(&mut parts,outcome.diagnostics());observations::append_law_reads(&mut parts,outcome.law_reads());','observations::append_decision_attempts(&mut parts,outcome.decision_attempts());\n    observations::append_diagnostics(&mut parts,&[]);observations::append_law_reads(&mut parts,&[]);',True),
        ('core_refusal_order',OUTCOME,'let candidate=match outcome.result(){Ok(c)=>c,Err(e)=>return Err(Refusal::Core(e))};','let candidate=match outcome.result(){Ok(c)=>c,Err(_e)=>return Err(Refusal::ReplayMismatch)};',True),
    ]:
        source=(ROOT/path).read_text()
        if path==POLICY:
            # The legacy and numeric policy encoders both contain these fields.
            # Preserve the legacy control's exact subject.
            numeric_start=source.rfind('#[cfg_attr',0,source.index('pub fn numeric_policy_bytes('))
            changed=once(source[:numeric_start],before,after)+source[numeric_start:]
        else:
            changed=once(source,before,after)
        controls[name]=(path,changed,'proof',native)
    source=(ROOT/CANONICAL).read_text()
    # A public replay method is not a callee of the proof harness. Weakening its
    # exact contract must verify and then fail that function's coverage record.
    controls['weak_exact_contract']=(BOUND,once((ROOT/BOUND).read_text(),
        'ensures model::transition(self.view(),original,result.view(),Some(expected@)),\n        result.view().1.is_ok() ==> result.view().1.unwrap()==expected@,',
        'ensures true,'),'coverage',False)
    controls['uncontracted_helper']=(CANONICAL,source+'\npub fn unchecked_authority_probe()->u64 {7}\n','coverage',False)
    changed=once(source,'let size = encoded_size(parts)?;\n    let mut out = Vec::with_capacity(size);',
        'let mut out = Vec::new();')
    changed=once(changed,'model::length(parts@,parts@.len())<=usize::MAX,','')
    changed=once(changed,'Some(out)','encoded_size(parts)?;\n    Some(out)')
    controls['serialize_before_size_check']=(CANONICAL,changed,'coverage',False)
    bound=(ROOT/BOUND).read_text()
    for name,before,after in [
        ('constructor_policy_substitution','framing::identity_bytes(catalog.original_contract(),&evaluator::EVALUATOR)','framing::identity_bytes(catalog.original_schema(),&evaluator::EVALUATOR)'),
        ('constructor_source_omission','&evaluator::EVALUATOR','&[]'),
        ('constructor_framing_substitution','let framing=*catalog.framing();','let framing=Framing{state:catalog.framing().command,command:catalog.framing().state,context:catalog.framing().context};'),
    ]:controls[name]=(BOUND,once(bound,before,after),'proof',True)
    controls['weak_constructor_contract']=(BOUND,once(bound,
        '(match result{Ok(a)=>Ok(a.view()),Err(e)=>Err(e)}) ==\n        model::bind_value(catalog),','true,'),'coverage',False)
    constructor_start=bound.rfind('#[cfg_attr',0,bound.index("pub fn bind<'p, D:"))
    constructor_end=bound.index("\n\nimpl<'p, D:",constructor_start)
    # The synthetic bypass keeps its signature, contract and empty roots. Its checked
    # extensional assertion only lets the bypass prove its own contract; no assumption.
    raw_constructor="""#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures
    (match result{Ok(a)=>Ok(a.view()),Err(e)=>Err(e)}) ==
    (if composition::descriptor_admitted(descriptor){Ok((descriptor,framing,identity@,Seq::empty()))}
     else{Err(Refusal::Core(composition::Failure::Metadata))}),
))]
pub fn bind<'p>(descriptor:&'p Descriptor<'p>,framing:Framing,identity:Vec<u8>)->Result<Authority<'p>,Refusal>{
    let core=match composition::bind(descriptor){Ok(core)=>core,Err(e)=>return Err(Refusal::Core(e))};
    let channel_roots:&[(u32,u32,u32)]=&[];
    #[cfg(verus_keep_ghost)]proof!{reveal(Authority::view);assert(channel_roots@=~=Seq::<(u32,u32,u32)>::empty());}
    Ok(Authority{core,framing,identity,channel_roots})
}
"""
    controls['unchecked_constructor_bypass']=(BOUND,bound[:constructor_start]+raw_constructor+bound[constructor_end:],'coverage',False)
    return controls

def coverage_control(vir,profile,name):
    """Require the intended negative; embedded-source drift is insufficient."""
    refusal=None
    try:require_coverage(vir,profile)
    except ValueError as e:refusal=str(e)
    if refusal is None:return False,None,None
    if 'abstract_declarations' in profile:
        actual,_declarations=descriptor_inventory(vir,profile['namespace'],
            profile['namespace']+'execution_v2::composition::descriptor::',tuple(profile['body_covered_functions']))
    else:
        actual=inventory(vir,profile['namespace'],tuple(profile['body_covered_functions']))
    expected=profile['functions']
    # require_coverage reports its first mismatch. Removing contract reads can
    # shift compiler read IDs in an earlier descriptor record, so establish the
    # intended refusal from the exact inventory and target field, not that text.
    prefix='authority_v2::execution_v2::authority::'
    if name=='uncontracted_helper':
        target=prefix+'canonical::unchecked_authority_probe'
        intended=(set(actual)==set(expected)|{target} and target not in expected
            and actual[target]['mode']=='Exec'
            and actual[target]['requires']==0 and actual[target]['ensures']==0)
        field='inventory'
    else:
        relative,field={
            'weak_exact_contract':('bound::impl&%0::replay','ensures_sha256'),
            'serialize_before_size_check':('canonical::encode','body_sha256'),
            'weak_constructor_contract':('bound::bind','ensures_sha256'),
            'unchecked_constructor_bypass':('bound::bind','signature_sha256'),
        }[name]
        target=prefix+relative
        intended=(set(actual)==set(expected) and target in actual and target in expected
            and actual[target][field]!=expected[target][field])
        if name=='serialize_before_size_check':
            intended=intended and all(actual[target][f]==expected[target][f] for f in ('signature_sha256','requires_sha256','ensures_sha256'))

    return intended,refusal,{'function':target,'field':field,'intended_refusal':intended}

def semantic_refusal(proc,report,path,specimen=ROOT,*,region=None,target_functions=()):
    """A timeout, type error or unrelated solver failure is not a killed control."""
    pin=json.loads((ROOT/verifier.PIN).read_text())
    # This pin also names entry invariants this way; raw logs remain unchanged.
    stderr=re.sub(r'^error: invariant not satisfied before loop$',
        'error: invariant not satisfied',proc.stderr,flags=re.M)
    proc=subprocess.CompletedProcess(proc.args,proc.returncode,proc.stdout,stderr)
    return verifier.semantic_refusal(proc,report,pin,path,specimen,
        region=region,target_functions=target_functions)

def custody_controls(out,rust,env):
    """Compile a usable external client, then reject attempts to forge private authority."""
    prefix='extern crate authority_v2 as api;\nuse api::execution_v2::{authority as a,composition as c};\n'
    cases=[
        ('external_client',"pub fn construct<'p>(catalog:&api::execution_v2::catalog::BoundCatalog<'p>)->Result<a::Authority<'p>,a::Refusal> {a::bind(catalog)}\npub fn inspect(authority:&a::Authority<'_>)->usize {authority.identity().len()}",'',()),
        ('forge_authority',"pub fn forge<'p>(core:c::BoundCore<'p>,framing:c::Framing)->a::Authority<'p> {a::Authority{core,framing,identity:Vec::new(),channel_roots:&[]}}",'E0451',('core','framing','identity')),
        ('forge_evaluation',"pub fn forge<'a>(outcome:c::Outcome<'a>)->a::Evaluation<'a> {a::Evaluation{outcome,subject:Ok(Vec::new())}}",'E0451',('outcome','subject')),
        ('forge_genesis',"pub fn forge<'a>(outcome:c::Outcome<'a>)->a::Evaluation<'a> {a::Evaluation{outcome,subject:Ok(Vec::new())}}",'E0451',('outcome','subject')),
        ('raw_sealer',"pub fn forge(outcome:&c::Outcome<'_>)->Result<Vec<u8>,a::Refusal> {a::outcome::seal(outcome,&[])}",'E0603',('outcome','private')),
        ('replace_identity',"pub fn replace(authority:&mut a::Authority<'_>) {authority.identity=Vec::new();}",'E0616',('identity','private')),
        ('replace_subject',"pub fn replace(evaluation:&mut a::Evaluation<'_>) {evaluation.subject=Ok(Vec::new());}",'E0616',('subject','private')),
        ('unchecked_descriptor',"pub fn construct<'p>(descriptor:&'p c::Descriptor<'p>)->Result<a::Authority<'p>,a::Refusal> {a::bind(descriptor)}",'E0308',('mismatched types',)),
        ('supplied_identity',"pub fn construct<'p>(catalog:&api::execution_v2::catalog::BoundCatalog<'p>,identity:&[u8])->Result<a::Authority<'p>,a::Refusal> {a::bind(catalog,identity)}",'E0061',('takes 1 argument',)),
        ('private_evaluator_module',"pub fn read_digest() {let _=a::evaluator::EVALUATOR;}",'E0603',('evaluator','private')),
        ('mutable_descriptor',"pub fn replace(authority:&mut a::Authority<'_>) {authority.descriptor().decision_output=7;}",'E0594',('cannot assign',)),
        ('mutable_identity',"pub fn replace(authority:&mut a::Authority<'_>) {authority.identity()[0]=7;}",'E0594',('cannot assign',)),

    ]
    directory=out/'custody';directory.mkdir();results=[]
    for name,body,code,terms in cases:
        path=directory/(name+'.rs');path.write_text(prefix+body+'\n')
        command=[*rust,'--crate-type=rlib','--error-format=json','--extern',f"authority_v2={out/'libauthority_v2.rlib'}",str(path),'-o',str(directory/(name+'.rlib'))]
        proc=verifier.run(command,ROOT,env,timeout=180)
        (directory/(name+'.stdout')).write_text(proc.stdout);(directory/(name+'.stderr')).write_text(proc.stderr)
        errors=[json.loads(line) for line in proc.stderr.splitlines() if line.startswith('{')]
        matched=[e for e in errors if (e.get('code') or {}).get('code')==code and all(t in e.get('message','') for t in terms)]
        passed=proc.returncode==0 if not code else proc.returncode!=0 and bool(matched)
        row={'name':name,'passed':passed,'command':command,'exit':proc.returncode,'expected_code':code,'source_sha256':verifier.digest(path),'matched_diagnostics':matched};results.append(row)
        (directory/'receipt.json').write_text(json.dumps(results,indent=2,sort_keys=True)+'\n')
        if not passed:raise RuntimeError(f'custody control failed or unrelated refusal: {name}')
    return results

def candidate_profile(vir: str, report: dict, pin: dict) -> dict:
    if not verifier.proof_succeeded(report, pin):
        raise ValueError('cannot inventory a failed or differently pinned proof')
    raw, _ = observed_inventory(vir, 'authority_v2::')
    bodies = sorted(n for n, v in raw.items() if v['mode'] == 'Exec')
    functions, declarations = observed_inventory(vir, 'authority_v2::', tuple(bodies))
    profile = {'namespace': 'authority_v2::', 'reviewed': False,
               'functions': functions, 'body_covered_functions': bodies,
               'expected_verified': report['verification-results']['verified'],
               'target_functions': sorted(n for n in report['func-details'] if n.startswith('authority_v2::'))}
    if declarations is not None:
        profile['abstract_declarations'] = declarations
    return profile


def check(out:Path,refresh=False,positive_only=False,selected=None):
    generation=check_generated_sources()
    initial=snapshot(True)
    pin=json.loads((ROOT/verifier.PIN).read_text())
    tool=verifier.cache_directory()/pin['version']/'qualified'
    verifier.verify_tool_files(tool,pin)
    env={k:v for k,v in os.environ.items() if not k.startswith(('VERUS_','VARGO_')) and k not in ('RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS')}
    env.update(RUSTUP_TOOLCHAIN=pin['rust_toolchain'],VERUS_Z3_PATH=str(tool/'z3'),CARGO_BUILD_JOBS='1',RUST_TEST_THREADS='1')
    command=[str(tool/'verus'),'--crate-type=lib','--edition=2024','--no-cheating','--no-external-by-default','--num-threads',verifier.proof_threads('2'),'-V','spinoff-all','--output-json','--triggers-mode','silent','--log','vir','--log','vir-option=no_span+no_type+no_fn_details']
    out.mkdir(parents=True,exist_ok=True)
    def proof(cwd,name):
        proc=verifier.run([*command,'--rlimit',str(CONTROL_RLIMITS.get(name,10)),
            '--log-dir',str(out/name),str(HARNESS)],cwd,env,timeout=600)
        (out/(name+'.stdout')).write_text(proc.stdout);(out/(name+'.stderr')).write_text(proc.stderr)
        return proc,json.loads(proc.stdout)
    positive,report=proof(ROOT,'positive');verifier.require_success(positive)
    vr=report['verification-results']
    if not vr.get('is-verifying-entire-crate') or not vr.get('success') or vr.get('errors')!=0:raise RuntimeError('whole closure proof failed')
    vir=(out/'positive/crate.vir').read_text()
    if refresh:
        profile=candidate_profile(vir,report,pin)
        (out/'candidate-profile.json').write_text(json.dumps(profile,indent=2,sort_keys=True)+'\n')
        if initial!=snapshot(True):raise RuntimeError('source drift during candidate proof')
        return {'schema':'zeno-fcis/authority-v2-evidence/1','status':'profile-review-required',
            'source_sha256':initial,'source_generation':generation,'verus_report':report,
            'candidate_profile':str(out/'candidate-profile.json'),'review_accepted':False,
            'all_mutation_controls':False}
    profile=json.loads((ROOT/PROFILE).read_text());coverage=require_coverage(vir,profile)
    if not verifier.accepted(report,{**pin,'expected_verified':profile['expected_verified'],'target_functions':profile['target_functions']}):raise RuntimeError('toolchain or exact function count mismatch')
    before=snapshot()
    target=Path(env.get('CARGO_TARGET_DIR',str(ROOT/'target')))
    target.mkdir(parents=True,exist_ok=True)
    dependencies=prepare_native_dependencies(ROOT,out/'native-dependencies',env)
    rust=['rustc',f"+{pin['runtime_rust']}",'--edition=2024','--check-cfg','cfg(verus_keep_ghost)','--check-cfg','cfg(test)']
    def native(cwd,name,filtered=False):
        binary=out/(name+'-native')
        native_env=native_test_environment(cwd,env)
        arguments=native_dependency_args(ROOT,out/(name+'.deps'),env,prepared=dependencies)
        build=verifier.run([*rust,*arguments,'--test',str(HARNESS),'-o',str(binary)],cwd,native_env,timeout=180)
        (out/(name+'.build.stdout')).write_text(build.stdout);(out/(name+'.build.stderr')).write_text(build.stderr)
        verifier.require_success(build)
        args=[str(binary),'--test-threads=1']
        proc=verifier.run(args,cwd,native_env,timeout=180)
        (out/(name+'.native.stdout')).write_text(proc.stdout);(out/(name+'.native.stderr')).write_text(proc.stderr)
        return proc
    native_result=native(ROOT,'positive');verifier.require_success(native_result)
    bound_rust=[*rust,*native_dependency_args(ROOT,out/'custody-deps',env,prepared=dependencies)]
    no_std=verifier.run([*bound_rust,'--crate-type=rlib',str(HARNESS),'-o',str(out/'libauthority_v2.rlib')],ROOT,env,timeout=180)
    (out/'positive.no_std.stdout').write_text(no_std.stdout);(out/'positive.no_std.stderr').write_text(no_std.stderr)
    verifier.require_success(no_std)
    custody=custody_controls(out,bound_rust,env)
    results=[]
    if not positive_only:
        controls=mutations()
        if selected:
            unknown=set(selected)-set(controls)
            if unknown:raise ValueError(f'unknown controls: {sorted(unknown)}')
            controls={name:controls[name] for name in selected}
        temporary=out/'source-mutants';temporary.mkdir()
        def run_control(control_item):
            (name, (path, changed, expected, native_control)) = control_item
            specimen=Path(temporary)/name
            for unit in sources():
                target=specimen/unit;target.parent.mkdir(parents=True,exist_ok=True)
                if unit==path:target.write_text(changed)
                else:shutil.copyfile(ROOT/unit,target)
            adjustments=adjust_specimen_source_lengths(specimen)
            proc,parsed=proof(specimen,name);r=parsed['verification-results'];refusal=None;intended=None
            if expected=='proof':
                region=verifier.function_lines(changed,SEMANTIC_TARGETS[name])
                killed=semantic_refusal(proc,parsed,path,specimen,region=region,
                    target_functions=profile['target_functions'])
            else:
                matched,refusal,intended=coverage_control((out/name/'crate.vir').read_text(),profile,name)
                killed=(proc.returncode==0 and verifier.proof_succeeded(parsed,pin) and matched
                    and not verifier.ERROR_HEADER.search(proc.stderr)
                    and not verifier.NON_SEMANTIC.search(proc.stderr))
            n=native(specimen,name,True) if native_control else None
            failed_tests=[] if n is None else re.findall(r'^test (\S+) \.\.\. FAILED$',n.stdout,re.MULTILINE)
            if n is not None:killed=killed and n.returncode!=0 and 'test result: FAILED' in n.stdout and bool(failed_tests)
            if name.startswith(('getter_','constructor_','approved_source_')) and n is not None:killed=killed and any(t.startswith('public_authority::') for t in failed_tests)
            row={'name':name,'expected':expected,'killed':killed,'proof_exit':proc.returncode,'verification_results':r,'coverage_refusal':refusal,'intended_coverage_control':intended,'source_array_length_adjustments':adjustments,'native_exit':None if n is None else n.returncode};control_result = row
            row['proof_command']=proc.args
            row['mutated_path']=str(path);row['mutated_sha256']=verifier.digest(specimen/path);row['native_failed_tests']=failed_tests
            retained=out/'specimens'/name;retained.mkdir(parents=True)
            row['retained_source_sha256']={}
            for changed_path in dict.fromkeys([path,EVALUATOR]):
                saved=retained/changed_path;saved.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(specimen/changed_path,saved)
                row['retained_source_sha256'][str(changed_path)]=verifier.digest(saved)
            if not killed:raise ControlRefused(control_result, f'mutation survived or unrelated failure: {row}')
            print(f'Caught {name} ({expected})',flush=True)
            return control_result
        def publish_control(control_result):
            results.append(control_result)
            (out/'control-progress.json').write_text(json.dumps({'source_sha256':before, "source_bindings": verifier.evidence_bindings(before, HARNESS),'controls':results},sort_keys=True,indent=2)+'\n')
        run_controls(run_control, controls.items(), timings=out / 'control-timings.json', publish=publish_control)
    if initial!=snapshot(True) or before!=snapshot():raise RuntimeError('source drift during checks')
    verifier.verify_tool_files(tool,pin)
    return {'schema':'zeno-fcis/authority-v2-evidence/1','status':'passed','revision':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'source_sha256':before, "source_bindings": verifier.evidence_bindings(before, HARNESS),'source_generation':generation,'verus_report':report,'translated_function_coverage':coverage,'mutations':results,'custody_controls':custody,'proof_exit':positive.returncode,'native_exit':native_result.returncode,'no_std_exit':no_std.returncode,'positive_only':positive_only,'selected_controls':selected,'all_mutation_controls':not positive_only and not selected,'unproved':['compiler/platform/allocator and external shell','root combined Miri/ATDD/template/law and ledger/migration integration','whole V2 release and exact-head review/CI']}

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--out',type=Path);p.add_argument('--refresh-coverage',action='store_true');p.add_argument('--positive-only',action='store_true');p.add_argument('--controls',nargs='+');p.add_argument('--generate-sources',type=Path);p.add_argument('--check-sources',action='store_true');p.add_argument('--include-released-24',action='store_true');p.add_argument('--include-value-decoder',action='store_true');a=p.parse_args()
    if a.include_value_decoder:
        print(json.dumps(include_value_decoder_sources(),sort_keys=True));return 0
    if a.include_released_24:
        print(json.dumps(include_released_24_sources(),sort_keys=True));return 0
    if a.check_sources:
        print(json.dumps(check_generated_sources(),sort_keys=True));return 0
    if a.generate_sources:
        print(json.dumps(generate_evaluator(a.generate_sources),sort_keys=True));return 0
    if a.out is None:p.error('--out is required for verification')
    if a.positive_only and a.controls:p.error('--positive-only cannot select mutation controls')
    try:receipt=check(a.out.resolve(),a.refresh_coverage,a.positive_only,a.controls)
    except (OSError,ValueError,KeyError,RuntimeError,subprocess.SubprocessError) as e:receipt={'schema':'zeno-fcis/authority-v2-evidence/1','status':'failed','error':str(e)}
    a.out.mkdir(parents=True,exist_ok=True);(a.out/'receipt.json').write_text(json.dumps(receipt,sort_keys=True,indent=2)+'\n');print(receipt['status'])
    if receipt['status'] not in ('passed','profile-review-required'):print(receipt['error']);return 1
    return 0
if __name__ == "__main__":
    try:
        from telemetry import run_cli
    except ImportError:
        # Reporting is optional, including for a standalone copy of this tool.
        run_cli = lambda function: function()
    def _entrypoint():
        raise SystemExit(main())
    run_cli(_entrypoint)
