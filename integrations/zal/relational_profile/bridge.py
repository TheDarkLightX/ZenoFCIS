"""Lower the supported current-ladder AST using PR153's optional C2 mapping."""
from __future__ import annotations

import hashlib
import json
import re
import sys
import tempfile
import types
import uuid
from pathlib import Path

from . import profile

LADDER = Path(__file__).resolve().parent.parent / 'ladder.py'
REGISTRY = LADDER.with_name('ladder_en.json')


class UnsupportedLowering(ValueError):
    """The ZAL expression is outside this optional finite reasoning profile."""


def _canonical(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=True, separators=(',', ':')).encode()


class _DecimalToken(str):
    """A JSON decimal token kept distinct from a string and from an integer."""


def _scope_identity(raw_context):
    # Called only after the upstream strict parser has enforced its existing
    # byte/depth/node budgets, duplicate-key rejection and numeric limits.
    # Preserve decimal lexemes: binary-float parsing can collapse distinct
    # supplied meanings before an ordinary JSON re-encoding sees them.
    supplied = json.loads(raw_context, parse_float=_DecimalToken)
    if type(supplied) is not dict:
        raise ValueError('context must be an object')
    scope = {key: value for key, value in supplied.items() if key not in ('id', 'formula', 'fixtures')}
    digest = hashlib.sha256()
    pending = [scope]
    while pending:
        value = pending.pop()
        kind = type(value)
        if kind is dict:
            keys = sorted(value)
            digest.update(b'D' + str(len(keys)).encode('ascii') + b':')
            for key in reversed(keys):
                pending.extend((value[key], key))
        elif kind is list:
            digest.update(b'A' + str(len(value)).encode('ascii') + b':')
            pending.extend(reversed(value))
        else:
            if kind is _DecimalToken:
                tag, payload = b'M', value.encode('ascii')
            elif kind is str:
                tag, payload = b'S', _canonical(value)
            elif kind is int:
                tag, payload = b'I', str(value).encode('ascii')
            elif kind is bool:
                tag, payload = b'B', b'1' if value else b'0'
            elif value is None:
                tag, payload = b'Z', b''
            else:
                raise ValueError('unsupported context value')
            digest.update(tag + str(len(payload)).encode('ascii') + b':' + payload)
    return digest.hexdigest()


def _context_descriptor(ladder, source, registry, bridge_sha256, raw_context):
    if ladder.plain_data(ladder.REGISTRY) != json.loads(registry):
        raise ValueError('loaded ladder registry differs from its bound source')
    return {'schema': 'zal/relational-ladder-context/2',
            'scope_sha256': _scope_identity(raw_context),
            'ladder_sha256': profile._sha(source), 'registry_sha256': profile._sha(registry),
            'bridge_sha256': bridge_sha256,
            'provider_manifest_sha256': profile.MANIFEST_SHA256}


def _context_id(descriptor):
    return 'ladder:' + profile._sha(_canonical(descriptor))


def _lower(node, ladder, domain, env=None):
    # Adapted from the retained, exact PR153 vendor/ladder_adapter.py. Lexical
    # liveness, not variable spelling, determines which of the two slots is free.
    env = {} if env is None else env
    tag = node[0]

    def var(value):
        if value[0] != 'var' or value[1] not in env:
            raise UnsupportedLowering('nonvariable argument or unbound name')
        return env[value[1]]

    if tag == 'bool':
        return {'op': 'true' if node[1] else 'false'}
    if tag == 'call':
        if node[2]:
            raise UnsupportedLowering('primed predicate needs another profile')
        return {'op': 'atom', 'predicate': node[1], 'args': [var(arg) for arg in node[3]]}
    if tag == 'not':
        return {'op': 'not', 'body': _lower(node[1], ladder, domain, env)}
    if tag in ('and', 'or', '=>', '<=>'):
        return {'op': {'and': 'and', 'or': 'or', '=>': 'implies', '<=>': 'iff'}[tag],
                'left': _lower(node[1], ladder, domain, env),
                'right': _lower(node[2], ladder, domain, env)}
    if tag in ('=', '!=') and node[1][0] == node[2][0] == 'var':
        equality = {'op': 'equal', 'args': [var(node[1]), var(node[2])]}
        return equality if tag == '=' else {'op': 'not', 'body': equality}
    if tag in ('forall', 'exists', 'exists1'):
        _, name, quantified_domain, body = node
        if quantified_domain != domain:
            raise UnsupportedLowering('all binders must use the one declared domain')
        needed = ladder.free(body) - {name}
        if not needed <= env.keys():
            raise UnsupportedLowering('unbound variable in ladder body')
        active = {key: env[key] for key in needed}
        available = [slot for slot in ('x', 'y') if slot not in active.values()]
        if not available:
            raise UnsupportedLowering('third live variable needs another logic profile')
        slot = available[0]
        active[name] = slot
        result = {'op': tag, 'var': slot, 'body': _lower(body, ladder, domain, active)}
        if tag == 'exists1':
            result.update(op='count', cmp='eq', threshold='1')
        return result
    if tag in ('=', '<=', '>=') and node[1][0] == 'count' and node[2][0] == 'int':
        _, name, quantified_domain, body = node[1]
        value = node[2][1]
        if type(value) is not int or value < 0:
            raise UnsupportedLowering('count threshold must be nonnegative')
        binder = _lower(('exists', name, quantified_domain, body), ladder, domain, env)
        binder.update(op='count', cmp={'=': 'eq', '<=': 'le', '>=': 'ge'}[tag], threshold=str(value))
        return binder
    raise UnsupportedLowering('unsupported ladder node: ' + str(tag))


