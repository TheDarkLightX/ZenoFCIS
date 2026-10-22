import json
import subprocess
import unittest
from unittest.mock import patch
from pathlib import Path

from behavior import Refusal
import report_numbers as N
import loop_doors as D
import loop_language as Z
import loop_learn as L
import loop_workspace as W
import capture_language as C

VALUES = [-(1 << 127), (1 << 127) - 1, -(1 << 100), 1 << 100, 1 << 64,
          -(1 << 63), (1 << 63) - 1, -N.SAFE - 1, -N.SAFE, -5, -1, 0, 1, 5, N.SAFE, N.SAFE + 1]


def subject(value):
    return L.subject({'schema': L.SCHEMA, 'inputs': [{'name': 'x', 'domain': {'kind': 'Int', 'min': value, 'max': value}}],
                      'outputs': [{'kind': 'Int', 'min': min(0, value), 'max': max(0, value)}],
                      'max_size': 1, 'vocabulary': {'constants': [0]}, 'examples': []})


class ReportIntegers(unittest.TestCase):
    def test_scalar_and_display_json_round_trip_both_extremes_and_controls(self):
        for value in VALUES:
            with self.subTest(value=value):
                encoded = N.encode(value)
                self.assertEqual(N.integer(json.loads(json.dumps(encoded))), value)
                self.assertEqual(type(encoded) is int, -N.SAFE <= value <= N.SAFE)
                ctx = Z.Context(subject(value))
                self.assertEqual(W._int_value(ctx.subject, 'x', encoded), value)
        self.assertEqual(N.encode({'value': True, 'expression': '1', 'variant': 'Ready'}),
                         {'value': True, 'expression': '1', 'variant': 'Ready'})

    def test_aliases_rounding_and_out_of_range_strings_refuse(self):
        for value in [True, False, 1.0, 1.5, None, '+1', '01', '-0', ' 1', '1e2',
                      str(1 << 127), str(-(1 << 127) - 1), 1 << 127, -(1 << 127) - 1]:
            with self.subTest(value=value), self.assertRaises(Refusal):
                N.integer(value)

    def test_paired_language_learn_and_capture_text_round_trip_wide_values(self):
        for value in VALUES:
            ctx = Z.Context(subject(value))
            a = Z.act('learn-question', 'q1', input=(value,), counts=(2,))
            for english in (False, True):
                self.assertEqual(Z.parse_statement(Z.render_act(a, ctx, english), ctx, english), a)
                node = Z.read(str(value), ctx, False)
                self.assertEqual(Z.read(Z.show(node, ctx, True), ctx, True), node)
                cap = Z.act('capture-example', 'c1', input=(value,), output=(value,))
                self.assertEqual(C.parse(C.render(cap, ctx, english), ctx, english), cap)

    def test_evaluate_reader_accepts_full_i128_and_legacy_numeric_controls(self):
        args = (Path('cli'), Path('program'), Path('expressions'), [0])
        for value in VALUES:
            for encoded in [str(value), N.encode(value)]:
                with patch.object(D, 'run', return_value=(0, {'status': 'evaluated', 'detail': {'values': [encoded]}})):
                    self.assertEqual(D.evaluate_expression(*args), value)

    def test_browser_json_parse_and_display_preserve_all_scalar_digits(self):
        tree = {'schema': W.VIEW_SCHEMA, 'integer_encoding': N.ENCODING,
                'points': [{'input': [value], 'observation': {'value': value}} for value in VALUES]}
        wire = json.dumps(N.encode(tree))
        # The actual JavaScript JSON parser/stringifier must preserve each
        # string and every safe numeric control; no BigInt rounding workaround.
        done = subprocess.run(['node', '-e', 'const fs=require("fs"); const v=JSON.parse(fs.readFileSync(0,"utf8")); process.stdout.write(JSON.stringify(v));'],
                              input=wire, text=True, capture_output=True, check=True)
        received = json.loads(done.stdout)
        self.assertEqual(received, N.encode(tree))
        self.assertEqual([N.integer(row['input'][0]) for row in received['points']], VALUES)
        self.assertEqual(tree['points'][0]['input'][0], VALUES[0])

    def test_independent_synthesis_vector_reader_matches_the_same_encoding(self):
        import importlib.util
        import tempfile
        path = Path(__file__).resolve().parents[2] / 'tools/check_synthesis.py'
        spec = importlib.util.spec_from_file_location('synthesis_report_reader', path)
        module = importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
        with tempfile.TemporaryDirectory() as folder:
            vector = Path(folder) / 'vectors.json'
            document = {'schema': 'zeno-fcis/synthesis-vectors/2', 'integer_encoding': N.ENCODING,
                        'cases': [{'input': VALUES, 'output': list(reversed(VALUES))}]}
            vector.write_text(json.dumps(N.encode(document)))
            self.assertEqual(module.read_vectors(vector), document['cases'])
            for bad in [True, 1.0, '+1', '01', '-0', str(1 << 127)]:
                document['cases'][0]['input'] = [bad]
                vector.write_text(json.dumps(document))
                with self.subTest(bad=bad), self.assertRaises(RuntimeError):
                    module.read_vectors(vector)


    def test_independent_capture_and_version_readers_recover_original_tuples(self):
        import sys
        sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'tools'))
        try:
            import capture_reference as capture
            import version_space_reference as versions
            self.assertEqual(capture.report_rows(N.encode([VALUES])), [VALUES])
            bounded = [value for value in VALUES if -(1 << 63) <= value < (1 << 63)]
            q = {'input': bounded, 'answers': [{'output': bounded, 'remaining': 1}]}
            self.assertEqual(versions.report_question(N.encode(q)), q)
            import hashlib
            for value in bounded:
                domain = {'kind': 'Int', 'min': str(value), 'max': str(value)}
                spec = {'inputs': [domain], 'outputs': [domain]}
                rows = [[value]]
                session = {'spec': spec, 'rows': N.encode(rows),
                           'table_sha256': hashlib.sha256(json.dumps(rows, separators=(',', ':')).encode('ascii')).hexdigest()}
                owner = {'schema': 'zeno-fcis/black-box-capture/2', 'integer_encoding': N.ENCODING,
                         'authority': 'none', 'status': 'opened',
                         'detail': {'session': session, 'table_program': {**spec, 'nodes': [['Input', 0]], 'roots': [0]}}}
                self.assertTrue(capture.check_owner_report(owner))
                session['rows'] = [[value + 1]]
                try:
                    self.assertFalse(capture.check_owner_report(owner))
                except ValueError:
                    pass  # Original independent row-domain check can refuse first.

            for bad in [True, 1.0, '+1', '01', '-0', str(1 << 127)]:
                with self.subTest(bad=bad), self.assertRaises(ValueError):
                    capture.report_rows([[bad]])
                with self.subTest(bad=bad), self.assertRaises(ValueError):
                    versions.report_question({'input': [bad], 'answers': []})
        finally:
            sys.path.pop(0)


    def test_independent_composition_table_reader_keeps_full_width_coordinates(self):
        import sys
        sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'tools'))
        try:
            import composition_domain_soundness as composition
            for value in VALUES:
                table = {'state_layout': ['core.value'], 'rows': [
                    {'state': [value], 'request': [value], 'successor': [value], 'outcome': {'kind': 'Accept'}}]}
                got = composition.canonical_rows_rust(N.encode(table))
                self.assertEqual(got, {((('core.value', value),), (value,)): ('Accept', None, None, (('core.value', value),))})
                witness = {'genesis': {'core.value': value}, 'failed': 'reason 1'}
                self.assertEqual(composition.report_witness(N.encode(witness)), witness)
            for bad in [True, 1.0, '+1', '01', '-0', str(1 << 127)]:
                with self.subTest(bad=bad), self.assertRaises(ValueError):
                    composition.report_integer(bad)
        finally:
            sys.path.pop(0)


if __name__ == '__main__':
    unittest.main()
