"""Revision-bound Lens host over the repository ZAL research libraries.

No package-declared code, eval, dynamic adapter names, or external execution.
Every 'checked' result is a replayed finite check, not theorem-kernel authority.
"""
from __future__ import annotations

from copy import deepcopy
from dataclasses import dataclass
from hashlib import sha256
from itertools import product
import json
from pathlib import Path
import sys

from importlib.util import module_from_spec, spec_from_file_location

from . import core, sharing, builtin, bridge, render, exchange


class Unsupported(ValueError):
    """No supported lens/profile, distinct from a refuted mathematical claim."""


ROOT = Path(__file__).resolve().parent
SEMANTIC = ROOT.parent / 'finite_library' / 'prototype.py'


def _load_finite():
    """Load only the checkout's fixed, trusted sibling; never an import from data.

    The optional research adapter is not included in standalone notation wheels.
    Importing the ordinary notation package or its other commands does not load it.
    A host must obtain/review the sibling source; no automatic install is attempted.
    """
    if not SEMANTIC.is_file():
        raise Unsupported('Lens adapter unavailable: use the repository checkout with '
                          'integrations/zal/finite_library/prototype.py; standalone '
                          'notation installs do not include this optional research adapter.')
    name = '_zal_notation_lens_finite'
    spec = spec_from_file_location(name, SEMANTIC)
    module = module_from_spec(spec)
    previous = sys.modules.get(name)
    sys.modules[name] = module  # dataclasses requires its exact host module.
    try:
        spec.loader.exec_module(module)
    except BaseException:
        if previous is None:
            sys.modules.pop(name, None)
        else:
            sys.modules[name] = previous
        raise
    return module


finite = _load_finite()

PROFILE = 'zal/lens-finite-mealy/1'
STATUSES = ('candidate', 'checked', 'refuted', 'unsupported', 'incomplete')
ASSUMPTIONS = [
    'The finite contract is total and deterministic on every declared input.',
    'A step uses the previous state; the linked next_state becomes the next state.',
    'All enumerated states are included in checks, not only reachable states.',
    'Labels, roles, and ownership are declarations, not external execution authority.',
]
OBSERVATION = {'id': 'full-finite-step', 'description': 'Observe every finite input, output, and linked state.',
               'keeps': ['state', 'event', 'authorized', 'outcome', 'next_state', 'reason'], 'omits': []}


def canonical(value):
    return core.canonical(value)


def digest(value):
    return core.digest(value)


def _need(ok, message):
    if not ok:
        raise Unsupported(message)


def _implementation_identity():
    paths = {f'zal_notation/{p.name}': p for p in sorted(ROOT.glob('*.py'))}
    paths['finite_library/prototype.py'] = SEMANTIC
    return digest({name: sha256(path.read_bytes()).hexdigest() for name, path in paths.items()})


IMPLEMENTATION = _implementation_identity()


def counter_contract():
    """Native check_projection's supported port layout; unseen explicitly encodes 0."""
    states = tuple(sorted(('unseen', 'one', 'two', 'three')))
    ins = (finite.Port('state', 'counter', 'mod_four', states),
           finite.Port('event', 'command', 'control', ('inc', 'reset')),
           finite.Port('authorized', 'environment', 'control', ('yes',)))
    outs = (finite.Port('outcome', 'observation', 'control', ('ok',)),
            finite.Port('next_state', 'counter', 'mod_four', states),
            finite.Port('reason', 'observation', 'control', ('done',)))
    successor = {'unseen': 'one', 'one': 'two', 'two': 'three', 'three': 'unseen'}
    rows = {(s, e, 'yes'): (('ok', successor[s] if e == 'inc' else 'unseen', 'done'),)
            for s in states for e in ('inc', 'reset')}
    return finite.build(ins, outs, rows, (('state', 'next_state', 'unseen'),))


def definition_model(contract):
    grammar, notation, _ = builtin.get_builtin('zal-folds')
    groups = []
    for state in contract.inputs[0].values:
        children = [builtin.term('Atom', name=f'{x[1]} [{x[2]}] -> {ys[0][1]} / {ys[0][0]}, {ys[0][2]}')
                    for x, ys in contract.rows if x[0] == state]
        groups.append(builtin.term('Sequence', *children, name=state))
    model = builtin.model(grammar, builtin.term('Sequence', *groups, name='Finite Mealy definition'))
    return grammar, notation, model


