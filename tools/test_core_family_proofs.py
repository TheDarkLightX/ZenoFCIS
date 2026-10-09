"""Finite certificate negative controls; reference semantics are not rewritten."""
import copy
import itertools
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import instantiate_core as core
import prove_core_families as proof
import check_core_components as checks


class CertificateBoundaryTests(unittest.TestCase):
    def test_build_outputs_do_not_stale_source_but_generator_and_siblings_do(self):
        with tempfile.TemporaryDirectory(prefix='core-runtime-source-') as temporary:
            root = Path(temporary)
            for name in ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml',
                         'crates/zeno-fcis-generated-code-tests/build.rs',
                         'crates/zeno-fcis-generated-code-tests/src/lib.rs'):
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text('original source')
            original = proof.runtime_sources(root)
            output = root / 'crates/zeno-fcis-generated-code-tests/python'
            output.mkdir()
            (output / 'codegen_fixture.py').write_text('generated test adapter')
            (output / 'zcve.py').write_text('generated test codec')
            self.assertEqual(proof.runtime_sources(root), original)
            (output / 'codegen_fixture.py').write_text('regenerated adapter')
            self.assertEqual(proof.runtime_sources(root), original)
            generator = root / 'crates/zeno-fcis-generated-code-tests/build.rs'
            generator.write_text('changed generator source')
            self.assertNotEqual(proof.runtime_sources(root), original)
            generator.write_text('original source')
            sibling = root / 'crates/zeno-fcis-generated-code-tests/python.rs'
            sibling.write_text('additional source beside the output directory')
            self.assertNotEqual(proof.runtime_sources(root), original)

    def qualification_inputs(self, directory):
        root = directory / 'source'
        root.mkdir()
        for family in core.FAMILIES:
            path = root / 'core-components' / family / 'policy.json.in'
            path.parent.mkdir(parents=True)
            path.write_text('original family input')
        names = (*proof.INPUTS, 'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml',
                 'tools/test_core_family_proofs.py', 'tools/test_core_components.py')
        for name in names:
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('original qualification input')
        native = checks.REFERENCE.with_name('native-report.json').read_bytes()
        (root / 'tools/test_data/core_components/native-report.json').write_bytes(native)
        cli = directory / 'cli'
        cli.write_bytes(b'original executable identity')
        return root, cli

    def test_current_build_check_refuses_drift_without_a_success_report(self):
        with tempfile.TemporaryDirectory(prefix='core-current-build-') as temporary:
            directory = Path(temporary)
            root, cli = self.qualification_inputs(directory)
            work = directory / 'qualification'

            def changed_source(*args, **kwargs):
                (root / 'core-components/reservation-pool/policy.json.in').write_text('changed source')
                raise RuntimeError('seeded child failure')

            with patch.object(core, 'ROOT', root), patch.object(checks, 'REFERENCE',
                    root / 'tools/test_data/core_components/reference.py'), \
                    patch.object(checks, 'run', side_effect=changed_source):
                with self.assertRaisesRegex(ValueError, 'family/checker source changed'):
                    checks.finite_proof(cli, work)
            self.assertFalse((work / 'report.json').exists())

    def test_current_build_failure_preserves_the_stored_certificate_set(self):
        with tempfile.TemporaryDirectory(prefix='core-current-build-') as temporary:
            directory = Path(temporary)
            root, cli = self.qualification_inputs(directory)
            stored = root / 'core-components/proofs/reservation-pool.json'
            stored.parent.mkdir()
            stored.write_bytes(b'preserved historical certificate')
            work = directory / 'qualification'
            with patch.object(core, 'ROOT', root), patch.object(checks, 'REFERENCE',
                    root / 'tools/test_data/core_components/reference.py'), \
                    patch.object(checks, 'run', side_effect=RuntimeError('seeded child failure')):
                with self.assertRaisesRegex(RuntimeError, 'seeded child failure'):
                    checks.finite_proof(cli, work)
            self.assertEqual(stored.read_bytes(), b'preserved historical certificate')
            self.assertFalse((work / 'source/core-components/proofs').exists())
            self.assertFalse((work / 'report.json').exists())

    def test_current_build_check_rejects_added_staged_runtime_input(self):
        with tempfile.TemporaryDirectory(prefix='core-current-build-') as temporary:
            directory = Path(temporary)
            root, cli = self.qualification_inputs(directory)
            work = directory / 'qualification'

            def added_input(command, stage, *args, **kwargs):
                extra = stage / 'crates/qualification-extra.txt'
                extra.parent.mkdir(exist_ok=True)
                extra.write_text('unbound additional runtime input')

            with patch.object(core, 'ROOT', root), patch.object(checks, 'REFERENCE',
                    root / 'tools/test_data/core_components/reference.py'), \
                    patch.object(checks, 'run', side_effect=added_input):
                with self.assertRaisesRegex(ValueError, 'compiler/evaluator source changed'):
                    checks.finite_proof(cli, work)
            self.assertFalse((work / 'report.json').exists())

    def test_all_families_are_bound_before_checking_and_drift_is_refused(self):
        with tempfile.TemporaryDirectory(prefix='core-proof-source-') as temporary:
            root = Path(temporary)
            cli = root / 'cli'
            cli.write_bytes(b'original executable identity')
            for family in core.FAMILIES:
                directory = root / 'core-components' / family
                directory.mkdir(parents=True)
                (directory / 'policy.json.in').write_text('{"post":"reserved + quantity"}')
            for name in proof.INPUTS:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text('original checker input')
            for name in ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml'):
                (root / name).write_text('original compiler input')
            with patch.object(core, 'ROOT', root):
                initial = proof.snapshot_inputs(cli)
                proof.require_unchanged_inputs(cli, initial)
                path = root / 'core-components/reservation-pool/policy.json.in'
                path.write_text('{"post":"reserved"}')
                with self.assertRaisesRegex(ValueError, 'family/checker source changed'):
                    proof.require_unchanged_inputs(cli, initial)
                self.assertNotEqual(proof.source_hashes('reservation-pool')['core-components/reservation-pool/policy.json.in'],
                                    initial['families']['reservation-pool']['core-components/reservation-pool/policy.json.in'])
                path.write_text('{"post":"reserved + quantity"}')
                (root / 'Cargo.lock').write_text('changed compiler dependency')
                with self.assertRaisesRegex(ValueError, 'compiler/evaluator source changed'):
                    proof.require_unchanged_inputs(cli, initial)

    def test_executable_drift_cannot_replace_the_initial_binding(self):
        with tempfile.TemporaryDirectory(prefix='core-proof-binary-') as temporary:
            cli = Path(temporary) / 'cli'
            cli.write_bytes(b'original executable identity')
            initial = proof.snapshot_inputs(cli)
            cli.write_bytes(b'different executable identity')
            with self.assertRaisesRegex(ValueError, 'CLI binary changed'):
                proof.require_unchanged_inputs(cli, initial)

    def test_canonical_report_comparison_rejects_every_changed_claim(self):
        original = {'instances': [{'parameters': {'K': 1}, 'domains': [[0, 1]],
                                   'decision_sha256': 'a' * 64}]}
        changed = copy.deepcopy(original)
        changed['instances'][0]['decision_sha256'] = 'b' * 64
        with self.assertRaises(ValueError):
            proof.compare_certificate(original, changed)
        with self.assertRaises(ValueError):
            proof.compare_certificate(original, {'instances': []})
        changed = copy.deepcopy(original)
        changed['instances'][0]['domains'] = [[False, True]]
        with self.assertRaises(ValueError):
            proof.compare_certificate(original, changed)

    def test_duplicate_or_noncanonical_certificate_data_is_refused(self):
        with tempfile.TemporaryDirectory(prefix='core-proof-data-') as temporary:
            path = Path(temporary) / 'certificate.json'
            for data in (b'{"a":1,"a":2}\n', b'{ "a": 1 }\n'):
                path.write_bytes(data)
                with self.assertRaises(ValueError):
                    proof.load(path)

    def test_replayer_independently_rejects_missing_actual_tuple(self):
        family, parameters = 'approval-queue', {'K': 1}
        files = core.source(family, parameters, include_proof=False)
        manifest = core.read_json(files['core-instance.json'].decode())
        expected = [row for row in proof.checks.expected_cases()
                    if row['family'] == 'approval_queue' and row['parameters'] == parameters]
        packet = {'inputs': {'construction': 'full-domain', 'count': 432, 'domain_size': '432'},
                  'summary': {'refusals': {'count': 0}, 'findings': 0,
                              'examples': {'compared': 432, 'disagreements': 0}},
                  'decision_table': {'rows': []}}
        with self.assertRaisesRegex(ValueError, 'missing actual decision rows'):
            proof.observed_rows(packet, expected, manifest, family)
        self.assertEqual(len(expected), len(list(itertools.product(*manifest['input_domains']))))

    def test_all_refusal_classes_fail_even_if_f2_findings_are_zero(self):
        packet = {'inputs': {'construction': 'full-domain', 'count': 432, 'domain_size': '432'},
                  'summary': {'refusals': {'count': 1}, 'findings': 0}, 'decision_table': {}}
        files = core.source('approval-queue', {'K': 1}, include_proof=False)
        manifest = core.read_json(files['core-instance.json'].decode())
        with self.assertRaisesRegex(ValueError, 'technical refusal'):
            proof.observed_rows(packet, [], manifest, 'approval-queue')

    def test_reject_all_cannot_acquire_a_finite_certificate(self):
        family = 'versioned-register'
        files = core.source(family, {'C': 1}, include_proof=False)
        manifest = core.read_json(files['core-instance.json'].decode())
        values = list(itertools.product(*manifest['input_domains']))
        # Even a matching reference and clean-looking packet cannot establish
        # useful behavior without an accepted Write in the full actual table.
        expected = [{'input': list(row), 'expected': {'class': 'Reject', 'reason': 200,
                    'post': list(row[:2]), 'deliveries': []}} for row in values]
        count = len(values)
        packet = {'inputs': {'construction': 'full-domain', 'count': count, 'domain_size': str(count)},
                  'summary': {'refusals': {'count': 0}, 'findings': 0,
                              'examples': {'compared': count, 'disagreements': 0}},
                  'decision_table': {'rows': [' '.join(map(str, row)) + ' | Reject 200 0 0' for row in values],
                                     'post_states': [{'fields': []}], 'outboxes': [{'deliveries': []}]}}
        with self.assertRaisesRegex(ValueError, 'vacuous command behavior'):
            proof.observed_rows(packet, expected, manifest, family)


if __name__ == '__main__':
    unittest.main()
