"""Bounded, source-identified process access to the optional PR153 provider.

The build record binds bytes; it is not a proof that a compiler honestly
produced the executable. The caller must preserve and assess build provenance.
"""
from __future__ import annotations

import hashlib
import json
import os
import re
import signal
import subprocess
import sys
import tempfile
import types
from fractions import Fraction
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parent
VENDOR = ROOT / 'vendor'
MANIFEST = ROOT / 'SOURCE_MANIFEST.json'
MANIFEST_SHA256 = 'b5dddff8d778bc5f43623f577aa77ee1c865c6740d078c83fdd7ee37536b0c10'
UPSTREAM_COMMIT = '73068b03598a39253af8870392674bab58a723c5'
RESULT_SCHEMA = 'zal/relational-profile-result/1'
PREPARATION_SCHEMA = 'zal/relational-runtime-preparation/1'
MAX_BYTES = 1024**2
MAX_BINARY = 128 * 1024**2
MAX_WITNESS_STEPS = 250_000
DEFAULT_LIMITS = {'entries': '100000', 'candidates': '1000000', 'work': '10000000',
                  'bits': '65536', 'domain': '10000'}
REQUEST_KEYS = {
    'zal/relational-query/1': {'schema', 'logic', 'signature', 'formula', 'operation', 'domain_size', 'weights', 'context_id'},
    'zal/relational-evaluation/1': {'schema', 'logic', 'signature', 'formula', 'domain_size', 'context_id', 'model'},
    'zal/relational-comparison/1': {'schema', 'logic', 'signature', 'domain_size', 'context_id', 'assumptions', 'left', 'right', 'relation'},
}
IDENTIFIER = re.compile(r'^[A-Za-z][A-Za-z0-9_]{0,127}$')
COMPARISON_SCOPE = 'all labeled structures at this domain and signature; under the supplied assumptions'


class ProfileFailure(Exception):
    def __init__(self, status: str, detail: str):
        super().__init__(detail)
        self.status, self.detail = status, detail


def _fail(status, detail):
    raise ProfileFailure(status, detail)


def _sha(data):
    return hashlib.sha256(data).hexdigest()


def _profile_identity():
    current = _sha(_read(Path(__file__)))
    captured = globals().get('__verified_source_sha256__')
    if captured is not None:
        if current != captured:
            _fail('Error', 'profile source changed after captured-byte loading')
        return captured, 'captured-standalone'
    return current, 'trusted-local-import'


def _read(path, limit=MAX_BYTES):
    with path.open('rb') as handle:
        data = handle.read(limit + 1)
    if len(data) > limit:
        _fail('Error', 'file exceeds bound: ' + str(path))
    return data


def _ordinary(root, relative):
    path = PurePosixPath(relative)
    if path.is_absolute() or not path.parts or any(p in ('', '.', '..') for p in path.parts):
        _fail('Error', 'unsafe relative path')
    current = root
    for part in path.parts:
        current = current / part
        if current.is_symlink():
            _fail('Error', 'symlink in bound input: ' + relative)
    if not current.is_file():
        _fail('Error', 'missing bound file: ' + relative)
    return current


def _load_manifest():
    data = _read(MANIFEST)
    if _sha(data) != MANIFEST_SHA256:
        _fail('Error', 'provider source manifest changed')
    # The manifest bytes are pinned above before any vendored code is loaded.
    manifest = json.loads(data)
    if (manifest.get('schema') != 'zal/relational-upstream-source/1'
            or manifest.get('commit') != UPSTREAM_COMMIT
            or not isinstance(manifest.get('files'), dict)):
        _fail('Error', 'provider source manifest malformed')
    return manifest


def verify_provider():
    """Check the exact selected upstream blob bytes, not an executable proof."""
    manifest = _load_manifest()
    if VENDOR.is_symlink() or not VENDOR.is_dir():
        _fail('Error', 'provider directory missing or symlinked')
    for relative, record in manifest['files'].items():
        if not isinstance(record, dict):
            _fail('Error', 'malformed provider file record')
        data = _read(_ordinary(VENDOR, relative))
        git_blob = hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()
        if _sha(data) != record.get('sha256') or git_blob != record.get('git_blob'):
            _fail('Error', 'provider source changed: ' + relative)
    sources = {name: rec['sha256'] for name, rec in manifest['files'].items()
               if name == 'app/Main.hs' or (name.startswith('src/') and name.endswith('.hs'))}
    if len(sources) != 8:
        _fail('Error', 'provider Haskell source inventory mismatch')
    return manifest, sources