def make_source(contract=None):
    contract = counter_contract() if contract is None else contract
    finite.machine_table(contract)  # Refuse unsupported native layout.
    grammar, _, model = definition_model(contract)
    return sharing.make_context(grammar, model, semantics={
        'profile': PROFILE, 'description': 'Exact table semantics supplied as inert finite contract data; term labels are structural presentation.',
        'parameters': {'contract': contract.data()}}, assumptions=ASSUMPTIONS, observation=OBSERVATION)


make_counter_source = make_source


def source_contract(source):
    sharing.validate_context(source)
    _need(source['semantics']['profile'] == PROFILE, 'No registered adapter for this semantic profile.')
    _need(set(source['semantics']['parameters']) == {'contract'}, 'Unknown semantic parameters are not executable adapters.')
    contract = finite.from_data(source['semantics']['parameters']['contract'])
    _need(canonical(source) == canonical(make_source(contract)), 'Source grammar, model, assumptions, or observation do not match the exact finite adapter context.')
    return contract


SPECS = {
    'definition-expand': ('Expanded definition', 'representation', 'Show every retained native ZAL term.', ['complete model and context'], [], ['visible detail']),
    'definition-fold': ('Folded definition', 'representation', 'Hide native term detail without changing its model.', ['complete model and context'], [], ['presentation-only folding']),
    'finite-table': ('Finite transition table', 'representation', 'Existing controlled-table syntax, replayed to the same contract.', ['all rows, port declarations, state link'], [], ['table presentation']),
    'state-machine': ('State-machine graph', 'representation', 'Existing ZAL finite-contract bridge; every table row is retained.', ['every input/output and transition in bridge receipt'], [], ['graph presentation']),
    'parity-quotient': ('Parity quotient', 'quotient', 'Merge states with a supplied total map; check native Mealy congruence.', ['commands, outcome, reason, projected next-state'], ['concrete state distinctions within each fiber'], ['quotient classes; explicit state map']),
    'transformation-monoid': ('Command-effect monoid', 'denotation', 'Close finite command functions under composition.', ['state transformations and their composition'], ['command-word spelling, intermediate states, trace timing, output observations'], ['identity and composed command effects']),
    'all-effects-group': ('All command effects form a group?', 'property', 'Test invertibility of every generated state transformation.', ['same finite command-effect carrier'], ['command-word spelling, intermediate states, trace timing, output observations'], ['group claim, only if all effects invertible']),
    'unit-group': ('Invertible effects only', 'restriction', 'Restrict the generated monoid to its invertible elements.', ['composition and identity among units'], ['noninvertible effects, if any', 'command-word spelling, intermediate states, trace timing, output observations'], ['group-of-units restriction']),
    'one-object-category': ('One-object category of effects', 'repackaging', 'One object; arrows are monoid effects, with composition and identity.', ['monoid composition and identity'], ['command-word spelling, intermediate states, trace timing, output observations'], ['one-object categorical packaging, not an arbitrary program category']),
    'execution-tree': ('Execution-tree unfolding', 'unfolding', 'Registered future finite-prefix unfolding with explicit frontier.', [], [], []),
    'automorphism-group': ('State symmetries', 'symmetry', 'Registered future automorphism checker; reset does not rule it out.', [], [], []),
}


def _spec(lens_id):
    _need(type(lens_id) is str and lens_id in SPECS, 'Lens is not in this registered catalog; absence is not refutation.')
    name, kind, summary, preserves, forgets, adds = SPECS[lens_id]
    return {'id': lens_id, 'name': name, 'kind': kind, 'summary': summary,
            'preserves': preserves, 'forgets': forgets, 'adds': adds,
            'revision': digest({'id': lens_id, 'version': 2, 'implementation': IMPLEMENTATION})}


def discover(source):
    try:
        source_contract(source)
        supported = True
    except (Unsupported, finite.Refused, core.NotationError, KeyError, TypeError, ValueError):
        supported = False
    return {'schema': 'zal.lens-catalog/1', 'catalog_revision': digest([_spec(k) for k in SPECS]),
            'enumeration': 'registered-candidates-only', 'universal_completeness': False,
            'source_context_digest': digest(source),
            'lenses': [{**_spec(k), 'status': 'candidate' if supported and k not in ('execution-tree', 'automorphism-group') else 'unsupported'} for k in SPECS]}


