"""Artifact-inventory controls; synthetic paths are not native replay evidence."""
from pathlib import Path
import unittest

import check_evaluator_replay as gate


class NativeArtifactTests(unittest.TestCase):
    def artifacts(self):
        directory = Path(__file__).resolve().parent / 'prepared-libraries'
        return {name: str(directory / ('lib' + name + '.rlib')) for name in (
            'zeno_fcis_codec', 'zeno_fcis_value', 'zeno_fcis_collections', 'zeno_fcis_crypto')}

    def test_actual_prepared_directory_needs_no_deps_suffix(self):
        artifacts = self.artifacts()
        arguments = gate.native_arguments(artifacts)
        self.assertEqual(arguments[:2], ['-L', 'dependency=' + str(Path(next(iter(artifacts.values()))).parent)])
        self.assertEqual(arguments[2:], [part for name, path in sorted(artifacts.items())
                                         for part in ('--extern', name + '=' + path)])

    def test_obsolete_two_library_inventory_is_refused(self):
        artifacts = self.artifacts()
        del artifacts['zeno_fcis_collections']
        del artifacts['zeno_fcis_crypto']
        with self.assertRaisesRegex(ValueError, 'collections and crypto'):
            gate.native_arguments(artifacts)

    def test_unexpected_direct_dependency_is_refused(self):
        artifacts = {**self.artifacts(), 'unreviewed': 'libunreviewed.rlib'}
        with self.assertRaisesRegex(ValueError, 'native artifacts'):
            gate.native_arguments(artifacts)

    def test_mixed_preparation_directories_are_refused(self):
        artifacts = self.artifacts()
        artifacts['zeno_fcis_crypto'] = str(Path(artifacts['zeno_fcis_crypto']).parent / 'other' / 'libcrypto.rlib')
        with self.assertRaisesRegex(ValueError, 'one prepared dependency directory'):
            gate.native_arguments(artifacts)


if __name__ == '__main__':
    unittest.main()
