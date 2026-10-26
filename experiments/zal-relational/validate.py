"""Run the optional backend's affected checks and emit a portable public summary."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def inputs():
    result = {}
    for path in ROOT.rglob('*'):
        rel = path.relative_to(ROOT)
        if any(part in {'.build', 'evidence', '__pycache__', 'dist-newstyle'} for part in rel.parts):
            continue
        if path.is_file() and path.suffix in {'.py', '.hs', '.lean', '.zeno', '.json', '.txt', '.cabal'} and path.name != 'VALIDATION.json':
            result[str(rel)] = sha(path)
    repo = ROOT.parents[1]
    for name in ('integrations/zal/ladder.py', 'integrations/zal/ladder_en.json'):
        result['repository:' + name] = sha(repo/name)
    return result

def require(condition, message):
    if not condition:
        raise RuntimeError(message)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--with-lean', action='store_true', help='also run the arithmetic theorem and semantic negative control')
    args = parser.parse_args()
    evidence = ROOT/'evidence'; evidence.mkdir(exist_ok=True)
    scratch = ROOT/'.build'; scratch.mkdir(exist_ok=True)
    target = evidence/'public-validation.json'
    target.unlink(missing_ok=True)
    before = inputs()
    steps = []
    def check(label, command, expected=0, timeout=180):
        process = subprocess.run(command, cwd=ROOT, capture_output=True, timeout=timeout)
        (evidence/(label+'.stdout')).write_bytes(process.stdout)
        (evidence/(label+'.stderr')).write_bytes(process.stderr)
        if process.returncode != expected:
            raise RuntimeError(label + ' failed; inspect its evidence logs')
        steps.append({'check': label, 'exit': process.returncode})
        print(label + ': passed', flush=True)
    py = sys.executable
    check('build', [py, 'build.py'])
    for script in ('test_backend.py', 'test_operations.py', 'test_ladder_adapter.py'):
        check(script.removesuffix('.py'), [py, script])
    owned = Path(tempfile.mkdtemp(prefix='qualification-', dir=scratch))
    check('test_mutations', [py, 'test_mutations.py', '--work-dir', str(owned/'mutations')], timeout=300)
    check('test_shell', [py, 'test_shell.py'])
    check('check_fcis_bridge', [py, 'check_fcis_bridge.py'])
    if args.with_lean:
        check('check_proof', [py, 'check_proof.py'])
    demo = owned/'discussion'
    check('demo_decisions', [py, 'demo_decisions.py', '--out', str(demo)])
    check('replay_cli', [py, 'replay_result.py', str(demo/'discussion.json')])

    # Independent original-formula enumeration of the complete size-3 demo.
    import run
    from test_operations import expected_comparison
    discussion = json.loads((demo/'discussion.json').read_text())
    expected = expected_comparison(discussion['request'])
    require(all(discussion['answer'][k] == value for k, value in expected.items()), 'discussion oracle disagrees')
    (evidence/'discussion-oracle.json').write_text(json.dumps(expected, indent=2)+'\n')
    print('discussion_oracle: passed', flush=True)
    steps.append({'check': 'discussion_oracle', 'exit': 0})

    def load(name): return json.loads((evidence/name).read_text())
    build = load('build.json')
    qualification = load('qualification.json')
    operations = load('operations.json')
    mutations = load('mutations.json')
    ladder = load('ladder-integration.json')
    bridge = load('observation-bridge.json')
    shell = load('shell-controls.json')
    require(qualification['binary_sha256'] == operations['binary_sha256'] == shell['haskell_binary_sha256'] == build['binary_sha256'], 'worker bindings differ')
    require(mutations['baseline'] == build['source_sha256'] and mutations['killed_by_wrong_count'] == 6, 'mutation qualification mismatch')
    require(ladder['native_binary_sha256'] == build['binary_sha256'], 'ladder worker differs')
    require(discussion['answer']['binary_sha256'] == build['binary_sha256'], 'discussion worker differs')
    proof = {'status': 'not-run', 'scope': 'arithmetic lemmas only; implementation proof open'}
    if args.with_lean:
        checked = load('lean-proof.json')
        require(checked['exit'] == 0 and checked['negative_exit'] != 0 and len(checked['theorems']) == 6, 'incomplete proof result')
        proof = {'status': 'checked', 'checker': checked['checker'], 'theorems': checked['theorems'],
                 'negative_control': 'wrong cutoff rejected for an arithmetic counterexample',
                 'scope': 'arithmetic lemmas only; implementation proof open'}
    require(inputs() == before, 'qualification inputs changed during the run')
    require(run.binding()[1]['binary_sha256'] == build['binary_sha256'], 'final worker binding changed')
    record = {
        'schema': 'zal/relational-public-validation/1', 'profile': 'optional-experimental-backend',
        'checks': steps, 'ghc': build['ghc'], 'worker_sha256': build['binary_sha256'],
        'source_sha256': before,
        'count_comparisons': qualification['count_comparisons'],
        'supplied_model_evaluations': qualification['supplied_model_evaluations'],
        'unique_extension_checks': qualification['pointwise_unique_expansions'],
        'graph_identities': qualification['closed_form_graph_comparisons'],
        'comparison_cases': operations['comparisons'], 'public_evaluations': operations['evaluations'],
        'invalid_resource_cases': operations['malformed_or_resource_controls'],
        'forged_response_controls': len(operations['response_controls']),
        'compiled_semantic_mutants_detected': mutations['killed_by_wrong_count'],
        'source_interruption_controls': len(shell['controls']),
        'ladder_roundtrips': ladder['roundtrip_count_agreements'], 'story_fixtures': ladder['existing_story_fixtures'],
        'fcis_observation_vectors': bridge['inputs'],
        'native_fcis_execution': 'not run by this driver; the bridge checks the included finite example vectors',
        'discussion': expected, 'discussion_replay': 'matched; changed request/witness/agreement controls detected',
        'proof': proof, 'native_registration': 'not installed', 'implementation_formal_proof': 'open',
        'timing_comparison': 'not claimed', 'human_semantic_approval': 'not inferred'
    }
    target.write_text(json.dumps(record, indent=2, sort_keys=True)+'\n')
    print(json.dumps({k: v for k, v in record.items() if k not in ('source_sha256', 'checks')}, indent=2))

if __name__ == '__main__': main()