def _parameters(lens_id, parameters):
    _need(type(parameters) is dict, 'Parameters must be inert JSON data.')
    if lens_id == 'definition-fold':
        _need(set(parameters) <= {'paths'}, 'Unknown fold parameter.')
        paths = parameters.get('paths', ['root'])
        _need(type(paths) is list and all(type(x) is str for x in paths), 'Fold paths must be strings.')
        return {'paths': sorted(set(paths))}
    if lens_id == 'parity-quotient':
        _need(set(parameters) <= {'mapping'}, 'Unknown quotient parameter.')
        return {'mapping': deepcopy(parameters.get('mapping', {'unseen': 'even', 'one': 'odd', 'two': 'even', 'three': 'odd'}))}
    _need(not parameters, 'This lens has no parameters; executable declarations are forbidden.')
    return {}


def _context_for_contract(contract, observation=None):
    grammar, graph, receipt = bridge.from_finite_contract(contract)
    context = sharing.make_context(grammar, graph, semantics={
        'profile': 'zal/lens-bridged-finite/1', 'description': 'Native finite-contract graph projection; semantics retained in full contract data.',
        'parameters': {'contract': contract.data()}}, assumptions=ASSUMPTIONS,
        observation=OBSERVATION if observation is None else observation)
    return context, receipt


def _quotient(contract, mapping):
    _need(type(mapping) is dict and set(mapping) == set(contract.inputs[0].values), 'Quotient map must be total with exact source domain.')
    for v in mapping.values():
        finite.name(v)
    table = {}
    conflicts = []
    for (state, event, auth), ys in contract.rows:
        outcome, target, reason = ys[0]
        key, value = (mapping[state], event, auth), (outcome, mapping[target], reason)
        if key in table and table[key] != (value,):
            conflicts.append({'abstract_input': list(key), 'left': list(table[key][0]), 'right': list(value)})
        else:
            table[key] = (value,)
    if conflicts:
        return None, conflicts
    values = tuple(sorted(set(mapping.values())))
    old_in, old_out = contract.inputs[0], contract.outputs[1]
    ins = (finite.Port(old_in.name, old_in.role, old_in.unit, values), *contract.inputs[1:])
    outs = (contract.outputs[0], finite.Port(old_out.name, old_out.role, old_out.unit, values), contract.outputs[2])
    return finite.build(ins, outs, table, (('state', 'next_state', mapping['unseen']),), contract.owns), []


def _compose(left, right):
    """left after right; operands index the same ordered finite state carrier."""
    return tuple(left[right[i]] for i in range(len(left)))


def _algebra(contract, budget):
    states = contract.inputs[0].values
    index = {s: i for i, s in enumerate(states)}
    table = finite.machine_table(contract)
    generators = {canonical([e, a]): tuple(index[table[s, e, a][1]] for s in states)
                  for e, a in product(contract.inputs[1].values, contract.inputs[2].values)}
    identity = tuple(range(len(states)))
    known, queue = {identity}, [identity]
    while queue:
        old = queue.pop(0)
        for gen in generators.values():
            budget.spend()
            new = _compose(gen, old)
            if new not in known:
                if len(known) >= 512:
                    raise finite.Incomplete('Transformation closure exceeds this adapter size cap.')
                known.add(new)
                queue.append(new)
    elements = sorted(known)
    idx = {e: i for i, e in enumerate(elements)}
    multiplication = []
    for x in elements:
        row = []
        for y in elements:
            budget.spend()
            row.append(idx[_compose(x, y)])
        multiplication.append(row)
    units = [i for i, e in enumerate(elements) if len(set(e)) == len(states)]
    return {'state_order': list(states), 'generators': {g: idx[v] for g, v in generators.items()},
            'elements': [list(e) for e in elements], 'multiplication': multiplication,
            'identity': idx[identity], 'units': units, 'composition_order': 'left after right'}


def _algebra_context(algebra, lens_id):
    grammar, notation, _ = builtin.get_builtin('zal-folds')
    carrier = algebra['units'] if lens_id == 'unit-group' else list(range(len(algebra['elements'])))
    children = [builtin.term('Atom', name=f"e{i}: {algebra['elements'][i]}" + (' (identity)' if i == algebra['identity'] else '')) for i in carrier]
    model = builtin.model(grammar, builtin.term('Sequence', *children, name=_spec(lens_id)['name']))
    payload = deepcopy(algebra)
    payload['carrier'] = carrier
    if lens_id == 'one-object-category':
        payload['objects'] = ['object']
        payload['arrows'] = [{'id': i, 'source': 'object', 'target': 'object'} for i in carrier]
    context = sharing.make_context(grammar, model, semantics={
        'profile': 'zal/lens-finite-algebra/1', 'description': _spec(lens_id)['summary'],
        'parameters': payload}, assumptions=['Finite transformations on the explicitly ordered state carrier.',
        'Composition is left after right; word syntax and intermediate observations are not retained.'],
        observation={'id': 'command-effect', 'description': 'Observe only the final state transformation.',
        'keeps': ['state-transformation', 'composition', 'identity'],
        'omits': ['command-word', 'intermediate-state', 'outcome', 'reason']})
    return context


