"""Untrusted proof proposal: replace specific holes with existing checked rules.

This does not accept proofs. Every generated step must pass the unchanged
pinned Carcara library and the final root-contradiction guard.
"""
import argparse
import json
from pathlib import Path
import re


def parse(text):
    tokens = re.findall(r'"(?:""|[^"\\]|\\.)*"|[()]|[^\s()]+', text)
    def value(index):
        if tokens[index] != '(':
            assert tokens[index] != ')'
            return tokens[index], index + 1
        result = []; index += 1
        while index < len(tokens) and tokens[index] != ')':
            item, index = value(index); result.append(item)
        assert index < len(tokens), 'truncated term'
        return result, index + 1
    result, index = value(0)
    assert index == len(tokens), 'trailing term'
    return result


def render(term):
    return '(' + ' '.join(map(render, term)) + ')' if isinstance(term, list) else term


def step(name, clause, rule, premises=None, args=None):
    value = ['step', name, ['cl'] + clause, ':rule', rule]
    if premises: value += [':premises', premises]
    if args: value += [':args', args]
    return value


def flatten(term):
    if not isinstance(term, list): return term
    op, *args = term; args = [flatten(x) for x in args]
    if op in ('and', 'or'):
        expanded = []
        for item in args:
            values = item[1:] if isinstance(item, list) and item[0] == op else [item]
            for value in values:
                if not expanded or value != expanded[-1]: expanded.append(value)
        return expanded[0] if len(expanded) == 1 else [op] + expanded
    return [op] + args


def boolean_equality(name, equality, forward, backward):
    left, right = equality[1:]; neg = lambda x: ['not', x]
    names = [name + '_fcis_eq_' + str(i) for i in range(4)]
    return [step(names[0], [equality, neg(left), neg(right)], 'equiv_neg1'),
            step(names[1], [equality, left, right], 'equiv_neg2'),
            step(names[2], [equality, neg(right)], 'resolution', [names[0], backward]),
            step(names[3], [equality, right], 'resolution', [names[1], forward]),
            step(name, [equality], 'resolution', [names[2], names[3]])]


def permute(name, left, right):
    assert left[0] == right[0] and left[0] in ('and', 'or')
    assert set(map(render, left[1:])) == set(map(render, right[1:]))
    neg = lambda x: ['not', x]; out = []
    for direction, (source, target) in enumerate(((left, right), (right, left))):
        prefix = name + '_fcis_dir_' + str(direction)
        premises = []
        if source[0] == 'and':
            for i, atom in enumerate(source[1:]):
                child = prefix + '_' + str(i); premises.append(child)
                out.append(step(child, [neg(source), atom], 'and_pos', args=[str(i)]))
            premises.append(prefix + '_join')
            out.append(step(premises[-1], [target] + [neg(x) for x in target[1:]], 'and_neg'))
        else:
            premises.append(prefix + '_split')
            out.append(step(premises[-1], [neg(source)] + source[1:], 'or_pos'))
            for i, atom in enumerate(target[1:]):
                child = prefix + '_' + str(i); premises.append(child)
                out.append(step(child, [target, neg(atom)], 'or_neg', args=[str(i)]))
        clause = [neg(left), right] if direction == 0 else [left, neg(right)]
        out.append(step(prefix, clause, 'resolution', premises))
    return out + boolean_equality(name, ['=', left, right], name + '_fcis_dir_0', name + '_fcis_dir_1')


def replacement(command):
    name = command[1]; equality = command[2][1]
    assert equality[0] == '=' and len(equality) == 3
    left, right = equality[1:]; op = left[0] if isinstance(left, list) else None
    if left == right: return [step(name, [equality], 'refl')]
    if op == 'not' and isinstance(left[1], list) and left[1][0] == 'not':
        return [step(name, [equality], 'not_simplify')]
    if op == '=' and left[1] == left[2] and right == 'true':
        return [step(name, [equality], 'eq_simplify')]
    if op == '-' and len(left) == 2:
        return [step(name, [equality], 'unary_minus_simplify')]
    if op == '-' and len(left) == 3:
        a, b = ['<=', left, right], ['<=', right, left]; both = ['and', a, b]
        names = [name + '_fcis_arith_' + str(i) for i in range(6)]
        return [step(names[0], [a], 'la_generic', args=['1']),
                step(names[1], [b], 'la_generic', args=['1']),
                step(names[2], [both, ['not', a], ['not', b]], 'and_neg'),
                step(names[3], [both], 'resolution', names[:3]),
                step(names[4], [['=', equality, both]], 'la_rw_eq'),
                step(names[5], [equality, ['not', both]], 'equiv2', [names[4]]),
                step(name, [equality], 'resolution', [names[5], names[3]])]
    if op == '<=' and isinstance(right, list) and right[0] == '>=':
        return [step(name, [equality], 'comp_simplify')]
    if op == '<=' and isinstance(right, list) and right[0] == 'not' and right[1][0] == '>=':
        # Exact integer boundary. The independent linear-arithmetic checker
        # validates both directions, including integer strictness strengthening.
        a, b = left, right[1]; neg = lambda x: ['not', x]
        names = [name + '_fcis_' + str(i) for i in range(6)]
        return [
            step(names[0], [neg(a), neg(b)], 'la_generic', args=['1', '1']),
            step(names[1], [a, b], 'la_generic', args=['1', '1']),
            step(names[2], [equality, neg(a), neg(neg(b))], 'equiv_neg1'),
            step(names[3], [equality, a, neg(b)], 'equiv_neg2'),
            step(names[4], [equality, b], 'resolution', [names[2], names[1]]),
            step(names[5], [equality, neg(b)], 'resolution', [names[3], names[0]]),
            step(name, [equality], 'resolution', [names[4], names[5]]),
        ]
    if op in ('and', 'or') and isinstance(right, list) and right[0] == op:
        flat = flatten(left)
        if flat != left:
            if flat == right: return [step(name, [equality], 'ac_simp')]
            mid = name + '_fcis_flat'; perm = name + '_fcis_permute'
            return [step(mid, [['=', left, flat]], 'ac_simp')] + permute(perm, flat, right) + [
                    step(name, [equality], 'trans', [mid, perm])]
        return [step(name, [equality], op + '_simplify')]
    raise ValueError('unsupported hole ' + name + ': ' + render(equality))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path); parser.add_argument('destination', type=Path)
    options = parser.parse_args()
    raw = options.source.read_bytes()
    assert len(raw) <= 33554432 and raw.startswith(b'unsat\n(\n') and raw.endswith(b'\n)\n')
    lines = raw[8:-3].decode().splitlines(); out = []; changes = []
    for line in lines:
        if ':rule hole' not in line:
            out.append(line); continue
        command = parse(line)
        assert command[:1] == ['step'] and command[2][0] == 'cl' and len(command[2]) == 2
        assert command[3:5] == [':rule', 'hole'] and command[5:7] == [':args', command[6]]
        assert command[6][0] == '"TRUST_THEORY_REWRITE"' and command[6][1] == command[2][1]
        proposal = replacement(command); out.extend(map(render, proposal))
        changes.append({'id': command[1], 'rules': [value[4] for value in proposal]})
    options.destination.write_text('unsat\n(\n' + '\n'.join(out) + '\n)\n')
    print(json.dumps({'status': 'proposal-only', 'changes': changes, 'independent_check': 'unrun'}))


if __name__ == '__main__': main()
