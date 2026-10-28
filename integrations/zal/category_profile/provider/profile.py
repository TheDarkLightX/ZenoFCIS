"""Private optional Lens profile; bounded strict ingress, captured execution/replay.

Invocation trusts this operator-owned capsule/build provenance. Hashes identify
bytes; they neither authenticate author intent nor attest the compiler/host.
"""
import hashlib
import json
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import types

ROOT = Path(__file__).resolve().parent
MAX_INPUT = 128 * 1024
MAX_OUTPUT = 1024 * 1024


def canonical(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=True, separators=(',', ':'), allow_nan=False).encode()


def sha(data):
    return hashlib.sha256(data).hexdigest()


def read(path, cap=MAX_OUTPUT):
    if path.is_symlink() or not stat.S_ISREG(path.stat().st_mode):
        raise ValueError('expected regular owned input: ' + str(path))
    with path.open('rb') as handle:
        data = handle.read(cap + 1)
    if len(data) > cap:
        raise ValueError('bounded file exceeds cap')
    return data


def strict(raw):
    if len(raw) > MAX_INPUT:
        raise OverflowError('input byte budget exhausted')
    def pairs(items):
        out = {}
        for key, value in items:
            if key in out:
                raise ValueError('duplicate JSON key')
            out[key] = value
        return out
    def number(text):
        if len(text) > 128:
            raise ValueError('integer token too long')
        return int(text)
    def no_decimal(_):
        raise ValueError('write exact nonintegral mathematical values as data strings')
    value = json.loads(raw.decode('utf-8'), object_pairs_hook=pairs, parse_int=number,
                       parse_float=no_decimal, parse_constant=no_decimal)
    pending = [(value, 0)]; nodes = 0
    while pending:
        item, depth = pending.pop(); nodes += 1
        if depth > 48 or nodes > 30000:
            raise OverflowError('JSON structural budget exhausted')
        if type(item) is dict:
            pending.extend((v, depth + 1) for v in item.values())
        elif type(item) is list:
            pending.extend((v, depth + 1) for v in item)
        elif type(item) is str and len(item) > 4096:
            raise OverflowError('text field budget exhausted')
    return value


def keys(value, expected):
    if type(value) is not dict or set(value) != set(expected.split()):
        raise ValueError('unknown or missing fields')


def prepare_request(request):
    keys(request, 'schema operation source_context categories functors relationships selection budget')
    if request['schema'] != 'zal/lens-category/1':
        raise ValueError('unsupported optional profile')
    if type(request['budget']) is not int or not 0 <= request['budget'] <= 1000000:
        raise ValueError('budget must be an integer in 0..1000000')
    if request['operation'] not in {'check-category', 'compose-path', 'invertible', 'check-functor', 'compose-functors', 'compose-symbolic'}:
        raise ValueError('unsupported operation')
    for field, limit in [('categories', 4), ('functors', 16), ('relationships', 32)]:
        if type(request[field]) is not list or len(request[field]) > limit:
            raise ValueError('declaration bound exceeded')
    wire = json.loads(canonical(request))
    for relation in wire['relationships']:
        keys(relation, 'id source target assumptions preserves forgets')
        if type(relation['id']) is not str or not 1 <= len(relation['id']) <= 128:
            raise ValueError('relationship ID must be bounded nonempty text')
        for field in ['source', 'target']:
            keys(relation[field], 'id object observation assumptions')
            endpoint = relation[field]
            if type(endpoint['id']) is not str or not 1 <= len(endpoint['id']) <= 128:
                raise ValueError('endpoint ID must be bounded nonempty text')
            if type(endpoint['assumptions']) is not list or len(endpoint['assumptions']) > 128 or not all(type(x) is str for x in endpoint['assumptions']):
                raise ValueError('endpoint assumptions must be bounded text declarations')
            relation[field + '_key'] = 'sha256:' + sha(canonical(relation[field]))
    if request['operation'] != 'compose-symbolic' and not request['categories']:
        raise ValueError('finite operations require explicit category declarations')
    return wire


def captured():
    manifest_bytes = read(ROOT / 'SOURCE_MANIFEST.json')
    expected = globals().get('__manifest_sha256__')
    if expected is None or sha(manifest_bytes) != expected:
        raise ValueError('use the source-captured standalone main.py ingress')
    pins = json.loads(manifest_bytes)['files']
    data = {name: read(ROOT / name) for name in pins}
    if any(sha(data[name]) != digest for name, digest in pins.items()):
        raise ValueError('trusted profile source changed')
    oracle = types.ModuleType('_captured_lens_oracle')
    exec(compile(data['oracle.py'], str(ROOT / 'oracle.py'), 'exec'), oracle.__dict__)
    return pins, data, oracle


