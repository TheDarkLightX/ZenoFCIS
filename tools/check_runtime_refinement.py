#!/usr/bin/env python3
"""Fail-closed MODEL_REFINEMENT checker; never an executable-assembly certificate.

The candidate producer is not imported. The trusted installed native verifier
executes both rules contracts. Package binding is identity evidence only.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import re
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
REFERENCE = ROOT / 'examples/runtime-refinement/task-intents/reference'
PROFILE = 'task-status-sqlite-local-test/1'
MODEL = 'MODEL_REFINEMENT'
CANDIDATE_FILES = ('concrete/project.zeno', 'concrete/v2/policy.json', 'abstraction.json')
CORE_FILES = ('project.zeno', 'v2/policy.json', 'v2/schema.zcve', 'v2/policy.zcve', 'src/v2_contract.rs')
OPEN_OBLIGATIONS = [
    'generated-source-and-copied-binary-semantics',
    'credential-to-principal-and-truth-of-manual-observations',
    'core-approved-Start-to-atomic-SQLite-state-revision-intent-publication',
    'actual-delivery-id-commit-lane-ordinal-destination-payload-to-model-slot',
    'typed-send-capability-and-ack-to-original-pending-entry',
    'FileReceiver-durable-idempotency-and-SQLite-recovery-refinement',
    'filesystem-power-loss-concurrency-and-host-integrity',
]


class Refused(ValueError):
    pass


def require(condition, code):
    if not condition:
        raise Refused(code)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def read(path, limit=1 << 20):
    path = Path(path)
    require(not any(p.is_symlink() for p in (path, *path.parents)), 'symlink')
    require(path.is_file() and path.stat().st_size <= limit, 'file-limit-or-absent')
    with path.open('rb') as stream:
        data = stream.read(limit + 1)
    require(len(data) <= limit, 'file-limit')
    return data


def pairs(items):
    result = {}
    for key, value in items:
        require(key not in result, 'duplicate-field')
        result[key] = value
    return result


def decode(data):
    try:
        return json.loads(data, object_pairs_hook=pairs,
                          parse_constant=lambda _: (_ for _ in ()).throw(Refused('nonfinite-json')))
    except (UnicodeError, json.JSONDecodeError) as error:
        raise Refused('malformed-json') from error


def exact_keys(value, keys):
    require(type(value) is dict and set(value) == set(keys), 'unknown-or-missing-field')


def inventory(root, maximum):
    found = set()
    for index, path in enumerate(root.rglob('*')):
        require(index < maximum * 8, 'directory-entry-limit')
        require(not path.is_symlink(), 'symlink')
        if path.is_file():
            found.add(path.relative_to(root).as_posix())
            require(len(found) <= maximum, 'file-count-limit')
    return found


def candidate(path):
    document = decode(read(path / 'candidate.json', 65536))
    exact_keys(document, ('schema', 'profile', 'claim', 'files'))
    require(document['schema'] == 'zeno-fcis/task-transport-candidate/1', 'candidate-schema')
    require(document['profile'] == PROFILE, 'unsupported-profile')
    require(document['claim'] == 'finite-transport-representation-only', 'unsupported-claim')
    exact_keys(document['files'], CANDIDATE_FILES)
    observed = inventory(path, 4)
    require(observed == set(CANDIDATE_FILES) | {'candidate.json'}, 'extra-or-missing-candidate-file')
    content = {name: read(path / name) for name in CANDIDATE_FILES}
    require(all(document['files'][name] == sha(data) for name, data in content.items()), 'candidate-hash')
    return document, content


def package_binding(path):
    """Do not execute or trust the package's claimed checks or executable semantics."""
    manifest_bytes = read(path / 'package.json')
    manifest = decode(manifest_bytes)
    exact_keys(manifest, ('assembly_sha256', 'authority', 'files', 'format', 'limits'))
    require(manifest['format'] == 'zeno-fcis/assembled-app/1' and manifest['authority'] == 'none', 'package-schema')
    require(type(manifest['files']) is dict and 1 <= len(manifest['files']) <= 128 and 'app-server' in manifest['files'], 'package-files')
    total = 0
    for name, expected in manifest['files'].items():
        relative = Path(name)
        require(not relative.is_absolute() and all(p not in ('', '.', '..') for p in name.split('/')) and '\\' not in name,
                'package-path')
        data = read(path / name, 128 << 20)
        total += len(data)
        require(total <= 256 << 20, 'package-total-byte-limit')
        require(expected == sha(data), 'package-file-hash')
    observed = inventory(path, 129)
    require(observed == set(manifest['files']) | {'package.json'}, 'package-file-set')
    assembly_bytes = read(path / 'assembly.json')
    assembly = decode(assembly_bytes)
    require(manifest['assembly_sha256'] == sha(assembly_bytes), 'assembly-binding')
    require(assembly.get('profile') == PROFILE, 'package-profile')
    for name in CORE_FILES:
        require(read(path / 'source' / name) == read(ROOT / 'examples/apps/task-status' / name), 'package-core-substitution')
    return {'package_sha256':sha(manifest_bytes), 'assembly_sha256':sha(assembly_bytes),
            'binary_sha256':manifest['files'].get('app-server'),
            'qualification':'IDENTITY_AND_CHECKED_CORE_SOURCE_BINDING_ONLY_NOT_BINARY_REFINEMENT'}


