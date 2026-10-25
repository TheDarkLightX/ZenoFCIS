#!/usr/bin/env python3
"""Structural grammar and generation controls; these are not lowering proofs.

The explicit examples exercise each 2.6 surface and the scalar/formula
parenthesis ambiguity against first-order generated decisions. The tiny test
tokenizer is only a fixture reader; production tokenization is independently
checked by the Rust/Verus lexical path.
"""
from __future__ import annotations

import copy
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest

import generate_spec_grammar as generator

ROOT = Path(__file__).resolve().parents[1]
EXTENDED = ROOT / 'crates/zeno-fcis-spec/grammar/zeno-v1-extended.json'
TOKEN = re.compile(r'\s+|//[^\n]*|#[^\n]*|[A-Za-z_][A-Za-z_0-9-]*|0x[0-9a-f]{64}|[0-9]+|&&|\|\||==|!=|<=|>=|->|\.\.|[{}()\[\];,.:=<>+*/!-]')


class Grammar:
    """First-match table interpreter used only for hand-authored examples."""

    def __init__(self, data):
        self.rules, self.terms, self.productions, self.rhs, _, self.decisions, self.ranges, _ = generator.compile_grammar(data)
        self.tags = {word: index for index, word in enumerate(self.terms)}
        self.rule_ids = {rule['name']: index for index, rule in enumerate(self.rules)}

    def recognize(self, source, root='Project'):
        tokens = []
        spelling = []
        cursor = 0
        while cursor < len(source):
            match = TOKEN.match(source, cursor)
            if match is None:
                return False, f'fixture lexer at byte {cursor}: {source[cursor:cursor + 20]!r}'
            word = match.group()
            cursor = match.end()
            if word.isspace() or word.startswith(('#', '//')):
                continue
            if word.startswith('0x'):
                tag = self.tags['HEX']
            elif word[0].isdigit():
                tag = self.tags['NUMBER']
            else:
                tag = self.tags.get(word, self.tags['IDENT'])
            tokens.append(tag)
            spelling.append(word)
        work = [(False, self.rule_ids[root])]
        index = 0
        steps = 0
        while work:
            steps += 1
            if steps > max(1000, len(tokens) * 100):
                return False, 'fixture interpreter work bound'
            terminal, symbol = work.pop()
            first = tokens[index] if index < len(tokens) else self.tags['EOF']
            second = tokens[index + 1] if index + 1 < len(tokens) else self.tags['EOF']
            if terminal:
                if first != symbol or index == len(tokens):
                    return False, f'expected {self.terms[symbol]} at {index}: {spelling[max(0,index - 4):index + 8]}'
                index += 1
                continue
            start, length = self.ranges[symbol]
            production = next((p for lhs, a, b, p in self.decisions[start:start + length] if (a is None or a == first) and (b is None or b == second)), None)
            if production is None:
                return False, f'no {self.rules[symbol]["name"]} at {index}: {spelling[max(0,index - 4):index + 8]}'
            _, start, length = self.productions[production]
            work.extend(reversed(self.rhs[start:start + length]))
        return (index == len(tokens), f'unconsumed at {index}: {spelling[index:index + 8]}')


class ExtendedGrammarTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.data = json.loads(EXTENDED.read_text())
        cls.grammar = Grammar(cls.data)

    def accepts(self, text, root='Project'):
        accepted, reason = self.grammar.recognize(text, root)
        self.assertTrue(accepted, reason)

    def refuses(self, text, root='Project'):
        accepted, _ = self.grammar.recognize(text, root)
        self.assertFalse(accepted, text)

    def project(self, declarations):
        return 'zeno 1; project 1 demo; ' + declarations

    def test_declaration_families(self):
        cases = [
            'type 2 bits Flags width 3; type 3 index Pos of 10; type 4 key Key; type 5 ordered Price in -2..=8; type 6 u128 Wide in 0..=340282366920938463463374607431768211455;',
            'table 10 2 entries 3 size 2; const table 20 limits 3 size 2 [-1, 2]; const table 21 enabled 4 size 2 [true, false]; const table 22 wide 5 size 1 [u128(340282366920938463463374607431768211455)];',
            'map 10 2 balances 3 4 capacity 100; entry 11 sender of 10 at command.5.6; summary 12 2 total 7 sum 10.8; summary 13 2 count 7 count 10; tally 14 matches 10.8 = count 15 by 16 where 17;',
            'order 18 smallest of 10 min into context.2.3 found context.2.4; order 19 next of 10 successor command.5.6 into context.2.3 found context.2.4; order 20 between of 10 range command.5.6 to command.5.7 into context.2.3 found context.2.4;',
            'derived table 20 reach[i][j] in table 10 rounds 4 { when i == j then true; default exists k in table 10 { reach[i][k] && reach[k][j] }; };',
            'derived table 21 permissions[i] in table 10 bits 3 rounds 6 { when true then join j in table 10 { permissions[j] }; default complement(0); };',
            'random 20 coin on context.2.3 bits 2 independent uniform; distribution 21 fair on post.4.5 = uniform over {0, 1,}; distribution 22 weighted on post.4.5 = {0: 1/2, committed-failure: 1/2}; reward 23 value random {20,} on post.4.5 failure -1 { expectation >= 0; variance <= 1/2; threshold 1 = 1/2; excess 0 <= 1; }',
            'law 20 scoped on reject:5, genesis = true; claim 21 live z3 finite 5 = always atom((1 + 2) == 3);',
        ]
        for case in cases:
            with self.subTest(case=case):
                self.accepts(self.project(case))

    def test_grouping_without_trusted_parenthesis_classification(self):
        for text in ['(1 + 2) == 3', '((1 + 2) * 3) == 9', '(1 == 2) || true', '((1 == 2) || false) && not true', 'choose((1 + 2) == 3, 4, 5) == 4', '(count i in table 10 { true } + 1) == 3']:
            with self.subTest(text=text):
                self.accepts(text, 'Rel')

    def test_aggregate_and_index_surfaces(self):
        cases = [
            'forall i in table 10 where i < 2 { pre.2.10[i] >= 0 }',
            'exists e in entries 10 where pre.2.e.4 == 1 { post.2.e.4 == 2 }',
            'forall i in -2..3 { i < 4 }',
            'count i in table 10 { pre.2.10[i] == 1 } <= 2',
            'count i in table 10 == 2',
            'sum i in table 10 where i != 0 { pre.2.10[i] } == 3',
            'sum i in 0..3 { i * i } == 5',
            'subset(3, join(3, 1, 2), complement(3, 0))',
            'join i in table 10 where i != 1 { policy[i] }',
            'circuit { formula { 1 == 1 }; true; not 1; and 0 1; or 2 3; select 0 1 2; } output 5',
            'count == 2', 'sum(1, 2)',
        ]
        for case in cases:
            with self.subTest(case=case):
                self.accepts(case, 'Rel')
        for case in ['pre.2.10[command.3.4 + 1]', 'policy[0][choose(true, 1, 2)]']:
            self.accepts(case, 'TableRead')

    def test_complete_consumption_and_structural_mutations(self):
        bad = [
            'table 10 2 entries 3 size 2',
            'type 2 bits Flags width ;',
            'const table 20 values 3 size 2 [1 2];',
            'derived table 20 reach[i] in table 10 rounds 2 { when true true; default false; };',
            'map 10 2 entries 3 4 100;',
            'random 20 coin on context.2.3 bits 2 uniform;',
            'law 20 test = (1 + ) == 2;',
            'law 20 test = true; trailing',
        ]
        for case in bad:
            with self.subTest(case=case):
                self.refuses(self.project(case))
        for case in ['pre.2.10[0] + 1', 'pre.2.10', 'policy[0', 'policy[][0]']:
            self.refuses(case, 'TableRead')

    def test_structural_acceptance_does_not_claim_types(self):
        # The semantic pass, not this structural relation, refuses these.
        self.accepts('true + false == 1', 'Rel')
        self.accepts('choose(1)', 'Rel')
        self.accepts('count e in entries 110 { post.100.e == 1 } <= 1', 'Rel')
        self.accepts('sum e in entries 110 { post.100.e.140 } <= 3', 'Rel')

    def test_generator_refuses_broken_rules_and_nonconsuming_cycles(self):
        duplicate = copy.deepcopy(self.data)
        duplicate['rules'].append(copy.deepcopy(duplicate['rules'][0]))
        with self.assertRaisesRegex(ValueError, 'duplicate rule'):
            generator.compile_grammar(duplicate)
        cycle = copy.deepcopy(self.data)
        cycle['rules'][0]['alternatives'] = [{'label':'loop','rhs':['Project']}]
        with self.assertRaisesRegex(ValueError, 'left-recursive'):
            generator.compile_grammar(cycle)
        missing = copy.deepcopy(self.data)
        missing['rules'][0]['alternatives'][0]['rhs'].append('MissingRule')
        with self.assertRaisesRegex(ValueError, 'unknown rule'):
            generator.compile_grammar(missing)

    def test_default_freshness_checks_both_grammars(self):
        # An isolated copy prevents this negative control mutating production.
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            paths = ['tools/generate_spec_grammar.py', 'crates/zeno-fcis-spec/grammar/zeno-v1.json', 'crates/zeno-fcis-spec/grammar/zeno-v1-extended.json', 'crates/zeno-fcis-spec/src/syntax_grammar.rs', 'crates/zeno-fcis-spec/src/syntax_grammar_extended.rs']
            for path in paths:
                target=root/path
                target.parent.mkdir(parents=True,exist_ok=True)
                target.write_bytes((ROOT/path).read_bytes())
            command=[sys.executable,str(root/'tools/generate_spec_grammar.py'),'--check']
            self.assertEqual(subprocess.run(command,capture_output=True).returncode,0)
            output=root/'crates/zeno-fcis-spec/src/syntax_grammar_extended.rs'
            output.write_text(output.read_text()+'// stale\n')
            result=subprocess.run(command,capture_output=True,text=True)
            self.assertNotEqual(result.returncode,0)
            self.assertIn('zeno-v1-extended.json',result.stderr)


if __name__ == '__main__':
    unittest.main()
