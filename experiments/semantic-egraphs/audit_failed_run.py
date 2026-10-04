#!/usr/bin/env python3
"""Audit an unsuccessful run without treating missing candidates as valid costs."""

import argparse
from collections import Counter
import hashlib
import json
import math
from pathlib import Path

from analyze import CONTROL_IDS, FROZEN_CORPUS_SHA256, REQUIRED_VARIANTS, program_signatures, require
from corpus import cost, parse, signatures


def invalid_inputs(nvars):
    probes = {(0,) * (nvars + 1)}
    if nvars:
        probes.add((0,) * (nvars - 1))
    for index in range(nvars):
        for value in (-1, 2, -(1 << 63), (1 << 63) - 1):
            probe = [0] * nvars
            probe[index] = value
            probes.add(tuple(probe))
    return sorted(probes)


def check_semantics(case, original, name, variant):
    where = case['id'] + '/' + name
    outputs = [parse(source) for source in variant['outputs']]
    require(len(outputs) == len(original['outputs']), where + ': output arity')
    actual = signatures(outputs, case['nvars'])
    require(actual == original['truth_signatures'], where + ': expression semantics')
    require(program_signatures(variant['program'], case['nvars'], len(outputs)) == actual, where + ': actual serialized FCIS semantics')
    require(cost(outputs) == variant['fcis_nodes'] == len(variant['program']['nodes']) <= 256, where + ': actual shared FCIS cost')
    evaluation = variant['evaluation']
    require(evaluation['gate_passed'] is True and not evaluation['mismatches'], where + ': Rust evaluation gate')
    require(evaluation['truth_signatures'] == actual and evaluation['valid_tuples'] == 1 << case['nvars'], where + ': complete Rust truth vector')
    probes = invalid_inputs(case['nvars'])
    require(evaluation['invalid_tuples'] == len(probes), where + ': invalid-input probe count')
    checks = evaluation['checks']
    require(len(checks) == (1 << case['nvars']) + len(probes), where + ': trace count')
    expected = []
    for mask in range(1 << case['nvars']):
        values = [int(signature[mask]) for signature in actual]
        expected.append({'input': [(mask >> index) & 1 for index in range(case['nvars'])], 'admitted': True, 'source': {'ok': values}, 'candidate': {'ok': values}})
    rejection = {'error': 'Invalid("input-domain")'}
    expected.extend({'input': list(probe), 'admitted': False, 'source': rejection, 'candidate': rejection} for probe in probes)
    require(checks == expected, where + ': complete ordered observation trace')
    if name in ('raw', 'local_baseline'):
        key = 'outputs' if name == 'raw' else 'baseline_outputs'
        require(outputs == [parse(source) for source in original[key]], where + ': frozen source/baseline')
    for value in variant.get('timings_ms', {}).values():
        require(type(value) in (int, float) and math.isfinite(value) and value >= 0, where + ': invalid timing')
    return {'case': case['id'], 'variant': name, 'valid_tuples': 1 << case['nvars'], 'invalid_tuples': len(probes), 'semantic_checks_passed': True}


def audit(corpus_bytes, result_bytes):
    corpus, result = json.loads(corpus_bytes), json.loads(result_bytes)
    require(hashlib.sha256(corpus_bytes).hexdigest() == FROZEN_CORPUS_SHA256, 'frozen corpus changed')
    require(result['corpus_sha256'] == FROZEN_CORPUS_SHA256, 'result uses another corpus')
    require(result['schema_version'] == 1 and result['gate']['passed'] is False, 'this audit is only for a failed run')
    originals = {case['id']: case for case in corpus['cases']}
    require(len(result['cases']) == len(originals) == 100, 'case count')
    require({case['id'] for case in result['cases']} == set(originals), 'case identity')
    require(len({case['id'] for case in result['cases']}) == 100, 'duplicate case')
    controls = result['controls']
    require(len(controls) == len(CONTROL_IDS) and {control['id'] for control in controls} == set(CONTROL_IDS), 'control identity')
    require(all(control['passed'] is True for control in controls), 'control failed')
    accepted, failures, retained = [], [], []
    counts = Counter()
    for case in result['cases']:
        original = originals[case['id']]
        require((case['nvars'], case['family']) == (original['nvars'], original['family']), 'case drift')
        require(set(case['variants']) == set(REQUIRED_VARIANTS), 'variant set')
        for name, variant in case['variants'].items():
            counts[name + '/' + variant['status']] += 1
            if variant['status'] == 'accepted':
                require(not variant.get('error'), 'accepted variant records an error')
                if name in ('rewrite_egg', 'semantic_egg'):
                    optimization = variant['optimization']
                    require(optimization['solver_status'] == 'optimal', 'accepted solver status')
                    for component in ('saturation_ms', 'extraction_ms'):
                        value = optimization[component]
                        require(type(value) in (int, float) and math.isfinite(value) and value >= 0, 'invalid optimizer timing')
                accepted.append(check_semantics(case, original, name, variant))
            else:
                error = variant.get('error', '')
                optimization = variant.get('optimization') or {}
                status = optimization.get('solver_status') or ('time_limit' if 'status=time_limit;' in error else 'unknown')
                classification = 'time_limit_decoder_panic' if 'LP extraction panic:' in error else 'time_limit_decodable_incumbent' if status == 'time_limit' else 'other_failure'
                failures.append({'case': case['id'], 'family': case['family'], 'variant': name, 'status': variant['status'], 'solver_status': status, 'classification': classification, 'error': error})
                if variant.get('program') and variant.get('evaluation', {}).get('gate_passed') is True:
                    retained.append(check_semantics(case, original, name, variant))
    require({(entry['case'], entry['variant']) for entry in failures} == {(entry['case'], entry['variant']) for entry in result['gate']['failed_variants']}, 'failure receipt mismatch')
    return {
        'schema_version': 1, 'corpus_sha256': FROZEN_CORPUS_SHA256,
        'result_sha256': hashlib.sha256(result_bytes).hexdigest(), 'config': result['config'],
        'registered_run_passed': False, 'preregistered_success': False,
        'reason': 'The registered no-tool-failures condition failed. No valid complete aggregate comparison is reported.',
        'variant_status_counts': dict(sorted(counts.items())), 'accepted_variants_independently_checked': len(accepted),
        'accepted_checks': accepted, 'failed_variants': failures,
        'failed_cases': len({entry['case'] for entry in failures}), 'classification_counts': dict(Counter(entry['classification'] for entry in failures)),
        'retained_rejected_incumbents_semantically_checked': retained,
        'negative_controls_all_reported_passed': True, 'controls': controls,
        'scope': 'Accepted candidates and retained decodable incumbents are checked independently over each entire declared Boolean domain, including exact output order and serialized FCIS programs. Recorded invalid-input observations are checked against the declared probe set, not all infinitely many invalid tuples. Negative-control pass flags and receipts are preserved; this audit does not re-execute their source programs.',
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('results', type=Path)
    parser.add_argument('--corpus', type=Path, default=Path(__file__).with_name('corpus.json'))
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    summary = audit(args.corpus.read_bytes(), args.results.read_bytes())
    args.out.write_text(json.dumps(summary, indent=2, sort_keys=True) + '\n')
    print(json.dumps({key: summary[key] for key in ('registered_run_passed', 'preregistered_success', 'accepted_variants_independently_checked', 'variant_status_counts', 'failed_cases', 'classification_counts')}))


if __name__ == '__main__':
    main()
