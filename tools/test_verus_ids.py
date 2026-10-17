"""Checked compiler-ID renaming; uses only parsed fixtures and SHA-256."""
import copy
from pathlib import Path
import unittest

import verus_coverage as coverage
from verus_ids import emit, reference


def source(reads=('10', '11'), local='2'):
    def read(value):
        return f'(ReadPlace (VarIdent "x" (VarIdentDisambiguate RustcId {local})) (UnfinalizedReadKind :preliminary_kind (ReadKind ImmutBor) :id {value}))'
    return f'''(Function (Fun :path example::run) :mode Exec :typ_bounds () :params ()
 :ret (VarIdent "x" (VarIdentDisambiguate RustcId {local})) :require ()
 :ensure (true {read(reads[0])}) :d ()
 :body (Block 10 {read(reads[0])} {read(reads[1])}))'''


def profile(text):
    p = {'namespace': 'example::', 'body_covered_functions': ['example::run']}
    p['functions'] = coverage.inventory(text, p['namespace'], tuple(p['body_covered_functions']))
    p['compiler_id_reference'] = reference(text, p)
    return p


class CheckedIds(unittest.TestCase):
    def test_optional_preliminary_kind_is_preserved_while_every_id_is_renamed(self):
        text = source().replace('(ReadKind ImmutBor)', 'None', 1)
        p = profile(text)
        renamed = source(('800', '900'), '45').replace('(ReadKind ImmutBor)', 'None', 1)
        self.assertEqual(coverage.require_coverage(renamed, p), p['functions'])
        with self.assertRaises(ValueError):
            coverage.require_coverage(renamed.replace('None', '(ReadKind Move)', 1), p)

    def test_bijection_accepts_arbitrary_permutation_with_identical_fingerprints(self):
        original = source(); p = profile(original); before = copy.deepcopy(p)
        for changed in (source(('800', '900'), '45'), source(('11', '10'), '57')):
            self.assertEqual(coverage.require_coverage(changed, p), p['functions'])
        self.assertEqual(p, before)

    def test_known_bad_body_requires_ensures_stay_refused_after_renaming(self):
        p = profile(source()); renamed = source(('800', '900'), '45')
        for before, after in (('Block 10', 'Block 11'), (':require ()', ':require (false)'),
                              (':ensure (true', ':ensure (false'), ('ImmutBor', 'Move'),
                              ('"x"', '"different"')):
            with self.subTest(before=before), self.assertRaises(ValueError):
                coverage.require_coverage(renamed.replace(before, after), p)

    def test_global_read_mapping_cannot_split_or_merge_identities(self):
        p = profile(source())
        for text in (source(('800', '800')), source(('800', '900')).replace(':id 800', ':id 950', 1)):
            with self.subTest(text=text), self.assertRaisesRegex(ValueError, 'nonbijective'):
                coverage.require_coverage(text, p)

    def test_bijection_spans_functions_and_local_ids_are_function_scoped(self):
        original = source() + source().replace('example::run', 'example::other')
        p = {'namespace': 'example::', 'body_covered_functions': ['example::run', 'example::other']}
        p['functions'] = coverage.inventory(original, 'example::', tuple(p['body_covered_functions']))
        p['compiler_id_reference'] = reference(original, p)
        consistent = source(('500', '600'), '3') + source(('500', '600'), '6').replace('example::run', 'example::other')
        self.assertEqual(coverage.require_coverage(consistent, p), p['functions'])
        split = source(('500', '600')) + source(('700', '800')).replace('example::run', 'example::other')
        with self.assertRaisesRegex(ValueError, 'nonbijective'):
            coverage.require_coverage(split, p)

    def test_original_profile_remains_exact_and_bad_reference_cannot_be_created(self):
        p = profile(source()); del p['compiler_id_reference']
        with self.assertRaises(ValueError):
            coverage.require_coverage(source(('800', '900')), p)
        with self.assertRaises(ValueError):
            reference(source().replace('Block 10', 'Block 99'), p)

    def test_malformed_new_or_missing_ids_refuse(self):
        p = profile(source())
        for text in (source(('01', '11')), source(('"10"', '11')),
                     source().replace(':id 10', ':other 10'),
                     source().replace('(ReadKind ImmutBor)', '(ReadKind Move)'),
                     source().replace('(Block 10', '(Block 10 (UnknownCompilerId 20)')):
            with self.subTest(text=text), self.assertRaises(ValueError):
                coverage.require_coverage(text, p)
        bad = copy.deepcopy(p); bad['compiler_id_reference']['functions']['example::run']['sha256'] = 'forged'
        with self.assertRaises(ValueError):
            coverage.require_coverage(source(), bad)

    def test_real_descriptor_fixture_retains_declarations_and_body_guards(self):
        text = (Path(__file__).parent / 'test-data/authority-descriptor-declarations.vir').read_text()
        namespace = 'authority_v2::'; prefix = namespace + coverage.DESCRIPTOR_MODULE
        functions, abstract = coverage.descriptor_inventory(text, namespace, prefix)
        p = {'namespace': namespace, 'functions': functions, 'abstract_declarations': abstract,
             'body_covered_functions': sorted(n for n, r in functions.items() if r['mode'] == 'Exec')}
        p['compiler_id_reference'] = reference(text, p)
        nodes = coverage.parse_vir(text)
        def change(node):
            if not isinstance(node, list): return
            if node and node[0] == 'UnfinalizedReadKind': node[4] = str(int(node[4]) + 991)
            for child in node: change(child)
        change(nodes)
        renamed = '\n'.join(emit(n) for n in nodes)
        self.assertEqual(coverage.require_coverage(renamed, p), functions)
        self.assertEqual(coverage.require_coverage(text, p), functions)
        # Changing a trait declaration's contract is refused even though no
        # executable body exists for the abstract declaration itself.
        changed = copy.deepcopy(nodes)
        decl = next(n for n in changed if n[1][2].endswith('DescriptorData::project'))
        decl[decl.index(':ensure') + 1] = ['true']
        with self.assertRaises(ValueError):
            coverage.require_coverage('\n'.join(emit(n) for n in changed), p)


if __name__ == '__main__': unittest.main()