def lower_tree(ladder, context, tree, domain_size, *, _bound_context_id=None):
    """Translate a trusted local tree; use lower_text for a bound source envelope."""
    if type(domain_size) is not int or not 1 <= domain_size <= int(profile.DEFAULT_LIMITS['domain']):
        raise ValueError('domain size must be an integer in the supported 1..10000 range')
    if ladder.validate(ladder.bounded(tree), context) != 'bool' or ladder.free(tree):
        raise ValueError('a closed Boolean ladder tree is required')
    domains = context.data['domains']
    if len(domains) != 1:
        raise UnsupportedLowering('one nonempty finite domain is required')
    domain = next(iter(domains))
    signature = []
    for name, symbol in context.data['symbols'].items():
        if (not re.fullmatch(r'[A-Za-z][A-Za-z0-9_]{0,127}', name)
                or symbol.get('kind') != 'predicate' or symbol.get('arity') not in (1, 2)
                or list(symbol.get('args', ())) != [domain] * symbol['arity']):
            raise UnsupportedLowering('signature outside unary/binary relational profile: ' + name)
        signature.append({'id': name, 'arity': symbol['arity']})
    if len(signature) > 128:
        raise UnsupportedLowering('relational signature exceeds 128 predicates')
    signature.sort(key=lambda predicate: predicate['id'])
    context_id = _bound_context_id or 'unbound-ladder:' + ladder.scope_identity(context)
    # Keep exists1 primitive: expanding it to first-order syntax can need an
    # unnecessary third variable; ordinary counting represents it directly.
    expanded = ladder.expand(tree, context, derived=False)
    return {'schema': 'zal/relational-query/1', 'logic': 'zal/c2/1', 'operation': 'count',
            'signature': signature, 'formula': _lower(expanded, ladder, domain),
            'domain_size': str(domain_size), 'weights': {p['id']: ['1', '1'] for p in signature},
            'context_id': context_id}


def comparison_request(left, right, assumptions, relation='equivalence'):
    if relation not in ('equivalence', 'implication'):
        raise UnsupportedLowering('comparison relation is unsupported')
    def bound(envelope):
        if (type(envelope) is not dict or envelope.get('schema') != 'zal/relational-lowering/3'
                or envelope.get('status') != 'Lowered'):
            raise ValueError('comparison requires source-bound lowering envelopes')
        bindings, query = envelope.get('bindings'), envelope.get('query')
        if type(bindings) is not dict or type(query) is not dict:
            raise ValueError('comparison requires source-bound lowering envelopes')
        descriptor = bindings.get('context_identity')
        fields = {'schema', 'scope_sha256', 'ladder_sha256', 'registry_sha256',
                  'bridge_sha256', 'provider_manifest_sha256'}
        try:
            tree_sha = profile._sha(_canonical(envelope.get('tree')))
            query_sha = profile._sha(_canonical(query))
        except (TypeError, ValueError, RecursionError) as exc:
            raise ValueError('invalid lowering envelope data') from exc
        if (type(descriptor) is not dict or set(descriptor) != fields
                or descriptor.get('schema') != 'zal/relational-ladder-context/2'
                or any(type(descriptor[key]) is not str or not re.fullmatch(r'[a-f0-9]{64}', descriptor[key])
                       for key in fields - {'schema'})
                or any(bindings.get(key) != descriptor[key] for key in
                       ('ladder_sha256', 'registry_sha256', 'bridge_sha256', 'provider_manifest_sha256'))
                or type(bindings.get('context_sha256')) is not str
                or not re.fullmatch(r'[a-f0-9]{64}', bindings['context_sha256'])
                or descriptor.get('provider_manifest_sha256') != profile.MANIFEST_SHA256
                or bindings.get('tree_sha256') != tree_sha
                or set(query) != profile.REQUEST_KEYS['zal/relational-query/1']
                or query.get('schema') != 'zal/relational-query/1'
                or type(query.get('weights')) is not dict
                or query.get('context_id') != _context_id(descriptor)
                or bindings.get('query_sha256') != query_sha):
            raise ValueError('lowering envelope identity or query binding mismatch')
        return query, descriptor

    left, identity = bound(left)
    right, right_identity = bound(right)
    assumptions, assumptions_identity = bound(assumptions)
    if identity != right_identity or identity != assumptions_identity:
        raise ValueError('comparison requires the same bound context identity')
    for other in (right, assumptions):
        for key in ('logic', 'signature', 'domain_size', 'context_id'):
            if left[key] != other[key]:
                raise ValueError('comparison requires a shared ' + key)
    if any(weight != ['1', '1'] for q in (left, right, assumptions) for weight in q['weights'].values()):
        raise ValueError('logical comparison is unweighted')
    return {'schema': 'zal/relational-comparison/1', 'logic': left['logic'],
            'signature': left['signature'], 'domain_size': left['domain_size'],
            'context_id': left['context_id'], 'relation': relation,
            'left': left['formula'], 'right': right['formula'], 'assumptions': assumptions['formula']}


