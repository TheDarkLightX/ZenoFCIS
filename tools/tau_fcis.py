#!/usr/bin/env python3
"""Capture and replay fail-closed Tau synthesis evidence for ZenoFCIS.

Evidence only: this tool cannot mint ZenoFCIS proof, release, settlement, or
value-moving authority.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import time
from pathlib import Path
FORMAT = 'zeno-fcis/tau-synthesis-evidence/1'
MAX_SPEC = 16 * 1024 * 1024
MAX_TOOL = 512 * 1024 * 1024
MAX_OUTPUT = 64 * 1024 * 1024
BLOCKED = 2

class Blocked(Exception):
    pass

def canonical(obj: object) -> bytes:
    return json.dumps(obj, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode() + b'\n'

def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()

def require_file(path: Path, limit: int, label: str) -> None:
    try:
        s = path.lstat()
    except OSError as e:
        raise Blocked(f'{label} cannot be inspected: {e}') from e
    if stat.S_ISLNK(s.st_mode):
        raise Blocked(f'{label} must not be a symlink: {path}')
    if not stat.S_ISREG(s.st_mode):
        raise Blocked(f'{label} must be a regular file: {path}')
    if s.st_size > limit:
        raise Blocked(f'{label} exceeds {limit} bytes: {path}')

def open_nofollow(path: Path, label: str):
    flags = os.O_RDONLY | getattr(os, 'O_NOFOLLOW', 0)
    try:
        fd = os.open(path, flags)
    except OSError as e:
        raise Blocked(f'{label} cannot be opened safely: {e}') from e
    try:
        if not stat.S_ISREG(os.fstat(fd).st_mode):
            raise Blocked(f'{label} stopped being a regular file: {path}')
        return os.fdopen(fd, 'rb')
    except Exception:
        os.close(fd)
        raise

def hash_file(path: Path, limit: int=MAX_TOOL) -> dict[str, object]:
    h = hashlib.sha256()
    n = 0
    with open_nofollow(path, 'file') as f:
        while (chunk := f.read(1024 * 1024)):
            n += len(chunk)
            if n > limit:
                raise Blocked(f'file exceeds {limit} bytes: {path}')
            h.update(chunk)
    return {'sha256': h.hexdigest(), 'byte_length': n}

def read_subject(path: Path) -> tuple[bytes, dict[str, object]]:
    require_file(path, MAX_SPEC, 'spec')
    with open_nofollow(path, 'spec') as f:
        data = f.read(MAX_SPEC + 1)
    if len(data) > MAX_SPEC:
        raise Blocked('spec exceeds size limit')
    return (data, {'sha256': digest(data), 'byte_length': len(data)})

def snapshot(src: Path, dst: Path, role: str) -> dict[str, object]:
    require_file(src, MAX_TOOL, role)
    h = hashlib.sha256()
    n = 0
    with open_nofollow(src, role) as inp, open(dst, 'xb') as out:
        while (chunk := inp.read(1024 * 1024)):
            n += len(chunk)
            if n > MAX_TOOL:
                raise Blocked(f'{role} exceeds size limit')
            h.update(chunk)
            out.write(chunk)
        out.flush()
        os.fsync(out.fileno())
    os.chmod(dst, 448)
    rec = {'role': role, 'sha256': h.hexdigest(), 'byte_length': n}
    if hash_file(dst) != {'sha256': rec['sha256'], 'byte_length': n}:
        raise Blocked(f'{role} snapshot mismatch')
    return rec

def run(argv: list[str], cwd: Path, env: dict[str, str], timeout_ms: int, output_limit: int):
    started = time.monotonic()
    kw = {'start_new_session': True} if os.name == 'posix' else {}
    p = subprocess.Popen(argv, cwd=cwd, env=env, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE, **kw)
    timed_out = False
    try:
        stdout, stderr = p.communicate(timeout=timeout_ms / 1000)
    except subprocess.TimeoutExpired:
        timed_out = True
        try:
            os.killpg(p.pid, signal.SIGKILL) if os.name == 'posix' else p.kill()
        except ProcessLookupError:
            pass
        stdout, stderr = p.communicate()
    if len(stdout) + len(stderr) > output_limit:
        raise Blocked('formal-tool output exceeded configured bound')
    return ({'exit_code': p.returncode, 'timed_out': timed_out, 'duration_ms_observed': int((time.monotonic() - started) * 1000)}, stdout, stderr)

def clean_env(tool_dir: Path, strategy: Path, additions: list[str]) -> dict[str, str]:
    env = {'PATH': str(tool_dir), 'TAU_LTL_EXPORT_STRATEGY': 'hoa', 'TAU_LTL_EXPORT_STRATEGY_FILE': strategy.name}
    for name in ('SYSTEMROOT', 'WINDIR', 'TMPDIR', 'TMP', 'TEMP'):
        if name in os.environ:
            env[name] = os.environ[name]
    for item in additions:
        if '=' not in item:
            raise Blocked(f'--env requires NAME=VALUE: {item}')
        k, v = item.split('=', 1)
        if not k or k in env:
            raise Blocked(f'reserved or empty environment name: {k!r}')
        env[k] = v
    return env

def write_new(path: Path, data: bytes) -> None:
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 384)
    with os.fdopen(fd, 'wb') as f:
        f.write(data)
        f.flush()
        os.fsync(f.fileno())

def empty_dir(path: Path) -> None:
    if path.exists():
        if path.is_symlink():
            raise Blocked(f'output directory must not be a symlink: {path}')
        if not path.is_dir() or any(path.iterdir()):
            raise Blocked(f'output directory must be an empty directory: {path}')
    else:
        path.mkdir(parents=True, mode=448)

def commit40(value: str) -> str:
    if len(value) != 40 or any((c not in '0123456789abcdef' for c in value)):
        raise Blocked('--tau-source-commit must be 40 lowercase hex characters')
    return value

def toolset(args, work: Path):
    d = work / 'toolchain'
    d.mkdir(mode=448)
    records, paths = ({}, {})
    for role, src, name in (('tau', args.tau, 'tau'), ('tau_codegen', args.tau_codegen, 'tau_codegen')):
        paths[role] = d / name
        records[role] = snapshot(src, paths[role], role)
    if args.ltlsynt:
        paths['ltlsynt'] = d / 'ltlsynt'
        records['ltlsynt'] = snapshot(args.ltlsynt, paths['ltlsynt'], 'ltlsynt')
    return (d, records, paths)

def semantic_part(receipt: dict) -> dict:
    arts = receipt['artifacts']
    return {'format': receipt['format'], 'authority': receipt['authority'], 'classification': receipt['classification'], 'block_reason': receipt['block_reason'], 'subject': receipt['subject'], 'toolchain': receipt['toolchain'], 'invocation': receipt['invocation'], 'products': {k: arts[k] for k in ('program.h', 'strategy.hoa') if k in arts}}

def capture(args) -> int:
    empty_dir(args.out)
    subject, subject_rec = read_subject(args.spec.resolve())
    source_commit = commit40(args.tau_source_commit)
    with tempfile.TemporaryDirectory(prefix='zeno-fcis-tau-') as tmp:
        work = Path(tmp)
        tool_dir, tools, paths = toolset(args, work)
        (work / 'subject.tau').write_bytes(subject)
        program, strategy = (work / 'program.h', work / 'strategy.hoa')
        env = clean_env(tool_dir, strategy, args.env)
        version_env = {k: v for k, v in env.items() if not k.startswith('TAU_LTL_EXPORT_STRATEGY')}
        vr, vout, verr = run([str(paths['tau']), '--version'], work, version_env, min(args.timeout_ms, 30000), args.max_output_bytes)
        if vr['timed_out'] or vr['exit_code'] != 0:
            raise Blocked('snapshotted tau --version failed')
        result, stdout, stderr = run([str(paths['tau_codegen']), 'subject.tau', '-o', 'program.h'], work, env, args.timeout_ms, args.max_output_bytes)
        reason = None
        if result['timed_out']:
            reason = 'timeout'
        elif result['exit_code'] != 0:
            reason = 'tau_codegen_failed'
        elif not program.is_file() or not program.stat().st_size:
            reason = 'missing_generated_program'
        elif args.require_strategy and (not strategy.is_file() or not strategy.stat().st_size):
            reason = 'missing_strategy'
        classification = 'blocked' if reason else 'proposed_synthesis'
        files = {'subject.tau': subject, 'stdout.bin': stdout, 'stderr.bin': stderr, 'tau-version.stdout.bin': vout, 'tau-version.stderr.bin': verr}
        if program.is_file():
            files['program.h'] = program.read_bytes()
        if strategy.is_file():
            files['strategy.hoa'] = strategy.read_bytes()
        for name, data in files.items():
            write_new(args.out / name, data)
        artifacts = {name: hash_file(args.out / name) for name in files}
        ignored = {'PATH', 'TAU_LTL_EXPORT_STRATEGY', 'TAU_LTL_EXPORT_STRATEGY_FILE', 'SYSTEMROOT', 'WINDIR', 'TMPDIR', 'TMP', 'TEMP'}
        receipt = {'format': FORMAT, 'authority': 'none', 'classification': classification, 'block_reason': reason, 'subject': {'declared_tau_source_commit': source_commit, 'source_commit_binding': 'caller_declared_metadata_only', 'spec_sha256': subject_rec['sha256'], 'spec_byte_length': subject_rec['byte_length']}, 'toolchain': {k: tools[k] for k in sorted(tools)}, 'invocation': {'operation': 'tau_codegen_synthesize', 'argv_template': ['$TAU_CODEGEN', '$SUBJECT', '-o', '$PROGRAM'], 'environment': {'PATH': '$PRIVATE_TOOLCHAIN', 'TAU_LTL_EXPORT_STRATEGY': 'hoa', 'TAU_LTL_EXPORT_STRATEGY_FILE': '$STRATEGY', 'extra_value_sha256': {k: digest(v.encode()) for k, v in sorted(env.items()) if k not in ignored}}, 'timeout_ms': args.timeout_ms, 'max_output_bytes': args.max_output_bytes, 'require_strategy': args.require_strategy}, 'result': result, 'artifacts': artifacts, 'nonclaims': ['Retained formal-tool evidence only; no ZenoFCIS authority.', 'Declared Tau source commit is not derived from executable bytes.', 'Requirement-to-Tau translation correctness is outside this record.', 'Host kernel, loader, shared libraries, and transitive runtime dependencies are outside this record.']}
        receipt['evidence_subject_sha256'] = digest(canonical(semantic_part(receipt)))
        receipt['receipt_sha256'] = digest(canonical(receipt))
        write_new(args.out / 'receipt.json', canonical(receipt))
    print(json.dumps({'format': FORMAT, 'classification': classification, 'block_reason': reason, 'evidence_subject_sha256': receipt['evidence_subject_sha256'], 'receipt_sha256': receipt['receipt_sha256'], 'output': str(args.out.resolve())}, sort_keys=True))
    return 0 if not reason else BLOCKED

def load_bundle(bundle: Path) -> dict:
    require_file(bundle / 'receipt.json', 4 * 1024 * 1024, 'receipt')
    try:
        r = json.loads((bundle / 'receipt.json').read_text())
    except (OSError, UnicodeError, json.JSONDecodeError) as e:
        raise Blocked(f'invalid receipt: {e}') from e
    keys = {'format', 'authority', 'classification', 'block_reason', 'subject', 'toolchain', 'invocation', 'result', 'artifacts', 'nonclaims', 'evidence_subject_sha256', 'receipt_sha256'}
    if set(r) != keys or r['format'] != FORMAT or r['authority'] != 'none':
        raise Blocked('invalid receipt envelope')
    claimed = r['receipt_sha256']
    core = dict(r)
    del core['receipt_sha256']
    if digest(canonical(core)) != claimed:
        raise Blocked('receipt identity mismatch')
    if digest(canonical(semantic_part(r))) != r['evidence_subject_sha256']:
        raise Blocked('evidence subject mismatch')
    return r

def verify_bundle_files(bundle: Path, r: dict) -> None:
    for name, expected in r['artifacts'].items():
        if not isinstance(name, str) or '/' in name or '\\' in name or (name in {'.', '..'}):
            raise Blocked('invalid artifact name')
        require_file(bundle / name, MAX_TOOL, f'artifact {name}')
        if hash_file(bundle / name) != expected:
            raise Blocked(f'artifact mismatch: {name}')

def replay(args) -> int:
    bundle = args.bundle.resolve()
    if not bundle.is_dir() or bundle.is_symlink():
        raise Blocked('bundle must be a real directory')
    r = load_bundle(bundle)
    verify_bundle_files(bundle, r)
    supplied_paths = {'tau': args.tau, 'tau_codegen': args.tau_codegen}
    if 'ltlsynt' in r['toolchain']:
        if not args.ltlsynt:
            raise Blocked('receipt requires --ltlsynt')
        supplied_paths['ltlsynt'] = args.ltlsynt
    elif args.ltlsynt:
        raise Blocked('receipt did not bind ltlsynt')
    for role, path in supplied_paths.items():
        require_file(path, MAX_TOOL, role)
        if hash_file(path) != {k: r['toolchain'][role][k] for k in ('sha256', 'byte_length')}:
            raise Blocked(f'{role} bytes do not match receipt')
    inv = r['invocation']
    expected_env = inv['environment'].get('extra_value_sha256', {})
    given = {}
    for item in args.env:
        if '=' not in item:
            raise Blocked(f'--env requires NAME=VALUE: {item}')
        k, v = item.split('=', 1)
        given[k] = v
    if set(given) != set(expected_env):
        raise Blocked('replay environment-name mismatch')
    for k, h in expected_env.items():
        if digest(given[k].encode()) != h:
            raise Blocked(f'replay environment value mismatch: {k}')
    with tempfile.TemporaryDirectory(prefix='zeno-fcis-tau-replay-') as tmp:
        work = Path(tmp)
        tool_dir, tools, paths = toolset(args, work)
        for role in tools:
            if tools[role]['sha256'] != r['toolchain'][role]['sha256']:
                raise Blocked(f'snapshotted {role} mismatch')
        shutil.copyfile(bundle / 'subject.tau', work / 'subject.tau')
        program, strategy = (work / 'program.h', work / 'strategy.hoa')
        env = clean_env(tool_dir, strategy, [f'{k}={v}' for k, v in sorted(given.items())])
        result, _, _ = run([str(paths['tau_codegen']), 'subject.tau', '-o', 'program.h'], work, env, inv['timeout_ms'], inv['max_output_bytes'])
        expected = r['result']
        if result['timed_out'] != expected['timed_out'] or (not expected['timed_out'] and result['exit_code'] != expected['exit_code']):
            raise Blocked('process outcome drift')
        for name, path in (('program.h', program), ('strategy.hoa', strategy)):
            old = r['artifacts'].get(name)
            if old is None:
                if path.exists():
                    raise Blocked(f'replay unexpectedly produced {name}')
            elif not path.is_file() or hash_file(path) != old:
                raise Blocked(f'replay artifact drift: {name}')
    print(json.dumps({'format': FORMAT, 'replay': 'matched', 'evidence_subject_sha256': r['evidence_subject_sha256'], 'receipt_sha256': r['receipt_sha256']}, sort_keys=True))
    return 0

def parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog='tau-fcis', description='Proof-carrying Tau synthesis evidence bridge')
    s = p.add_subparsers(dest='command', required=True)
    c = s.add_parser('capture')
    c.add_argument('--spec', type=Path, required=True)
    c.add_argument('--tau', type=Path, required=True)
    c.add_argument('--tau-codegen', type=Path, required=True)
    c.add_argument('--ltlsynt', type=Path)
    c.add_argument('--tau-source-commit', required=True)
    c.add_argument('--out', type=Path, required=True)
    c.add_argument('--timeout-ms', type=int, default=60000)
    c.add_argument('--max-output-bytes', type=int, default=MAX_OUTPUT)
    c.add_argument('--env', action='append', default=[])
    c.add_argument('--allow-no-strategy', action='store_false', dest='require_strategy')
    c.set_defaults(require_strategy=True, func=capture)
    r = s.add_parser('replay')
    r.add_argument('--bundle', type=Path, required=True)
    r.add_argument('--tau', type=Path, required=True)
    r.add_argument('--tau-codegen', type=Path, required=True)
    r.add_argument('--ltlsynt', type=Path)
    r.add_argument('--env', action='append', default=[])
    r.set_defaults(func=replay)
    return p

def main(argv=None) -> int:
    try:
        a = parser().parse_args(argv)
        if hasattr(a, 'timeout_ms') and (not 1 <= a.timeout_ms <= 600000):
            raise Blocked('timeout out of range')
        if hasattr(a, 'max_output_bytes') and (not 1 <= a.max_output_bytes <= MAX_OUTPUT):
            raise Blocked('output bound out of range')
        return a.func(a)
    except (Blocked, OSError, subprocess.SubprocessError) as e:
        print(json.dumps({'format': FORMAT, 'status': 'blocked', 'error': str(e)}, sort_keys=True), file=sys.stderr)
        return BLOCKED
if __name__ == '__main__':
    raise SystemExit(main())
