"""Replay a shared formal request/result; no human approval or source-prose proof."""
import argparse
import json
from pathlib import Path
import sys
import run

def projection(answer):
    status = answer.get('status')
    if status == 'ExactCount':
        keys = ('status', 'value', 'domain_size')
    elif status == 'Evaluated':
        keys = ('status', 'value', 'domain_size')
    elif status in run.COMPARISONS:
        keys = ('status', 'holds', 'vacuous', 'relation', 'domain_size',
                'enumerated_models', 'admitted_models', 'partition', 'counterexample')
    else:
        raise ValueError('only completed semantic results can be replayed')
    return {k: answer[k] for k in keys}

def replay(record, seconds=60):
    request, retained = record['request'], record['answer']
    digest = run.sha(run.canonical(request))
    key = 'query_sha256' if request.get('schema') == 'zal/relational-query/1' else 'request_sha256'
    if retained.get(key) != digest:
        raise ValueError('retained answer is not bound to this formal request')
    expected = projection(retained)
    fresh = run.semantic_request(request, seconds=seconds)
    if fresh.get('status') not in run.COMPARISONS | {'ExactCount', 'Evaluated'}:
        return {'status': 'ReplayIncomplete', 'request_sha256': digest, 'fresh': fresh,
                'scope': 'no completed result for comparison'}
    observed = projection(fresh)
    same = run.canonical(observed) == run.canonical(expected)
    return {'status': 'ReplayedSemanticResult' if same else 'ReplayMismatch',
            'request_sha256': digest, 'retained': expected, 'observed': observed,
            'implementation_matches': retained.get('binary_sha256') == fresh['binary_sha256'],
            'fresh_binding': {k: fresh[k] for k in ('binary_sha256', 'source_sha256', 'shell_sha256')},
            'scope': 'formal request and exact semantic result, including retained counterexample',
            'human_semantic_approval': 'not inferred',
            'source_prose_correspondence': 'not established by replay; inspect retained lowering bindings'}

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('record', type=Path)
    parser.add_argument('--seconds', type=int, default=60)
    args = parser.parse_args()
    try:
        if not 1 <= args.seconds <= 60:
            raise ValueError('seconds must be in 1..60')
        with args.record.open('rb') as handle:
            record = run.strict_loads(handle.read(run.MAX_BYTES + 1))
        result = replay(record, args.seconds)
    except (ValueError, KeyError, TypeError, OSError, RecursionError) as error:
        result = {'status': 'InvalidReplay', 'message': str(error)[:1000]}
    except run.subprocess.TimeoutExpired:
        result = {'status': 'ReplayIncomplete', 'resource': 'wall time'}
    except RuntimeError as error:
        result = {'status': 'ReplayIncomplete', 'message': str(error)[:1000]}
    print(json.dumps(result, indent=2, sort_keys=True))
    sys.exit(0 if result['status'] == 'ReplayedSemanticResult' else 1 if result['status'] == 'ReplayMismatch' else 2)

if __name__ == '__main__': main()
