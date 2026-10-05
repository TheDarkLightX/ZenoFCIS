"""Independent development-fixture sanity checks; not a product checker.

This host oracle interprets the proposed node arrays using explicit checked-i64
rules. It does not establish executable Rust correspondence, canonical-byte
cost, optimization quality, or whole-application equivalence.
"""
import hashlib
import itertools
import json
import re
import sys
from pathlib import Path

MIN_I64, MAX_I64 = -(1 << 63), (1 << 63) - 1
BOOL_OPS = {'Input', 'Bool', 'And', 'Not', 'Select'}
OP_ARITY = {'Input': 1, 'Int': 1, 'Bool': 1, 'Add': 2, 'Sub': 2,
            'Eq': 2, 'Lt': 2, 'And': 2, 'Not': 1, 'Select': 3}


def integer(value):
    assert isinstance(value, str), ('i64 must use a string', value)
    assert re.fullmatch(r'0|-?[1-9][0-9]*', value), value
    result = int(value)
    assert MIN_I64 <= result <= MAX_I64, value
    return result


def domain(spec):
    if spec['kind'] == 'Bool':
        return 'Bool', 0, 1
    assert spec['kind'] == 'Int', spec
    low, high = integer(spec['min']), integer(spec['max'])
    assert low <= high, spec
    return 'Int', low, high


def admission(program, inputs, outputs, boolean):
    nodes, roots = program['nodes'], program['roots']
    assert len(inputs) <= 32
    assert 1 <= len(nodes) <= 256 and len(roots) == len(outputs)
    assert 1 <= len(outputs) <= 16
    if boolean:
        assert len(inputs) <= 6
        assert all(d[0] == 'Bool' for d in inputs + outputs)
    types = []
    for index, node in enumerate(nodes):
        opcode, args = node[0], node[1:]
        assert opcode in OP_ARITY and len(args) == OP_ARITY[opcode], node
        if boolean:
            assert opcode in BOOL_OPS, node
        if opcode == 'Input':
            assert type(args[0]) is int and 0 <= args[0] < len(inputs), node
            kind = inputs[args[0]][0]
        elif opcode == 'Int':
            integer(args[0])
            kind = 'Int'
        elif opcode == 'Bool':
            assert type(args[0]) is bool, node
            kind = 'Bool'
        else:
            assert all(type(a) is int and 0 <= a < index for a in args), node
            operand_types = [types[a] for a in args]
            if opcode in {'Add', 'Sub', 'Lt'}:
                assert operand_types == ['Int', 'Int'], node
                kind = 'Bool' if opcode == 'Lt' else 'Int'
            elif opcode == 'Eq':
                assert operand_types[0] == operand_types[1], node
                kind = 'Bool'
            elif opcode == 'And':
                assert operand_types == ['Bool', 'Bool'], node
                kind = 'Bool'
            elif opcode == 'Not':
                assert operand_types == ['Bool'], node
                kind = 'Bool'
            else:
                assert operand_types[0] == 'Bool', node
                assert operand_types[1] == operand_types[2], node
                kind = operand_types[1]
        types.append(kind)
    assert all(type(root) is int and 0 <= root < len(nodes) for root in roots)
    assert [types[root] for root in roots] == [d[0] for d in outputs]


def execute(program, values, inputs, outputs):
    if len(values) != len(inputs) or any(not lo <= v <= hi for v, (_, lo, hi) in zip(values, inputs)):
        return ('error', 'InputDomain')
    stack = []
    for opcode, *args in program['nodes']:
        if opcode == 'Input':
            value = values[args[0]]
        elif opcode == 'Int':
            value = integer(args[0])
        elif opcode == 'Bool':
            value = int(args[0])
        elif opcode in {'Add', 'Sub'}:
            a, b = (stack[i] for i in args)
            value = a + b if opcode == 'Add' else a - b
            if not MIN_I64 <= value <= MAX_I64:
                return ('error', 'Arithmetic')
        elif opcode == 'Eq':
            value = int(stack[args[0]] == stack[args[1]])
        elif opcode == 'Lt':
            value = int(stack[args[0]] < stack[args[1]])
        elif opcode == 'And':
            value = int(stack[args[0]] == 1 and stack[args[1]] == 1)
        elif opcode == 'Not':
            value = int(stack[args[0]] == 0)
        else:
            condition, yes, no = (stack[i] for i in args)
            value = yes if condition == 1 else no
        stack.append(value)
    result = tuple(stack[root] for root in program['roots'])
    if any(not lo <= v <= hi for v, (_, lo, hi) in zip(result, outputs)):
        return ('error', 'OutputDomain')
    return ('ok', result)


