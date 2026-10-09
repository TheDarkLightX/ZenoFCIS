#!/usr/bin/env python3
"""Issue/replay finite certificates for the ten closed component data families.

The certificate is Identified data. Only successful exhaustive replay establishes
its finite Proved claim (ADR 0003), under the explicitly named trusted base.
This tool never supplies an application decision or publication callback.
"""
from __future__ import annotations

import argparse
import itertools
import json
import shutil
from pathlib import Path
import subprocess
import sys

import instantiate_core as core
import check_core_components as checks

PROOFS = core.CATALOG / 'proofs'
SCHEMA = 'zeno-fcis/core-family-finite-certificate/1'
RUNTIME_SCHEMA = 'zeno-fcis/core-family-runtime-source/1'
# build.rs regenerates these ignored test products from the pinned Rust source.
# They are neither compiler inputs nor part of a clean source checkout.
GENERATED_TEST_OUTPUT = 'crates/zeno-fcis-generated-code-tests/python'
INPUTS = ('tools/instantiate_core.py', 'tools/check_core_components.py',
          'tools/prove_core_families.py', 'tools/test_data/core_components/reference.py')
NATIVE_REPORT_SHA256 = '150de442ff39d54f3cf9a19be5dd0ca5578b791ed34190233e7f4f079bf53f8c'
COUNTS = {'reservation-pool': (9, 2216), 'rate-limiter': (9, 2646), 'approval-queue': (3, 1296),
          'bounded-counter': (8, 176), 'consumable-budget': (8, 568),
          'versioned-register': (8, 30664), 'idempotency-slot': (8, 61328),
          'retry-budget': (8, 352), 'finite-phase-machine': (8, 176), 'logical-deadline': (8, 8096)}
CONSERVATION = {
    'reservation-pool': 'Reserve and Release preserve a+r; Consume subtracts exactly q; Replenish adds exactly q. Supplied authorization is not authentication or physical asset evidence.',
    'rate-limiter': 'Every accepted request charges one unit. Only now>=start+W resets the window, and its first request charges one. Supplied time is bounded0..6, not a clock attestation.',
    'approval-queue': 'Each accepted Vote adds one previously unset supplied principal slot and frames other slots. Execute preserves votes and requires K; Enqueue introduces no votes. Slots do not authenticate humans.',
    'bounded-counter': 'Each accepted Inc adds exactly one and Dec subtracts exactly one within 0..C.',
    'consumable-budget': 'Every accepted Spend satisfies remaining_post+amount=remaining_pre. Zero spends are allowed; no refill is modeled.',
    'versioned-register': 'Write requires the expected version and advances it exactly once without wrapping, preserving the supplied new value. Logical concurrency is not a store operation.',
    'idempotency-slot': 'Matching the retained key cannot replace its value. A different key replaces the slot; historical exactly-once delivery is not established.',
    'retry-budget': 'Attempt consumes one remaining unit. Finish closes once without a charge. This does not schedule or execute retries.',
    'finite-phase-machine': 'Advance increments exactly once; Reset is accepted only at the final phase and returns to zero. No external completion is inferred.',
    'logical-deadline': 'Tick requires now>=last_seen and now>=deadline, preserves the deadline, records now and reaches the deadline once. Supplied logical time is not a clock attestation.',
}


def encoded(value):
    return (json.dumps(value, sort_keys=True, separators=(',', ':'), allow_nan=False) + '\n').encode()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def load(path):
    data = path.read_bytes()
    value = core.read_json(data.decode())
    require(data == encoded(value), f'noncanonical JSON: {path}')
    return value


def source_hashes(family, root=None):
    root = core.ROOT if root is None else root
    directory = root / 'core-components' / family
    names = list(INPUTS) + [str(path.relative_to(root)) for path in sorted(directory.rglob('*'))
                            if path.is_file()]
    return {name: core.digest((root / name).read_bytes()) for name in sorted(names)}