def _load_vendor(manifest):
    """Execute only the already hash-checked bytes of the two Python helpers."""
    modules = []
    for name in ('run', 'oracle'):
        relative = name + '.py'
        path = _ordinary(VENDOR, relative)
        data = _read(path)
        record = manifest['files'][relative]
        blob = hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()
        if _sha(data) != record['sha256'] or blob != record['git_blob']:
            _fail('Error', 'provider source changed: ' + relative)
        module = types.ModuleType('_zeno_relational_verified_' + name)
        module.__file__ = str(path)
        exec(compile(data, str(path), 'exec'), module.__dict__)  # noqa: S102 - manifest-verified provider bytes
        modules.append(module)
    if modules[0].DEFAULT_LIMITS != DEFAULT_LIMITS:
        _fail('Error', 'provider limits mismatch')
    return modules


def prepare_runtime(directory):
    """Create a new external source copy; do not compile or copy a worker."""
    manifest, _ = verify_provider()
    destination = Path(directory).resolve()
    if destination.exists() or destination.is_symlink():
        _fail('Invalid', 'runtime destination already exists')
    if destination.is_relative_to(ROOT) or ROOT.is_relative_to(destination):
        _fail('Invalid', 'runtime must be separate from the profile source')
    destination.mkdir(parents=False)
    for relative in sorted(manifest['files']):
        source = _ordinary(VENDOR, relative)
        target = destination.joinpath(*PurePosixPath(relative).parts)
        target.parent.mkdir(parents=True, exist_ok=True)
        with target.open('xb') as handle:
            handle.write(_read(source))
    return {'schema': PREPARATION_SCHEMA, 'status': 'Prepared',
            'directory': str(destination), 'upstream_commit': UPSTREAM_COMMIT,
            'provider_manifest_sha256': MANIFEST_SHA256, 'files': len(manifest['files']),
            'worker_built': False}


def _source_inventory(root):
    result = set()
    for directory in ('src', 'app'):
        base = root / directory
        if base.is_symlink() or not base.is_dir():
            _fail('Error', 'runtime source directory missing or symlinked')
        for current, dirs, files in os.walk(base, followlinks=False):
            for name in dirs + files:
                path = Path(current) / name
                if path.is_symlink():
                    _fail('Error', 'symlink in runtime source inventory')
                if name.endswith('.hs') and path.is_file():
                    result.add(path.relative_to(root).as_posix())
    return result


def _hash_binary(path):
    size = path.stat().st_size
    if not 0 < size <= MAX_BINARY:
        _fail('Error', 'worker binary size outside bound')
    h = hashlib.sha256()
    with path.open('rb') as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def _binding(directory, sources, manifest, upstream):
    root = Path(directory).absolute()
    if root.is_symlink() or not root.is_dir():
        _fail('Unavailable', 'runtime directory is missing or symlinked')
    record_path = root / 'evidence/build.json'
    worker_path = root / '.build/zal-relational-worker'
    if not record_path.exists() or not worker_path.exists():
        _fail('Unavailable', 'worker has not been built in this runtime')
    record_data = _read(_ordinary(root, 'evidence/build.json'))
    record = upstream.strict_loads(record_data)
    if (not isinstance(record, dict) or record.get('schema') != 'zal/relational-build/1'
            or type(record.get('exit')) is not int or record['exit'] != 0
            or record.get('source_sha256') != sources):
        _fail('Error', 'worker build record is malformed or binds different sources')
    if _source_inventory(root) != set(sources):
        _fail('Error', 'runtime Haskell source inventory changed')
    for name, digest in sources.items():
        if _sha(_read(_ordinary(root, name))) != digest:
            _fail('Error', 'runtime Haskell source changed: ' + name)
    if _sha(_read(_ordinary(root, 'build.py'))) != manifest['files']['build.py']['sha256']:
        _fail('Error', 'runtime build script changed')
    worker_path = _ordinary(root, '.build/zal-relational-worker')
    binary_sha256 = _hash_binary(worker_path)
    if binary_sha256 != record.get('binary_sha256'):
        _fail('Error', 'worker binary changed after build')
    return worker_path, {'binary_sha256': binary_sha256, 'build_record_sha256': _sha(record_data),
                         'haskell_source_sha256': dict(sources), 'ghc': record.get('ghc')}


