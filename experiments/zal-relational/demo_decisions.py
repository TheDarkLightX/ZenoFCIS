"""Execute a CNL discussion, inspect its witness and check the stronger rule."""
import argparse
import copy
import json
from pathlib import Path
import subprocess
import sys
import oracle
import run
import replay_result

ROOT = Path(__file__).resolve().parent

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--native-root', type=Path, default=ROOT.parents[1])
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    record_path = args.out/'discussion.json'
    command = [sys.executable, str(ROOT/'ladder_adapter.py'), '--native-root', str(args.native_root),
               '--context', str(ROOT/'examples/owner_cars_context.json'),
               '--rung', 'intuitive', '--text', 'every owner has a car.',
               '--compare-rung', 'typed', '--compare-to',
               'forall x in D: Owner(x) => (exists1 y in D: Car(y) and Has(x, y))',
               '--assuming', '(#{x in D: Owner(x)} = 1) and (forall x in D: not (Owner(x) and Car(x)))',
               '--domain-size', '3', '--out', str(record_path)]
    process = subprocess.run(command, capture_output=True, text=True, timeout=60)
    (args.out/'cli.stdout').write_text(process.stdout)
    (args.out/'cli.stderr').write_text(process.stderr)
    if process.returncode != 1:
        raise AssertionError(('expected a semantic refutation', process.returncode, process.stdout, process.stderr))
    record = json.loads(record_path.read_text())
    request, answer = record['request'], record['answer']
    if answer['status'] != 'RefutedAt': raise AssertionError(answer)
    witness = answer['counterexample']['model']
    model = {p: {tuple(map(int, row)) for row in rows} for p, rows in witness.items()}
    if not oracle.evaluate(request['assumptions'], model, 3): raise AssertionError('witness violates assumptions')
    evaluations = []
    for side, expected in [('left', True), ('right', False)]:
        payload = {k: request[k] for k in ('logic', 'signature', 'domain_size', 'context_id')}
        payload.update(schema='zal/relational-evaluation/1', formula=request[side], model=witness)
        result = run.semantic_request(payload)
        if result.get('status') != 'Evaluated' or result.get('value') is not expected:
            raise AssertionError((side, result))
        if oracle.evaluate(request[side], model, 3) is not expected:
            raise AssertionError(('independent witness disagreement', side))
        (args.out/(side+'-model.json')).write_text(json.dumps(payload, indent=2)+'\n')
        evaluations.append({'side': side, 'request': payload, 'answer': result})
    reverse = copy.deepcopy(request)
    reverse.update(relation='implication', left=request['right'], right=request['left'])
    stronger = run.semantic_request(reverse)
    if stronger.get('status') != 'ImpliesAt': raise AssertionError(stronger)
    (args.out/'stronger-rule.json').write_text(json.dumps(reverse, indent=2)+'\n')
    replayed = replay_result.replay(record)
    if replayed['status'] != 'ReplayedSemanticResult': raise AssertionError(replayed)
    tampered = copy.deepcopy(record); tampered['answer']['counterexample']['left'] = False
    rejected = replay_result.replay(tampered)
    if rejected['status'] != 'ReplayMismatch': raise AssertionError(rejected)
    changed = copy.deepcopy(record); changed['request']['context_id'] += '-changed'
    try: replay_result.replay(changed)
    except ValueError: pass
    else: raise AssertionError('changed request accepted without matching binding')
    false_agreement = copy.deepcopy(record)
    false_agreement['answer'].update(status='EquivalentAt', holds=True, counterexample=None)
    if replay_result.replay(false_agreement)['status'] != 'ReplayMismatch':
        raise AssertionError('forged agreement accepted')
    report = {'schema': 'zal/meaning-comparison-demo/1', 'command': command, 'cli_exit': process.returncode,
              'binary_sha256': answer['binary_sha256'], 'script_sha256': run.sha(Path(__file__).read_bytes()),
              'dependencies': {str(p): run.sha(p.read_bytes()) for p in [ROOT/'replay_result.py', ROOT/'oracle.py', ROOT/'run.py']},
              'discussion': record, 'witness_evaluations': evaluations, 'stronger_rule_request': reverse,
              'stronger_rule': stronger, 'replay': replayed, 'tampered_witness': rejected,
              'replay_controls': ['changed request refused', 'tampered witness mismatch', 'forged agreement mismatch'],
              'lesson': 'Exactly one car implies at least one car; the reverse has a concrete counterexample at size 3.',
              'scope': 'one shared vocabulary and explicit assumptions, finite size 3, experimental implementation'}
    (args.out/'validation.json').write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps({'status': answer['status'], 'partition': answer['partition'],
                      'counterexample': answer['counterexample'], 'stronger_rule': stronger['status'],
                      'replay': replayed['status'], 'record': str(record_path)}, indent=2))

if __name__ == '__main__': main()
