#!/usr/bin/env python3
"""Retain scoped mount checks and named mutation failures outside the source tree."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import time

APP = Path(__file__).resolve().parents[1]
ROOT = APP.parents[1]
CONTROLS = {
    'trust-body-actor': 'test_authenticated_identity_and_closed_ingress',
    'retained-command-alias': 'test_identity_graph_and_request_aliases',
    'invent-genesis-funds': 'test_zero_genesis_legitimate_creation_is_blocked',
    'wrong-http-reason': 'test_all_zero_genesis_http_decisions_match_native',
    'fee-event-mismatch': 'test_c1_l207_native_mount_and_fee_event',
    'substituted-commit-plan': 'test_restart_retry_and_all_decision_bundle_replay',
    'retry-latest-receipt': 'test_restart_retry_and_all_decision_bundle_replay',
    'adopter-skips-artifact-check': 'test_adopter_refuses_missing_or_substituted_artifacts',
    'half-applied-state': 'test_process_kill_before_commit_is_atomic',
    'native-transition-mismatch': 'test_no_free_value_native_representatives',
}
ROUND2_CONTROLS = {
    'fixture-funding-drift': 'test_fixture_only_changes_approved_genesis',
    'credit-incomplete-journey': 'test_zero_genesis_cannot_be_reported_as_accepted_journey',
    'fixture-http-class-mismatch': 'test_funded_fixture_journey_accepts_and_replays',
    'credit-partial-bound': 'test_k4_budget_records_only_completed_depth',
    'cap-budget-at-120': 'test_k4_budget_records_only_completed_depth',
    'composition-mapping-drift': 'test_component_assembly_matches_root',
    'component-decision-mismatch': 'test_standalone_component_native_decisions',
    'fixture-half-applied': 'test_funded_create_kill_before_commit_is_atomic',
}
ROUND3_CONTROLS = {
    'adopter-ignores-receipt-digest': 'test_adopter_refuses_receipt_digest_corruption',
    'adopter-ignores-outbox-channel': 'test_adopter_refuses_outbox_channel_corruption',
    'adopter-ignores-outbox-payload': 'test_adopter_refuses_outbox_payload_corruption',
    'adopter-ignores-commit-row': 'test_adopter_refuses_commit_row_corruption',
    'adopter-ignores-patch': 'test_adopter_refuses_patch_byte_corruption',
    'adopter-uses-latest-key': 'test_adopter_reconstructs_original_key_after_later_commits',
    'adopter-trusts-report-as-replay': 'test_adopter_requires_separate_immutable_history',
}


def run(argv, out, name, env, expected=0, timeout=120):
    started = time.monotonic()
    try:
        r = subprocess.run(list(map(str, argv)), env=env, cwd=ROOT,
                           capture_output=True, text=True, timeout=timeout, check=False)
        record = {'argv': list(map(str, argv)), 'exit_code': r.returncode,
                  'seconds': time.monotonic()-started, 'stdout': r.stdout, 'stderr': r.stderr}
    except subprocess.TimeoutExpired as error:
        record = {'argv': list(map(str, argv)), 'exit_code': 124, 'status': 'timeout-UNKNOWN',
                  'seconds': time.monotonic()-started,
                  'stdout': str(error.stdout or ''), 'stderr': str(error.stderr or '')}
    (out / (name+'.json')).write_text(json.dumps(record, indent=2)+'\n')
    if record['exit_code'] != expected:
        raise ValueError(f'{name}: expected {expected}, got {record["exit_code"]}')
    return record


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--spot-binary', type=Path, required=True)
    p.add_argument('--counter-binary', type=Path, required=True)
    p.add_argument('--fixture-binary', type=Path, required=True)
    p.add_argument('--cli', type=Path, required=True)
    p.add_argument('--out', type=Path, required=True)
    a = p.parse_args()
    a.out.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, SPOT_MOUNT_BINARY=str(a.spot_binary.resolve()),
               COUNTER_MOUNT_BINARY=str(a.counter_binary.resolve()),
               SPOT_FIXTURE_MOUNT_BINARY=str(a.fixture_binary.resolve()),
               ZENO_FCIS_CLI=str(a.cli.resolve()),
               PYTHONPATH=str(APP/'tests'), SPOT_STAGE5_TRANSCRIPT=str(a.out.resolve()/'native-transcripts.jsonl'))
    env.pop('SPOT_STAGE5_DEFECT', None)
    env.pop('SPOT_ROUND2_DEFECT', None)
    env.pop('SPOT_ROUND3_DEFECT', None)
    baseline = run([sys.executable, '-m', 'unittest', 'discover', '-s', APP/'tests', '-v'],
                   a.out, 'baseline', env, timeout=600)
    controls = []
    for defect, name in CONTROLS.items():
        result = run([sys.executable, '-m', 'unittest', 'test_stage5.Stage5.'+name, '-v'],
                     a.out, defect, dict(env, SPOT_STAGE5_DEFECT=defect), expected=1)
        if name not in result['stderr'] or 'FAIL' not in result['stderr']:
            raise ValueError(defect + ': named test did not turn red')
        controls.append({'defect': defect, 'named_test': name, 'exit_code': 1})
    for defect, name in ROUND2_CONTROLS.items():
        result = run([sys.executable, '-m', 'unittest', 'test_stage5_round2.Round2.'+name, '-v'],
                     a.out, defect, dict(env, SPOT_ROUND2_DEFECT=defect), expected=1)
        if name not in result['stderr'] or 'FAIL' not in result['stderr']:
            raise ValueError(defect + ': named round-2 test did not turn red')
        controls.append({'defect': defect, 'named_test': name, 'exit_code': 1})
    for defect, name in ROUND3_CONTROLS.items():
        result = run([sys.executable, '-m', 'unittest', 'test_stage5_round3.Round3.'+name, '-v'],
                     a.out, defect, dict(env, SPOT_ROUND3_DEFECT=defect), expected=1)
        if name not in result['stderr'] or 'FAIL' not in result['stderr']:
            raise ValueError(defect + ': named round-3 test did not turn red')
        controls.append({'defect': defect, 'named_test': name, 'exit_code': 1})
    summary = {'baseline_exit': baseline['exit_code'], 'planted_defects': controls,
               'scope': 'HTTP byte-stream handler plus native SQLite; TCP is separately gated'}
    (a.out/'summary.json').write_text(json.dumps(summary, indent=2)+'\n')
    print(json.dumps(summary))


if __name__ == '__main__':
    main()
