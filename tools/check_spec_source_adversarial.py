#!/usr/bin/env python3
"""Process-isolated parser refusal controls for adversarial source structures.

This executes the compiled public-API observer, not a source-level model.
Timeouts, native crashes, memory failures and unexpected acceptance are failures.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

from check_spec_source_compatibility import DEFAULT_LIMITS, HEADER, PROJECT, observe


def named_entries_project(count):
    source = HEADER + '''type 100 state S; type 101 command C; type 102 context X;
type 103 key K; type 104 data V; type 105 bool B; field 140 104 value 105;
map 110 100 items 103 104 capacity 2048;\n'''
    for index in range(count):
        source += (f'field {10000 + index} 101 key_{index} 103; '
                   f'entry {20000 + 2 * index} entry_{index} of 110 '
                   f'at command.101.{10000 + index};\n')
    return source


def cases():
    # Each local source nesting and operator chain is small, but their composed
    # AST height is much larger than either independent source limit.
    composed = '0'
    for _ in range(20):
        composed = '(' + composed + ' + 0' * 30 + ')'
    yield 'composed-chain-height', 'project', HEADER + f'law 2 deep = {composed} == 0;', ''
    yield 'composed-chain-malformed-tail', 'project', HEADER + f'law 2 deep = {composed} == 0; law', ''
    yield 'huge-prefix-invalid-character', 'project', HEADER + f'law 2 deep = {composed} == 0; @', ''
    yield 'deep-parentheses', 'project', HEADER + 'law 2 deep = ' + '(' * 300 + 'true' + ')' * 300 + ';', ''
    yield 'deep-unary-prefix', 'project', HEADER + 'law 2 deep = ' + 'not ' * 300 + 'true;', ''
    yield 'long-implication-chain', 'project', HEADER + 'law 2 deep = ' + 'true -> ' * 300 + 'true;', ''
    yield 'long-conjunction-chain', 'project', HEADER + 'law 2 deep = ' + 'true && ' * 300 + 'true;', ''
    yield 'long-product-chain', 'project', HEADER + 'law 2 deep = ' + '1 * ' * 300 + '1 == 1;', ''
    too_long = 'pre' + '.1' * 65
    yield 'lost-projection-suffix-bound-variable', 'project', HEADER + f'law 2 path = forall pre in 0..1 {{ {too_long} == 0 }};', ''
    for action in ['formula', 'law_formula', 'table_expression']:
        yield 'standalone-' + action + '-composed-chain', action, PROJECT, f'{composed} == 0'
        yield 'standalone-' + action + '-trailing-declaration', action, PROJECT, 'true; law 900 injected = true;'
    yield 'table-read-deep-index', 'table_read', PROJECT, 'fees[' + '(' * 300 + '0' + ')' * 300 + ']'
    yield 'derived-rounds-node-overflow', 'project', HEADER + '''type 100 state S; type 101 int I in 0..=63;
table 200 100 items 101 size 64;
derived table 700 reach[i][j] in table 200 rounds 4096 { when true then i == j; default false; };''', ''
    expanded = 'pre.100.200[i0]'
    for index in range(4):
        expanded = f'sum i{index} in table 200 {{ {expanded} }}'
    yield 'nested-table-expansion-work', 'project', HEADER + f'''type 100 state S;
type 101 int I in 0..=63; table 200 100 items 101 size 64;
law 900 huge = {expanded} >= 0;''', ''
    # Named entries are not bounded by the fixed table-entry count. Both the
    # project and the standalone APIs must use the actual ambient map count.
    for count in [300, 1000]:
        source = named_entries_project(count)
        formula = 'forall e in entries 110 { post.100.e == 1 }'
        yield f'named-entry-project-{count}', 'project', source + f'law 900 all = {formula};', ''
        for action in ['formula', 'law_formula', 'table_expression']:
            yield f'named-entry-{action}-{count}', action, source, formula
    word_project = HEADER + '''type 2 state State;
type 3 u128 Word in 0..=255; field 4 2 value 3;'''
    for operation in ['complement(8, pre.2.4)', 'join(8, pre.2.4, pre.2.4)',
                      'meet(8, pre.2.4, pre.2.4)']:
        expression = operation + ' + u128(0)' * 255 + ' == u128(0)'
        name = operation.split('(', 1)[0]
        yield f'bit-expansion-{name}-project', 'project', word_project + f' law 900 deep = {expression};', ''
        yield f'bit-expansion-{name}-ambient', 'table_expression', word_project, expression
    arithmetic = ' + '.join(['1'] * 255)
    expression = f'choose(circuit {{ formula {{ {arithmetic} == 1 }}; }} output 0, 1, 0) == 1'
    yield 'circuit-expansion-project', 'project', HEADER + f'law 2 deep = {expression};', ''
    yield 'circuit-expansion-ambient', 'table_expression', HEADER, expression
    arithmetic = 'pre.100.120' + ' + 0' * 180
    derived_project = HEADER + f'''type 100 state S; type 101 int I in 0..=1;
field 120 100 value 101; table 200 100 items 101 size 1;
derived table 700 words[i] in table 200 bits 1 rounds 1 {{
when {arithmetic} == 0 then u128(1); default u128(0); }};'''
    expression = 'words[0]' + ' + u128(0)' * 100 + ' == u128(0)'
    yield 'derived-height-project', 'project', derived_project + f'law 900 deep = {expression};', ''
    yield 'derived-height-ambient', 'table_expression', derived_project, expression


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--observer', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    records=[]
    for index, (name, action, source, payload) in enumerate(cases()):
        source_path=out/f'{index:02}-{name}.zeno'
        payload_path=out/f'{index:02}-{name}.expression'
        source_path.write_text(source)
        payload_path.write_text(payload)
        observation, stdout, stderr=observe(args.observer.resolve(),action,source_path,payload_path,DEFAULT_LIMITS)
        passed=observation['status']=='ERR'
        if name.startswith(('named-entry-', 'bit-expansion-', 'circuit-expansion-', 'derived-height-')):
            passed = passed and b'ResourceLimit' in stdout
        record={'name':name,'action':action,'passed':passed,'observation':observation,
                'source_sha256':hashlib.sha256(source.encode()).hexdigest(),
                'payload_sha256':hashlib.sha256(payload.encode()).hexdigest()}
        records.append(record)
        (out/f'{index:02}-{name}.stdout').write_bytes(stdout)
        if stderr:
            (out/f'{index:02}-{name}.stderr').write_bytes(stderr)
        print(json.dumps({'name':name,'status':observation['status'],'passed':passed}),flush=True)
    report={'observer':str(args.observer.resolve()),'observer_sha256':hashlib.sha256(args.observer.read_bytes()).hexdigest(),
            'driver_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
            'cases':len(records),'passed':sum(record['passed'] for record in records),'records':records}
    (out/'report.json').write_text(json.dumps(report,indent=2)+'\n')
    raise SystemExit(0 if report['passed']==report['cases'] else 1)


if __name__=='__main__':
    main()