def _materialize(source, lens_id, params, budget):
    contract = source_contract(source)
    if lens_id in ('execution-tree', 'automorphism-group'):
        raise Unsupported('Registered construction has no checked adapter in this slice; no semantic refutation is asserted.')
    if lens_id in ('definition-expand', 'definition-fold', 'finite-table'):
        budget.spend()
        target = deepcopy(source)
        if lens_id == 'finite-table':
            payload = {'controlled_table': finite.render_table(contract, True)}
        else:
            folded = params.get('paths', [])
            _, notation, _ = definition_model(contract)
            svg = render.render_svg(source['grammar'], source['model'], notation, folded=folded)
            payload = {'folded': folded, 'svg_digest': sha256(svg.encode()).hexdigest()}
        return target, payload
    if lens_id == 'state-machine':
        budget.spend(len(contract.rows))
        return _context_for_contract(contract)
    if lens_id == 'parity-quotient':
        budget.spend(len(contract.rows))
        quotient, conflicts = _quotient(contract, params['mapping'])
        if quotient is None:
            return None, {'conflicts': conflicts}
        obs = {'id': 'projected-state', 'description': 'Observe the mapped state; retain event and observable outputs.',
               'keeps': ['projected-state', 'event', 'authorized', 'outcome', 'reason'], 'omits': ['concrete-state']}
        target, receipt = _context_for_contract(quotient, obs)
        return target, {'bridge': receipt, 'quotient_contract': quotient.data()}
    algebra = _algebra(contract, budget)
    return _algebra_context(algebra, lens_id), {'algebra': algebra}


def apply(source, lens_id, parameters=None, *, budget=100000):
    """Construct an unverified candidate; resource exhaustion raises Incomplete."""
    return _apply_with_budget(source, lens_id, parameters, _budget(budget))


def _budget(limit):
    _need(type(limit) is int and 0 <= limit <= 1_000_000, 'Budget must be an integer in 0..1000000.')
    return finite.Budget(limit)


def _apply_with_budget(source, lens_id, parameters, budget):
    spec = _spec(lens_id)
    params = _parameters(lens_id, {} if parameters is None else parameters)
    source_contract(source)
    target, payload = _materialize(source, lens_id, params, budget)
    result = {'schema': 'zal.lens-application/1', 'status': 'candidate', 'lens_id': lens_id,
            'lens_revision': spec['revision'], 'implementation_digest': IMPLEMENTATION,
            'source_context_digest': digest(source),
            'source_model_digest': core.model_digest(source['grammar'], source['model']),
            'source_contract_digest': source_contract(source).revision,
            'target_context_digest': digest(target) if target is not None else None,
            'target_model_digest': core.model_digest(target['grammar'], target['model']) if target is not None else None,
            'target_context': target, 'parameters': params, 'mapping_digest': digest(params),
            'preserves': deepcopy(spec['preserves']), 'forgets': deepcopy(spec['forgets']), 'adds': deepcopy(spec['adds']),
            'payload': payload, 'authority': 'none', 'native_fcis_qualification': 'not-performed'}
    canonical(result)  # A returned application must fit the native exchange bounds.
    return result