def runtime_sources(root=None):
    # Complete crate trees include F1 include_str inputs, templates and the
    # mandatory interpreter/law/Authority implementation; no output/cache tree.
    root = core.ROOT if root is None else root
    paths = [path for path in (root / 'crates').rglob('*')
             if path.is_file() and not path.is_relative_to(root / GENERATED_TEST_OUTPUT)]
    paths += [root / name for name in ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml')]
    paths += [path for path in (root / '.cargo').rglob('*') if path.is_file()]
    return {'schema': RUNTIME_SCHEMA,
            'files': {str(path.relative_to(root)): core.digest(path.read_bytes())
                      for path in sorted(paths)}}


def snapshot_inputs(cli):
    """Bind every family and the executable before dispatching any check."""
    return {'runtime': runtime_sources(),
            'families': {family: source_hashes(family) for family in core.FAMILIES},
            'cli_sha256': core.digest(cli.read_bytes())}


def require_unchanged_inputs(cli, snapshot, root=None):
    require(runtime_sources(root) == snapshot['runtime'],
            'compiler/evaluator source changed during checking')
    for family in core.FAMILIES:
        require(source_hashes(family, root) == snapshot['families'][family],
                'family/checker source changed during checking')
    require(core.digest(cli.read_bytes()) == snapshot['cli_sha256'],
            'CLI binary changed during checking')


def classification(family):
    return {
        'stored_artifact': 'Identified', 'after_successful_replay': 'Proved',
        'method': 'exhaustive mandatory-library Authority execution over the entire closed finite domain',
        'scope': 'Every declared parameter and raw state/command/context tuple; every applicable transition law passes; exact independent decisions and positive witnesses. Includes invalid prestates as business Rejects.',
        'conservation': CONSERVATION[family],
        'genesis': 'Not re-proved by this transition certificate. Separate retained generated-native evidence checks each exact genesis; no general reachability theorem is inferred.',
        'assumptions': ['supplied authorization, principal and time have only their declared data meanings',
                        'the frozen compiler/runtime and native toolchain execute their source faithfully',
                        'the independent reference specifies the intended finite observations'],
        'trusted_base': ['F1 declaration compiler and canonical codecs',
                         'mandatory V2 Authority and eager-i64 evaluator and library law evaluator',
                         'F2 complete-domain enumeration, framing and decision reporting',
                         'this certificate replayer and its full-product/identity comparisons',
                         'Python, Rust compiler/runtime, OS, SHA256 and execution hardware'],
        'not_claimed': ['Lean KernelChecked', 'solver UNSAT proof', 'unbounded parameters or timestamps',
                        'authentication', 'physical conservation', 'owner adoption', 'release authority'],
    }


def shape(certificate, family):
    require(certificate['schema'] == SCHEMA and certificate['family'] == family, 'certificate subject differs')
    require(certificate['classification'] == classification(family), 'certificate assurance claim differs')
    require(certificate['inherited_native_report_sha256'] == NATIVE_REPORT_SHA256, 'inherited native evidence identity differs')
    require(certificate['authority'] == 'none' and certificate['owner_adoption'] is False, 'certificate invents authority')
    instances = certificate['instances']
    parameters = core.definition(family)['instances']
    require([item['parameters'] for item in instances] == parameters, 'missing, reordered or changed parameter instance')
    require(len(instances) == COUNTS[family][0], 'parameter range differs')
    for item in instances:
        manifest = core.read_json(core.source(family, item['parameters'], include_proof=False)['core-instance.json'].decode())
        require(item['domains'] == manifest['input_domains'] and item['tuples'] == manifest['input_tuples'],
                'certificate narrowed or changed an input domain')
    require(sum(item['tuples'] for item in instances) == COUNTS[family][1], 'incomplete family product')


def validate_reference(family):
    """Check identity/staleness only. Reading this reference does NOT replay it."""
    path = PROOFS / f'{family}.json'
    certificate = load(path)
    shape(certificate, family)
    require(certificate['sources'] == source_hashes(family), 'stale family or checker source')
    runtime = load(PROOFS / 'runtime-source.json')
    require(runtime == runtime_sources(), 'stale compiler/evaluator source')
    require(certificate['runtime_source_sha256'] == core.digest(encoded(runtime)), 'wrong runtime source binding')
    return {'path': f'core-components/proofs/{family}.json',
            'sha256': core.digest(path.read_bytes()), 'evidence_level': 'Identified',
            'claim_after_replay': 'Proved', 'scope': 'closed finite family',
            'replay': 'python3 tools/prove_core_families.py replay --cli CLI --work-dir NEW_DIR',
            'replay_required': True}


def run(cli, directory, label, args, expected_exit=0):
    try:
        result = subprocess.run([str(cli), *map(str, args)], cwd=core.ROOT, capture_output=True,
                                timeout=60, check=False)
    except subprocess.TimeoutExpired as error:
        (directory / f'{label}.timeout').write_text(str(error) + '\n')
        raise ValueError(f'{label} timed out; no certificate') from error
    (directory / f'{label}.stdout').write_bytes(result.stdout)
    (directory / f'{label}.stderr').write_bytes(result.stderr)
    (directory / f'{label}.exit').write_text(f'{result.returncode}\n')
    require(result.returncode == expected_exit,
            f'{label}: exit{result.returncode}, expected{expected_exit}; see {directory}')
    return result.stdout


def observed_rows(packet, expected, manifest, family):
    """Decode actual library results; no alternative decision algorithm."""
    inputs, summary, table = packet['inputs'], packet['summary'], packet['decision_table']
    count = manifest['input_tuples']
    require(inputs['construction'] == 'full-domain' and inputs['count'] == count and inputs['domain_size'] == str(count),
            'not the complete admitted product')
    require(summary['refusals']['count'] == 0 and summary['findings'] == 0, 'a raw input caused a technical refusal or finding')
    require(summary['examples'] == {'compared': count, 'disagreements': 0}, 'independent examples differ')
    rows = table['rows']
    require(rows is not None and len(rows) == count, 'missing actual decision rows')
    tuples = list(itertools.product(*manifest['input_domains']))
    require([tuple(map(int, row['input'])) for row in expected] == tuples, 'reference does not cover the entire product')
    width = core.definition(family)['state_width']
    normalized, accepts = [], {name: 0 for name in core.COMMANDS[family][1].values()}
    rejects = 0
    for text, values, reference in zip(rows, tuples, expected):
        written, decision = text.split(' | ')
        require(tuple(map(int, written.split())) == values, 'missing, duplicate or reordered actual tuple')
        cls, reason, post_index, outbox_index = decision.split()
        reason = None if reason == '-' else int(reason)
        fields = table['post_states'][int(post_index)]['fields']
        if cls == 'Reject':
            require(fields == [], 'business Reject carries a successor patch')
            post = list(values[:width]); rejects += 1
        else:
            require(cls == 'Accept', 'unexpected decision class')
            require([field['field'] for field in fields] == list(range(110, 110 + width)), 'successor fields differ')
            post = [int(field['value']['variant'] if isinstance(field['value'], dict) else field['value']) for field in fields]
            accepts[core.command_name(family, values)] += 1
        deliveries = table['outboxes'][int(outbox_index)]['deliveries']
        actual = {'class': cls, 'reason': reason, 'post': post, 'deliveries': deliveries}
        require(actual == reference['expected'], f'actual decision differs at {values}')
        normalized.append({'input': list(values), 'decision': actual})
    require(all(accepts.values()) and rejects > 0, 'vacuous command behavior')
    return {'decision_sha256': core.digest(encoded(normalized)), 'accepts_by_command': accepts, 'rejects': rejects}


def collect(family, cli, work, snapshot):
    reference = checks.expected_cases()
    instances = []
    for parameters in core.definition(family)['instances']:
        directory = work / (family + '-' + core.instance_id(parameters)); directory.mkdir()
        manifest = core.instantiate(family, parameters, directory / 'contract', include_proof=False)
        generated = core.read_json(run(cli, directory, 'generate', ['generate', 'contract', directory / 'contract', '--format', 'json']).decode())
        run(cli, directory, 'regenerate', ['generate', 'contract', directory / 'contract', '--check'])
        run(cli, directory, 'review', ['contract', 'review', directory / 'contract', '--out', directory / 'review.json', '--format', 'json'])
        packet = core.read_json((directory / 'review.json').read_text())
        expected = [row for row in reference if row['family'] == family.replace('-', '_') and row['parameters'] == parameters]
        observations = observed_rows(packet, expected, manifest, family)
        application_laws = [500, 501] if family == 'rate-limiter' else [500, 501, 502]
        if family == 'retry-budget':
            application_laws.append(503)
        laws = application_laws + [908, 909, 990, 991]
        require(generated['summary']['law_ids'] == laws, 'applicable law inventory differs')
        artifact_hashes = {name: core.digest((directory / 'contract' / name).read_bytes())
                           for name in ('v2/schema.zcve', 'v2/policy.zcve', 'src/v2_contract.rs')}
        instance = {'parameters': parameters, 'domains': manifest['input_domains'], 'tuples': manifest['input_tuples'],
                    'artifacts': artifact_hashes, 'law_ids': laws,
                    'authority_identity_sha256': packet['tool']['library_identity_sha256'],
                    'source_set_sha256': manifest['source_set_sha256'], **observations}
        instances.append(instance)
        print(f'{family} {parameters}: {manifest["input_tuples"]} complete actual-library decisions', flush=True)
    certificate = {'schema': SCHEMA, 'family': family, 'classification': classification(family),
                   'authority': 'none', 'owner_adoption': False, 'cli_sha256': snapshot['cli_sha256'],
                   'sources': snapshot['families'][family],
                   'runtime_source_sha256': core.digest(encoded(snapshot['runtime'])),
                   'inherited_native_report_sha256': NATIVE_REPORT_SHA256,
                   'instances': instances}
    shape(certificate, family)
    return certificate


def preflight(certificate, family, cli, runtime):
    shape(certificate, family)
    require(certificate['sources'] == source_hashes(family), 'stale family or checker source')
    require(certificate['runtime_source_sha256'] == core.digest(encoded(runtime)), 'stale compiler/evaluator source')
    require(certificate['cli_sha256'] == core.digest(cli.read_bytes()), 'CLI binary differs; requalify this source/build')


def compare_certificate(certificate, recomputed):
    require(encoded(certificate) == encoded(recomputed), 'certificate differs from exhaustive actual replay')



def qualify(cli, work):
    """Real bad policies, replay tampering and normal generated-app references."""
    reports = {'law_mutants': [], 'tampered_certificates': [], 'app_references': []}
    for family in core.FAMILIES:
        parameters = core.definition(family)['instances'][0]
        policy = core.read_json(core.source(family, parameters, include_proof=False)['v2/policy.json'].decode())
        for name, changed in checks.law_mutants(family, policy):
            directory = work / (family + '-bad-policy-' + name); directory.mkdir()
            manifest = core.instantiate(family, parameters, directory / 'contract', include_proof=False)
            (directory / 'contract/v2/policy.json').write_bytes(encoded(changed))
            run(cli, directory, 'generate', ['generate', 'contract', directory / 'contract'])
            run(cli, directory, 'review', ['contract', 'review', directory / 'contract', '--out', directory / 'review.json'], 1)
            packet = core.read_json((directory / 'review.json').read_text())
            require(packet['summary']['law_refusal_findings'] > 0, 'bad family lacked a lawful-prestate law refusal')
            try:
                observed_rows(packet, [], manifest, family)
            except ValueError as error:
                require('technical refusal' in str(error), 'bad family failed for a different reason')
            else:
                raise ValueError('bad family acquired a finite certificate')
            reports['law_mutants'].append({'family': family, 'mutation': name, 'refusals': packet['summary']['refusals'],
                                           'review_sha256': core.digest((directory / 'review.json').read_bytes())})
    for name in ('missing-parameter', 'narrow-domain', 'wrong-decision', 'wrong-binary',
                 'stale-family', 'stale-runtime', 'inflated-status'):
        directory = work / name; directory.mkdir()
        changed = directory / 'certificates'; shutil.copytree(PROOFS, changed)
        path = changed / 'reservation-pool.json'; certificate = load(path)
        if name == 'missing-parameter':
            certificate['instances'].pop()
        elif name == 'narrow-domain':
            certificate['instances'][0]['domains'][0] = [0]
        elif name == 'wrong-decision':
            certificate['instances'][0]['decision_sha256'] = '0' * 64
        elif name == 'wrong-binary':
            certificate['cli_sha256'] = '0' * 64
        elif name == 'stale-family':
            certificate['sources']['core-components/reservation-pool/policy.json.in'] = '0' * 64
        elif name == 'stale-runtime':
            runtime_path = changed / 'runtime-source.json'; runtime = load(runtime_path)
            runtime['files'][next(iter(runtime['files']))] = '0' * 64
            runtime_path.write_bytes(encoded(runtime))
        else:
            certificate['classification']['stored_artifact'] = 'Proved'
        path.write_bytes(encoded(certificate))
        result = subprocess.run([sys.executable, str(Path(__file__).resolve()), 'replay', '--cli', str(cli),
                                 '--work-dir', str(directory / 'replay'), '--certificates', str(changed)],
                                cwd=core.ROOT, capture_output=True, timeout=180, check=False)
        (directory / 'replay.stdout').write_bytes(result.stdout)
        (directory / 'replay.stderr').write_bytes(result.stderr)
        (directory / 'replay.exit').write_text(f'{result.returncode}\n')
        require(result.returncode == 1 and b'finite family proof refused:' in result.stderr,
                f'tampered certificate was not refused normally: {name}')
        reports['tampered_certificates'].append({'control': name, 'exit': result.returncode,
                                               'reason': result.stderr.decode().strip()})
    for family in core.FAMILIES:
        certificate = load(PROOFS / f'{family}.json')
        certificate_sha = core.digest(encoded(certificate))
        for item in certificate['instances']:
            directory = work / (family + '-' + core.instance_id(item['parameters'])); directory.mkdir()
            manifest = core.instantiate(family, item['parameters'], directory / 'contract')
            require(manifest['finite_family_certificate']['evidence_level'] == 'Identified', 'reference inflates evidence')
            require(manifest['finite_family_certificate']['sha256'] == certificate_sha, 'reference identity differs')
            run(cli, directory, 'scaffold', ['new', directory / 'app', '--contract', directory / 'contract', '--source', core.ROOT])
            project = (directory / 'app/project.zeno').read_text()
            require(certificate_sha in project and 'replay required' in project and 'Identified' in project,
                    'normal app scaffold lost proof reference')
            for path, expected in item['artifacts'].items():
                require(core.digest((directory / 'app' / path).read_bytes()) == expected,
                        'proof-referenced app canonical artifacts differ')
            reports['app_references'].append({'family': family, 'parameters': item['parameters'],
                                              'certificate_sha256': certificate_sha, 'canonical_artifacts': 'identical'})
    (work / 'qualification.json').write_bytes(encoded(reports))
    print(json.dumps({'status': 'passed', 'law_mutants': len(reports['law_mutants']),
                      'tamper_controls': len(reports['tampered_certificates']),
                      'app_references': len(reports['app_references']), 'authority': 'none'}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=('issue', 'replay', 'qualify'))
    parser.add_argument('--cli', type=Path, required=True)
    parser.add_argument('--work-dir', type=Path, required=True)
    parser.add_argument('--native-report', type=Path, help='required for issue: retained all77 native qualification report')
    parser.add_argument('--certificates', type=Path, help='replay-only alternate certificate data directory')
    args = parser.parse_args()
    cli, work = args.cli.resolve(), args.work_dir.resolve()
    require(cli.is_file(), 'CLI binary missing')
    work.mkdir(exist_ok=False)
    if args.mode == 'qualify':
        require(args.certificates is None and args.native_report is None, 'qualify uses the installed certificate set')
        qualify(cli, work)
        return
    snapshot = snapshot_inputs(cli)
    runtime = snapshot['runtime']
    certificates_dir = args.certificates.resolve() if args.certificates else PROOFS
    require(args.mode == 'replay' or args.certificates is None, 'alternate certificates are replay-only')
    native = None
    if args.mode == 'issue':
        require(args.native_report is not None, 'issue requires the retained native report')
        require(core.digest(args.native_report.read_bytes()) == NATIVE_REPORT_SHA256, 'native report identity differs')
        native = core.read_json(args.native_report.read_text())
        require(native['schema'] == 'zeno-fcis/core-seed-check/1' and
                native['evidence'] == 'complete-finite-execution' and
                native['raw_tuples'] == 107518 and len(native['instances']) == 77,
                'native report does not cover the complete catalogue')
        require(native['runtime_source_sha256'] == core.digest(encoded(runtime)),
                'native report checked a different compiler/evaluator source')
        require(not PROOFS.exists(), 'refusing to overwrite an existing certificate set')
    else:
        require(load(certificates_dir / 'runtime-source.json') == runtime, 'runtime source changed')
        for family in core.FAMILIES:
            preflight(load(certificates_dir / f'{family}.json'), family, cli, runtime)
    certificates = {family: collect(family, cli, work, snapshot) for family in core.FAMILIES}
    if native is not None:
        for family, certificate in certificates.items():
            for instance in certificate['instances']:
                matching = [item for item in native['instances']
                            if item['instance']['family'] == family and item['instance']['parameters'] == instance['parameters']]
                require(len(matching) == 1 and matching[0]['generated_sha256'] == instance['artifacts'],
                        'inherited native evidence has different canonical artifacts')
    require_unchanged_inputs(cli, snapshot)
    if args.mode == 'issue':
        # No partially checked family is published as a successful certificate.
        PROOFS.mkdir()
        (PROOFS / 'runtime-source.json').write_bytes(encoded(runtime))
        for family, certificate in certificates.items():
            (PROOFS / f'{family}.json').write_bytes(encoded(certificate))
    else:
        for family, certificate in certificates.items():
            compare_certificate(load(certificates_dir / f'{family}.json'), certificate)
    report = {'schema': 'zeno-fcis/core-family-replay/1', 'mode': args.mode, 'status': 'passed',
              'evidence_level': 'Proved', 'scope': '77 closed parameter instances and 107518 raw transition inputs only',
              'authority': 'none', 'owner_adoption': False,
              'certificates': {family: core.digest(encoded(certificate)) for family, certificate in certificates.items()}}
    (work / 'report.json').write_bytes(encoded(report))
    print(json.dumps(report, sort_keys=True))


if __name__ == '__main__':
    try:
        main()
    except (OSError, KeyError, TypeError, ValueError, RuntimeError) as error:
        print(f'finite family proof refused: {error}', file=sys.stderr)
        sys.exit(1)
