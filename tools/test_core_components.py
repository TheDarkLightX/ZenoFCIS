"""Data-boundary controls for component instantiation; no decision evaluator."""
import tempfile
import unittest
from pathlib import Path

import check_core_components as checks
import instantiate_core as core


class ComponentDataTests(unittest.TestCase):
    def test_full_independent_domain_is_present(self):
        instances = checks.static_check()
        self.assertEqual(len(instances), 77)
        self.assertEqual(sum(item['input_tuples'] for item in instances), 107518)

    def test_numeric_arguments_are_not_command_kinds(self):
        rows = [row for row in checks.expected_cases()
                if row['family'] == 'versioned_register' and row['parameters'] == {'C': 1}]
        self.assertEqual({core.command_name('versioned-register', row['input']) for row in rows}, {'Write'})
        self.assertTrue(any(row['expected']['class'] == 'Accept' for row in rows))
        self.assertFalse(any(row['input'][2] == 1 and row['expected']['class'] == 'Accept' for row in rows))
        self.assertEqual(core.command_name('retry-budget', [1, 0, 0, 1]), 'Attempt')
        self.assertEqual(core.command_name('retry-budget', [1, 0, 1, 1]), 'Finish')

    def test_static_construction_does_not_invent_a_certificate(self):
        files = core.source('versioned-register', {'C': 1}, include_proof=False)
        manifest = core.read_json(files['core-instance.json'].decode())
        self.assertIsNone(manifest['finite_family_certificate'])
        self.assertEqual(manifest['parameter_range_evidence'], 'pending')

    def test_outside_closed_parameter_space_is_refused(self):
        invalid = [
            ('reservation-pool', {'C': 0, 'Q': 1}),
            ('reservation-pool', {'C': 1, 'Q': 2}),
            ('reservation-pool', {'C': 4, 'Q': 4}),
            ('rate-limiter', {'W': 4, 'N': 1}),
            ('rate-limiter', {'W': 1}),
            ('approval-queue', {'K': True}),
            ('approval-queue', {'K': 1.0}),
            ('approval-queue', {'K': '1'}),
            ('approval-queue', {'K': 1, 'OTHER': 0}),
            ('../approval-queue', {'K': 1}),
        ]
        for family in core.FAMILIES[3:]:
            invalid.extend((family, parameters) for parameters in
                           ({'C': 0}, {'C': 9}, {'C': True}, {'C': 1.0}, {'C': '1'}, {}, {'C': 1, 'EXTRA': 0}))
        for family, parameters in invalid:
            with self.subTest(family=family, parameters=parameters):
                with self.assertRaises(ValueError):
                    core.source(family, parameters)

    def test_existing_contract_is_never_overwritten(self):
        with tempfile.TemporaryDirectory(prefix='core-data-test-') as temporary:
            destination = Path(temporary) / 'instance'
            core.instantiate('approval-queue', {'K': 1}, destination)
            files = {path: path.read_bytes() for path in destination.rglob('*') if path.is_file()}
            with self.assertRaises(FileExistsError):
                core.instantiate('approval-queue', {'K': 2}, destination)
            self.assertEqual(files, {path: path.read_bytes() for path in files})

    def test_invalid_parameters_leave_no_output(self):
        with tempfile.TemporaryDirectory(prefix='core-data-test-') as temporary:
            destination = Path(temporary) / 'instance'
            with self.assertRaises(ValueError):
                core.instantiate('approval-queue', {'K': 0}, destination)
            self.assertFalse(destination.exists())

    def test_json_repeated_keys_are_not_silently_lost(self):
        with self.assertRaises(ValueError):
            core.read_json('{"instances": [], "instances": [{"K": 1}]}')


if __name__ == '__main__':
    unittest.main()
