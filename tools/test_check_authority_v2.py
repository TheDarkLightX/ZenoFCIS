#!/usr/bin/env python3
"""Check source custody and the authority gate's intended negative controls."""
import copy
import hashlib
import json
import re
import subprocess
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import check_authority_v2 as gate


class GateTests(unittest.TestCase):
    def test_released_extension_preserves_approved_paths_and_build_pins(self):
        raw = {'schema': 'zeno-fcis/authority-source-closure/1', 'paths': ['retained.rs'],
               'pins': [{'path': '@pin/runtime-toolchain', 'bytes': 'fixed\n'}]}
        copies = {'tag': 'v2.4.0', 'subject': '8c444487f96d6acd14c9f7fa55d9ba88ca780fb6',
                  'files': [{'copy': 'historical.rs'}], 'binding_sources': [{'path': 'binding.rs'}]}
        before = copy.deepcopy(raw)
        extended = gate.released_24_source_extension(raw, copies)
        self.assertEqual(raw, before)
        self.assertEqual(extended['pins'], before['pins'])
        self.assertTrue(set(before['paths']) <= set(extended['paths']))
        self.assertIn('historical.rs', extended['paths'])
        self.assertIn('binding.rs', extended['paths'])
        for field in ('tag', 'subject'):
            forged = {**copies, field: 'arbitrary'}
            with self.assertRaises(ValueError): gate.released_24_source_extension(raw, forged)
        copies['files'] = [{'copy': '../omitted.rs'}]
        with self.assertRaises(ValueError): gate.released_24_source_extension(raw, copies)

    def test_decoder_extension_retains_membership_pins_and_input(self):
        raw = {'schema': 'zeno-fcis/authority-source-closure/1',
               'paths': ['retained.rs'],
               'pins': [{'path': '@pin/runtime-toolchain', 'bytes': 'fixed\n'}]}
        before = copy.deepcopy(raw)
        extended = gate.value_decoder_source_extension(raw)
        self.assertEqual(raw, before)
        self.assertEqual(extended['pins'], before['pins'])
        self.assertIn('retained.rs', extended['paths'])
        self.assertEqual(len(extended['paths']), 8)
        self.assertIn('crates/zeno-fcis-codec/src/decoder_spec.rs', extended['paths'])
        self.assertIn('crates/zeno-fcis-value/src/decoder_model.rs', extended['paths'])
        for defect in ('omission', 'pin'):
            changed = copy.deepcopy(extended)
            if defect == 'omission':
                changed['paths'].remove('retained.rs')
            else:
                changed['pins'][0]['bytes'] = 'substituted'
            with self.subTest(defect=defect), patch.object(gate, 'source_manifest', return_value=raw):
                with self.assertRaisesRegex(ValueError, 'preserve approved'):
                    gate._include_source_extension(changed)

    def test_before_loop_invariant_keeps_source_and_resource_guards(self):
        pin=json.loads((gate.ROOT/gate.verifier.PIN).read_text())
        report={'verus':{'version':pin['version'],'commit':pin['commit'],
                        'toolchain':pin['rust_toolchain']},'func-details':{},
                'verification-results':{'success':False,'errors':1,'verified':1,
                    'encountered-error':True,'encountered-vir-error':False,
                    'is-verifying-entire-crate':True}}
        path=Path('policy.rs')
        raw='error: invariant not satisfied before loop\n --> policy.rs:33:77\nerror: aborting due to 1 previous error\n'
        proc=subprocess.CompletedProcess([],1,'',raw)
        self.assertTrue(gate.semantic_refusal(proc,report,path,region=(17,65)))
        self.assertEqual(proc.stderr,raw)
        self.assertFalse(gate.semantic_refusal(proc,report,path,region=(40,65)))
        for text in (raw.replace('policy.rs','unrelated.rs'),
                     raw.replace('before loop','during an unrelated phase'),
                     raw+'note: Resource limit (rlimit) exceeded\n'):
            self.assertFalse(gate.semantic_refusal(
                subprocess.CompletedProcess([],1,'',text),report,path,region=(17,65)))

    def test_candidate_inventory_never_promotes_or_overwrites_the_profile(self):
        pin = json.loads((gate.ROOT / gate.verifier.PIN).read_text())
        report = {'verus': {key: pin[key] for key in ('version', 'commit')},
                  'verification-results': {'success': True, 'errors': 0, 'verified': 1,
                     'is-verifying-entire-crate': True, 'encountered-error': False,
                     'encountered-vir-error': False},
                  'func-details': {'authority_v2::probe': {}}}
        report['verus']['toolchain'] = pin['rust_toolchain']
        vir = ('(Function (Fun :path authority_v2::probe) :mode Exec :typ_bounds () '
               ':params () :ret () :require () :ensure ((Bool true)) :d () :body (Int 0))')
        before = (gate.ROOT / gate.PROFILE).read_bytes()
        candidate = gate.candidate_profile(vir, report, pin)
        self.assertFalse(candidate['reviewed'])
        self.assertEqual(candidate['body_covered_functions'], ['authority_v2::probe'])
        self.assertIn('body_sha256', candidate['functions']['authority_v2::probe'])
        self.assertEqual((gate.ROOT / gate.PROFILE).read_bytes(), before)
        report['verification-results']['errors'] = 1
        with self.assertRaisesRegex(ValueError, 'failed or differently pinned'):
            gate.candidate_profile(vir, report, pin)

    def test_solver_exhaustion_does_not_count_as_a_semantic_refusal(self):
        pin = json.loads((gate.ROOT / gate.verifier.PIN).read_text())
        report = {'verus': {'version': pin['version'], 'commit': pin['commit'],
                           'toolchain': pin['rust_toolchain']},
                  'func-details': {},
                  'verification-results': {'success': False, 'errors': 1, 'verified': 1,
                                          'encountered-error': True,
                                          'encountered-vir-error': False,
                                          'is-verifying-entire-crate': True}}
        path = Path('outcome.rs')
        diagnostic = ('error: postcondition not satisfied\n --> outcome.rs:64:1\n')
        summary = 'error: aborting due to 1 previous error\n'
        for refusal in ('Resource limit (rlimit) exceeded', 'timed out', 'out of memory',
                        'SMT result UNKNOWN', 'solver returned unknown', 'transport error'):
            proc = subprocess.CompletedProcess([], 1, '', diagnostic + 'note: ' + refusal + '\n' + summary)
            with self.subTest(diagnostic=refusal):
                self.assertFalse(gate.semantic_refusal(proc, report, path))
        proc = subprocess.CompletedProcess([], 1, '', diagnostic + summary)
        self.assertTrue(gate.semantic_refusal(proc, report, path))
        proc = subprocess.CompletedProcess([], 1, '', diagnostic.replace('outcome.rs', 'unrelated.rs') + summary)
        self.assertFalse(gate.semantic_refusal(proc, report, path))
        report['verification-results']['encountered-vir-error'] = True
        self.assertFalse(gate.semantic_refusal(
            subprocess.CompletedProcess([], 1, '', 'outcome.rs:64: type error'), report, path))

    def test_each_proof_mutation_has_one_exact_annotated_target(self):
        controls = gate.mutations()
        self.assertEqual(set(gate.SEMANTIC_TARGETS),
                         {name for name, row in controls.items() if row[2] == 'proof'})
        for name, target in gate.SEMANTIC_TARGETS.items():
            region = gate.verifier.function_lines(controls[name][1], target)
            self.assertLess(region[0], region[1], name)

    def test_mutations_are_distinct_exact_subjects(self):
        controls = gate.mutations()
        self.assertEqual(len(controls), 42)
        self.assertEqual(sum(v[2] == 'proof' for v in controls.values()), 37)
        self.assertEqual(sum(v[3] for v in controls.values()), 37)
        for path, changed, _expected, _native in controls.values():
            self.assertIn(path, gate.sources())
            self.assertNotEqual(changed, (gate.ROOT / path).read_text())

    def test_all_executable_contracts_and_bodies_are_frozen(self):
        profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        runtime = {n for n, v in profile['functions'].items() if v['mode'] == 'Exec'}
        self.assertEqual(runtime, set(profile['body_covered_functions']))
        for name in runtime:
            function = profile['functions'][name]
            self.assertEqual(function['requires'], 0, name)
            self.assertGreater(function['ensures'], 0, name)
            self.assertIn('body_sha256', function)
        for name in ['canonical::encode', 'canonical::encoded_size', 'canonical::exact',
                     'candidate::candidate_parts', 'framing::subject_bytes',
                     'observations::append_law_reads',
                     'metadata::program', 'metadata::branches', 'metadata::law_list',
                     'metadata::limits', 'bound::bind']:
            self.assertIn('authority_v2::execution_v2::authority::' + name, runtime)

    def test_evaluator_matches_complete_approved_generation(self):
        result = gate.check_generated_sources()
        self.assertEqual(result['paths'], 267)
        self.assertIn('crates/zeno-fcis-synthesis/src/finite/value/scalar.rs', gate.source_manifest(gate.ROOT / gate.SOURCE_MANIFEST)['paths'])
        self.assertEqual(result['pins'], 3)
        self.assertEqual(result['approved_manifest_sha256'], gate.APPROVED_SOURCE_CLOSURE_SHA256)

    def test_source_omission_substitution_and_pin_change_refused(self):
        original = json.loads((gate.ROOT / gate.SOURCE_MANIFEST).read_text())
        with tempfile.TemporaryDirectory(prefix='authority-source-list-test-') as temporary:
            path = Path(temporary) / 'sources.json'
            for name in ['omit', 'substitute', 'pin']:
                altered = copy.deepcopy(original)
                if name == 'omit':
                    altered['paths'].pop(0)
                elif name == 'substitute':
                    altered['paths'][0] = 'unreviewed.rs'
                else:
                    altered['pins'][0]['bytes'] += 'unreviewed'
                path.write_text(json.dumps(altered, indent=2, sort_keys=True) + '\n')
                with self.subTest(name=name), self.assertRaisesRegex(ValueError, 'root-approved'):
                    gate.source_manifest(path)

    def test_incidental_embedded_source_drift_cannot_kill_helper_control(self):
        prefix = 'authority_v2::execution_v2::authority::'
        target = prefix + 'canonical::encode'
        constant = prefix + 'evaluator::EVALUATOR'
        record = {'body_sha256': 'body', 'signature_sha256': 'signature',
                  'requires_sha256': 'requires', 'ensures_sha256': 'ensures'}
        functions = {target: record, constant: {'body_sha256': 'old-source'}}
        actual = {target: record, constant: {'body_sha256': 'changed-source'}}
        profile = {'functions': functions, 'namespace': 'authority_v2::',
                   'body_covered_functions': [target]}
        with patch.object(gate, 'require_coverage', side_effect=ValueError(constant)), \
             patch.object(gate, 'inventory', return_value=actual):
            killed, _refusal, intended = gate.coverage_control('', profile, 'serialize_before_size_check')
        self.assertFalse(killed)
        self.assertEqual(intended['function'], target)

    def test_unchecked_helper_requires_the_exact_added_native_function(self):
        target = 'authority_v2::execution_v2::authority::canonical::unchecked_authority_probe'
        existing = 'authority_v2::execution_v2::authority::canonical::encode'
        record = {'mode': 'Exec', 'requires': 0, 'ensures': 0}
        profile = {'functions': {existing: {}}, 'namespace': 'authority_v2::',
                   'body_covered_functions': [existing]}
        for actual, expected in (
            ({existing: {}, target: record}, True),
            ({existing: {}, target: {**record, 'ensures': 1}}, False),
            ({existing: {}, target: record, 'authority_v2::unexpected': {}}, False),
            ({target: record}, False),
        ):
            with patch.object(gate, 'require_coverage', side_effect=ValueError(target)), \
                 patch.object(gate, 'inventory', return_value=actual):
                self.assertEqual(gate.coverage_control('', profile, 'uncontracted_helper')[0], expected)

    def test_coverage_targets_do_not_depend_on_the_first_refusal_message(self):
        # Real parser/profile comparisons: descriptor metadata is checked first.
        namespace = 'authority_v2::'
        authority = namespace + 'execution_v2::authority::'
        descriptor = namespace + 'execution_v2::composition::descriptor::'
        def function(name, *, mode='Exec', ensures='true', body='(42)', params=''):
            return (f'(Function (Fun :path {name}) :mode {mode} :typ_bounds () '
                    f':params ({params}) :ret () :require () :ensure ({ensures}) '
                    f':d () :body {body})')
        declarations = (function(descriptor + 'DescriptorData::model', mode='Spec',
                                 ensures='', body='None') +
                        function(descriptor + 'DescriptorData::project', body='None'))
        concrete = ''.join(function(descriptor + f'impl&%{index}::{method}',
                                   mode=mode, ensures='' if mode == 'Spec' else 'true')
                           for index in (2, 3)
                           for method, mode in (('model', 'Spec'), ('project', 'Exec')))
        for name, suffix, field in (
                ('weak_exact_contract', 'bound::impl&%0::replay', 'ensures_sha256'),
                ('serialize_before_size_check', 'canonical::encode', 'body_sha256'),
                ('weak_constructor_contract', 'bound::bind', 'ensures_sha256'),
                ('unchecked_constructor_bypass', 'bound::bind', 'signature_sha256'),
                ('uncontracted_helper', 'canonical::unchecked_authority_probe', 'inventory')):
            target = authority + suffix
            original = function(authority + 'canonical::encoded_size')
            if field != 'inventory':
                original += function(target)
            positive = declarations + concrete + original
            raw, _ = gate.descriptor_inventory(positive, namespace, descriptor)
            bodies = [n for n, row in raw.items() if row['mode'] == 'Exec']
            records, abstract = gate.descriptor_inventory(positive, namespace, descriptor, tuple(bodies))
            profile = {'namespace': namespace, 'functions': records,
                       'abstract_declarations': abstract, 'body_covered_functions': bodies}
            gate.require_coverage(positive, profile)
            drift = declarations.replace(':ensure (true)', ':ensure (changed-read-id)') + concrete
            changed = (function(target, ensures='') if field == 'inventory' else
                       function(target, **{'ensures_sha256': {'ensures': 'weakened'},
                                           'body_sha256': {'body': '(43)'},
                                           'signature_sha256': {'params': 'raw-descriptor'}}[field]))
            intended = (original + changed if field == 'inventory' else
                        original.replace(function(target), changed))
            with self.subTest(control=name):
                caught, refusal, detail = gate.coverage_control(drift + intended, profile, name)
                self.assertTrue(caught)
                self.assertEqual(refusal, 'translated descriptor declarations changed')
                self.assertEqual((detail['function'], detail['field']), (target, field))
                # Descriptor drift alone never stands in for the intended change.
                self.assertFalse(gate.coverage_control(drift + original, profile, name)[0])
                self.assertFalse(gate.coverage_control(positive, profile, name)[0])
                extra = function(authority + 'unrelated')
                self.assertFalse(gate.coverage_control(drift + intended + extra, profile, name)[0])
                missing = intended.replace(function(authority + 'canonical::encoded_size'), '')
                self.assertFalse(gate.coverage_control(drift + missing, profile, name)[0])
                if field != 'inventory':
                    wrong_field = ('body_sha256' if field != 'body_sha256' else 'ensures_sha256')
                    wrong = function(target, **{'body_sha256': {'body': '(43)'},
                                               'ensures_sha256': {'ensures': 'wrong'}}[wrong_field])
                    self.assertFalse(gate.coverage_control(
                        drift + original.replace(function(target), wrong), profile, name)[0])
                if name == 'serialize_before_size_check':
                    for altered in (changed.replace(':ensure (true)', ':ensure (weakened)'),
                                    changed.replace(':require ()', ':require (narrowed)'),
                                    changed.replace(':params ()', ':params (different)')):
                        self.assertFalse(gate.coverage_control(
                            drift + original.replace(function(target), altered), profile, name)[0])

    def test_late_size_check_control_preserves_complete_allocation_statements(self):
        source = (gate.ROOT / gate.CANONICAL).read_text()
        path, changed, expected, native = gate.mutations()['serialize_before_size_check']
        self.assertEqual((path, expected, native), (gate.CANONICAL, 'coverage', False))
        # The production allocation now uses the checked size. The late-check
        # specimen must remove the whole binding and use an independent vector.
        original = ('let size = encoded_size(parts)?;\n'
                    '    let mut out = Vec::with_capacity(size);')
        expected_source = source.replace(original, 'let mut out = Vec::new();', 1)
        expected_source = expected_source.replace('model::length(parts@,parts@.len())<=usize::MAX,', '', 1)
        expected_source = expected_source.replace('    Some(out)', '    encoded_size(parts)?;\n    Some(out)', 1)
        self.assertEqual(changed, expected_source)
        self.assertEqual(changed.count('encoded_size(parts)?;'), 1)
        self.assertNotIn('let size =', changed)
        self.assertLess(changed.index('while i < parts.len()', changed.index('pub fn encode(')),
                        changed.index('encoded_size(parts)?;'))

    def test_unchecked_bypass_changes_only_its_proof_aid_and_keeps_the_bypass(self):
        bound = (gate.ROOT / gate.BOUND).read_text()
        start = bound.rfind('#[cfg_attr', 0, bound.index("pub fn bind<'p, D:"))
        end = bound.index("\n\nimpl<'p, D:", start)
        path, changed, expected, native = gate.mutations()['unchecked_constructor_bypass']
        self.assertEqual((path, expected, native), (gate.BOUND, 'coverage', False))
        self.assertTrue(changed.startswith(bound[:start]) and changed.endswith(bound[end:]))
        replacement = changed[start:len(changed) - len(bound) + end]
        # Raw descriptor/framing/identity signature, unchanged contract, descriptor-only
        # admission and empty roots: the catalog checks remain bypassed.
        for text in ("pub fn bind<'p>(descriptor:&'p Descriptor<'p>,framing:Framing,identity:Vec<u8>)"
                     "->Result<Authority<'p>,Refusal>{",
                     "(if composition::descriptor_admitted(descriptor){Ok((descriptor,framing,identity@,Seq::empty()))}",
                     "let core=match composition::bind(descriptor){",
                     "let channel_roots:&[(u32,u32,u32)]=&[];",
                     "assert(channel_roots@=~=Seq::<(u32,u32,u32)>::empty());",
                     "Ok(Authority{core,framing,identity,channel_roots})"):
            self.assertEqual(replacement.count(text), 1, text)
        self.assertNotIn('catalog', replacement)
        self.assertNotIn('requires', replacement)
        # No assumption, admitted obligation, trusted body, axiom or verifier attribute.
        for escape in (r'\bassume\s*\(', r'\badmit\s*\(', r'\bexternal_(?:body|fn)', r'\baxiom', r'#\[\s*verifier'):
            self.assertIsNone(re.search(escape, replacement), escape)
        # The historical raw constructor remains byte-identical. The surrounding
        # source now uses the closed generic family, and is checked separately above.
        aid = ("    let channel_roots:&[(u32,u32,u32)]=&[];\n"
               "    #[cfg(verus_keep_ghost)]proof!{reveal(Authority::view);"
               "assert(channel_roots@=~=Seq::<(u32,u32,u32)>::empty());}\n"
               "    Ok(Authority{core,framing,identity,channel_roots})")
        retained = changed.replace(aid, "    #[cfg(verus_keep_ghost)]proof!{reveal(Authority::view);}\n"
                                        "    Ok(Authority{core,framing,identity,channel_roots:&[]})")
        self.assertEqual(changed.count(aid), 1)
        retained_constructor = retained[start:len(retained) - len(bound) + end]
        self.assertEqual(hashlib.sha256(retained_constructor.encode()).hexdigest(),
                         '9860ba879db2c61d6571450fe32c43670ce63c1278b2c8a1cc2f70cc3ad8886c')

    def test_unrelated_refusal_cannot_satisfy_the_bypass_coverage_control(self):
        prefix = 'authority_v2::execution_v2::authority::'
        bind = prefix + 'bound::bind'
        other = prefix + 'evaluator::EVALUATOR'
        record = {'body_sha256': 'body', 'signature_sha256': 'signature',
                  'requires_sha256': 'requires', 'ensures_sha256': 'ensures'}
        profile = {'functions': {bind: record, other: {'body_sha256': 'old'}},
                   'namespace': 'authority_v2::', 'body_covered_functions': [bind]}
        for actual, refused, intended in (
                ({bind: record, other: {'body_sha256': 'changed'}}, other, False),
                ({bind: {**record, 'ensures_sha256': 'other'}, other: {'body_sha256': 'old'}}, bind, False),
                ({bind: {**record, 'signature_sha256': 'raw'}, other: {'body_sha256': 'old'}}, bind, True)):
            with patch.object(gate, 'require_coverage', side_effect=ValueError(f"changed: ['{refused}']")), \
                 patch.object(gate, 'inventory', return_value=actual):
                matched, _refusal, detail = gate.coverage_control('', profile, 'unchecked_constructor_bypass')
            self.assertEqual((matched, detail['function'], detail['field'], detail['intended_refusal']),
                             (intended, bind, 'signature_sha256', intended))


if __name__ == '__main__':
    unittest.main()
