"""Fixed optional entry-bound category host; JSON chooses data, never code/paths.

Native finite laws, exact source representation and authored interpretation are
separate results. A static audit is not a fresh Haskell execution.
"""
from copy import deepcopy
from dataclasses import dataclass
import hashlib
import importlib.abc
import importlib.util
import json
from pathlib import Path
import sys
import types

PROFILE = Path(__file__).resolve().parent
ZAL_ROOT = PROFILE.parent
DEPENDENCIES_SHA256 = '8a1aa3fad07e791bd02962e8a7cd8bd4b250db3ae9bf8ce722b71003a4adfd95'
PROVIDER_MANIFEST = '2628e184504a84753b9a775779e9e9660ea3f6947734e96de64aae41e7c91cec'
FORMAL_PROFILE = 'zal/finite-category-source/1'
OPERATIONS = ('check-category', 'compose-path', 'invertible', 'check-functor', 'compose-functors', 'compose-symbolic')


def _sha(data):
    return hashlib.sha256(data).hexdigest()


def _read(path, cap=2_000_000):
    if path.is_symlink() or not path.is_file():
        raise ValueError('ordinary captured file required')
    with path.open('rb') as stream:
        blob = stream.read(cap + 1)
    if len(blob) > cap:
        raise ValueError('captured file exceeds bound')
    return blob


def _capture():
    raw = _read(PROFILE/'DEPENDENCIES.json')
    if _sha(raw) != DEPENDENCIES_SHA256:
        raise ValueError('fixed dependency manifest changed')
    pins = json.loads(raw)['files']
    blobs = {name: _read(ZAL_ROOT/name) for name in pins}
    if any(_sha(blobs[name]) != pin for name, pin in pins.items()):
        raise ValueError('fixed dependency source changed')
    return pins, blobs


_PINS, _BLOBS = _capture()


class _Captured(importlib.abc.MetaPathFinder, importlib.abc.Loader):
    """Private namespace for fixed captured library modules, no request imports."""
    prefix = '_zenofcis_category_dependencies'

    def find_spec(self, fullname, path=None, target=None):
        if fullname.startswith(self.prefix + '.'):
            suffix = fullname[len(self.prefix)+1:].replace('.', '/')
        elif fullname in ('category_profile', 'category_profile.tools'):
            suffix = fullname.replace('.', '/')
        else:
            return None
        base = suffix
        package = base + '/__init__.py' in _BLOBS
        name = base + ('/__init__.py' if package else '.py')
        if name not in _BLOBS:
            raise ImportError('module absent from fixed captured library: ' + fullname)
        return importlib.util.spec_from_loader(fullname, self, origin=str(ZAL_ROOT/name), is_package=package)

    def create_module(self, spec):
        return None

    def exec_module(self, module):
        path = Path(module.__spec__.origin)
        name = str(path.relative_to(ZAL_ROOT))
        module.__file__ = str(path)
        module.__captured_sha256__ = _PINS[name]
        exec(compile(_BLOBS[name], str(path), 'exec'), module.__dict__)


if _Captured.prefix in sys.modules:
    raise RuntimeError('load the fixed category host once per process')
_package = types.ModuleType(_Captured.prefix)
_package.__path__ = []
sys.modules[_Captured.prefix] = _package
sys.meta_path.insert(0, _Captured())
from _zenofcis_category_dependencies.zal_notation import abstractions as A, builtin, core, lens_library as L, sharing  # noqa: E402

_backend_ns = {'__name__': '_captured_backend_loader', '__file__': str(PROFILE/'provider/main.py')}
exec(compile(_BLOBS['category_profile/provider/main.py'], str(PROFILE/'provider/main.py'), 'exec'), _backend_ns)
BACKEND = _backend_ns['profile']


def _hex(value):
    return type(value) is str and len(value) == 64 and all(c in '0123456789abcdef' for c in value)


