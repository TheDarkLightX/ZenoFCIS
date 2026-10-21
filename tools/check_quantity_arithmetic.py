#!/usr/bin/env python3
"""Native D6/D10 semantic mutants, with admitted witnesses and bypass controls.

Runs only the named oracle tests, in an isolated source copy. It never mutates
this checkout, runs a verifier, accesses the network, or changes proof admission.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import shlex
import subprocess
import tempfile

from quantity_arithmetic_reference import fused, recurrence
from scalar_arithmetic_controls import replace_once

ROOT = Path(__file__).resolve().parents[1]
SCALING = Path('crates/zeno-fcis-contract/src/internals/quantity.rs')
GRAPH = Path('crates/zeno-fcis-contract/src/internals/graph.rs')
TEST = Path('crates/zeno-fcis-contract/tests/quantity_arithmetic.rs')


def specimens(root):
    yield ('bps-denominator', SCALING,
           replace_once((root / SCALING).read_text(),
                        '10i128.checked_pow(power)',
                        '10i128.checked_pow(power).map(|value| value / 10)'),
           'w25_bps_oracle', fused(123456789, 25, 1, p=8, t=4, q=8),
           3086419)
    fused_steps = '''if *count == 2 {
        let aa = self.op(Op::Mul(a.index, a.index), Kind::Int)?;
        let dd = self.op(Op::Mul(d.index, d.index), Kind::Int)?;
        if *signed {
            return self.op(Op::SignedMulDivFloor(value.index, aa.index, dd.index), Kind::Int);
        }
        let x = self.product_operand(value)?;
        let aa = self.product_operand(aa)?;
        let dd = self.product_operand(dd)?;
        let result = self.op(Op::MulDiv(Rounding::Floor, x.index, aa.index, dd.index), Kind::Int)?;
        return self.product_result(result);
    }
    for _ in 0..*count {'''
    yield ('chain-prefix', GRAPH,
           replace_once((root / GRAPH).read_text(), 'for _ in 0..*count {', fused_steps),
           'w25_chain_prefix_oracle', recurrence(1001, 225000, 300000, 2)[1],
           (1001 * 225000 ** 2) // 300000 ** 2)


def run(root, test, log, target_dir, namespace, disable=False):
    # The guard runs before every native build, including incremental reruns.
    disk = subprocess.check_output(['df', '-B1G', '/'], text=True)
    if shutil.disk_usage('/').free < 6 * 1024 ** 3:
        raise RuntimeError('STOP: below 6 GiB before build')
    env = dict(os.environ, CARGO_TARGET_DIR=str(target_dir),
               CARGO_PROFILE_DEV_DEBUG='0', CARGO_PROFILE_TEST_DEBUG='0', CARGO_BUILD_JOBS='2')
    # Cargo target directories can reuse a package fingerprint across copied
    # source roots. Distinct symbol metadata makes those artifacts disjoint
    # without changing arithmetic or the mandated target directory.
    flags = env.get('CARGO_ENCODED_RUSTFLAGS')
    flags = flags.split('\x1f') if flags else shlex.split(env.get('RUSTFLAGS', ''))
    flags += ['-C', 'metadata=quantity_controls_' + namespace]
    env['CARGO_ENCODED_RUSTFLAGS'] = '\x1f'.join(flags)
    env.pop('W25_DISABLE_COMPARISON', None)
    if disable:
        env['W25_DISABLE_COMPARISON'] = '1'
    cmd = ['cargo', '+1.97.1', 'test', '--locked', '--offline', '-j', '2',
           '-p', 'zeno-fcis-contract', '--test', 'quantity_arithmetic', test,
           '--', '--exact', '--nocapture', '--test-threads=1']
    with log.open('w') as output:
        output.write(disk)
        output.flush()
        result = subprocess.run(cmd, cwd=root, env=env, stdout=output, stderr=subprocess.STDOUT)
    text = re.sub(r'\x1b\[[0-9;]*m', '', log.read_text())
    if 'error[E' in text or 'could not compile' in text:
        raise RuntimeError(f'compile error is not a semantic kill: {log}')
    binary = re.search(r'Running tests/quantity_arithmetic.rs \(([^)]+)\)', text)
    if binary is None:
        raise RuntimeError(f'missing native test binary: {log}')
    executable = Path(binary.group(1))
    identity = dict(binary=str(executable), binary_sha256=hashlib.sha256(executable.read_bytes()).hexdigest(),
                    compiler_flags=flags, test=test, comparison_enabled=not disable, exit=result.returncode)
    log.with_suffix('.binary.json').write_text(json.dumps(identity, indent=2) + '\n')
    return result.returncode, text


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--out', type=Path)
    parser.add_argument('--work-dir', type=Path, required=True)
    parser.add_argument('--target-dir', type=Path, default=os.environ.get('CARGO_TARGET_DIR'))
    parser.add_argument('--evidence-dir', type=Path, required=True)
    parser.add_argument('--disable-comparison', action='store_true')
    args = parser.parse_args()
    if args.target_dir is None:
        parser.error('--target-dir or CARGO_TARGET_DIR is required')
    args.work_dir.mkdir(parents=True, exist_ok=True)
    args.evidence_dir.mkdir(parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix='quantity-controls-', dir=args.work_dir))
    copy = work / 'source'
    shutil.copytree(ROOT, copy, ignore=shutil.ignore_patterns('.git', 'target', '.agents', '.codex', '.aws', '__pycache__', 'node_modules'))
    namespace = hashlib.sha256(str(copy).encode()).hexdigest()[:16]
    rows = []
    for name, path, mutant, test, expected, actual in specimens(ROOT):
        baseline_log = args.evidence_dir / f'{name}-baseline.log'
        rc, text = run(copy, test, baseline_log, args.target_dir, namespace)
        baseline = rc == 0 and '1 passed; 0 failed' in text
        original = (copy / path).read_text()
        (copy / path).write_text(mutant)
        specimen = args.evidence_dir / f'{name}-mutant.rs'
        specimen.write_text(mutant)
        kill_log = args.evidence_dir / f'{name}-comparison.log'
        rc, text = run(copy, test, kill_log, args.target_dir, namespace, args.disable_comparison)
        witness = f'W25_WITNESS {name} admitted actual={actual} expected={expected}'
        admitted = witness in text
        killed = rc != 0 and admitted and f'W25_ORACLE_MISMATCH {name}' in text
        bypass_log = args.evidence_dir / f'{name}-bypass.log'
        bypass_rc, bypass_text = run(copy, test, bypass_log, args.target_dir, namespace, True)
        bypass = bypass_rc == 0 and witness in bypass_text and '1 passed; 0 failed' in bypass_text
        (copy / path).write_text(original)
        rows.append(dict(name=name, source=str(path), test=test, baseline=baseline,
                         admitted=admitted, expected=expected, mutant_value=actual,
                         semantic_kill=killed, comparison_bypass_survives=bypass,
                         source_sha256=hashlib.sha256(original.encode()).hexdigest(),
                         mutant_sha256=hashlib.sha256(mutant.encode()).hexdigest(),
                         logs=[str(baseline_log), str(kill_log), str(bypass_log)]))
        print(f'{name}: baseline={baseline} admitted={admitted} killed={killed} bypass={bypass}', flush=True)
    report = dict(schema='zeno-fcis/quantity-native-controls/1', authority='none',
                  comparison_enabled=not args.disable_comparison, artifact_namespace=namespace,
                  passed=all(r['baseline'] and r['admitted'] and r['semantic_kill'] and r['comparison_bypass_survives'] for r in rows),
                  source_test_sha256=hashlib.sha256((ROOT / TEST).read_bytes()).hexdigest(),
                  rows=rows, specimen_root=str(copy), verus='not-run')
    if args.out:
        args.out.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))
    return 0 if report['passed'] else 1


if __name__ == "__main__":
    from telemetry import run_cli
    def _entrypoint():
        raise SystemExit(main())
    run_cli(_entrypoint)