def _limits(limits, seconds):
    if type(seconds) is not int or not 1 <= seconds <= 60:
        _fail('Invalid', 'seconds must be an integer in 1..60')
    try:
        selected = dict(DEFAULT_LIMITS if limits is None else limits)
    except (ValueError, TypeError):
        _fail('Invalid', 'limits must be a mapping')
    if set(selected) != set(DEFAULT_LIMITS):
        _fail('Invalid', 'limits must contain exactly the five declared keys')
    for key, value in selected.items():
        if (type(value) is not str or not value.isascii() or not value.isdigit()
                or value.startswith('0') or len(value) > len(DEFAULT_LIMITS[key])
                or not 1 <= int(value) <= int(DEFAULT_LIMITS[key])):
            _fail('Invalid', 'invalid or excessive limit: ' + key)
    return selected


def _snapshot_request(request, limits, upstream):
    if isinstance(request, (bytes, bytearray)):
        raw = bytes(request)
    else:
        try:
            raw = json.dumps(request, ensure_ascii=True, sort_keys=True,
                             separators=(',', ':'), allow_nan=False).encode()
        except (ValueError, TypeError, RecursionError) as exc:
            _fail('Invalid', 'request is not bounded JSON: ' + str(exc))
    try:
        value = upstream.strict_loads(raw)
    except (ValueError, UnicodeDecodeError, RecursionError) as exc:
        detail = str(exc)
        status = 'UNKNOWN' if 'budget exceeded' in detail or 'exceeds 1 MiB' in detail or 'nesting exceeds' in detail else 'Invalid'
        _fail(status, 'invalid or out-of-budget JSON request: ' + detail)
    if not isinstance(value, dict):
        _fail('Invalid', 'request must be an object')
    schema = value.get('schema')
    if type(schema) is not str:
        _fail('Invalid', 'request schema must be text')
    if schema not in REQUEST_KEYS:
        _fail('Unsupported', 'unknown relational request schema')
    if set(value) != REQUEST_KEYS[schema]:
        _fail('Invalid', 'unknown or missing request fields')
    if value.get('logic') not in ('zal/c2/1', 'zal/c2mod/1'):
        _fail('Unsupported', 'unknown relational logic profile')
    if schema == 'zal/relational-query/1' and value.get('operation') != 'count':
        _fail('Unsupported', 'unknown count operation')
    if type(value.get('context_id')) is not str or not 1 <= len(value['context_id']) <= 256:
        _fail('Invalid', 'invalid context identifier')
    signature = value.get('signature')
    if type(signature) is not list or len(signature) > 128:
        _fail('Invalid', 'invalid relational signature')
    names = []
    for declaration in signature:
        if (type(declaration) is not dict or set(declaration) != {'id', 'arity'}
                or type(declaration['id']) is not str or not IDENTIFIER.fullmatch(declaration['id'])
                or type(declaration['arity']) is not int or declaration['arity'] not in (1, 2)):
            _fail('Invalid', 'invalid predicate declaration')
        names.append(declaration['id'])
    if len(names) != len(set(names)):
        _fail('Invalid', 'duplicate predicate declaration')
    try:
        n = upstream.response_natural(value['domain_size'])
    except (RuntimeError, KeyError):
        _fail('Invalid', 'domain size must be a canonical natural string')
    if n == 0:
        _fail('Invalid', 'domain must be nonempty')
    if n > int(limits['domain']):
        _fail('UNKNOWN', 'domain exceeds the declared limit')
    formula_keys = ('assumptions', 'left', 'right') if schema == 'zal/relational-comparison/1' else ('formula',)
    try:
        for key in formula_keys:
            upstream.preflight_query({'formula': value[key]})
    except (ValueError, TypeError, RecursionError) as exc:
        detail = str(exc)
        _fail('UNKNOWN' if 'budget exceeded' in detail else 'Invalid', 'invalid or out-of-budget formula: ' + detail)
    if schema == 'zal/relational-comparison/1' and value['relation'] not in ('equivalence', 'implication'):
        _fail('Unsupported', 'unknown comparison relation')
    canonical = upstream.canonical(value)
    if len(canonical) > MAX_BYTES:
        _fail('Invalid', 'canonical request exceeds 1 MiB')
    return value, _sha(raw), _sha(canonical)


