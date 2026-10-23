"""Actual CLI reporting/reader regressions; the Rust workspace gate supplies CLI."""
import json
import os
import re
from pathlib import Path
import subprocess
import tempfile
import unittest

import loop_doors as D
import loop_language as Z
import loop_learn as L
import loop_workspace as W
import report_numbers as N

ENDPOINTS = [-(1 << 127), (1 << 127) - 1, -(1 << 100), 1 << 100, 1 << 64,
             -(1 << 63), (1 << 63) - 1, 0, 5]


def contract(folder, value):
    (folder / 'v2').mkdir(parents=True)
    (folder / 'project.zeno').write_text(f'''zeno 1;
project 1 wide_reports;
namespace 2 core;
type 100 state Counter;
type 101 command Request;
type 102 context Environment;
type 105 int Value in 0..=1;
type 106 int Amount in {value}..={value};
field 110 100 value 105;
field 111 100 anchor 106;
field 120 101 amount 106;
reason 201 outside_scope precedence 0;
component 400 calculate {{ owns 100; reads pre.100; writes post.100; contexts context.102; budget steps 256; }}
merge [400];
law 500 nonnegative on commit, genesis = post.100.110 >= 0;
''')
    policy = {'schema': 'zeno-fcis/template-declarative-policy/2', 'template': 'wide-reports',
              'roots': {'state': 100, 'command': 101, 'context': 102}, 'leaf_bindings': {'102': ['I128', 0, 0]},
              'variables': {'amount': 'command.101.120', 'value': 'pre.100.110', 'anchor': 'pre.100.111'},
              'cases': [{'when': True, 'class': 'Accept', 'reason': None, 'post': {'110': 'value', '111': value}, 'outbox': []},
                        {'when': True, 'class': 'Reject', 'reason': 201, 'post': {}, 'outbox': []}],
              'genesis': {'110': 0, '111': value}, 'law_kinds': {'500': 'StateInvariant'}}
    (folder / 'v2/policy.json').write_text(json.dumps(policy))
    return folder


def declaration(value):
    return {'schema': L.SCHEMA, 'inputs': [{'name': 'x', 'domain': {'kind': 'Int', 'min': value, 'max': value}}],
            'outputs': [{'kind': 'Int', 'min': min(0, value), 'max': max(0, value)}], 'max_size': 1,
            'vocabulary': {'constants': [0], 'arithmetic': [], 'compare': [], 'boolean': []}, 'examples': []}