def binding(runtime, pins):
    runtime = Path(runtime)
    if runtime.is_symlink() or not runtime.is_dir():
        raise ValueError('missing owned runtime')
    for name, digest in pins.items():
        if sha(read(runtime / name)) != digest:
            raise ValueError('runtime source changed: ' + name)
    inventory = {str(p.relative_to(runtime)) for folder in ['src', 'app'] for p in (runtime / folder).rglob('*.hs')}
    if inventory != {name for name in pins if name.endswith('.hs')}:
        raise ValueError('runtime Haskell inventory changed')
    record_raw = read(runtime / 'build.json')
    record = json.loads(record_raw)
    binary = runtime / '.build/lens-worker'
    binary_bytes = read(binary, 64 * 1024**2)
    if (record['exit'] != 0 or type(record['exit']) is not int or record['source_sha256'] != pins
            or record['binary_sha256'] != sha(binary_bytes) or record['ghc'] != '9.4.7'):
        raise ValueError('build/source/binary binding mismatch')
    return binary, {'binary_sha256': sha(binary_bytes), 'build_record_sha256': sha(record_raw), 'source_sha256': pins}


def execute(raw, runtime):
    metadata = {'schema': 'zal/lens-category-receipt/1', 'authority': 'none',
                'work_scope': 'charged semantic visits; excludes bounded decoding, allocation, process setup and independent oracle'}
    try:
        request = strict(raw); wire = prepare_request(request)
        pins, sources, oracle = captured()
        binary, bound = binding(runtime, pins)
        metadata.update(input_utf8=raw.decode('utf-8'), input_sha256=sha(raw), request_sha256=sha(canonical(request)),
                        context_sha256=sha(canonical(request['source_context'])), request=request, worker=bound,
                        source_manifest_sha256=globals()['__manifest_sha256__'])
        with tempfile.TemporaryDirectory(prefix='lens-captured-') as temporary:
            launcher = Path(temporary) / 'worker_exec.py'
            launcher.write_bytes(sources['worker_exec.py'])
            with tempfile.TemporaryFile() as output, tempfile.TemporaryFile() as error:
                try:
                    proc = subprocess.run([sys.executable, '-I', '-B', str(launcher), str(binary)],
                                          input=canonical(wire), stdout=output, stderr=error, timeout=12,
                                          env={'PATH': '/usr/bin:/bin', 'LANG': 'C.UTF-8', 'LC_ALL': 'C.UTF-8'})
                except subprocess.TimeoutExpired:
                    return {**metadata, 'status': 'UNKNOWN', 'message': 'worker wall time exhausted'}
                output.seek(0); reply_bytes = output.read(MAX_OUTPUT + 1)
                error.seek(0); stderr = error.read(500)
        if binding(runtime, pins)[1] != bound or captured()[0] != pins:
            raise ValueError('runtime/profile changed during execution')
        if proc.returncode != 0:
            return {**metadata, 'status': 'UNKNOWN' if proc.returncode < 0 else 'Error',
                    'message': 'worker exit ' + str(proc.returncode), 'stderr': stderr.decode(errors='replace')}
        if len(reply_bytes) > MAX_OUTPUT or len(reply_bytes.splitlines()) != 1:
            raise ValueError('response bound/framing failure')
        answer = json.loads(reply_bytes)
        status = answer['status']
        if (status not in {'CheckedAt', 'Authored', 'RefutedAt', 'Invalid', 'UNKNOWN'}
                or type(answer['work_used']) is not int or not 0 <= answer['work_used'] <= request['budget']):
            raise ValueError('malformed status/work accounting')
        if not oracle.audit(wire, answer):
            raise ValueError('independent oracle disagrees with native claim/witness')
        return {**metadata, **answer,
                'evidence_scope': 'exact supplied finite declarations; symbolic/custom source interpretation and hop claims remain authored',
                'independent_check': 'explicit-table oracle/1'}
    except (OverflowError, RecursionError) as error:
        return {**metadata, 'status': 'UNKNOWN', 'message': str(error)}
    except Exception as error:
        if type(error).__name__ == 'OracleBound':
            return {**metadata, 'status': 'UNKNOWN', 'message': str(error)}
        return {**metadata, 'status': 'Invalid' if not metadata.get('worker') else 'Error', 'message': str(error)}


def verify(receipt, runtime):
    """Receipt status/hash alone is never authority: execute exact captured input."""
    if type(receipt) is not dict or type(receipt.get('input_utf8')) is not str:
        return False
    fresh = execute(receipt['input_utf8'].encode('utf-8'), runtime)
    return canonical(fresh) == canonical(receipt)


def main():
    if len(sys.argv) != 4 or sys.argv[1] not in ('run', 'verify'):
        raise SystemExit('usage: main.py run|verify RUNTIME INPUT.json')
    raw = read(Path(sys.argv[3]), MAX_OUTPUT)
    if sys.argv[1] == 'verify':
        result = {'schema': 'zal/lens-category-verification/1', 'matches_recomputation': verify(json.loads(raw), sys.argv[2]), 'authority': 'none'}
    else:
        result = execute(raw, sys.argv[2])
    print(json.dumps(result, sort_keys=True, ensure_ascii=True, allow_nan=False))


if __name__ == '__main__':
    raise SystemExit('use captured main.py')