def _worker_request(request, limits):
    schema = request['schema']
    if schema == 'zal/relational-query/1':
        return {'mode': 'count', 'query': request, 'model': None, 'limits': limits}
    mode = 'evaluate-public' if schema == 'zal/relational-evaluation/1' else 'compare'
    return {'mode': mode, 'request': request, 'limits': limits}


def _witness_cost(node, n, cap=MAX_WITNESS_STEPS):
    op = node['op']
    if op in ('true', 'false', 'atom', 'equal'):
        return 1
    if op == 'not':
        return min(cap + 1, 1 + _witness_cost(node['body'], n, cap))
    if op in ('and', 'or', 'implies', 'iff'):
        return min(cap + 1, 1 + _witness_cost(node['left'], n, cap) + _witness_cost(node['right'], n, cap))
    if op in ('forall', 'exists', 'count', 'mod_count'):
        return min(cap + 1, 1 + n * _witness_cost(node['body'], n, cap))
    _fail('Error', 'unknown witness formula constructor')


def _validate_witness(request, answer, oracle):
    witness = answer['counterexample']
    n = int(request['domain_size'])
    cost = sum(_witness_cost(request[key], n) for key in ('assumptions', 'left', 'right'))
    if cost > MAX_WITNESS_STEPS:
        _fail('UNKNOWN', 'independent witness replay exceeds its work bound')
    model = {name: {tuple(int(v) for v in row) for row in rows}
             for name, rows in witness['model'].items()}
    expected = (oracle.evaluate(request['assumptions'], model, n),
                oracle.evaluate(request['left'], model, n),
                oracle.evaluate(request['right'], model, n))
    if expected != (True, witness['left'], witness['right']):
        _fail('Error', 'counterexample disagrees with independent original-formula evaluation')


def _validate_answer(request, answer, limits, upstream, oracle):
    if not isinstance(answer, dict) or type(answer.get('status')) is not str:
        _fail('Error', 'worker response is not a status object')
    status = answer['status']
    failures = {'Invalid', 'Unsupported', 'ResourceExhausted', 'InternalDisagreement'}
    if status in failures:
        if any(key in answer for key in ('value', 'holds', 'counterexample', 'partition')):
            _fail('Error', 'worker failure contains semantic success fields')
        if status == 'ResourceExhausted':
            if set(answer) != {'status', 'resource', 'used', 'limit'} or not all(type(answer[k]) is str for k in ('resource', 'used', 'limit')):
                _fail('Error', 'malformed resource exhaustion')
            return 'UNKNOWN'
        if set(answer) != {'status', 'message'} or type(answer['message']) is not str:
            _fail('Error', 'malformed worker failure')
        return {'Invalid': 'Invalid', 'Unsupported': 'Unsupported', 'InternalDisagreement': 'Error'}[status]
    schema = request['schema']
    if schema == 'zal/relational-query/1' and status == 'ExactCount':
        expected = {'status', 'value', 'domain_size', 'engine', 'work', 'peak_entries', 'peak_bits', 'assurance'}
        if (set(answer) != expected or answer['domain_size'] != request['domain_size']
                or answer['engine'] != 'population-definitions/1' or type(answer['value']) is not str):
            _fail('Error', 'count response schema or scope mismatch')
        try:
            if str(Fraction(answer['value'])) != answer['value']:
                _fail('Error', 'noncanonical exact count')
        except (ValueError, ZeroDivisionError):
            _fail('Error', 'invalid exact count')
        for key, limit in (('work', 'work'), ('peak_entries', 'entries'), ('peak_bits', 'bits')):
            if upstream.response_natural(answer[key]) > int(limits[limit]):
                _fail('Error', 'count exceeds declared limit: ' + key)
        if answer['assurance'] != 'experimental; source-level proof open':
            _fail('Error', 'count assurance mismatch')
        return status
    if schema == 'zal/relational-evaluation/1' and status == 'Evaluated':
        if (set(answer) != {'status', 'value', 'domain_size', 'work_bound', 'engine'}
                or type(answer['value']) is not bool or answer['domain_size'] != request['domain_size']
                or answer['engine'] != 'original-formula-evaluation/1'
                or upstream.response_natural(answer['work_bound']) > int(limits['work'])):
            _fail('Error', 'evaluation response schema or scope mismatch')
        return status
    if schema == 'zal/relational-comparison/1' and status in upstream.COMPARISONS:
        upstream.validate_comparison(request, answer, limits)
        if answer['scope'] != COMPARISON_SCOPE:
            _fail('Error', 'comparison scope mismatch')
        if status == 'RefutedAt':
            _validate_witness(request, answer, oracle)
        return status
    _fail('Error', 'unknown or mismatched worker response status')