@dataclass(frozen=True)
class RuntimeApproval:
    """Immutable operator approval, never constructed by a JSON tool call."""
    directory: Path
    binary_sha256: str
    build_record_sha256: str
    provider_manifest_sha256: str = PROVIDER_MANIFEST

    def __post_init__(self):
        object.__setattr__(self, 'directory', Path(self.directory).absolute())
        core.need(_hex(self.binary_sha256) and _hex(self.build_record_sha256) and
                  self.provider_manifest_sha256 == PROVIDER_MANIFEST, 'invalid operator runtime approval')

    @classmethod
    def from_operator_file(cls, path):
        data = core.loads(_read(Path(path), 65536).decode('utf-8'))
        core.keys(data, {'schema', 'directory', 'binary_sha256', 'build_record_sha256', 'provider_manifest_sha256'}, where='operator runtime approval')
        core.need(data['schema'] == 'zal/category-runtime-approval/1' and type(data['directory']) is str,
                  'unsupported operator approval')
        return cls(Path(data['directory']), data['binary_sha256'], data['build_record_sha256'], data['provider_manifest_sha256'])


@dataclass(frozen=True)
class EvidenceApproval:
    """Operator-pinned retained artifacts and their approved source/build/binary."""
    record_digests: frozenset
    binary_sha256: str
    build_record_sha256: str
    provider_manifest_sha256: str = PROVIDER_MANIFEST

    def __post_init__(self):
        values = frozenset(self.record_digests)
        core.need(len(values) <= 64 and all(_hex(v) for v in values) and _hex(self.binary_sha256)
                  and _hex(self.build_record_sha256) and self.provider_manifest_sha256 == PROVIDER_MANIFEST,
                  'invalid operator evidence approval')
        object.__setattr__(self, 'record_digests', values)

    @classmethod
    def from_pinned_file(cls, path, expected_sha256):
        raw = _read(Path(path), 65536)
        core.need(_hex(expected_sha256) and _sha(raw) == expected_sha256, 'operator record catalog changed')
        value = core.loads(raw.decode('utf-8'))
        core.keys(value, {'schema', 'records', 'binary_sha256', 'build_record_sha256', 'provider_manifest_sha256'}, where='operator record catalog')
        core.need(value['schema'] == 'zal/category-evidence-approval/1' and type(value['records']) is list,
                  'unsupported record catalog')
        return cls(frozenset(value['records']), value['binary_sha256'], value['build_record_sha256'], value['provider_manifest_sha256'])


def _same(left, right):
    return _canonical(left) == _canonical(right)


def _canonical(value):
    raw = BACKEND.canonical(value)
    core.need(len(raw) <= 2_000_000, 'adapter value byte budget exceeded')
    return raw


def _digest(value):
    return _sha(_canonical(value))


def _identity():
    captured = globals().get('__verified_source_sha256__')
    core.need(captured is not None and _sha(_read(Path(__file__))) == captured,
              'use the captured category_profile host entry point')
    return captured


def category_grammar():
    fields = {'Object': {'id': 'str'}, 'Arrow': {'id': 'str', 'source': 'str', 'target': 'str'},
              'Identity': {'object': 'str', 'arrow': 'str'}, 'Compose': {'before': 'str', 'after': 'str', 'result': 'str'}}
    return {'schema': 'zal.grammar/1', 'id': 'zal/finite-category-declaration', 'version': 1,
            'kind': 'term', 'root': 'Category', 'constructors': {
                'Category': {'result': 'Category', 'fields': {'id': 'str'}, 'variadic': 'Item'},
                **{tag: {'result': 'Item', 'fields': value, 'children': []} for tag, value in fields.items()}}}


def category_model(category):
    core.keys(category, {'id', 'objects', 'arrows', 'identities', 'composition'}, where='finite category')
    items = [builtin.term('Object', id=o) for o in category['objects']]
    items += [builtin.term('Arrow', **a) for a in category['arrows']]
    items += [builtin.term('Identity', object=o, arrow=a) for o, a in category['identities']]
    items += [builtin.term('Compose', before=f, after=g, result=h) for f, g, h in category['composition']]
    model = builtin.model(category_grammar(), builtin.term('Category', *items, id=category['id']))
    core.validate_model(category_grammar(), model)  # Existing 512-node bound; no truncated model.
    return model