def _check_algebra(algebra, lens_id, budget):
    mul, identity, elements = algebra['multiplication'], algebra['identity'], algebra['elements']
    carrier = algebra['units'] if lens_id == 'unit-group' else list(range(len(elements)))
    if lens_id == 'all-effects-group':
        for i, effect in enumerate(elements):
            budget.spend()
            if len(set(effect)) != len(effect):
                collision = next((a, b) for a in range(len(effect)) for b in range(a + 1, len(effect)) if effect[a] == effect[b])
                return 'refuted', {'claim': 'Every generated command effect is invertible.',
                        'noninvertible_effect': i, 'state_collision': list(collision), 'images': effect,
                        'nonclaim': 'This does not refute group-theoretic symmetry or a group-of-units lens.'}
    for a in carrier:
        budget.spend()
        _need(mul[a][identity] == a == mul[identity][a], 'Identity check failed.')
        if lens_id in ('unit-group', 'all-effects-group'):
            _need(any(mul[a][b] == identity == mul[b][a] for b in carrier), 'Inverse check failed.')
        for b in carrier:
            budget.spend()
            _need(mul[a][b] in carrier, 'Carrier is not closed.')
            for c in carrier:
                budget.spend()
                _need(mul[mul[a][b]][c] == mul[a][mul[b][c]], 'Associativity check failed.')
    return 'checked', {'carrier_size': len(carrier), 'generated_monoid_size': len(elements),
                       'unit_count': len(algebra['units']), 'identity': identity,
                       'finite_laws': ['closure', 'identity', 'associativity'] + (['two-sided inverses'] if lens_id in ('unit-group', 'all-effects-group') else []),
                       'category_objects': 1 if lens_id == 'one-object-category' else None}


def check(source, application, budget=100000):
    result = {'schema': 'zal.lens-check/1', 'status': 'unsupported', 'authority': 'none',
              'native_fcis_qualification': 'not-performed', 'proof_scope': 'finite research adapter only',
              'implementation_digest': IMPLEMENTATION, 'source_context_digest': None,
              'application_digest': None, 'budget': budget,
              'budget_scope': 'Counts finite construction/replay/law work; parsing, validation, hashing, and rendering have separate native size bounds, not this logical counter.'}
    work = None
    try:
        work = _budget(budget)
        result.update(source_context_digest=digest(source), application_digest=digest(application))
        _need(type(application) is dict and application.get('schema') == 'zal.lens-application/1', 'Malformed application.')
        _need(application['source_context_digest'] == digest(source), 'Source context changed; stale application.')
        expected = _apply_with_budget(source, application['lens_id'], application['parameters'], work)
        _need(canonical(application) == canonical(expected), 'Application, target, mapping, or implementation changed; regenerate and recheck.')
        lens_id = application['lens_id']
        contract = source_contract(source)
        if lens_id == 'parity-quotient':
            # The trusted sibling's counterexample uses tuples. Normalize that
            # host result to inert JSON so persisted receipts replay identically.
            native = json.loads(json.dumps(finite.check_projection(contract, application['parameters']['mapping'], work), allow_nan=False))
            status = {'exact-quotient-within-finite-model': 'checked', 'not-an-exact-quotient': 'refuted', 'incomplete': 'incomplete'}[native['status']]
            evidence = {'finite_library_checker': 'integrations/zal/finite_library/prototype.check_projection; implementation-digest-bound', 'native_result': native,
                        'claim': 'For every source state/input, mapped successor and output agree within each fiber.'}
        elif lens_id in ('transformation-monoid', 'all-effects-group', 'unit-group', 'one-object-category'):
            algebra = expected['payload']['algebra']
            _need(algebra == application['payload']['algebra'], 'Algebra does not match regenerated effects.')
            status, evidence = _check_algebra(algebra, lens_id, work)
            evidence['checker'] = 'supplemental exhaustive finite transformation checker, not native theorem kernel'
        elif lens_id == 'finite-table':
            work.spend(len(contract.rows))
            _need(finite.parse_table(application['payload']['controlled_table'], True) == contract, 'Controlled table changed contract.')
            status, evidence = 'checked', {'finite_library_checker': 'prototype.parse_table; implementation-digest-bound', 'rows': len(contract.rows), 'claim': 'Exact table round trip.'}
        elif lens_id == 'state-machine':
            work.spend(len(contract.rows))
            projected, receipt = _context_for_contract(contract)
            _need(projected == application['target_context'] and receipt == application['payload'], 'Bridge replay mismatch.')
            # Recover all rows independently from graph endpoints and JSON labels.
            recovered = []
            for edge in projected['model']['body']['edges']:
                event, output = json.loads(edge['fields']['event']), json.loads(edge['fields']['outcome'])
                recovered.append(((edge['source'][0], event['event'], event['authorized']),
                                  ((output['outcome'], edge['target'][0], output['reason']),)))
            _need(tuple(sorted(recovered)) == contract.rows, 'Graph row recovery mismatch.')
            status, evidence = 'checked', {'native_bridge': 'bridge.from_finite_contract', 'rows_recovered': len(recovered),
                                          'claim': 'Exact row representation with declarations retained in receipt; not a new behavior theorem.'}
        else:
            work.spend()
            _, notation, _ = definition_model(contract)
            svg = render.render_svg(source['grammar'], source['model'], notation, folded=application['payload']['folded'])
            recovered = exchange.recover_source(svg)
            _need(recovered['model'] == source['model'], 'Native fold lost model.')
            status, evidence = 'checked', {'native_renderer': 'render.render_svg', 'native_recovery': 'exchange.recover_source',
                                          'model_digest': core.model_digest(source['grammar'], source['model']),
                                          'claim': 'Presentation only; complete native model retained.'}
        result.update(status=status, evidence=evidence)
    except finite.Incomplete:
        result.update(status='incomplete', reason='Finite work or construction size budget exhausted; neither checked nor refuted.')
    except (Unsupported, finite.Refused, core.NotationError, KeyError, TypeError, ValueError) as exc:
        result.update(status='unsupported', reason=str(exc))
    result['work'] = work.used if work is not None else 0
    return result