def lower_text(context_path, text, rung, domain_size):
    """Return source-bound lowering or an explicit refusal; never infer approval."""
    schema = 'zal/relational-lowering/3'
    module_name = '_zenofcis_relational_ladder_' + uuid.uuid4().hex
    try:
        bridge_sha256 = globals().get('__verified_source_sha256__')
        if bridge_sha256 is None:
            # Ordinary Python imports are trusted local invocation. Re-execute
            # captured bridge bytes so the returned identity names actual code.
            bridge_source = profile._read(Path(__file__))
            bridge_name = __package__ + '._captured_bridge_' + uuid.uuid4().hex
            captured = types.ModuleType(bridge_name)
            captured.__file__, captured.__package__ = __file__, __package__
            captured.__verified_source_sha256__ = profile._sha(bridge_source)
            sys.modules[bridge_name] = captured
            try:
                exec(compile(bridge_source, str(__file__), 'exec'), captured.__dict__)  # noqa: S102 - trusted local bytes
                return captured.lower_text(context_path, text, rung, domain_size)
            finally:
                sys.modules.pop(bridge_name, None)
        manifest, _ = profile.verify_provider()
        upstream, _ = profile._load_vendor(manifest)
        if type(text) is not str or len(text) > 2000 or type(rung) is not str:
            return {'schema': schema, 'status': 'Invalid', 'detail': 'invalid source text or rung'}
        source = profile._read(LADDER)
        registry = profile._read(REGISTRY)
        raw_context = profile._read(Path(context_path))
        context_data = upstream.strict_loads(raw_context)
        with tempfile.TemporaryDirectory(prefix='zeno-relational-ladder-') as temporary:
            isolated = Path(temporary)
            (isolated / 'ladder_en.json').write_bytes(registry)
            ladder = types.ModuleType(module_name)
            ladder.__file__ = str(isolated / 'ladder.py')
            sys.modules[module_name] = ladder
            exec(compile(source, str(LADDER), 'exec'), ladder.__dict__)  # noqa: S102 - captured local parser
            context = ladder.Context.from_data(context_data)
            tree = ladder.parse(text, rung, context)
            descriptor = _context_descriptor(ladder, source, registry, bridge_sha256, raw_context)
            query = lower_tree(ladder, context, tree, domain_size, _bound_context_id=_context_id(descriptor))
        if profile._read(LADDER) != source or profile._read(REGISTRY) != registry:
            raise RuntimeError('ladder source or registry changed during lowering')
        if profile._sha(profile._read(Path(__file__))) != bridge_sha256:
            raise RuntimeError('bridge source changed during lowering')
        tree_json = json.dumps(tree, sort_keys=True, ensure_ascii=True, separators=(',', ':')).encode()
        return {'schema': schema, 'status': 'Lowered', 'source_text': text, 'rung': rung,
                'tree': tree, 'query': query, 'human_semantic_approval': 'not inferred',
                'authority': 'none', 'bindings': {'context_sha256': profile._sha(raw_context),
                'ladder_sha256': profile._sha(source), 'registry_sha256': profile._sha(registry),
                'tree_sha256': profile._sha(tree_json), 'bridge_sha256': bridge_sha256,
                'provider_manifest_sha256': profile.MANIFEST_SHA256,
                'context_identity': descriptor, 'query_sha256': profile._sha(_canonical(query))}}
    except UnsupportedLowering as exc:
        return {'schema': schema, 'status': 'Unsupported', 'detail': str(exc)[:1000]}
    except profile.ProfileFailure as exc:
        return {'schema': schema, 'status': exc.status, 'detail': exc.detail[:1000]}
    except (ValueError, TypeError, KeyError, IndexError, RecursionError) as exc:
        detail = str(exc)
        return {'schema': schema, 'status': 'UNKNOWN' if 'UNKNOWN' in detail else 'Invalid', 'detail': detail[:1000]}
    except (OSError, RuntimeError, SyntaxError, ImportError) as exc:
        return {'schema': schema, 'status': 'Error', 'detail': str(exc)[:1000]}
    finally:
        sys.modules.pop(module_name, None)


def lower_and_execute(context_path, text, rung, domain_size, runtime, *, limits=None, seconds=60, scratch=None):
    """Keep the original interpretation and bound reasoning outcome together."""
    lowering = lower_text(context_path, text, rung, domain_size)
    result = {'schema': 'zal/relational-ladder-result/3', 'status': lowering['status'],
              'lowering': lowering, 'authority': 'none'}
    if lowering['status'] == 'Lowered':
        answer = profile.execute(lowering['query'], runtime, limits=limits, seconds=seconds, scratch=scratch)
        result.update(status=answer['status'], answer=answer)
    return result