def category_context(category):
    return sharing.make_context(category_grammar(), category_model(category),
        semantics={'profile': FORMAL_PROFILE, 'parameters': {},
                   'description': 'Complete explicit category declaration; laws require the finite checker.'},
        assumptions=['Every constructor is declared data; no category law is assumed.'],
        observation={'id': 'whole-category', 'description': 'Retain every object, arrow, identity and composition row.',
                     'keeps': ['objects', 'arrows', 'identities', 'composition'], 'omits': []})


def _source_category(context):
    sharing.validate_context(context)
    core.need(context['semantics']['profile'] == FORMAL_PROFILE, 'unsupported formal category source profile')
    core.need(context['semantics']['parameters'] == {} and _same(context['grammar'], category_grammar()), 'formal profile grammar/parameters changed')
    core.need(context['observation']['id'] == 'whole-category' and
              context['observation']['keeps'] == ['objects', 'arrows', 'identities', 'composition'] and
              context['observation']['omits'] == [], 'formal profile requires the complete category observation')
    body = context['model']['body']
    category = {'id': body['fields']['id'], 'objects': [], 'arrows': [], 'identities': [], 'composition': []}
    for item in body['children']:
        tag, f = item['tag'], item['fields']
        if tag == 'Object':
            category['objects'].append(f['id'])
        elif tag == 'Arrow':
            category['arrows'].append(deepcopy(f))
        elif tag == 'Identity':
            category['identities'].append([f['object'], f['arrow']])
        elif tag == 'Compose':
            category['composition'].append([f['before'], f['after'], f['result']])
        else:
            raise ValueError('unknown category constructor')
    core.need(_same(category_model(category), context['model']), 'whole model reconstruction differs')
    return category


def _interpret(entry, operation, parameters, declaration):
    core.keys(declaration, {'mode', 'description', 'assumptions', 'preserves', 'forgets'}, where='interpretation')
    A.text(declaration['description'], 'interpretation description')
    for field in ('assumptions', 'preserves', 'forgets'):
        core.need(type(declaration[field]) is list and len(declaration[field]) <= 64, 'interpretation list bound')
        for item in declaration[field]:
            A.text(item, 'interpretation declaration')
    mode = declaration['mode']
    core.need(mode in ('authored', 'formal-category-source'), 'unsupported interpretation mode')
    result = {'status': 'Authored', 'declaration': deepcopy(declaration),
              'claim': 'The source-to-structure interpretation is supplied by the author, not proved.'}
    if mode == 'formal-category-source':
        try:
            core.need(operation in ('check-category', 'compose-path', 'invertible'), 'formal identity bridge supports one selected source category')
            core.need(entry['discussion'] is not None, 'formal source context absent')
            source = _source_category(entry['discussion']['context'])
            chosen = parameters['selection']['category']
            matches = [c for c in parameters['categories'] if c['id'] == chosen]
            core.need(len(matches) == 1 and _same(source, matches[0]), 'complete source model differs from selected category payload')
            result.update(status='ExactRepresentation', source_payload_digest=core.digest(source),
                          claim='Every constructor reconstructs the exact selected declaration; finite laws still require checking.')
        except (ValueError, KeyError, TypeError) as error:
            result.update(status='Unsupported', reason=str(error))
    return result