def verify_receipt(source, application, receipt):
    """Receipt validity includes replay, never merely its declared status/hash."""
    try:
        return type(receipt) is dict and canonical(receipt) == canonical(check(source, application, receipt['budget']))
    except (ValueError, TypeError, KeyError):
        return False


def zoom_back(retained_source, application):
    _need(digest(retained_source) == application['source_context_digest'], 'Exact retained source is required; a lossy target is not an inverse.')
    source_contract(retained_source)
    return deepcopy(retained_source)


def render_application(source, application):
    """Render native model only; no checker status is inserted into native SVG."""
    _need(application['source_context_digest'] == digest(source), 'Source context changed; stale rendering application.')
    target = application['target_context']
    _need(target is not None, 'Refuted construction has no deterministic target model.')
    if target['model']['kind'] == 'graph':
        _, notation, _ = builtin.get_builtin('state-machine')
    else:
        _, notation, _ = builtin.get_builtin('zal-folds')
    return render.render_svg(target['grammar'], target['model'], notation,
                             folded=application['payload'].get('folded', []))


def dispatch(request):
    """Closed host dispatch; JSON may choose only these fixed actions."""
    canonical(request)
    _need(type(request) is dict and type(request.get('action')) is str, 'Malformed request.')
    action = request['action']
    fields = {'source': {'action', 'contract'}, 'discover': {'action', 'source'},
              'apply': {'action', 'source', 'lens_id', 'parameters'},
              'check': {'action', 'source', 'application', 'budget'},
              'verify': {'action', 'source', 'application', 'receipt'},
              'restore': {'action', 'source', 'application'}}
    optional = {'budget'} if action == 'apply' else set()
    _need(action in fields and fields[action] <= set(request) <= fields[action] | optional,
          'Unknown action or fields; imported code is not supported.')
    try:
        if action == 'source':
            return make_source(finite.from_data(request['contract']))
        if action == 'discover':
            return discover(request['source'])
        if action == 'apply':
            return apply(request['source'], request['lens_id'], request['parameters'],
                         budget=request.get('budget', 100000))
        if action == 'check':
            return check(request['source'], request['application'], request['budget'])
        if action == 'verify':
            return {'schema': 'zal.lens-verification/1',
                    'matches_replay': verify_receipt(request['source'], request['application'], request['receipt']),
                    'authority': 'none', 'scope': 'Receipt equality after replay, not a checked claim unless the replayed status is checked.'}
        return zoom_back(request['source'], request['application'])
    except finite.Incomplete:
        return {'schema': 'zal.lens-construction/1', 'status': 'incomplete', 'authority': 'none',
                'reason': 'Finite work or construction size budget exhausted; neither checked nor refuted.'}
    except (Unsupported, finite.Refused, core.NotationError, KeyError, TypeError, ValueError) as exc:
        return {'schema': 'zal.lens-response/1', 'status': 'unsupported', 'authority': 'none',
                'reason': str(exc)}


@dataclass(frozen=True)
class SourceSnapshot:
    """Immutable retained source bytes; callers receive disposable deep copies."""
    _json: str

    @classmethod
    def capture(cls, source):
        source_contract(source)
        return cls(canonical(source))

    @property
    def context(self):
        return json.loads(self._json)

    @property
    def context_digest(self):
        return digest(self.context)

    def apply(self, lens_id, parameters=None):
        return apply(self.context, lens_id, parameters)

    def restore(self, application):
        return zoom_back(self.context, application)
