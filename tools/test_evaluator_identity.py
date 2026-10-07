"""S1 identity controls: approved membership, source freshness and framing."""
import copy
import hashlib
import json
from pathlib import Path
import shutil
import re
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import evaluator_identity as evaluator
import check_authority_v2 as gate


class EvaluatorIdentityTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='evaluator-identity-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.raw = gate.source_manifest(gate.ROOT / gate.SOURCE_MANIFEST)
        for name in [*self.raw['paths'], str(gate.SOURCE_MANIFEST), str(gate.EVALUATOR),
                     'verification/verus/toolchain.json']:
            target = self.root / name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(gate.ROOT / name, target)
        self.scope = patch.object(gate, 'ROOT', self.root)
        self.scope.start()
        self.addCleanup(self.scope.stop)

    def write_manifest(self, raw):
        (self.root / gate.SOURCE_MANIFEST).write_text(json.dumps(raw, indent=2, sort_keys=True) + '\n')

    def test_fixed_encoding_vector(self):
        expected = (Path(__file__).resolve().parents[1] / 'verification/verus/evaluator_encoding_vector.txt').read_text().strip()
        self.assertEqual(evaluator.encode([('b.rs', b''), ('a.rs', b'abc')]).hex(), expected)

    def test_all_source_payloads_are_bound(self):
        gate.check_generated_sources()
        for name in self.raw['paths']:
            path = self.root / name
            original = path.read_bytes()
            path.write_bytes(original + b'\n')
            with self.subTest(source=name), self.assertRaisesRegex(ValueError, 'digest differs'):
                gate.check_generated_sources()
            path.write_bytes(original)

    def test_evaluator_digest_flip(self):
        p = self.root / gate.EVALUATOR
        text, flipped = re.subn(r'(= \[\n    0x)([0-9a-f]{2})',
                                lambda m: f'{m[1]}{int(m[2], 16) ^ 1:02x}', p.read_text(), count=1)
        self.assertEqual(flipped, 1)
        p.write_text(text)
        with self.assertRaisesRegex(ValueError, 'digest differs'):
            gate.check_generated_sources()

    @unittest.skipUnless(shutil.which('rustfmt'), 'rustfmt is not installed')
    def test_generated_source_is_rustfmt_clean_for_any_digest(self):
        for value in (bytes(32), bytes(range(32)), bytes([255] * 32)):
            with self.subTest(digest=value.hex()), patch.object(evaluator, 'digest', return_value=value):
                text = evaluator.render(self.root, self.raw)
                formatted = subprocess.run(['rustfmt', '+1.97.1', '--edition', '2024', '--emit', 'stdout'],
                                           input=text, text=True, capture_output=True, check=True).stdout
                self.assertEqual(formatted, text)

    def test_manifest_omission_even_after_regeneration(self):
        altered = copy.deepcopy(self.raw)
        altered['paths'].pop(0)
        self.write_manifest(altered)
        (self.root / gate.EVALUATOR).write_text(evaluator.render(self.root, altered))
        with self.assertRaisesRegex(ValueError, 'root-approved'):
            gate.check_generated_sources()

    def test_source_path_and_every_pin_are_bound(self):
        for index in range(4):
            altered = copy.deepcopy(self.raw)
            if index == 0:
                old = altered['paths'][0]
                name = old + '-changed'
                shutil.copyfile(self.root / old, self.root / name)
                altered['paths'][0] = name
            else:
                altered['pins'][index - 1]['bytes'] += 'changed'
            self.write_manifest(altered)
            (self.root / gate.EVALUATOR).write_text(evaluator.render(self.root, altered))
            with self.subTest(index=index), self.assertRaisesRegex(ValueError, 'root-approved'):
                gate.check_generated_sources()

    def test_duplicate_unsafe_and_self_paths_refused(self):
        for name in [self.raw['paths'][0], '../outside', '/absolute', 'a//b', evaluator.GENERATED]:
            altered = copy.deepcopy(self.raw)
            altered['paths'].append(name)
            altered['paths'].sort()
            self.write_manifest(altered)
            with self.subTest(name=name), self.assertRaises(ValueError):
                evaluator.manifest(self.root, self.root / gate.SOURCE_MANIFEST)

    def test_missing_and_extra_pins_refused(self):
        for pins in [self.raw['pins'][:-1], self.raw['pins'] + [self.raw['pins'][0]]]:
            altered = copy.deepcopy(self.raw)
            altered['pins'] = pins
            self.write_manifest(altered)
            with self.assertRaisesRegex(ValueError, 'pin'):
                evaluator.manifest(self.root, self.root / gate.SOURCE_MANIFEST)

    def test_missing_and_symlink_sources_refused(self):
        path = self.root / self.raw['paths'][0]
        original = path.read_bytes()
        path.unlink()
        with self.assertRaisesRegex(ValueError, 'missing'):
            evaluator.manifest(self.root, self.root / gate.SOURCE_MANIFEST)
        target = self.root / 'substitute'
        target.write_bytes(original)
        path.symlink_to(target)
        with self.assertRaisesRegex(ValueError, 'symlink'):
            evaluator.manifest(self.root, self.root / gate.SOURCE_MANIFEST)

    def test_path_payload_boundaries_and_order(self):
        original = [('first.rs', b'source one'), ('second.rs', b'source two')]
        encoded = evaluator.encode(original)
        self.assertEqual(encoded, evaluator.encode(list(reversed(original))))
        for rows in [original[:1], [('first.rs', b'Source one'), original[1]],
                     [('First.rs', b'source one'), original[1]]]:
            self.assertNotEqual(encoded, evaluator.encode(rows))
        self.assertNotEqual(evaluator.encode([('ab', b'c')]), evaluator.encode([('a', b'bc')]))
        with self.assertRaisesRegex(ValueError, 'duplicate'):
            evaluator.encode([original[0], original[0]])


if __name__ == '__main__':
    unittest.main()