def prepare(library, entry_digest, operation, parameters, interpretation, budget=100000):
    _capture()
    entry = L._entry(library, entry_digest)
    core.need(type(operation) is str and operation in OPERATIONS, 'unsupported category operation')
    core.need(type(budget) is int and 0 <= budget <= 1000000, 'invalid work budget')
    core.keys(parameters, {'categories', 'functors', 'relationships', 'selection'}, where='category parameters')
    semantic = _interpret(entry, operation, parameters, interpretation)
    context = entry['discussion']['context'] if entry['discussion'] else None
    # The provider's exact mathematical protocol rejects floats. Retain drawing
    # coordinates as inert canonical text, bounded by its existing string cap.
    entry_text = core.canonical(entry)
    source = {'library_digest': core.digest(library), 'entry_digest': entry_digest,
              'source_entry': {'encoding': 'zal/canonical-json-text-chunks/1', 'sha256': core.digest(entry),
                               'chunks': [entry_text[i:i+2048] for i in range(0, len(entry_text), 2048)]},
              'context_digest': core.digest(context), 'context': deepcopy(context), 'interpretation': semantic}
    request = {'schema': 'zal/lens-category/1', 'operation': operation, 'source_context': source,
               **deepcopy(parameters), 'budget': budget}
    # Use the existing strict bounded protocol; no runtime/path/code fields.
    BACKEND.prepare_request(BACKEND.strict(BACKEND.canonical(request)))
    return {'schema': 'zal/library-category-application/1', 'authority': 'none',
            'adapter_sha256': _identity(), 'dependency_manifest_sha256': DEPENDENCIES_SHA256,
            'library_digest': core.digest(library), 'entry_digest': entry_digest, 'source_entry': deepcopy(entry),
            'entry_status': A.inspect_entry(entry),
            'context_digest': core.digest(context), 'context': deepcopy(context), 'interpretation': semantic,
            'operation': operation, 'parameters': deepcopy(parameters), 'interpretation_declaration': deepcopy(interpretation),
            'budget': budget, 'request': request, 'request_digest': core.digest(request)}


def _rebuild(library, entry_digest, application):
    expected = prepare(library, entry_digest, application['operation'], application['parameters'],
                       application['interpretation_declaration'], application['budget'])
    core.need(_same(application, expected), 'source-entry/context/request/interpretation/application changed')
    return expected


def _record(application, native, execution):
    return {'schema': 'zal/library-category-record/1', 'authority': 'none',
            'application_digest': core.digest(application), 'library_digest': application['library_digest'],
            'entry_digest': application['entry_digest'], 'context_digest': application['context_digest'],
            'request_digest': application['request_digest'], 'interpretation': deepcopy(application['interpretation']),
            'status': native['status'], 'refutation_scope': 'entire supplied request, including unselected declarations',
            'native_execution': execution, 'native_receipt': deepcopy(native),
            'native_receipt_digest': _digest(native),
            'claim_scope': 'finite supplied declarations only; authored interpretation is a separate claim'}


