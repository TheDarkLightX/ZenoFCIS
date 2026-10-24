#!/usr/bin/env python3
"""Qualify the source-closed arithmetic fragment; no release authority.

The profile is reviewed data, never learned from the run being accepted. The
source closure is included by path without compiled application dependencies.
Method: Verus correspondence layers; compiler/native binding remains explicit.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time

import check_verus as verifier
from scalar_arithmetic_reference import boundary_cases, result

ROOT = Path(__file__).resolve().parents[1]
HARNESS = Path("verification/verus/arithmetic_proof.rs")
SPEC = Path("verification/verus/arithmetic_spec.rs")
PROFILE = Path("verification/verus/arithmetic-proof.json")
SCALAR = Path("crates/zeno-fcis-synthesis/src/finite/value/scalar.rs")
WIDE = Path("crates/zeno-fcis-synthesis/src/finite/value/wide.rs")
NATIVE = Path("verification/verus/arithmetic_std_tests.rs")
PROBE = Path("verification/verus/arithmetic_probe.rs")
CLOSURE = (HARNESS, SPEC, SCALAR, WIDE, Path("verification/verus/arithmetic_vectors.rs"))
SOURCES = (*CLOSURE, PROFILE, NATIVE, PROBE, Path("verification/verus/toolchain.json"),
           Path("tools/check_arithmetic_proof.py"), Path("tools/test_check_arithmetic_proof.py"),
           Path("tools/check_verus.py"), Path("tools/scalar_arithmetic_reference.py"),
           Path("tools/arithmetic_proof_vectors.py"),
           *verifier.QUALIFICATION_SOURCES)


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def snapshot(root: Path = ROOT) -> dict:
    return {str(p): digest(root / p) for p in SOURCES}


def audit_native_binding(source: str) -> None:
    """Conservative source policy: unknown configuration and macros refuse.

    Inspect raw text as well as attributes. This deliberately refuses suspicious
    words even in comments/strings; it cannot hide native code through a Rust
    lexer disagreement. Hashes retain the full reviewed input for replay.
    """
    if re.search(r"\b(?:assume|assume_specification|admit|external_body|unsafe)\b|"
                 r"verifier\s*::\s*(?:external|axiom)|\bcfg\s*!", source):
        raise ValueError("unreviewed trust escape or cfg! in arithmetic closure")
    for match in re.finditer(r"#\s*!?\s*\[", source):
        start, at, depth = match.start(), match.end(), 1
        while at < len(source) and depth:
            depth += (source[at] == '[') - (source[at] == ']')
            at += 1
        if depth:
            raise ValueError("unterminated source attribute")
        attr = re.sub(r"\s+", "", source[match.end():at-1])
        if attr.startswith("cfg(") and attr != "cfg(verus_keep_ghost)":
            raise ValueError("unreviewed conditional code in arithmetic closure")
        if attr == "cfg(verus_keep_ghost)":
            tail = source[at:].lstrip()
            if not re.match(r'(?:use\s+vstd::|proof\s*!|verus\s*!|'
                            r'#\[path\s*=\s*"[A-Za-z0-9_./-]+"\]\s*(?:pub\((?:crate|super)\)\s+)?mod\s+|'
                            r'(?:pub\((?:crate|super)\)\s+)?mod\s+)', tail):
                raise ValueError("ghost configuration encloses executable code")
        if attr.startswith("cfg_attr(") and not (
            attr.startswith("cfg_attr(verus_keep_ghost,verus_spec")
            or attr == "cfg_attr(verus_keep_ghost,feature(proc_macro_hygiene))"
        ):
            raise ValueError("unreviewed conditional attribute in arithmetic closure")
        if attr.startswith("cfg_attr(") and re.search(r"\b(?:cfg|cfg_attr|path)\(", attr[9:]):
            raise ValueError("nested conditional attribute in arithmetic closure")
        if "include" in attr:
            raise ValueError("unreviewed attribute expansion")
    # No arbitrary user macro may add or replace native executable statements.
    allowed = {"verus", "proof", "assert", "matches"}
    macros = set(re.findall(r"\b([A-Za-z_]\w*)\s*!\s*[({[]", source))
    if macros - allowed:
        raise ValueError("unreviewed macro in arithmetic closure: " + ','.join(sorted(macros-allowed)))
    for match in re.finditer(r'\bverus\s*!\s*\{', source):
        at=match.end(); depth=1
        while at<len(source) and depth:
            depth+=(source[at]=='{')-(source[at]=='}');at+=1
        block=source[match.end():at-1]
        # Ghost verus! blocks are declaration containers, never executable
        # replacement blocks. Proof-mode statements use proof!, which Verus
        # prevents from changing executable state.
        if not re.match(r'\s*(?:(?://[^\n]*\n)\s*)*(?:pub(?:\(crate\))?\s+)?(?:open\s+|closed\s+)?(?:spec|proof)\s+fn\b',block):
            raise ValueError('ghost verus block is not a specification declaration')
        for fn in re.finditer(r'\bfn\s+',block):
            if not re.search(r'\b(?:spec|proof)\s+$',block[:fn.start()]):
                raise ValueError('ghost verus block contains an executable function')


def accepted(report: dict, pin: dict, profile: dict) -> bool:
    return (verifier.proof_succeeded(report, pin)
            and type(report['verification-results'].get('verified')) is int
            and report['verification-results']['verified'] == profile['expected_verified']
            and sorted(report['func-details']) == profile['functions'])


def process(command: list[str], root: Path, environment: dict, out: Path,
            input_text: str | None = None, timeout: int = 1200):
    out.mkdir(parents=True, exist_ok=False)
    command = ['nice', '-n', '19', 'ionice', '-c3', *command]
    start = time.monotonic()
    try:
        p = subprocess.run(command, cwd=root, env=environment, input=input_text,
                           text=True, capture_output=True, timeout=timeout)
    except subprocess.TimeoutExpired as error:
        def decoded(value):
            return value.decode(errors='replace') if isinstance(value,bytes) else value or ''
        (out/'stdout.txt').write_text(decoded(error.stdout))
        (out/'stderr.txt').write_text(decoded(error.stderr))
        (out/'process.json').write_text(json.dumps(dict(command=command,exit_code=None,
            outcome='timeout',timeout_seconds=timeout,seconds=time.monotonic()-start),indent=2)+'\n')
        raise
    (out/'stdout.txt').write_text(p.stdout)
    (out/'stderr.txt').write_text(p.stderr)
    (out/'process.json').write_text(json.dumps(dict(command=command, exit_code=p.returncode,
        seconds=time.monotonic()-start), indent=2)+'\n')
    return p


def once(source: str, old: str, new: str) -> str:
    if source.count(old) != 1:
        raise ValueError("control anchor changed: " + old)
    return source.replace(old, new)


def qualify(out: Path, work: Path, controls: bool) -> dict:
    before = snapshot()
    pin = json.loads((ROOT / 'verification/verus/toolchain.json').read_text())
    profile = json.loads((ROOT / PROFILE).read_text())
    if profile['source_closure'] != [str(p) for p in CLOSURE]:
        raise ValueError('source closure differs from reviewed inventory')
    for path in CLOSURE:
        audit_native_binding((ROOT/path).read_text())
    from arithmetic_proof_vectors import source as reference_source
    if (ROOT/'verification/verus/arithmetic_vectors.rs').read_text()!=reference_source():
        raise ValueError('abstract reference-vector proof differs from its producer')
    tools = verifier.prepare_tools(verifier.cache_directory(), pin, False)
    environment = {k: v for k, v in os.environ.items()
                   if not k.startswith(('VERUS_', 'VARGO_'))
                   and k not in ('RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS')}
    environment.update(RUSTUP_TOOLCHAIN=pin['rust_toolchain'], VERUS_Z3_PATH=str(tools/'z3'))
    command = [str(tools/'verus'), '--crate-type=lib', '--edition=2024', '--no-cheating',
               '--no-external-by-default', '--num-threads', '4', '--output-json',
               '--triggers-mode', 'silent', str(HARNESS)]
    positive = process(command, ROOT, environment, out/'proof')
    verifier.require_success(positive)
    report = json.loads(positive.stdout)
    if not accepted(report, pin, profile):
        raise ValueError('proof version, entire-crate status, count or exact function inventory differs')
    native_env = dict(environment)
    native_env.pop('RUSTUP_TOOLCHAIN', None)
    rust = ['rustc', '+1.97.1', '--edition=2024', '--check-cfg', 'cfg(verus_keep_ghost)']
    test_binary = work/'std-tests'
    verifier.require_success(process([*rust, '--test', str(NATIVE), '-o', str(test_binary)],
                                    ROOT, native_env, out/'native-build'))
    native = process([str(test_binary)], ROOT, native_env, out/'native-tests')
    verifier.require_success(native)
    probe = work/'arithmetic-probe'
    verifier.require_success(process([*rust, str(PROBE), '-o', str(probe)],
                                    ROOT, native_env, out/'reference-build'))
    cases = boundary_cases(128)
    text = ''.join(c.operation+' '+' '.join(map(str,c.operands))+'\n' for c in cases)
    observed = process([str(probe)], ROOT, native_env, out/'reference', text)
    verifier.require_success(observed)
    expected = ['null' if result(c,128) is None else str(result(c,128)) for c in cases]
    if observed.stdout.splitlines() != expected:
        raise ValueError('production arithmetic disagrees with independent Fraction oracle')
    faults = []
    if controls:
        scalar = (ROOT/SCALAR).read_text()
        changes = {
            'ceiling_off_by_one': (once(scalar, 'q.checked_add(1)?', 'q.checked_add(2)?'),
                                   'divide', 'ceiling_division_witness'),
            'wrapped_mul_div': (once(scalar,
                'wide::mul_div_floor(a as u128, b as u128, den as u128)',
                'Some((a as u128).wrapping_mul(b as u128) / (den as u128))'),
                'mul_div', 'full_product_refuses_overflow'),
        }
        for name, (changed, function, test) in changes.items():
            specimen = work/name
            for path in (*CLOSURE, NATIVE):
                (specimen/path).parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(ROOT/path, specimen/path)
            original = (specimen/SCALAR).read_bytes()
            (specimen/SCALAR).write_text(changed)
            binary = specimen/'control-tests'
            verifier.require_success(process([*rust, '--test', str(NATIVE), '-o', str(binary)],
                specimen, native_env, out/name/'build'))
            bad = process([str(binary), '--exact', test], specimen, native_env, out/name/'assertion')
            if bad.returncode != 101 or 'assertion `left == right` failed' not in bad.stdout:
                raise ValueError('control did not fail its named assertion: '+name)
            proof = process(command, specimen, environment, out/name/'proof')
            bad_report = json.loads(proof.stdout)
            if not verifier.semantic_refusal(proof,bad_report,pin,SCALAR,specimen,
                    region=verifier.function_lines(changed,function),
                    target_functions=['arithmetic_proof::scalar::'+function]):
                raise ValueError('control did not fail the intended proof: '+name)
            (specimen/SCALAR).write_bytes(original)
            if digest(specimen/SCALAR) != before[str(SCALAR)]:
                raise ValueError('control source restoration differs')
            verifier.require_success(process([*rust, '--test', str(NATIVE), '-o', str(binary)],
                specimen, native_env, out/name/'restored-build'))
            verifier.require_success(process([str(binary), '--exact', test], specimen,
                                            native_env, out/name/'restored-assertion'))
            restored = process(command, specimen, environment, out/name/'restored-proof')
            verifier.require_success(restored)
            if not accepted(json.loads(restored.stdout),pin,profile):
                raise ValueError('restored proof inventory differs')
            faults.append(dict(name=name, intended_function=function, normal=0, build=0,
                mutated_assertion=101, mutated_proof=proof.returncode, restored=0,
                restored_sha256=digest(specimen/SCALAR)))
    verifier.verify_tool_files(tools,pin)
    if before != snapshot():
        raise ValueError('proof inputs changed during qualification')
    return dict(schema='zeno-fcis/arithmetic-proof-evidence/1', status='passed',
        scope=profile['scope'], source_sha256=before, source_closure=profile['source_closure'],
        toolchain=pin, verus_report=report, independent_reference_rows=len(cases),
        controls=faults, all_planned_primitive_controls=controls,
        unproved=['full A1-A3 coverage', 'typed numeric casts correspondence',
                  'E1 emitted-program correspondence', 'E2 complete generated application binding',
                  'rustc code generation, standard-library contracts, Verus/vstd/Z3, platform'])


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--work', type=Path, required=True)
    parser.add_argument('--positive-only', action='store_true')
    args = parser.parse_args()
    args.out = args.out.resolve(); args.work = args.work.resolve()
    args.out.mkdir(parents=True, exist_ok=False)
    args.work.mkdir(parents=True, exist_ok=False)
    try:
        receipt = qualify(args.out, args.work, not args.positive_only)
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        receipt = dict(schema='zeno-fcis/arithmetic-proof-evidence/1', status='failed', error=str(error))
    (args.out/'receipt.json').write_text(json.dumps(receipt,indent=2,sort_keys=True)+'\n')
    print(json.dumps({k:v for k,v in receipt.items() if k in ('status','error','scope','independent_reference_rows','controls')}))
    return 0 if receipt['status']=='passed' else 1


if __name__ == '__main__':
    raise SystemExit(main())