def run(cli, arguments, destination, expected_exit=0, timeout=120):
    stdout, stderr = destination.with_suffix('.json'), destination.with_suffix('.log')
    try:
        with stdout.open('wb') as out, stderr.open('wb') as err:
            completed = subprocess.run([str(cli), *arguments, '--no-kb'], stdout=out, stderr=err,
                                       timeout=timeout, check=False, cwd=ROOT)
    except subprocess.TimeoutExpired as error:
        raise Refused('verifier-timeout') from error
    require(completed.returncode == expected_exit, 'verifier-nonzero-or-wrong-exit')
    return decode(read(stdout, 4 << 20))


def check(cli, candidate_dir, package, output, claim=MODEL, max_checks=1152, timeout=120):
    require(claim == MODEL, 'unsupported-executable-or-external-claim')
    require(type(max_checks) is int and 0 <= max_checks <= 1152, 'comparison-budget')
    require(type(timeout) is int and 1 <= timeout <= 120, 'timeout-budget')
    require(not output.exists(), 'output-exists')
    require(not any(p.is_symlink() for p in (output,*output.parents)), 'symlink')
    resolved = output.resolve()
    require(not any(resolved.is_relative_to(p.resolve()) for p in (ROOT,candidate_dir,package)), 'output-inside-input')
    checker_bytes = read(Path(__file__))
    candidate_bytes = read(candidate_dir/'candidate.json',65536)
    cli_hash = sha(read(cli, 1 << 30))
    document, inputs = candidate(candidate_dir)
    bound_package = package_binding(package)
    reference = {name:read(REFERENCE/name) for name in inventory(REFERENCE,32)}
    task = {name:read(ROOT/'examples/apps/task-status'/name) for name in CORE_FILES}
    properties = {name:read(ROOT/'tools/runtime-assembly'/name) for name in
                  ('task-intent-run-properties.json','task-intent-global-negative.json')}
    # Source text is frozen before invoking the trusted verifier. Generator paths
    # and certificate status fields are never executed or used as acceptance.
    output.mkdir(parents=True)
    for name, data in inputs.items():
        path = output / 'input' / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
    for name, data in reference.items():
        path = output / 'reference' / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
    for name, data in {**task, **properties}.items():
        path = output/'task'/name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
    report = run(cli, ['contract','refine-check',str(output/'reference'),str(output/'input/concrete'),
        '--abstraction',str(output/'input/abstraction.json'),'--max-checks',str(max_checks),'--format','json'],
        output/'refinement', timeout=timeout)
    require(report.get('schema') == 'zeno-fcis/contract-refinement-result/1'
            and report.get('claim') == 'finite-contract-refinement-only'
            and report.get('verdict') == 'refines' and type(report.get('checked_rows')) is int
            and report['checked_rows'] == 1152 and report.get('raw_states') == 96
            and report.get('abstract_states') == 96 and report.get('inputs') == 12,
            'incomplete-or-failed-refinement')
    expected_sources = {'abstract_project':sha(reference['project.zeno']),
        'abstract_policy':sha(reference['v2/policy.json']),
        'concrete_project':sha(inputs['concrete/project.zeno']),
        'concrete_policy':sha(inputs['concrete/v2/policy.json']), 'abstraction':sha(inputs['abstraction.json'])}
    require(report.get('sources') == expected_sources, 'stale-verifier-evidence')
    for label, path, property_file, expected in (
        ('transport', output/'reference', output/'reference/v2/run-properties.json', 40),
        ('task', output/'task', output/'task/task-intent-run-properties.json', 13),
    ):
        project = read(path/'project.zeno').decode()
        pattern = r'law (?:501 valid_post|500 attempt_matches_lifecycle) on commit, genesis = (.+);'
        match = re.search(pattern, project)
        require(match is not None, 'missing-known-invariant')
        lemma = run(cli, ['contract','lemma',str(path),'--lemma',match[1],'--max-tuples','1152' if label=='transport' else '2646','--format','json'], output/(label+'-lemma'), timeout=timeout)
        require(lemma.get('status') == 'inductive' and lemma.get('lemmas')
                and all(x.get('verdict') == 'inductive' and x.get('initiation') is True
                        and x.get('states',{}).get('reachable') == expected for x in lemma['lemmas']), 'invariant-incomplete')
        runs = run(cli, ['contract','runs',str(path),'--file',str(property_file),'--format','json'],
                   output/(label+'-runs'), timeout=timeout)
        require(runs.get('status') == 'inductive' and type(runs.get('properties')) is list
                and len(runs['properties']) == (4 if label=='transport' else 1)
                and all(p.get('verdict') == 'inductive' and p.get('initiation') is True for p in runs['properties']),
                'run-property-incomplete')
        for packet in (lemma,runs):
            require(packet.get('sources',{}).get('project_sha256') == sha(read(path/'project.zeno'))
                    and packet.get('sources',{}).get('rules_sha256') == sha(read(path/'v2/policy.json')), 'stale-core-evidence')
    negative = run(cli, ['contract','runs',str(output/'task'), '--file',
        str(output/'task/task-intent-global-negative.json'),'--format','json'],
        output/'task-global-negative', expected_exit=1, timeout=timeout)
    require(negative.get('status') == 'refuted' and len(negative.get('properties',[])) == 1
            and negative['properties'][0].get('trace_from_genesis') ==
            [[160,0,1,170],[161,1,1,171],[163,1,1,171],[166,1,1,170],[161,2,1,171]],
            'missing-retry-positive-control')
    scenarios = run(cli, ['contract','scenario',str(output/'reference'),'--format','json'],
                    output/'transport-scenarios', timeout=timeout)
    require(scenarios.get('status') == 'passed' and scenarios.get('count') == 2
            and scenarios.get('failed_on_contract') == 0
            and scenarios.get('reject_everything',{}).get('killed') is True, 'missing-transport-progress-control')
    require(candidate(candidate_dir) == (document, inputs) and read(candidate_dir/'candidate.json',65536) == candidate_bytes, 'candidate-drift')
    require(read(Path(__file__)) == checker_bytes, 'checker-drift')
    for directory, snapshot in ((output/'input',inputs),(output/'reference',reference),(output/'task',{**task,**properties})):
        require(inventory(directory,32) == set(snapshot), 'frozen-source-set-drift')
        require(all(read(directory/name) == data for name,data in snapshot.items()), 'frozen-source-drift')
    require(inventory(REFERENCE,32) == set(reference), 'reference-set-drift')
    require(package_binding(package) == bound_package, 'package-drift')
    require(sha(read(cli, 1 << 30)) == cli_hash, 'verifier-drift')
    require(all(read(ROOT/'examples/apps/task-status'/name) == data for name,data in task.items()), 'task-source-drift')
    require(all(read(ROOT/'tools/runtime-assembly'/name) == data for name,data in properties.items()), 'task-properties-drift')
    require(all(read(REFERENCE/name) == data for name,data in reference.items()), 'reference-drift')
    receipt = {'schema':'zeno-fcis/runtime-refinement-evidence/1', 'authority':'none',
        'claim_level':MODEL, 'status':'CHECKED', 'executable_assembly_refinement':False,
        'publication_authority':False, 'open_obligations':OPEN_OBLIGATIONS,
        'scope':{'raw_model_states':96,'inputs':12,'compared_pairs':1152,'reachable_reference_states':40,
                 'histories':'arbitrary finite histories of this fixed finite model by initial-state and complete one-step correspondence plus invariant closure'},
        'package_binding':bound_package,'verifier_sha256':cli_hash,
        'checker_sha256':sha(checker_bytes), 'candidate_sha256':sha(candidate_bytes),
        'sources':expected_sources,
        'evidence':{p.relative_to(output).as_posix():sha(read(p,4<<20)) for p in output.rglob('*') if p.is_file()},
        'runtime_correspondence':'OPEN; source binding and native core properties do not establish executable or persistence refinement'}
    temporary = output/'receipt.json.tmp'
    temporary.write_text(json.dumps(receipt,indent=2)+'\n')
    temporary.rename(output/'receipt.json')
    return receipt


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ('cli','candidate','package','out'):parser.add_argument('--'+name,type=Path,required=True)
    parser.add_argument('--claim',default=MODEL)
    parser.add_argument('--max-checks',type=int,default=1152)
    args=parser.parse_args()
    try:
        check(args.cli.absolute(), args.candidate.absolute(), args.package.absolute(), args.out.absolute(), args.claim, args.max_checks)
    except (Refused,OSError,ValueError) as error:
        print(json.dumps({'status':'REFUSED','claim_level':args.claim,'reason':str(error) if isinstance(error,Refused) else type(error).__name__}),file=sys.stderr)
        return 3
    print('MODEL_REFINEMENT checked; executable assembly refinement remains OPEN.')
    return 0

if __name__=='__main__':raise SystemExit(main())