def _typed_native(request, native):
    """Closed completed-result/witness schema before Python oracle equality.

    Exact authored endpoint values may legitimately contain booleans; only
    schema-defined integer/text/table positions receive those specific types.
    """
    common = {'schema', 'authority', 'work_scope', 'input_utf8', 'input_sha256', 'request_sha256',
              'context_sha256', 'request', 'worker', 'source_manifest_sha256', 'status', 'work_used',
              'evidence_scope', 'independent_check'}
    status = native['status']
    core.need(type(status) is str and status in ('CheckedAt', 'RefutedAt', 'Authored'), 'no completed native result')
    core.keys(native, common | ({'law', 'witness'} if status == 'RefutedAt' else {'result'}), where='native completed receipt')
    for key in common - {'request', 'worker', 'work_used'}:
        core.need(type(native[key]) is str, 'native receipt text type changed: ' + key)
    core.need(native['schema'] == 'zal/lens-category-receipt/1' and native['authority'] == 'none' and
              native['independent_check'] == 'explicit-table oracle/1', 'native receipt schema/scope changed')
    core.need(type(native['work_used']) is int and 0 <= native['work_used'] <= request['budget'], 'native work must be exact bounded integer')
    core.keys(native['worker'], {'binary_sha256', 'build_record_sha256', 'source_sha256'}, where='native worker identity')

    def strings(values):
        return type(values) is list and all(type(x) is str for x in values)

    def pairs(values):
        return type(values) is list and all(type(p) is list and len(p) == 2 and all(type(x) is str for x in p) for p in values)

    def fibers(values):
        return type(values) is list and all(type(p) is list and len(p) == 2 and type(p[0]) is str and strings(p[1]) for p in values)

    if status == 'RefutedAt':
        core.need(type(native['law']) is str and pairs(native['witness']), 'native witness must contain text pairs')
        core.need(len({p[0] for p in native['witness']}) == len(native['witness']), 'duplicate witness key')
        return
    operation = request['operation']
    core.need((status == 'Authored') == (operation == 'compose-symbolic'), 'native status/operation mismatch')
    result = native['result']
    fields = {
        'check-category': {'claim', 'category', 'objects', 'arrows'},
        'compose-path': {'claim', 'path', 'composite'},
        'invertible': {'claim', 'arrow', 'inverse'},
        'check-functor': {'claim', 'functor', 'hop_declarations', 'cumulative_forgets'},
        'compose-functors': {'claim', 'functor', 'hop_declarations', 'cumulative_forgets'},
        'compose-symbolic': {'claim', 'path', 'endpoints', 'hop_declarations', 'cumulative_forgets'}}
    core.keys(result, fields[operation], where='native result')
    core.need(type(result['claim']) is str, 'native claim must be text')
    if operation == 'check-category':
        core.need(type(result['category']) is str and type(result['objects']) is int and
                  type(result['arrows']) is int and 1 <= result['objects'] <= 8 and 1 <= result['arrows'] <= 48,
                  'category counts must be exact bounded integers')
    elif operation == 'compose-path':
        core.need(strings(result['path']) and type(result['composite']) is str, 'path result type mismatch')
    elif operation == 'invertible':
        core.need(type(result['arrow']) is str and type(result['inverse']) is str, 'inverse result type mismatch')
    else:
        core.need(strings(result['cumulative_forgets']) and type(result['hop_declarations']) is list,
                  'hop/loss result type mismatch')
        if operation == 'compose-symbolic':
            core.need(strings(result['path']) and strings(result['endpoints']) and len(result['endpoints']) == 2,
                      'symbolic endpoint/path type mismatch')
            wire = BACKEND.prepare_request(request)
            by_id = {r['id']: r for r in wire['relationships']}
            expected = [by_id[i] for i in request['selection']['path']]
        else:
            f = result['functor']
            core.keys(f, {'id', 'source', 'target', 'objects', 'arrows', 'object_fibers', 'arrow_fibers'}, where='native functor result')
            core.need(all(type(f[k]) is str for k in ('id', 'source', 'target')) and pairs(f['objects']) and pairs(f['arrows'])
                      and fibers(f['object_fibers']) and fibers(f['arrow_fibers']), 'native functor mapping/fiber type mismatch')
            by_id = {f['id']: f for f in request['functors']}
            path = [request['selection']['functor']] if operation == 'check-functor' else request['selection']['path']
            expected = [by_id[i] for i in path]
        core.need(_same(result['hop_declarations'], expected), 'native authored hop values or types changed')


