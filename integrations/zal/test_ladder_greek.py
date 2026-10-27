"""Greek display aliases preserve ASCII canonical names and finite meanings."""
from itertools import product
import json
from pathlib import Path
import tempfile
import unittest

import ladder as l
from test_ladder import small_data

HERE = Path(__file__).resolve().parent
# Independent display goldens, including the conventional mathematical variants.
LETTERS = (
    ('alpha', 'α'), ('beta', 'β'), ('gamma', 'γ'), ('delta', 'δ'),
    ('epsilon', 'ϵ'), ('zeta', 'ζ'), ('eta', 'η'), ('theta', 'θ'),
    ('iota', 'ι'), ('kappa', 'κ'), ('lambda', 'λ'), ('mu', 'μ'),
    ('nu', 'ν'), ('xi', 'ξ'), ('omicron', 'ο'), ('pi', 'π'),
    ('rho', 'ρ'), ('sigma', 'σ'), ('tau', 'τ'), ('upsilon', 'υ'),
    ('phi', 'ϕ'), ('chi', 'χ'), ('psi', 'ψ'), ('omega', 'ω'),
    ('Alpha', 'Α'), ('Beta', 'Β'), ('Gamma', 'Γ'), ('Delta', 'Δ'),
    ('Epsilon', 'Ε'), ('Zeta', 'Ζ'), ('Eta', 'Η'), ('Theta', 'Θ'),
    ('Iota', 'Ι'), ('Kappa', 'Κ'), ('Lambda', 'Λ'), ('Mu', 'Μ'),
    ('Nu', 'Ν'), ('Xi', 'Ξ'), ('Omicron', 'Ο'), ('Pi', 'Π'),
    ('Rho', 'Ρ'), ('Sigma', 'Σ'), ('Tau', 'Τ'), ('Upsilon', 'Υ'),
    ('Phi', 'Φ'), ('Chi', 'Χ'), ('Psi', 'Ψ'), ('Omega', 'Ω'),
    ('varepsilon', 'ε'), ('varphi', 'φ'), ('vartheta', 'ϑ'),
)