def external(values, domains):
    return [bool(v) if d[0] == 'Bool' else str(v) for v, d in zip(values, domains)]


def external_observation(observation, outputs):
    kind, value = observation
    return {'ok': external(value, outputs)} if kind == 'ok' else {'error': value}


def main():
    source = Path(sys.argv[1])
    destination = Path(sys.argv[2])
    source_bytes = source.read_bytes()
    packet = json.loads(source_bytes)
    assert packet['schema_version'] == 'zenofcis-benchmark-seeds-v1'
    assert packet['status'] == 'public-development-seeds'
    assert packet['enumeration'] == 'ordered-product-last-input-fastest-v1'
    cases = packet['cases']
    assert len(cases) == 32
    assert len({case['id'] for case in cases}) == 32
    results, profiles, signatures = [], {}, {}
    for case in cases:
        inputs = [domain(d) for d in case['input_domains']]
        outputs = [domain(d) for d in case['output_domains']]
        boolean = 'bool' in case['profile'].lower()
        profiles['Boolean' if boolean else 'checked_i64'] = profiles.get('Boolean' if boolean else 'checked_i64', 0) + 1
        cardinality = 1
        for _, lo, hi in inputs:
            cardinality *= hi - lo + 1
        assert cardinality <= (64 if boolean else 65536), case['id']
        admission(case['original'], inputs, outputs, boolean)
        admission(case['candidate'], inputs, outputs, boolean)
        assert len(case['abi']['inputs']) == len(inputs), case['id']
        assert len(case['abi']['outputs']) == len(outputs), case['id']
        difference = None
        expected_witness = None
        observed_original = []
        for ordinal, values in enumerate(itertools.product(*(range(lo, hi + 1) for _, lo, hi in inputs))):
            original = execute(case['original'], values, inputs, outputs)
            candidate = execute(case['candidate'], values, inputs, outputs)
            observed_original.append(original)
            if boolean:
                assert original[0] == candidate[0] == 'ok', case['id']
            if original != candidate and difference is None:
                difference = {'input': external(values, inputs),
                              'original': external_observation(original, outputs),
                              'candidate': external_observation(candidate, outputs)}
                expected_witness = {'ordinal': ordinal, 'tuple': external(values, inputs),
                                    'original': external_observation(original, outputs),
                                    'candidate': external_observation(candidate, outputs)}
        relation = 'different' if difference else 'equivalent'
        assert ordinal + 1 == cardinality, case['id']
        assert relation == case['expected_relation'].lower(), (case['id'], relation, case['expected_relation'])
        assert case['witness'] == expected_witness, (case['id'], 'stored witness differs', case['witness'], expected_witness)
        signature = hashlib.sha256(json.dumps([inputs, outputs, observed_original], sort_keys=True).encode()).hexdigest()
        signatures.setdefault(signature, []).append(case['id'])
        results.append({'id': case['id'], 'profile': case['profile'], 'tuples_checked': cardinality,
                        'relation': relation, 'first_difference': difference,
                        'stored_witness_checked': True,
                        'original_fixture_nodes': len(case['original']['nodes']),
                        'candidate_fixture_nodes': len(case['candidate']['nodes'])})
    assert profiles == {'Boolean': 16, 'checked_i64': 16}, profiles
    result = {'kind': 'development_fixture_reference_check_only',
              'cases_sha256': hashlib.sha256(source_bytes).hexdigest(),
              'oracle_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              'case_count': len(cases), 'profile_counts': profiles,
              'tuples_checked': sum(r['tuples_checked'] for r in results),
              'same_ordered_original_behavior_groups': [v for v in signatures.values() if len(v) > 1],
              'cases': results, 'success': True,
              'limits': ['Host reference fixture checks, not a qualified production checker.',
                         'No canonical-byte costs, optimizer benchmark, Rust correspondence, or formal proof.',
                         'Public development seeds; no held-out result or Herbie-effectiveness claim.']}
    destination.write_text(json.dumps(result, indent=2)+'\n')
    print(json.dumps({k: v for k, v in result.items() if k not in {'cases'}}, indent=2))


if __name__ == '__main__':
    main()
