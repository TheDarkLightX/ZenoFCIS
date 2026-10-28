#!/usr/bin/env python3
"""Strict public count, model evaluation and fixed-domain comparison ports."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import resource
import subprocess
import sys

ROOT = Path(__file__).resolve().parent
DEFAULT_LIMITS = {'entries': '100000', 'candidates': '1000000', 'work': '10000000',
                  'bits': '65536', 'domain': '10000'}
MAX_BYTES = 1024**2

def sha(data):
    return hashlib.sha256(data).hexdigest()

def canonical(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=True, separators=(',', ':')).encode()

def strict_loads(raw):
    if len(raw) > MAX_BYTES:
        raise ValueError('input exceeds 1 MiB')
    # Before recursive parsing: count delimiters outside strings, respecting escapes.
    depth = 0
    quoted = escaped = False
    for byte in raw:
        if quoted:
            if escaped:
                escaped = False
            elif byte == 92:
                escaped = True
            elif byte == 34:
                quoted = False
        elif byte == 34:
            quoted = True
        elif byte in (91, 123):
            depth += 1
            if depth > 300:
                raise ValueError('JSON nesting exceeds 300')
        elif byte in (93, 125):
            depth -= 1
    def pairs(items):
        obj = {}
        for key, value in items:
            if key in obj:
                raise ValueError('duplicate JSON key: ' + key[:128])
            obj[key] = value
        return obj
    def integer(s):
        if len(s) > 256:
            raise ValueError('JSON number too large')
        return int(s)
    def bad(_):
        raise ValueError('nonfinite JSON number')
    value = json.loads(raw.decode('utf-8'), object_pairs_hook=pairs,
                       parse_int=integer, parse_constant=bad)
    todo = [(value, 0)]
    nodes = 0
    while todo:
        obj, depth = todo.pop()
        nodes += 1
        if nodes > 100000:
            raise ValueError('JSON node budget exceeded')
        if isinstance(obj, dict):
            todo.extend((v, depth + 1) for v in obj.values())
        elif isinstance(obj, list):
            todo.extend((v, depth + 1) for v in obj)
    return value

def preflight_query(query):
    if not isinstance(query, dict):
        raise ValueError('query must be an object')
    formula = query.get('formula')
    todo = [(formula, 1)]
    nodes = 0
    while todo:
        node, depth = todo.pop()
        nodes += 1
        if nodes > 10000 or depth > 256:
            raise ValueError('formula node/depth budget exceeded')
        if isinstance(node, dict):
            for k in ('body', 'left', 'right'):
                if k in node:
                    todo.append((node[k], depth + 1))

def binding():
    record = json.loads((ROOT / 'evidence/build.json').read_text())
    binary = ROOT / '.build/zal-relational-worker'
    if sha(binary.read_bytes()) != record['binary_sha256']:
        raise ValueError('worker binary changed after build')
    actual_sources = {str(p.relative_to(ROOT)) for pattern in ('src/**/*.hs', 'app/*.hs') for p in ROOT.glob(pattern)}
    if actual_sources != set(record['source_sha256']):
        raise ValueError('worker source inventory changed after build')
    for path, digest in record['source_sha256'].items():
        if sha((ROOT / path).read_bytes()) != digest:
            raise ValueError('worker source changed after build: ' + path)
    return binary, record

def child_limits(seconds):
    def apply():
        os.nice(10)
        os.sched_setaffinity(0, set(sorted(os.sched_getaffinity(0))[:12]))
        resource.setrlimit(resource.RLIMIT_AS, (2 * 1024**3,) * 2)
        resource.setrlimit(resource.RLIMIT_CPU, (max(1, int(seconds)),) * 2)
        resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    return apply

def worker(requests, seconds=60):
    binary, record = binding()
    payload = b'\n'.join(canonical(r) for r in requests) + b'\n'
    result = subprocess.run([str(binary), '+RTS', '-M1536m', '-K32m', '-RTS'], input=payload,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=seconds,
                            preexec_fn=child_limits(seconds))
    if result.returncode:
        raise RuntimeError('worker incomplete: exit ' + str(result.returncode) + ' ' + result.stderr.decode(errors='replace')[:500])
    lines = result.stdout.splitlines()
    if len(lines) != len(requests):
        raise RuntimeError('incomplete worker response')
    return [strict_loads(line) for line in lines], record

def count_query(query, limits=None, seconds=60):
    preflight_query(query)
    answers, record = worker([{'query': query, 'limits': limits or DEFAULT_LIMITS, 'mode': 'count', 'model': None}], seconds)
    answer = answers[0]
    answer.update({'schema': 'zal/relational-result/1', 'query_sha256': sha(canonical(query)),
                   'context_id': query.get('context_id'), 'binary_sha256': record['binary_sha256'],
                   'source_sha256': record['source_sha256'], 'shell_sha256': sha(Path(__file__).read_bytes())})
    if answer.get('status') == 'ExactCount':
        from fractions import Fraction
        if str(Fraction(answer['value'])) != answer['value']:
            raise RuntimeError('noncanonical exact response')
        if answer.get('domain_size') != query.get('domain_size'):
            raise RuntimeError('response domain mismatch')
    return answer

FAILURES = {'Invalid', 'Unsupported', 'ResourceExhausted', 'InternalDisagreement'}
COMPARISONS = {'EquivalentAt', 'ImpliesAt', 'RefutedAt', 'VacuousAt'}

def response_natural(value):
    if type(value) is not str or len(value) > 256 or not value.isascii() or not value.isdigit() or (len(value) > 1 and value[0] == '0'):
        raise RuntimeError('noncanonical natural in worker response')
    return int(value)

def validate_comparison(payload, answer, limits):
    expected = {'status', 'holds', 'vacuous', 'relation', 'domain_size', 'enumerated_models',
                'admitted_models', 'partition', 'counterexample', 'work_bound', 'engine', 'scope'}
    if set(answer) != expected or answer['relation'] != payload['relation'] or answer['domain_size'] != payload['domain_size']:
        raise RuntimeError('comparison response schema/scope mismatch')
    if answer['engine'] != 'complete-structure-comparison/1':
        raise RuntimeError('comparison engine mismatch')
    n = response_natural(answer['domain_size'])
    atoms = sum(n ** p['arity'] for p in payload['signature'])
    cap = int(limits['candidates'])
    if atoms >= cap.bit_length():
        raise RuntimeError('comparison claimed completion outside model budget')
    total = response_natural(answer['enumerated_models'])
    allowed = response_natural(answer['admitted_models'])
    histogram = answer['partition']
    if not isinstance(histogram, dict) or set(histogram) != {'both_true', 'left_only', 'right_only', 'both_false'}:
        raise RuntimeError('comparison partition schema mismatch')
    counts = {k: response_natural(v) for k, v in histogram.items()}
    if total != 2 ** atoms or allowed != sum(counts.values()) or allowed > total:
        raise RuntimeError('comparison coverage/partition disagreement')
    if response_natural(answer['work_bound']) > int(limits['work']):
        raise RuntimeError('comparison exceeded declared work budget')
    failed = counts['left_only'] > 0 or (payload['relation'] == 'equivalence' and counts['right_only'] > 0)
    status = 'VacuousAt' if allowed == 0 else 'RefutedAt' if failed else 'EquivalentAt' if payload['relation'] == 'equivalence' else 'ImpliesAt'
    wanted = None if allowed == 0 else not failed
    if answer['status'] != status or answer['holds'] is not wanted or answer['vacuous'] is not (allowed == 0):
        raise RuntimeError('comparison verdict contradicts its partition')
    witness = answer['counterexample']
    if not failed:
        if witness is not None:
            raise RuntimeError('spurious comparison counterexample')
        return
    if not isinstance(witness, dict) or set(witness) != {'model', 'assumptions', 'left', 'right'}:
        raise RuntimeError('missing or malformed counterexample')
    if witness['assumptions'] is not True or type(witness['left']) is not bool or type(witness['right']) is not bool:
        raise RuntimeError('invalid counterexample truth values')
    a, b = witness['left'], witness['right']
    if (a == b if payload['relation'] == 'equivalence' else not (a and not b)):
        raise RuntimeError('counterexample does not refute the relation')
    if counts['left_only' if a else 'right_only'] == 0:
        raise RuntimeError('counterexample absent from partition')
    model = witness['model']
    signature = {p['id']: p['arity'] for p in payload['signature']}
    if not isinstance(model, dict) or set(model) != set(signature):
        raise RuntimeError('counterexample vocabulary mismatch')
    for name, rows in model.items():
        if type(rows) is not list or any(type(row) is not list or len(row) != signature[name] for row in rows):
            raise RuntimeError('counterexample tuple shape mismatch')
        tuples = [tuple(response_natural(v) for v in row) for row in rows]
        if len(set(tuples)) != len(tuples) or any(v >= n for row in tuples for v in row):
            raise RuntimeError('counterexample tuple out of domain or repeated')

def semantic_request(payload, limits=None, seconds=60):
    # Own the request snapshot before crossing the process boundary. Receipt
    # identity must not depend on a caller mutating a retained dictionary.
    payload = strict_loads(canonical(payload))
    if not isinstance(payload, dict):
        raise ValueError('request must be an object')
    schema = payload.get('schema')
    if schema == 'zal/relational-query/1':
        return count_query(payload, limits, seconds)
    lim = dict(DEFAULT_LIMITS if limits is None else limits)
    if schema == 'zal/relational-evaluation/1':
        preflight_query(payload)
        mode = 'evaluate-public'
    elif schema == 'zal/relational-comparison/1':
        for key in ('assumptions', 'left', 'right'):
            preflight_query({'formula': payload.get(key)})
        mode = 'compare'
    else:
        raise ValueError('unsupported public request schema')
    answers, record = worker([{'mode': mode, 'request': payload, 'limits': lim}], seconds)
    answer = answers[0]
    if not isinstance(answer, dict):
        raise RuntimeError('worker result must be an object')
    status = answer.get('status')
    if status in FAILURES:
        if any(k in answer for k in ('value', 'holds', 'counterexample', 'partition')):
            raise RuntimeError('failure contained a semantic success payload')
    elif mode == 'evaluate-public' and status == 'Evaluated':
        if set(answer) != {'status', 'value', 'domain_size', 'work_bound', 'engine'} or type(answer['value']) is not bool or answer['domain_size'] != payload['domain_size']:
            raise RuntimeError('evaluation response schema/scope mismatch')
        if answer['engine'] != 'original-formula-evaluation/1' or response_natural(answer['work_bound']) > int(lim['work']):
            raise RuntimeError('evaluation engine/work mismatch')
    elif mode == 'compare' and status in COMPARISONS:
        validate_comparison(payload, answer, lim)
    else:
        raise RuntimeError('unknown or mismatched worker result')
    answer.update({'schema': 'zal/relational-result/2', 'request_sha256': sha(canonical(payload)),
                   'context_id': payload.get('context_id'), 'limits': dict(lim), 'wall_seconds': seconds,
                   'binary_sha256': record['binary_sha256'], 'source_sha256': record['source_sha256'],
                   'shell_sha256': sha(Path(__file__).read_bytes()),
                   'assurance': 'experimental; source-level proof open'})
    return answer

def exit_status(answer):
    if answer.get('status') in {'ExactCount', 'Evaluated', 'EquivalentAt', 'ImpliesAt'}:
        return 0
    return 1 if answer.get('status') == 'RefutedAt' else 2

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('query', help='count, evaluation or comparison JSON file, or - for stdin')
    parser.add_argument('--seconds', type=int, default=60)
    parser.add_argument('--max-models', type=int, default=1000000, help='comparison/Boolean-assignment cap, 1..1000000')
    args = parser.parse_args()
    try:
        if not 1 <= args.seconds <= 60:
            raise ValueError('seconds must be in 1..60')
        if not 1 <= args.max_models <= 1000000:
            raise ValueError('max-models must be in 1..1000000')
        if args.query == '-':
            raw = sys.stdin.buffer.read(MAX_BYTES + 1)
        else:
            with open(args.query, 'rb') as handle:
                raw = handle.read(MAX_BYTES + 1)
        lim = dict(DEFAULT_LIMITS, candidates=str(args.max_models))
        answer = semantic_request(strict_loads(raw), limits=lim, seconds=args.seconds)
    except (ValueError, KeyError, OSError, TypeError, RecursionError) as error:
        answer = {'status': 'Invalid', 'message': str(error)[:1000]}
    except subprocess.TimeoutExpired:
        answer = {'status': 'ResourceExhausted', 'resource': 'wall time'}
    except RuntimeError as error:
        answer = {'status': 'Incomplete', 'message': str(error)}
    print(json.dumps(answer, sort_keys=True, indent=2))
    raise SystemExit(exit_status(answer))

if __name__ == '__main__':
    main()
