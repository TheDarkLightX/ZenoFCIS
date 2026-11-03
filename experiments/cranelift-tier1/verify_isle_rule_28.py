#!/usr/bin/env python3
"""Fail-closed, source-pinned VeriISLE qualification of the Tier-1 opt rule.

This does NOT verify target x86 machine semantics, flags, ABI, performance, or
that the rule is desirable on every target. It verifies only the named mid-end
rule's reported applicable expansions, using the pinned VeriISLE specifications.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import re
import shutil
import subprocess
from pathlib import Path
from install_isle_candidate import UPSTREAM, SOURCE, ORIGINAL_GIT_BLOB, RULE, RULE_NAME

COUNTS = ('Total expansions', 'In scope expansions', 'Expansion errors',
          'Type instantiations', 'Applicable', 'Verification passed',
          'Verification failed', 'Verification unknown')


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def run_cmd(args: list[str], timeout: int = 30, cwd: Path | None = None):
    return subprocess.run(args, cwd=cwd, text=True, capture_output=True,
                          timeout=timeout, check=True).stdout.strip()


def check_source(upstream: Path) -> dict:
    actual = run_cmd(['git', '-C', str(upstream), 'rev-parse', 'HEAD'])
    if actual != UPSTREAM:
        raise ValueError('wrong pinned upstream revision')
    blob = run_cmd(['git', '-C', str(upstream), 'rev-parse', f'HEAD:{SOURCE}'])
    if blob != ORIGINAL_GIT_BLOB:
        raise ValueError('original ISLE blob differs from source pin')
    original = run_cmd(['git', '-C', str(upstream), 'show', f'HEAD:{SOURCE}'])
    # git show output is stripped; installer itself uses old.rstrip('\n').
    expected = original.rstrip('\n') + '\n' + RULE
    source = (upstream / SOURCE).read_text()
    if source != expected:
        raise ValueError('not exactly the one reviewed ISLE rule patch')
    modified = run_cmd(['git', '-C', str(upstream), 'diff', '--name-only'])
    if modified.splitlines() != [SOURCE]:
        raise ValueError('unexpected tracked modifications in compiler checkout')
    return {'upstream_commit': actual, 'original_blob': blob,
            'patched_source_sha256': sha(source.encode()),
            'rule_sha256': sha(RULE.encode()), 'source_path': SOURCE}


def parse_summary(output: str, *, returncode: int = 0) -> dict:
    if returncode != 0:
        raise ValueError('VeriISLE command returned nonzero')
    if len(re.findall(r'(?m)^only rule:\s+' + re.escape(RULE_NAME) + r'\s*$', output)) != 1:
        raise ValueError('missing or ambiguous targeted rule selection')
    if output.count('Verification summary') != 1:
        raise ValueError('missing or ambiguous verifier summary')
    result = {}
    for name in COUNTS:
        matches = re.findall(r'(?m)^' + re.escape(name) + r':\s*(\d+)\s*$', output)
        if len(matches) != 1:
            raise ValueError(f'missing or duplicated verifier counter: {name}')
        result[name.lower().replace(' ', '_')] = int(matches[0])
    if (result['total_expansions'] < 1 or result['in_scope_expansions'] < 1 or
            result['type_instantiations'] < 1 or result['applicable'] < 1 or
            result['verification_passed'] != result['applicable'] or
            result['expansion_errors'] != 0 or result['verification_failed'] != 0 or
            result['verification_unknown'] != 0):
        raise ValueError('non-applicable, incomplete, failed or unknown VeriISLE verification')
    return result


def qualify(upstream: Path, output: Path) -> dict:
    if output.exists():
        raise ValueError('refusing to overwrite verifier receipt')
    pin = check_source(upstream)
    tools = {}
    for program in ('cargo', 'rustc', 'cvc5', 'z3'):
        executable = shutil.which(program)
        if executable is None:
            raise ValueError(f'missing verifier dependency: {program}')
        tools[program] = {'path': executable,
                          'version': run_cmd([executable, '--version'])[:500]}
    command = ['cargo', 'run', '--locked', '--manifest-path', str(upstream/'Cargo.toml'),
               '-p', 'cranelift-isle-veri', '--bin', 'veri', '--',
               '--name', 'opt', '--rule', RULE_NAME, '--num-threads', '2', '--timeout', '60']
    result = subprocess.run(command, cwd=upstream, text=True, capture_output=True, timeout=1200)
    output.parent.mkdir(parents=True, exist_ok=True)
    stdout, stderr = output.with_suffix('.stdout.txt'), output.with_suffix('.stderr.txt')
    if stdout.exists() or stderr.exists():
        raise ValueError('refusing to overwrite verifier raw log')
    stdout.write_text(result.stdout)
    stderr.write_text(result.stderr)
    counts = parse_summary(result.stdout, returncode=result.returncode)
    receipt = {'schema': 'zeno-fcis/tier1-veriisle-named-rule/1',
               'status': 'passed-named-opt-rule-applicable-expansions',
               'pin': pin, 'tools': tools, 'command': command, 'counts': counts,
               'stdout_sha256': sha(result.stdout.encode()),
               'stderr_sha256': sha(result.stderr.encode()),
               'nonclaims': ['not full x86 ISA, ABI, poison or flag-liveness proof',
                             'not speed or throughput proof',
                             'not cross-target optimizer performance qualification']}
    output.write_text(json.dumps(receipt, indent=2, sort_keys=True) + '\n')
    return receipt


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument('--upstream', type=Path, required=True)
    p.add_argument('--out', type=Path, required=True)
    args = p.parse_args()
    result = qualify(args.upstream.resolve(), args.out.resolve())
    print(json.dumps({'status': result['status'], 'counts': result['counts']}))


if __name__ == '__main__':
    main()
