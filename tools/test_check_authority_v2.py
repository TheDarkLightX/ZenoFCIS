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
    def test_solver_exhaustion_does_not_count_as_a_semantic_refusal(self):
        report = {'verification-results': {'success': False, 'errors': 1,
                                          'encountered-vir-error': False}}
        path = Path('outcome.rs')
        for diagnostic in ('Resource limit (rlimit) exceeded', 'timed out', 'out of memory'):
            proc = subprocess.CompletedProcess([], 1, '', f'outcome.rs:64: {diagnostic}')
            with self.subTest(diagnostic=diagnostic):
                self.assertFalse(gate.semantic_refusal(proc, report, path))
        proc = subprocess.CompletedProcess([], 1, '', 'outcome.rs:63: postcondition not satisfied')
        self.assertTrue(gate.semantic_refusal(proc, report, path))
        proc = subprocess.CompletedProcess([], 1, '', 'unrelated.rs:63: postcondition not satisfied')
        self.assertFalse(gate.semantic_refusal(proc, report, path))
        report['verification-results']['encountered-vir-error'] = True
        self.assertFalse(gate.semantic_refusal(
            subprocess.CompletedProcess([], 1, '', 'outcome.rs:64: type error'), report, path))

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
        self.assertEqual(result['paths'], 90)
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

    def test_unchecked_bypass_changes_only_its_proof_aid_and_keeps_the_bypass(self):
        bound = (gate.ROOT / gate.BOUND).read_text()
        start = bound.rfind('#[cfg_attr', 0, bound.index("pub fn bind<'p>"))
        end = bound.index("\n\nimpl<'p> Authority<'p>", start)
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
        # Reverting only the typed binding and checked assertion reproduces the retained
        # failed hosted mutant (run 37404295156), so nothing else changed.
        aid = ("    let channel_roots:&[(u32,u32,u32)]=&[];\n"
               "    #[cfg(verus_keep_ghost)]proof!{reveal(Authority::view);"
               "assert(channel_roots@=~=Seq::<(u32,u32,u32)>::empty());}\n"
               "    Ok(Authority{core,framing,identity,channel_roots})")
        retained = changed.replace(aid, "    #[cfg(verus_keep_ghost)]proof!{reveal(Authority::view);}\n"
                                        "    Ok(Authority{core,framing,identity,channel_roots:&[]})")
        self.assertEqual(changed.count(aid), 1)
        self.assertEqual(hashlib.sha256(retained.encode()).hexdigest(),
                         '4f6705b0a12737af9928bc8604e9fd1b52ebfc2101e2c2a8ecd3b7374e373bed')

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