def check(library, entry_digest, application, *, runtime=None):
    try:
        app = _rebuild(library, entry_digest, application)
        if app['interpretation']['status'] == 'Unsupported':
            return {'schema': 'zal/library-category-record/1', 'status': 'Unsupported', 'authority': 'none',
                    'application_digest': core.digest(app), 'interpretation': app['interpretation']}
        if runtime is None:
            return {'schema': 'zal/library-category-record/1', 'status': 'Unavailable', 'authority': 'none',
                    'application_digest': core.digest(app), 'interpretation': app['interpretation'],
                    'reason': 'No trusted operator runtime is configured; JSON cannot select one.'}
        core.need(type(runtime) is RuntimeApproval, 'runtime must be trusted operator configuration')
        approved_binary, approved_build, approved_manifest = runtime.binary_sha256, runtime.build_record_sha256, runtime.provider_manifest_sha256
        directory = Path(runtime.directory).absolute()  # One invocation directory, independent of later CWD/config changes.
        pins, _, _ = BACKEND.captured()
        _, binding = BACKEND.binding(directory, pins)
        core.need(binding['binary_sha256'] == approved_binary and binding['build_record_sha256'] == approved_build,
                  'runtime differs from the reviewed fixed source/binary build')
        native = BACKEND.execute(BACKEND.canonical(app['request']), directory)
        core.need(_same(native.get('worker'), binding) and native.get('source_manifest_sha256') == approved_manifest,
                  'returned native source/build/binary identity differs from approved invocation')
        if native['status'] in ('CheckedAt', 'RefutedAt', 'Authored'):
            _typed_native(app['request'], native)
        result = _record(app, native, 'fresh-native')
        _capture()
        return result
    except (ValueError, KeyError, TypeError, OSError) as error:
        return {'schema': 'zal/library-category-record/1', 'status': 'Invalid', 'authority': 'none', 'reason': str(error)}
    except (OverflowError, RecursionError) as error:
        return {'schema': 'zal/library-category-record/1', 'status': 'UNKNOWN', 'authority': 'none', 'reason': str(error)}


def verify_receipt(library, entry_digest, application, record, *, runtime=None):
    return _same(check(library, entry_digest, application, runtime=runtime), record)


def _retained_record(library, entry_digest, application, record, evidence):
    """Common source/provenance binding; by itself establishes no finite law."""
    core.need(type(evidence) is EvidenceApproval and _digest(record) in evidence.record_digests,
                  'record is not in the trusted operator-pinned retained evidence set')
    app = _rebuild(library, entry_digest, application)
    core.need(app['interpretation']['status'] != 'Unsupported', 'interpretation bridge is unsupported')
    native = record['native_receipt']
    core.need(_same(record, _record(app, native, 'fresh-native')), 'entry/interpretation/result envelope changed')
    core.need(native['source_manifest_sha256'] == PROVIDER_MANIFEST and
              native['worker']['binary_sha256'] == evidence.binary_sha256 and
              native['worker']['build_record_sha256'] == evidence.build_record_sha256, 'prior native qualification pins differ')
    core.need(type(native['input_utf8']) is str, 'retained native input must be text')
    raw = native['input_utf8'].encode('utf-8')
    core.need(native['input_sha256'] == _sha(raw) and
              native['request_sha256'] == _sha(BACKEND.canonical(app['request'])) and
              native['context_sha256'] == _sha(BACKEND.canonical(app['request']['source_context'])) and
              _same(native['request'], app['request']) and
              _same(BACKEND.strict(raw), app['request']), 'native input/context binding changed')
    pins, _, oracle = BACKEND.captured()
    core.need(native['worker']['source_sha256'] == pins, 'prior native source inventory differs')
    return app, native, oracle