class GreekIdentifierTests(unittest.TestCase):
    def check_views(self, data, expected_formula, domain='D', predicate='P'):
        ctx = l.Context.from_data(data)
        tree = l.parse(data['formula'], 'typed', ctx)
        self.assertEqual(l.render(tree, 'formula', ctx), expected_formula)
        self.assertTrue(l.render(tree, 'typed', ctx).isascii())
        self.assertEqual(l.render(tree, 'typed', ctx), data['formula'])
        self.assertEqual(l.syntax(expected_formula), l.syntax(data['formula']))
        checks = 0
        for rung in l.RUNGS:
            text = l.render(tree, rung, ctx)
            parsed = l.parse(text, rung, ctx)
            self.assertEqual(l.normal(parsed, ctx), l.normal(tree, ctx), (rung, text))
            self.assertEqual(l.render(parsed, rung, ctx), text)
            # All interpretations of one predicate on the declared two-member domain.
            for selected in product((False, True), repeat=2):
                fixture = {'domains': {domain: [0, 1]}, 'symbols': {
                    predicate: {'rows': [[[i], value] for i, value in enumerate(selected)]}}}
                self.assertIs(l.evaluate(parsed, ctx, fixture), all(selected), (rung, selected, text))
                checks += 1
        return ctx, tree, checks

    def test_no_free_value_formula_and_legend(self):
        ctx = l.load(HERE / 'examples/ladders/no_free_value.json')
        tree = l.parse(ctx.data['formula'], 'typed', ctx)
        formula = l.render(tree, 'formula', ctx)
        self.assertTrue(formula.startswith('∀σ ∈ Σ :'), formula)
        self.assertNotIn('sigma', formula)
        self.assertNotIn('Sigma', formula)
        legend = {key: (read, meaning) for key, read, meaning in l.legend(tree, ctx)}
        self.assertEqual(legend['σ'][0], 'sigma')
        self.assertEqual(legend['Σ'][0], 'Sigma')
        self.assertTrue(l.render(tree, 'typed', ctx).startswith('forall sigma in Sigma:'))
        self.assertIn('sigma in Sigma', l.render(tree, 'formal', ctx))
        for rung in l.RUNGS:
            text = l.render(tree, rung, ctx)
            if rung != 'formula':
                self.assertNotIn('σ', text)
                self.assertNotIn('Σ', text)
            parsed = l.parse(text, rung, ctx)
            self.assertEqual(l.normal(parsed, ctx), l.normal(tree, ctx))
            for fixture in ctx.data['fixtures']:
                self.assertIs(l.evaluate(parsed, ctx, fixture), fixture['expected'])
        # Either spelling is admitted on either symbolic input rung.
        for rung in ('formula', 'typed'):
            self.assertEqual(l.parse(formula, rung, ctx), l.parse(ctx.data['formula'], rung, ctx))
        with tempfile.TemporaryDirectory() as directory:
            paths = [Path(directory) / (name + '.json') for name in ('ascii', 'unicode')]
            data = l.plain_data(ctx.data)
            for path, text in zip(paths, (data['formula'], formula), strict=True):
                path.write_text(json.dumps({**data, 'formula': text}))
            self.assertEqual(l.meaning_file(paths[0]), l.meaning_file(paths[1]))

    def test_all_greek_names_as_variables_domains_and_symbols(self):
        checks = 0
        for name, glyph in LETTERS:
            with self.subTest(name=name, role='variable'):
                data = small_data()
                data['formula'] = f'forall {name} in D: P({name})'
                ctx, tree, count = self.check_views(data, f'∀{glyph} ∈ D : P({glyph})')
                self.assertIn((glyph, name, 'bound variable; its domain is stated by the quantifier'), l.legend(tree, ctx))
                checks += count
            with self.subTest(name=name, role='domain'):
                data = small_data()
                data['domains'] = {name: data['domains']['D']}
                for symbol in data['symbols'].values():
                    symbol['args'] = [name for _ in symbol['args']]
                for definition in data['definitions']:
                    for param in definition['params']:
                        param['domain'] = name
                data['formula'] = f'forall x in {name}: P(x)'
                ctx, tree, count = self.check_views(data, f'∀x ∈ {glyph} : P(x)', domain=name)
                self.assertEqual(next(row[1] for row in l.legend(tree, ctx) if row[0] == glyph), name)
                checks += count
            with self.subTest(name=name, role='symbol'):
                data = small_data()
                data['symbols'][name] = data['symbols'].pop('P')
                data['formula'] = f'forall x in D: {name}(x)'
                ctx, tree, count = self.check_views(data, f'∀x ∈ D : {glyph}(x)', predicate=name)
                self.assertEqual(next(row[1] for row in l.legend(tree, ctx) if row[0] == glyph + '(x)'), name + ' of x')
                checks += count
        print(f'GREEK ALIASES: {len(LETTERS)} names, three identifier roles, {checks} finite rung/valuation checks')

    def test_longer_identifiers_are_unchanged(self):
        data = small_data()
        data['symbols']['deltaPrice'] = data['symbols'].pop('P')
        data['formula'] = 'forall alphabet in D: deltaPrice(alphabet)'
        self.check_views(data, '∀alphabet ∈ D : deltaPrice(alphabet)', predicate='deltaPrice')
        for name in ('alphabet', 'deltaPrice', 'alpha2', 'Alpha_beta', 'sigma_total', 'varphiValue'):
            self.assertEqual(l.formula_identifier(name), name)
            self.assertEqual(l.tokenize(name), (name,))
        self.assertEqual(l.tokenize('αPrice'), ('αPrice',))

    def test_primed_function_and_definition_names_keep_canonical_identity(self):
        for name, glyph in (('theta', 'θ'), ('varepsilon', 'ε'), ('Phi', 'Φ')):
            data = small_data()
            data['definitions'][0]['name'] = name
            data['formula'] = f"forall sigma in D: {name}'(sigma) >= {name}(sigma)"
            ctx = l.Context.from_data(data)
            tree = l.parse(data['formula'], 'typed', ctx)
            self.assertEqual(l.render(tree, 'formula', ctx), f'∀σ ∈ D : {glyph}′(σ) ≥ {glyph}(σ)')
            for rung in l.RUNGS:
                parsed = l.parse(l.render(tree, rung, ctx), rung, ctx)
                self.assertEqual(l.normal(parsed, ctx), l.normal(tree, ctx))
                for before, after in product(range(2), repeat=2):
                    fixture = {'domains': {'D': [0]}, 'symbols': {
                        'f': {'rows': [[[0], before]]}, "f'": {'rows': [[[0], after]]}}}
                    self.assertIs(l.evaluate(parsed, ctx, fixture), after >= before)


if __name__ == '__main__':
    unittest.main()