class WideCliReports(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cli = os.environ.get('ZENO_WIDE_REPORT_TEST_CLI')
        if not cli:
            raise unittest.SkipTest('actual CLI supplied by the Rust wide_reports integration gate')
        cls.cli = str(Path(cli).resolve())

    def setUp(self):
        self.enterContext(D.disable_automatic_knowledge())
        self.temp = tempfile.TemporaryDirectory(prefix='zeno-wide-report-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def run_cli(self, *args, exits=(0,)):
        done = subprocess.run([self.cli, '--no-kb', *map(str, args)], capture_output=True, text=True, timeout=40)
        self.assertIn(done.returncode, exits, (args, done.stdout, done.stderr))
        report = json.loads(done.stdout)
        self.assertEqual(report.get('authority'), 'none', report)
        self.assertNotIn('panicked at', done.stderr)
        return report

    def assert_named(self, tree, key, value):
        found = []
        def visit(node):
            if isinstance(node, dict):
                if key in node and (type(node[key]) is int or (type(node[key]) is str and re.fullmatch(r'-?(?:0|[1-9][0-9]*)', node[key]))):
                    found.append(N.integer(node[key]))
                for child in node.values(): visit(child)
            elif isinstance(node, list):
                for child in node: visit(child)
        visit(tree)
        self.assertIn(value, found, tree)

    def test_returned_questions_teach_back_examples_and_successors_at_every_endpoint(self):
        for index, value in enumerate(ENDPOINTS):
            with self.subTest(value=value):
                folder = contract(self.root / f'contract-{index}', value)
                question = self.run_cli('loop', 'question', '--contract', folder)
                self.assertEqual(question['schema'], 'zeno-fcis/loop-doors/2')
                self.assertEqual(question['integer_encoding'], N.ENCODING)
                self.assertEqual(question['status'], 'open')
                self.assertEqual(N.integer(question['detail']['question']['example']['amount']), value)
                self.assert_named(question, 'anchor', value)
                rules = self.root / f'rules-{index}.json'
                rules.write_text(json.dumps({'schema': 'zeno-fcis/loop-rules/1', 'rules': [
                    {'id': 'wrong', 'where': 'true', 'class': 'Accept', 'reason': None,
                     'sets': {'value': '1 - value', 'anchor': str(value)}}]}))
                judged = self.run_cli('loop', 'teach-back', '--contract', folder, '--rule', rules, exits=(1,))
                self.assertEqual(judged['detail']['rules'][0]['verdict'], 'wrong')
                self.assertGreater(judged['detail']['rules'][0]['exceptions'], 0)
                self.assert_named(judged, 'amount', value)
                self.assert_named(judged, 'anchor', value)

    def test_learn_questions_observations_and_confirmed_workspace_examples_round_trip(self):
        for index, value in enumerate(ENDPOINTS):
            with self.subTest(value=value):
                spec = self.root / f'learn-{index}.json';spec.write_text(json.dumps(declaration(value)))
                report = self.run_cli('loop', 'learn', '--spec', spec, exits=(0,) if value == 0 else (2,))
                if value == 0:
                    self.assertEqual(report['status'], 'determined');continue
                self.assertEqual(report['status'], 'question')
                q = report['detail']['question']
                self.assertEqual(N.integers(q['input']), (value,))
                observations = [N.integer(b['observation']['value']) for b in q['buckets'][0]]
                self.assertEqual(set(observations), {0, value})
                workspace = W.LoopWorkspace(self.root / f'learn-{index}.workspace', cli=self.cli)
                view = workspace.init_learn(spec)
                self.assertEqual(view['schema'], 'zeno-fcis/zal-loop-view/2')
                self.assertEqual(N.integers(view['open_questions'][0]['input']), (value,))
                state = workspace.load();ctx = Z.Context(state['subject'])
                for event in state['events']:
                    for english in (False, True):
                        self.assertEqual(Z.parse_statement(event['english' if english else 'symbolic'], ctx, english).data(), event['act'])
                qid = view['open_questions'][0]['id'];workspace.answer(qid, f'[{value}]')
                saved = workspace.load()
                self.assertEqual(saved['learn']['confirmed'][0]['input'], [value])
                self.assertEqual(saved['learn']['confirmed'][0]['output'], [value])
                self.assertFalse(workspace.view()['open_questions'])

    def test_contract_judge_witness_vectors_are_lossless_at_every_endpoint(self):
        for index, value in enumerate(ENDPOINTS):
            with self.subTest(value=value):
                folder = contract(self.root / f'judge-{index}', value)
                profile = self.run_cli('contract', 'profile', folder)
                self.assertEqual(N.integers(profile['genesis']), (0, value))
                lemma = self.run_cli('contract', 'lemma', folder, '--transition', '--lemma',
                                     'post.100.110 != pre.100.110', exits=(1,))
                # Every transition keeps value, so the actual counterexample
                # carries the wide anchor in both state and successor.
                text = json.dumps(lemma)
                self.assertIn('counterexample', text)
                self.assertIn(str(value), text)
                probe = self.run_cli('contract', 'probe', 'explain', folder, '--input', f'0 {value} {value} 0')
                self.assertEqual(N.integer(probe['result']['input']['command.101.120']), value)
                # The fixture's first guard is true, so its required fallback
                # is dead. Keep those semantics and check the diagnostic as
                # well as the lossless gap witnesses.
                gaps = self.run_cli('contract', 'review', folder, '--gaps', '--format', 'json', exits=(1,))
                self.assertEqual(gaps['schema'], 'zeno-fcis/contract-review/4')
                self.assertIn(str(value), json.dumps(gaps['gaps']))
                self.assertEqual(gaps['summary']['case_findings'], 1)
                self.assertEqual(gaps['case_coverage']['status'], 'checked')
                finding, = gaps['findings']
                self.assertEqual((finding['case'], finding['kind'], finding['cause']),
                                 (1, 'case-never-decides', 'covered by earlier cases'))
                self.assertEqual(finding['covered_by'], [
                    {'case': 0, 'when': 'true', 'witness': {
                        'ordinal': 0, 'input': ['0', str(value), str(value), '0']}}])
                architecture = self.root / f'architecture-{index}.json'
                architecture.write_text(json.dumps({'schema': 'zeno-fcis/loop-architecture/1', 'components': [
                    {'name': 'calculate', 'owns': ['value', 'anchor'], 'reads': ['amount', 'context'], 'decides': []}]}))
                exported = self.root / f'composed-{index}'
                composed = self.run_cli('loop', 'decompose', '--contract', folder, '--architecture', architecture,
                                        '--compose', '--export', exported)
                self.assertEqual(composed['status'], 'accepted', composed)
                self.assertEqual(composed['detail']['compose']['differ'], 0)
                manifest = exported / 'composition.json'
                # Compiler input documents retain raw integer tokens, decoded
                # here by an exact integer parser, rather than report strings.
                request = json.loads(manifest.read_text())['request']
                amount = next(field for field in request if field['name'] == 'amount')
                self.assertEqual(amount['domain']['min'], value)
                self.assertIs(type(amount['domain']['min']), int)
                checked = self.run_cli('contract', 'check-composition', manifest, '--format', 'json')
                self.assertEqual(checked['status'], 'proved')
                explored = self.run_cli('contract', 'check-composition', manifest, '--joint-table', 8, '--format', 'json')
                self.assertEqual(explored['schema'], 'zeno-fcis/composed-joint-table/2')
                self.assertEqual(N.integers(explored['genesis']), (0, value))
                for row in explored['rows']:
                    self.assertEqual(N.integers(row['state']), (0, value))
                    self.assertEqual(N.integers(row['successor']), (0, value))
                    self.assertEqual(N.integer(row['request'][0]), value)
                bad = json.loads(manifest.read_text());bad['components'][0]['invariant'] = 'false'
                failing = exported / 'failing.json';failing.write_text(json.dumps(bad))
                violated = self.run_cli('contract', 'check-composition', failing, '--format', 'json', exits=(1,))
                self.assertEqual(violated['result']['schema'], 'zeno-fcis/composed-domain-result/2')
                witness = violated['result']['witness']['witness']['genesis']
                self.assertIn(value, [N.integer(v) for v in witness.values()])
                receipt = exported / 'receipt.json'
                self.run_cli('contract', 'check-composition', manifest, '--receipt', receipt, '--format', 'json')
                self.assertEqual(json.loads(receipt.read_text())['schema'], 'zeno-fcis/composed-domain-receipt/2')
                self.run_cli('contract', 'check-composition', manifest, '--replay', receipt, '--format', 'json')
                old = json.loads(receipt.read_text());old['schema'] = 'zeno-fcis/composed-domain-receipt/1'
                legacy = exported / 'old-schema.json';legacy.write_text(receipt.read_text().replace('zeno-fcis/composed-domain-receipt/2', 'zeno-fcis/composed-domain-receipt/1', 1))
                refused = self.run_cli('contract', 'check-composition', manifest, '--replay', legacy, '--format', 'json', exits=(1,))
                self.assertEqual(refused['status'], 'replay-mismatch')
                self.assertEqual(refused['replay'], {'kind': 'malformed', 'form': 'schema'})



    def test_sketch_fills_search_ranges_and_unrealizable_witnesses_are_lossless(self):
        for index, value in enumerate(ENDPOINTS):
            with self.subTest(value=value):
                domain = {'kind': 'Int', 'min': str(value), 'max': str(value)}
                source = self.root / f'identity-{index}.json'
                source.write_text(json.dumps({'inputs': [domain], 'outputs': [domain], 'nodes': [['Input', 0]], 'roots': [0]}))
                encoded = self.root / f'identity-{index}.zcve'
                self.run_cli('loop', 'encode', '--program', source, '--out', encoded)
                session = self.root / f'session-{index}'
                self.run_cli('loop', 'open', '--original', encoded, '--session', session, '--profile', 'checked-i128-v1')
                sketch = self.root / f'sketch-{index}.json'
                sketch.write_text(json.dumps({'schema': 'zeno-fcis/loop-expressions/1', 'vocabulary': {'inputs': ['x']},
                                              'outputs': [f'?c[{value}..{value}]']}))
                completed = self.run_cli('loop', 'sketch', '--session', session, '--sketch', sketch, exits=(0, 1))
                self.assertEqual(N.integer(completed['detail']['door']['holes']['0']['value']), value)
                # A single shared constant cannot satisfy two distinct target
                # values. This yields actual conflicting-input witnesses, not
                # a global-impossibility claim from bounded search exhaustion.
                neighbor = value - 1 if value == (1 << 127) - 1 else value + 1
                conflict_source = self.root / f'conflict-{index}.json'
                conflict_source.write_text(json.dumps({'inputs': [{'kind': 'Bool'}, domain],
                    'outputs': [{'kind': 'Int', 'min': str(min(value, neighbor)), 'max': str(max(value, neighbor))}],
                    'nodes': [['Input', 0], ['Input', 1], ['Int', str(neighbor)], ['Select', 0, 1, 2]], 'roots': [3]}))
                conflict_program = self.root / f'conflict-{index}.zcve'
                self.run_cli('loop', 'encode', '--program', conflict_source, '--out', conflict_program)
                conflict_session = self.root / f'conflict-session-{index}'
                self.run_cli('loop', 'open', '--original', conflict_program, '--session', conflict_session, '--profile', 'checked-i128-v1')
                sketch.write_text(json.dumps({'schema': 'zeno-fcis/loop-expressions/1',
                    'vocabulary': {'inputs': ['flag', 'amount']}, 'outputs': ['?c']}))
                impossible = self.run_cli('loop', 'sketch', '--session', conflict_session, '--sketch', sketch, exits=(0, 1))
                conflicts = impossible['detail']['door']['searched'][0]['conflicting_inputs']
                self.assertEqual(len(conflicts), 2, impossible)
                self.assertEqual({N.integer(row['required_output']) for row in conflicts}, {value, neighbor})
                for row in conflicts:
                    self.assertEqual(N.integer(row['input']['amount']), value)

    def test_actual_contract_workspace_question_display_and_paired_events_round_trip(self):
        for index, value in enumerate(ENDPOINTS[:5]):
            with self.subTest(value=value):
                folder = contract(self.root / f'workspace-contract-{index}', value)
                workspace = W.LoopWorkspace(self.root / f'contract-{index}.workspace', cli=self.cli)
                workspace.init(contract=folder)
                workspace.next_question()
                state = workspace.load();ctx = Z.Context(state['subject'])
                self.assertTrue(workspace.view()['open_questions'])
                self.assertIn(str(value), workspace.packet())
                for event in state['events']:
                    for english in (False, True):
                        self.assertEqual(Z.parse_statement(event['english' if english else 'symbolic'], ctx, english).data(), event['act'])
                self.assertEqual(workspace.view()['integer_encoding'], N.ENCODING)


if __name__ == '__main__':
    unittest.main()
