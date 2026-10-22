#!/usr/bin/env python3
"""Generate a test fixture, fake value, not a deployment genesis."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

APP = Path(__file__).resolve().parents[1]
LABEL = 'test fixture, fake value, not a deployment genesis'


def fixture_policy(production):
    # Preparation data only. No caller-supplied state enters the decision API.
    policy = json.loads(json.dumps(production))
    if any(value != 0 for value in policy['genesis'].values()):
        raise ValueError('expected unchanged zero-genesis production contract')
    for field in (2016, 2020, 2024, 2028):
        for limb in range(4):
            policy['genesis'][str(field+limb)] = (10**12 >> (32*limb)) & ((1 << 32)-1)
    return policy


def digest(data):
    return hashlib.sha256(data).hexdigest()


def create(cli, out):
    out = out.resolve()
    if out.is_relative_to(APP.resolve()):
        raise ValueError('generate the test fixture outside the public application tree')
    original = json.loads((APP/'v2/policy.json').read_text())
    policy = fixture_policy(original)
    out.mkdir(parents=True, exist_ok=False)
    (out/'v2').mkdir()
    (out/'project.zeno').write_bytes((APP/'project.zeno').read_bytes())
    (out/'v2/policy.json').write_text(json.dumps(policy, indent=2)+'\n')
    subprocess.run([str(cli), '--no-kb', 'generate', 'contract', str(out)], check=True)
    canonical = lambda value: json.dumps(value, sort_keys=True, separators=(',', ':')).encode()
    record = {'schema': 'zeno-fcis/spot-test-fixture/1', 'label': LABEL,
              'funded_per_ordinary_account_per_asset': 10**12,
              'production_genesis_sha256': digest(canonical(original['genesis'])),
              'fixture_genesis_sha256': digest(canonical(policy['genesis'])),
              'production_policy_sha256': digest((APP/'v2/policy.zcve').read_bytes()),
              'fixture_policy_sha256': digest((out/'v2/policy.zcve').read_bytes()),
              'same_schema': (APP/'v2/schema.zcve').read_bytes() == (out/'v2/schema.zcve').read_bytes(),
              'same_rules_and_laws': {k:v for k,v in original.items() if k!='genesis'} ==
                                     {k:v for k,v in policy.items() if k!='genesis'}}
    if not record['same_schema'] or not record['same_rules_and_laws']:
        raise ValueError('fixture changed more than the exact genesis identity')
    (out/'TEST_FIXTURE.json').write_text(json.dumps(record, indent=2)+'\n')
    return record


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--cli', type=Path, required=True)
    p.add_argument('--out', type=Path, required=True)
    a = p.parse_args()
    print(json.dumps(create(a.cli.resolve(), a.out)))