def inspect_incomplete_record(library, entry_digest, application, record, *, evidence=None):
    """Pinned incomplete bytes/envelope only: no table audit or positive law."""
    try:
        app, native, _ = _retained_record(library, entry_digest, application, record, evidence)
        fields = {'schema', 'authority', 'work_scope', 'input_utf8', 'input_sha256', 'request_sha256',
                  'context_sha256', 'request', 'worker', 'source_manifest_sha256', 'status', 'work_used',
                  'message', 'evidence_scope', 'independent_check'}
        core.keys(native, fields, where='incomplete native receipt')
        core.need(all(type(native[k]) is str for k in fields - {'request', 'worker', 'work_used'}),
                  'incomplete receipt text type changed')
        core.need(native['status'] == 'UNKNOWN' and native['schema'] == 'zal/lens-category-receipt/1'
                  and native['authority'] == 'none' and native['independent_check'] == 'explicit-table oracle/1'
                  and native['message'] == 'work exhausted; next charged visit not performed'
                  and native['evidence_scope'] == 'exact supplied finite declarations; symbolic/custom source interpretation and hop claims remain authored'
                  and native['work_scope'] == 'charged semantic visits; excludes bounded decoding, allocation, process setup and independent oracle',
                  'unsupported incomplete receipt')
        core.need(type(native['work_used']) is int and native['work_used'] == app['budget'],
                  'incomplete work must be exact bounded integer')
        core.keys(native['worker'], {'binary_sha256', 'build_record_sha256', 'source_sha256'}, where='incomplete worker')
        return {'schema': 'zal/library-category-incomplete-provenance/1', 'status': 'RecordedIncomplete',
                'authority': 'none', 'application_digest': core.digest(app), 'record_digest': _digest(record),
                'recorded_status': 'UNKNOWN', 'interpretation': deepcopy(app['interpretation']),
                'native_execution': 'operator-pinned prior bytes; no worker launched by inspection',
                'static_table_audit': 'not performed: incomplete prior result',
                'claim_scope': 'source/input/worker/envelope binding only; no finite law, refutation or completion claim',
                'refutation_scope': record['refutation_scope']}
    except (ValueError, KeyError, TypeError, OSError) as error:
        return {'status': 'Invalid', 'authority': 'none', 'reason': str(error)}
    except (OverflowError, RecursionError) as error:
        return {'status': 'UNKNOWN', 'authority': 'none', 'reason': str(error)}


def audit_record(library, entry_digest, application, record, *, evidence=None):
    """Fresh bounded static table audit; never launches or selects a worker."""
    try:
        app, native, oracle = _retained_record(library, entry_digest, application, record, evidence)
        status = native['status']
        core.need(status in ('CheckedAt', 'RefutedAt', 'Authored'), 'no completed semantic result to audit')
        _typed_native(app['request'], native)
        core.need(oracle.audit(BACKEND.prepare_request(app['request']), native), 'fresh explicit-table audit disagrees')
        return {'schema': 'zal/library-category-audit/1', 'status': 'Audited', 'authority': 'none',
                'application_digest': core.digest(app), 'record_digest': _digest(record),
                'native_execution': 'operator-pinned prior native record; no worker launched by audit',
                'recorded_status': status, 'static_table_audit': 'fresh explicit-table oracle',
                'interpretation': deepcopy(app['interpretation']), 'refutation_scope': record['refutation_scope']}
    except Exception as error:
        return {'schema': 'zal/library-category-audit/1', 'status': 'UNKNOWN' if type(error).__name__ in ('OracleBound', 'OverflowError', 'RecursionError') else 'Invalid',
                'authority': 'none', 'reason': str(error)}


def toolkit():
    from _zenofcis_category_dependencies.category_profile.tools import toolkit as actual_tools
    return actual_tools()


def dispatch(name, arguments, *, runtime=None, evidence=None):
    routes = {row['name']: row['input_schema'] for row in toolkit()['tools']}
    try:
        core.need(name in routes, 'unknown fixed category tool')
        schema = routes[name]
        core.keys(arguments, set(schema['required']), set(schema['properties']) - set(schema['required']), where='tool arguments')
        if name == 'zal_library_category_prepare':
            return prepare(**arguments)
        if name == 'zal_library_category_check':
            return check(**arguments, runtime=runtime)
        if name == 'zal_library_category_verify':
            return {'schema': 'zal/library-category-verification/1', 'authority': 'none',
                    'matches_recomputation': verify_receipt(**arguments, runtime=runtime),
                    'recorded_status': arguments['record'].get('status') if type(arguments['record']) is dict else None,
                    'scope': 'Record equality after recomputation; a matching negative/incomplete record is not a positive claim.'}
        return audit_record(**arguments, evidence=evidence)
    except (ValueError, KeyError, TypeError) as error:
        return {'status': 'Invalid', 'authority': 'none', 'reason': str(error)}
    except (OverflowError, RecursionError) as error:
        return {'status': 'UNKNOWN', 'authority': 'none', 'reason': str(error)}