def _outcome(status, detail=None, **fields):
    reply = {'schema': RESULT_SCHEMA, 'status': status, **fields}
    if detail is not None:
        reply['detail'] = str(detail)[:1000]
    return reply


def execute(request, runtime, *, limits=None, seconds=60, scratch=None):
    """Return one typed outcome; never turn exhaustion or malformed output into success."""
    metadata = {}
    try:
        manifest, sources = verify_provider()
        upstream, oracle = _load_vendor(manifest)
        selected = _limits(limits, seconds)
        payload, raw_sha, request_sha = _snapshot_request(request, selected, upstream)
        profile_sha, profile_binding = _profile_identity()
        metadata = {'request_sha256': request_sha, 'input_sha256': raw_sha,
                    'logic': payload['logic'], 'context_id': payload['context_id'],
                    'limits': selected, 'wall_seconds': seconds,
                    'provider_manifest_sha256': MANIFEST_SHA256,
                    'profile_sha256': profile_sha, 'profile_binding': profile_binding,
                    'launcher_sha256': _sha(_read(ROOT / 'worker_exec.py'))}
        binary, binding = _binding(runtime, sources, manifest, upstream)
        metadata['worker'] = binding
        wire = upstream.canonical(_worker_request(payload, selected)) + b'\n'
        if len(wire) > MAX_BYTES:
            _fail('Invalid', 'worker request exceeds 1 MiB')
        helper = ROOT / 'worker_exec.py'
        with tempfile.TemporaryFile(dir=scratch) as output, tempfile.TemporaryFile(dir=scratch) as error:
            try:
                process = subprocess.run([sys.executable, '-I', '-B', str(helper), str(binary), str(seconds)],
                                         input=wire, stdout=output, stderr=error, timeout=seconds,
                                         env={'PATH': '/usr/bin:/bin', 'LANG': 'C.UTF-8', 'LC_ALL': 'C.UTF-8'},
                                         check=False)
            except subprocess.TimeoutExpired:
                _binding(runtime, sources, manifest, upstream)
                if _profile_identity() != (profile_sha, profile_binding):
                    _fail('Error', 'profile source changed during execution')
                _fail('UNKNOWN', 'worker wall time exhausted')
            _, after = _binding(runtime, sources, manifest, upstream)
            if after != binding:
                _fail('Error', 'worker or build record changed during execution')
            if _profile_identity() != (profile_sha, profile_binding):
                _fail('Error', 'profile source changed during execution')
            output.seek(0)
            raw = output.read(MAX_BYTES + 1)
            error.seek(0)
            stderr = error.read(500).decode('utf-8', errors='replace')
        if len(raw) > MAX_BYTES:
            _fail('Error', 'worker response exceeds 1 MiB')
        if process.returncode != 0:
            if process.returncode in (-signal.SIGXCPU, -signal.SIGXFSZ, -signal.SIGKILL):
                _fail('UNKNOWN', 'worker terminated under a process resource bound')
            _fail('Error', 'worker failed: ' + str(process.returncode) + ' ' + stderr)
        lines = raw.splitlines()
        if len(lines) != 1:
            _fail('Error', 'worker did not return exactly one response')
        answer = upstream.strict_loads(lines[0])
        status = _validate_answer(payload, answer, selected, upstream, oracle)
        if status in ('Invalid', 'Unsupported', 'UNKNOWN', 'Error'):
            return _outcome(status, answer.get('message', answer.get('resource')),
                            provider_status=answer['status'], provider_failure=answer, **metadata)
        return _outcome(status, result=answer, **metadata)
    except ProfileFailure as exc:
        return _outcome(exc.status, exc.detail, **metadata)
    except (ValueError, TypeError, KeyError, IndexError, OverflowError, RecursionError, OSError,
            RuntimeError, json.JSONDecodeError) as exc:
        return _outcome('Error', str(exc), **metadata)
