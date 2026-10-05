#!/usr/bin/env python3
"""Check source custody and the authority gate's intended negative controls."""
import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import check_authority_v2 as gate


class GateTests(unittest.TestCase):
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


if __name__ == '__main__':
    unittest.main()
