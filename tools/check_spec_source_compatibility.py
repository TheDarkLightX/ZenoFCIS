#!/usr/bin/env python3
"""Compare complete public parser outputs against a frozen 2.6 reference.

Accepted results must match their complete Debug representation byte for byte.
Refused results compare admission status, because diagnostic wording/order is
intentionally allowed to improve. Crash, timeout and resource failure are never
counted as semantic refusals or successful negative controls.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import resource
import subprocess
import time

from generate_spec_grammar import compile_grammar

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_LIMITS = (1_048_576, 262_144, 256)
HEADER = 'zeno 1; project 1 checks; '
PROJECT = '''zeno 1; project 1 checks;
type 100 state S; type 101 command C; type 102 context X;
type 105 int Amount in 0..=3; type 106 int Index in 0..=1; type 107 bool Flag;
field 120 101 tier 106; field 130 100 result 105;
table 210 100 amounts 105 size 2;
const table 310 fees 105 size 2 [1,2];
const table 320 flags 107 size 2 [true,false];
derived table 700 live[i] in table 210 rounds 2 { when true then fees[i] > 0; default false; };
derived table 701 words[i] in table 210 bits 2 rounds 4 { when true then u128(3); default u128(0); };
derived table 702 pairs[i][j] in table 210 rounds 4 { when true then i == j; default false; };
'''


def sha(data):
    return hashlib.sha256(data).hexdigest()


def process_limits():
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    resource.setrlimit(resource.RLIMIT_AS, (2 * 1024**3, 2 * 1024**3))
    resource.setrlimit(resource.RLIMIT_CPU, (10, 10))


def compile_observer(rustc, library, out):
    command = [str(rustc), '--edition=2024', '-C', 'opt-level=2', '-C', 'overflow-checks=yes',
               '--extern', f'zeno_fcis_spec={library}', '-L', f'dependency={library.parent}',
               str(ROOT / 'tools/spec_source_oracle.rs'), '-o', str(out)]
    completed = subprocess.run(command, capture_output=True, text=True)
    (out.parent / (out.name + '-compile.log')).write_text(completed.stdout + completed.stderr)
    if completed.returncode:
        raise RuntimeError(f'observer compile failed: {out.name}')
    return {'command': command, 'binary_sha256': sha(out.read_bytes()), 'library': str(library),
            'library_sha256': sha(library.read_bytes())}


def observe(binary, action, source_path, payload_path, limits):
    command = [str(binary), action, str(source_path), str(payload_path), *map(str, limits)]
    start = time.monotonic()
    try:
        result = subprocess.run(command, capture_output=True, timeout=12, preexec_fn=process_limits)
    except subprocess.TimeoutExpired as error:
        return {'status': 'timeout', 'seconds': time.monotonic() - start}, error.stdout or b'', error.stderr or b''
    status = result.stdout.split(b'\n', 1)[0].decode(errors='replace') if result.returncode == 0 else 'process_failure'
    return {'status': status, 'returncode': result.returncode, 'seconds': time.monotonic() - start,
            'stdout_sha256': sha(result.stdout), 'stdout_bytes': len(result.stdout),
            'stderr_sha256': sha(result.stderr)}, result.stdout, result.stderr


def cases(root):
    # Every repository .zeno fixture, including the intentionally partial law.
    # Use the tracked corpus so a replay never feeds its own retained input
    # directory back into the next run when --out is inside the checkout.
    tracked = subprocess.run(['git', 'ls-files', '-z', '--', '*.zeno'], cwd=root,
                             capture_output=True, check=True).stdout.decode().split('\0')
    for relative in sorted(path for path in tracked if path):
        path = root / relative
        source = path.read_text()
        name = path.relative_to(root).as_posix()
        yield {'name': 'corpus/project/' + name, 'action': 'project', 'source': source}
        if 'contract-tables' in path.parts or 'collections' in path.parts or 'distribution-laws' in path.parts:
            yield {'name': 'corpus/expand/' + name, 'action': 'expand_tables', 'source': source}
    for action in ['project', 'elaborate', 'expand_tables', 'index_sets']:
        yield {'name': 'public/' + action, 'action': action, 'source': PROJECT, 'expect': 'OK'}
    formulas = [
        'true', '(1 + 2) == 3', '((1 + 2) * 3) == 9', '(1 == 2) || true',
        'not (1 == 2)', 'choose(true, 1, 2) == 1',
        'count i in table 210 { pre.100.210[i] > 0 } <= 2',
        '(count i in table 210 { pre.100.210[i] > 0 } + 1) >= 1',
        'sum i in table 210 where i != 1 { pre.100.210[i] } >= 0',
        'forall i in table 210 { live[i] }', 'fees[command.101.120] > 0',
        'flags[command.101.120]', 'pairs[0][command.101.120]',
        'words[command.101.120] == u128(3)', 'subset(2, u128(1), words[0])',
        'circuit { true; not 0; or 0 1; } output 2',
        'forall count in 0..2 { count < 2 }',
        'forall where in 0..2 { where < 2 }',
    ]
    for action in ['formula', 'law_formula', 'table_expression']:
        for number, formula in enumerate(formulas):
            yield {'name': f'public/{action}/{number}', 'action': action, 'source': PROJECT,
                   'payload': formula, 'expect': 'OK'}
        for number, bad in enumerate(['true;', 'true law 900 injected = true;', '(1 + ) == 3', 'fees[] == 1', 'true + false == 1']):
            yield {'name': f'negative/{action}/{number}', 'action': action, 'source': PROJECT,
                   'payload': bad, 'expect': 'ERR'}
    for number, read in enumerate(['pre.100.210[command.101.120]', 'fees[1 - command.101.120]',
                                   'flags[command.101.120]', 'live[0]', 'pairs[0][1]', 'words[command.101.120]']):
        yield {'name': f'public/table_read/{number}', 'action': 'table_read', 'source': PROJECT,
               'payload': read, 'expect': 'OK'}
    for number, read in enumerate(['fees[0] + 1', 'fees[0] law 900 injected = true;', 'pre.100.210', 'fees[2]']):
        yield {'name': f'negative/table_read/{number}', 'action': 'table_read', 'source': PROJECT,
               'payload': read, 'expect': 'ERR'}
    grammar = json.loads((ROOT/'crates/zeno-fcis-spec/grammar/zeno-v1-extended.json').read_text())
    _, _, _, _, _, _, _, words = compile_grammar(grammar)
    for word in words:
        for mode, formula in [('bound', f'forall {word} in 0..1 {{ {word} == 0 }}'),
                              ('predicate', f'{word}(0)')]:
            yield {'name':f'contextual/{mode}/{word}','action':'formula','source':PROJECT,'payload':formula}
    # The operator-chain guard counts the existing left accumulator as well as
    # each new operator. Preserve the actual 2.6 boundary on both sides.
    for count in [255, 256, 257]:
        for name, formula in [
            ('add', ' + '.join(['1'] * count) + f' == {count}'),
            ('multiply', ' * '.join(['1'] * count) + ' == 1'),
            ('and', ' && '.join(['true'] * count)),
            ('or', ' || '.join(['true'] * count)),
        ]:
            yield {'name':f'boundary/{name}/{count}', 'action':'project',
                   'source':HEADER + f'law 2 edge = {formula};',
                   'expect':'OK' if count <= 256 else 'ERR'}
        for name, formula in [
            ('implies', ' -> '.join(['true'] * count)),
            ('not', 'not ' * count + 'true'),
        ]:
            yield {'name':f'boundary/{name}/{count}', 'action':'project',
                   'source':HEADER + f'law 2 edge = {formula};'}
    # These inputs exercise the safe iterative core route beyond the native
    # semantic parser's conservative recursion threshold. Compare full ASTs,
    # including any 2.6 Boolean sharing/normalization, before retaining a route.
    for depth in [40, 80]:
        formulas = {
            'rel_groups': '(' * depth + '1 == 1' + ')' * depth,
            'value_groups': '(' * depth + '1' + ')' * depth + ' == 1',
            'value_arithmetic_groups': '(' * depth + '1 + 2' + ')' * depth + ' == 3',
            'mixed_relational_groups': '(' * depth + '(1 + 2) == 3' + ')' * depth,
            'predicate_groups': 'p(' + '(' * depth + '1' + ')' * depth + ')',
            'predicate_multiple_groups': 'p(' + '(' * depth + '1' + ')' * depth + ','
                + '(' * depth + '2' + ')' * depth + ')',
            'grouped_signed_minimum': '(' * depth + '-170141183460469231731687303715884105728'
                + ')' * depth + ' == -170141183460469231731687303715884105728',
            # In a scalar group the 2.6 parser treats `true` as a variable;
            # preserve that contextual interpretation in the parsed AST.
            'contextual_true_value': '(' * depth + 'true' + ')' * depth + ' + 1 == 2',
            'not': 'not ' * depth + 'true',
            'negative': '-' * depth + '1 == 1',
        }
        quantified = 'v0 == 0'
        for index in range(depth):
            quantified = f'forall v{index} in 0..1 {{ {quantified} }}'
        formulas['quantified'] = quantified
        for name, formula in formulas.items():
            yield {'name':f'deep/{name}/{depth}','action':'project',
                   'source':HEADER + f'law 2 deep = {formula};','expect':'OK'}
        for name, formula in {
            'parenthesized_minimum_magnitude': '-' + '(' * depth
                + '170141183460469231731687303715884105728' + ')' * depth + ' == 0',
            'nonliteral_unsigned_call': 'u128(' + '(' * depth + '1' + ')' * depth + ') == u128(1)',
            'nonliteral_repeat_count': 'repeat_mul_div_floor(1,1,1,' + '(' * depth + '1'
                + ')' * depth + ') == 1',
        }.items():
            yield {'name':f'deep/semantic_refusal/{name}/{depth}','action':'project',
                   'source':HEADER + f'law 2 deep = {formula};','expect':'ERR'}
        # Parentheses separated by operators are not redundant. These controls
        # expose any compatibility cutoff separately from semantic changes.
        scalar = '1'
        relational = 'true'
        for index in range(depth):
            scalar = '(' + scalar + (' + 2' if index % 2 else ' * 3') + ')'
            relational = '(' + relational + (' || false' if index % 2 else ' && true') + ')'
        for name, formula in [('alternating_scalar_groups',scalar+' == 1'),
                              ('alternating_relational_groups',relational)]:
            yield {'name':f'deep/{name}/{depth}','action':'project',
                   'source':HEADER + f'law 2 deep = {formula};','expect':'OK'}
        for action in ['formula','law_formula','table_expression']:
            for name, formula in [('scalar',formulas['value_arithmetic_groups']),
                                  ('predicate',formulas['predicate_groups'])]:
                yield {'name':f'deep/public/{action}/{name}/{depth}','action':action,
                       'source':PROJECT,'payload':formula,'expect':'OK'}
        for name in ['u128','choose','circuit','join','meet','complement','to_i128','to_u128','table','count']:
            formula = '(' * depth + '1 == 1' + ')' * depth
            yield {'name':f'deep/contextual_project_name/{name}/{depth}','action':'project',
                   'source':f'zeno 1; project 1 {name}; law 2 deep = {formula};','expect':'OK'}
    # Exact token-count trailing trivia is an intentional repair of the baseline.
    for number, trivia in enumerate([' ', '\n', '// retained trailing comment\n', '\t \r\n']):
        yield {'name': f'repair/trailing_trivia/{number}', 'action': 'project', 'source': HEADER + trivia,
               'limits': (4096, 7, 8), 'expect': 'OK', 'intentional_change': 'exact-token-budget trivia'}
    for segments in [1, 64, 65]:
        source = HEADER + 'law 2 path = pre' + '.1' * segments + ' == 0;'
        case = {'name': f'projection/{segments}', 'action': 'project', 'source': source,
                'expect': 'ERR' if segments == 65 else 'OK'}
        if segments == 65:
            case['intentional_change'] = 'complete projection suffix preservation'
        yield case


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline-lib', required=True, type=Path)
    parser.add_argument('--candidate-lib', required=True, type=Path)
    parser.add_argument('--rustc', required=True, type=Path)
    parser.add_argument('--corpus-root', type=Path, default=ROOT)
    parser.add_argument('--out', required=True, type=Path)
    parser.add_argument('--case-prefix', action='append', default=[],
                        help='Run only named case prefixes; repeat for a focused replay.')
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    inputs = out / 'inputs'
    inputs.mkdir(exist_ok=True)
    versions = subprocess.run([str(args.rustc), '--version', '--verbose'], capture_output=True, text=True, check=True).stdout
    builds = {name: compile_observer(args.rustc, path.resolve(), out/name)
              for name, path in [('baseline', args.baseline_lib), ('candidate', args.candidate_lib)]}
    identity_source, identity_payload = out/'identity-source.txt', out/'identity-payload.txt'
    identity_source.write_text(HEADER)
    identity_payload.write_text('')
    for name in builds:
        observation, stdout, stderr = observe(out/name, 'source_identity', identity_source,
                                              identity_payload, DEFAULT_LIMITS)
        if observation['status'] != 'OK' or stderr:
            raise RuntimeError(f'compiled source identity unavailable: {name}')
        builds[name]['compiled_spec_source_digest'] = json.loads(stdout.decode().split('\n')[1])
    records = []
    for number, case in enumerate(cases(args.corpus_root.resolve())):
        if args.case_prefix and not any(case['name'].startswith(prefix) for prefix in args.case_prefix):
            continue
        source = case.pop('source').encode()
        payload = case.pop('payload', '').encode()
        source_path, payload_path = inputs/f'{number:04}-source.zeno', inputs/f'{number:04}-payload.txt'
        source_path.write_bytes(source)
        payload_path.write_bytes(payload)
        limits = case.pop('limits', DEFAULT_LIMITS)
        before, left, left_err = observe(out/'baseline', case['action'], source_path, payload_path, limits)
        after, right, right_err = observe(out/'candidate', case['action'], source_path, payload_path, limits)
        expected = case.get('expect')
        processes_ok = before['status'] in ['OK', 'ERR'] and after['status'] in ['OK', 'ERR']
        same = before['status'] == after['status'] and (before['status'] != 'OK' or left == right)
        known_repair = bool(case.get('intentional_change')) and after['status'] == expected and processes_ok
        passed = processes_ok and (same or known_repair) and (expected is None or after['status'] == expected)
        record = {**case, 'limits': limits, 'source_sha256': sha(source), 'payload_sha256': sha(payload),
                  'baseline': before, 'candidate': after, 'same': same, 'passed': passed}
        records.append(record)
        if not passed:
            for label, content in [('baseline.stdout',left),('candidate.stdout',right),('baseline.stderr',left_err),('candidate.stderr',right_err)]:
                (out/f'{number:04}-{label}').write_bytes(content)
            print(json.dumps({'failure':case['name'],'baseline':before['status'],'candidate':after['status']},sort_keys=True),flush=True)
    report = {'toolchain': versions, 'observer_sha256': sha((ROOT/'tools/spec_source_oracle.rs').read_bytes()),
              'case_prefixes': args.case_prefix,
              'driver_sha256': sha(Path(__file__).read_bytes()), 'builds': builds,
              'grammar_sha256': sha((ROOT/'crates/zeno-fcis-spec/grammar/zeno-v1-extended.json').read_bytes()),
              'cases': len(records), 'passed': sum(x['passed'] for x in records),
              'identical': sum(x['same'] for x in records), 'records': records,
              'scope': 'Exact accepted Debug results; refusal status only. Partial exact2.6 source overlay with full specification crate. No whole-repository or end-to-end formal theorem.'}
    (out/'report.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({key:report[key] for key in ['cases','passed','identical']},sort_keys=True))
    raise SystemExit(0 if report['passed']==report['cases'] else 1)


if __name__ == '__main__':
    main()
