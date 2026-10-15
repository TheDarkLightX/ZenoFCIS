#!/usr/bin/env python3
"""Check one closed core-component application through ordinary F1/F2/Authority.

Each application in core-apps/ has a spec in tools/core_app_specs/ naming its
profile, independent reference, coupled facts, metadata controls, law-violating
witnesses and a reviewed copy of its composition. The contract's own rules,
read through the names the composition's views use, must first reproduce every
example decision. The composition check then derives every component step from
composition.json alone; the reviewed pins separately refuse any change to its
non-prose fields. Composition metadata grants no
authority. Static checks are not execution evidence. Heavy execution needs an
externally assigned bounded slot. Neither mode supplies publication or release
authority.
"""
import argparse
import copy
import functools
import hashlib
import importlib
import importlib.util
import itertools
import json
import os
from pathlib import Path
import re
import shutil
import tomllib

import check_core_components as components
import check_generated_application as applications
import instantiate_core as core

ROOT = Path(__file__).resolve().parents[1]
SPECS = ROOT / 'tools/core_app_specs'
REFERENCES = ROOT / 'tools/test_data/core_apps'
LOCAL_REFERENCE = ROOT / 'tools/test_data/core_components/reference.py'
LOCAL_REFERENCE_SHA = 'c5c74f405e4c55cee85e15231e384cb029fa939a7eab9432dc83295c03bc0868'
GENERATED_TESTS = (
    'every_example_is_the_authority_decision', 'genesis_publishes_only_the_contract_genesis',
    'examples_run_as_one_persistent_session', 'every_written_form_of_an_example_reads_alike',
    'the_journey_checks_every_example_in_any_file_order',
    'commands::version_prints_the_contract_version_and_identity',
    'commands::init_creates_a_store_and_refuses_an_existing_one',
    'commands::state_reads_the_store_and_refuses_another_file',
    'commands::decide_commits_nothing_and_refuses_an_unknown_field',
    'commands::submit_commits_records_and_refuses_bad_input_without_writing',
    'commands::a_rejected_submission_writes_nothing',
    'commands::concurrent_submissions_keep_the_journal_true_to_the_store',
    'commands::a_journal_line_past_the_head_is_ignored_and_then_removed',
    'commands::a_missing_journal_at_genesis_is_created_by_submit',
    'commands::a_failed_deliver_names_the_deliveries_it_completed',
    'commands::history_decides_every_submission_again_and_refuses_a_lost_journal',
    'commands::pending_and_deliver_send_each_delivery_once_to_the_file',
    'session::tests::a_failure_after_deliveries_names_them_and_keeps_its_kind',
    'commands::scalar_frame_cli_matches_native_decisions_and_refusals',
    'session::scalar_consumer_tests::scalar_frames_preserve_native_envelopes_and_refuse_aliases')
# Each family's step in the local reference, and the parameters it takes in order.
LOCAL_STEPS = {
    'reservation-pool': ('reservation', ('C', 'Q')), 'rate-limiter': ('rate', ('W', 'N')),
    'approval-queue': ('approval', ('K',)), 'bounded-counter': ('counter', ('C',)),
    'consumable-budget': ('budget', ('C',)), 'versioned-register': ('register', ('C',)),
    'idempotency-slot': ('slot', ('C',)), 'retry-budget': ('retry', ('C',)),
    'finite-phase-machine': ('phase', ('C',)), 'logical-deadline': ('deadline', ('C',)),
}
# Views, guards and effect payloads are rule expressions (docs/CONTRACT_RULES.md,
# operators and functions): the CLI's grammar and precedence (contract/expr.rs)
# without the law-only `*`, `div_floor` and `div_ceil`, evaluated over the
# contract's names, `variables` in v2/policy.json. As in the decision program,
# every operand is evaluated, a boolean used as a number is 0 or 1, a number
# used as a boolean holds when it equals 1, and arithmetic is 64-bit.
TOKEN = re.compile(r'->|&&|\|\||==|!=|<=|>=|[!<>()+*,-]|[A-Za-z_][A-Za-z0-9_.]*|[0-9]+')
BINARY = {'->': 1, '||': 2, '&&': 3, '==': 4, '!=': 4, '<': 5, '<=': 5, '>': 5, '>=': 5, '+': 6, '-': 6}
FUNCTIONS = {'choose': 3}
FIELD = re.compile(r'(pre|command|context)\.\d+\.(\d+)')


def number(value):
    require(-2 ** 63 <= value < 2 ** 63, 'arithmetic overflows 64 bits')
    return value


OPERATIONS = {
    '->': lambda a, b: a != 1 or b == 1, '||': lambda a, b: a == 1 or b == 1, '&&': lambda a, b: a == 1 and b == 1,
    '==': lambda a, b: a == b, '!=': lambda a, b: a != b, '<': lambda a, b: a < b, '<=': lambda a, b: a <= b,
    '>': lambda a, b: a > b, '>=': lambda a, b: a >= b, '+': lambda a, b: number(a + b), '-': lambda a, b: number(a - b),
    'not': lambda a: a != 1, 'neg': lambda a: number(-a), 'choose': lambda c, a, b: a if c == 1 else b,
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def module(name, path, expected):
    require(sha(path) == expected, f'{path}: reference changed; review required')
    spec = importlib.util.spec_from_file_location(name, path)
    loaded = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(loaded)
    return loaded


def app_names():
    return sorted(path.stem.replace('_', '-') for path in SPECS.glob('*.py'))


def load_spec(name):
    require(name in app_names(), f'no application spec: {name}')
    return importlib.import_module('core_app_specs.' + name.replace('-', '_'))


def reference(spec):
    return module(spec.APP.replace('-', '_') + '_reference', REFERENCES / spec.REFERENCE, spec.REFERENCE_SHA)


def all_cases(ref):
    for raw in itertools.product(*ref.DOMAINS):
        yield {'input': raw, 'expected': ref.decision(raw)}


def example_rows(ref):
    """Journey first, so the generated persistent session replays it, then the rest of the product."""
    seen = set()
    for row in itertools.chain(ref.journey(), all_cases(ref)):
        if tuple(row['input']) not in seen:
            seen.add(tuple(row['input']))
            yield row


def example_line(row):
    e = row['expected']
    numbers = lambda values: ' '.join(map(str, values))
    reason = '-' if e['reason'] is None else str(e['reason'])
    deliveries = '; '.join(str(channel) + ' ' + numbers(payload) for channel, payload in e['deliveries']) or '-'
    return f"{numbers(row['input'])} | {e['class'].lower()} {reason} {numbers(e['post'])} | {deliveries}"


def parse(text):
    """One rule expression as a tree, by precedence climbing; `->` associates right."""
    require(isinstance(text, str), f'expression {text!r} is not text')
    tokens, rest = [], text.strip()
    while rest:
        match = TOKEN.match(rest)
        require(match is not None, f'unsupported text {rest!r} in {text!r}')
        tokens.append(match.group())
        rest = rest[match.end():].lstrip()
    position = 0

    def take():
        nonlocal position
        require(position < len(tokens), f'{text!r} ends early')
        position += 1
        return tokens[position - 1]

    def expression(level, depth):
        nonlocal position
        require(depth <= 64, f'{text!r} nests too deeply')
        token = take()
        if token == '(':
            tree = expression(0, depth + 1)
            require(take() == ')', f'expected `)` in {text!r}')
        elif token in ('!', '-'):
            tree = ('not' if token == '!' else 'neg', expression(8, depth + 1))
        elif token in ('true', 'false'):
            tree = ('value', token == 'true')
        elif token.isdigit():
            tree = ('value', int(token))
        elif position < len(tokens) and tokens[position] == '(':
            position += 1
            arguments = [expression(0, depth + 1)]
            while position < len(tokens) and tokens[position] == ',':
                position += 1
                arguments.append(expression(0, depth + 1))
            require(take() == ')' and FUNCTIONS.get(token) == len(arguments),
                    f'`{token}` with {len(arguments)} arguments in {text!r}')
            tree = (token, *arguments)
        elif token[0].isalpha() or token[0] == '_':
            tree = ('name', token)
        else:
            raise ValueError(f'unexpected `{token}` in {text!r}')
        while position < len(tokens) and BINARY.get(tokens[position], 0) >= max(level, 1):
            operator = take()
            tree = (operator, tree, expression(BINARY[operator] + (operator != '->'), depth + 1))
        return tree

    tree = expression(0, 0)
    require(position == len(tokens), f'unexpected `{tokens[position] if position < len(tokens) else ""}` in {text!r}')
    return tree


def evaluate(tree, name):
    if tree[0] == 'value':
        return tree[1]
    if tree[0] == 'name':
        return name(tree[1])
    return OPERATIONS[tree[0]](*[evaluate(part, name) for part in tree[1:]])


def rule_names(policy, state_fields, width):
    """Bind the contract's names to raw positions: state, then command, then
    context fields, each in field-ID order. `check_rules_binding` confirms this
    binding against every example decision."""
    fields = {name: FIELD.fullmatch(text) for name, text in policy['variables'].items()}
    order = [(kind, field) for kind in ('pre', 'command', 'context')
             for field in sorted({int(match[2]) for match in fields.values() if match and match[1] == kind})]
    require([field for kind, field in order if kind == 'pre'] == state_fields and len(order) == width,
            "the contract's names do not cover the raw input")
    positions = {name: order.index((match[1], int(match[2]))) for name, match in fields.items() if match}
    derived = {name: parse(text) for name, text in policy['variables'].items() if not fields[name]}
    return positions, derived


def names(binding, values):
    """Resolve the contract's names, derived ones included, against one row's values."""
    positions, derived = binding
    known = {}

    def name(text):
        if text in positions:
            return values[positions[text]]
        require(text in derived, f'unknown name {text!r}')
        if text not in known:
            known[text] = None
            known[text] = evaluate(derived[text], name)
        require(known[text] is not None, f'{text!r} is defined in terms of itself')
        return known[text]
    return name


def check_rules_binding(policy, state_fields, rows):
    """The contract's own rules, read through `rule_names`, decide every example as written.

    This guards the binding and the expression semantics that the composition
    check relies on: a name bound to the wrong raw position, or an operator the
    rules use read differently from the contract, changes some decision. As in
    the contract, the first case whose `when` holds decides; an accept writes
    its `post` fields over the pre-state and emits its `outbox` entries in
    ordinal order; a reject changes nothing and emits nothing.
    """
    state_fields = list(state_fields)
    binding = rule_names(policy, state_fields, len(rows[0]['input']))
    term = lambda value: ('value', value) if isinstance(value, (bool, int)) else parse(value)
    cases = []
    for case in policy['cases']:
        require(set(case) <= {'rule', 'when', 'class', 'reason', 'post', 'outbox'} and case['class'] in ('Accept', 'Reject')
                and all(int(field) in state_fields for field in case['post']), f"unsupported rule {case.get('when')!r}")
        post = {state_fields.index(int(field)): term(value) for field, value in case['post'].items()}
        outbox = [(entry['channel'], [term(entry['payload'][key]) for key in sorted(entry['payload'], key=int)])
                  for entry in sorted(case['outbox'], key=lambda entry: entry['ordinal'])]
        require(case['class'] == 'Accept' or not (post or outbox), f"reject rule {case['when']!r} writes or emits")
        cases.append((parse(case['when']), case['class'], case['reason'], post, outbox))
    for row in rows:
        raw, expected = list(row['input']), row['expected']
        name = names(binding, raw)
        decided = next((case for case in cases if evaluate(case[0], name) == 1), None)
        require(decided is not None, f'no rule decides {raw}')
        _, kind, reason, post, outbox = decided
        state = raw[:len(state_fields)]
        for index, value in post.items():
            state[index] = evaluate(value, name)
        deliveries = [(channel, [evaluate(value, name) for value in payload]) for channel, payload in outbox]
        written = (expected['class'], expected['reason'], list(expected['post']),
                   [(channel, list(payload)) for channel, payload in expected['deliveries']])
        require((kind, reason, state, deliveries) == written,
                f'the rules, read through the names, decide {raw} as {(kind, reason, state, deliveries)}, '
                f'not as the example {written}')
    return len(rows)


def check_composition(app, m, ref, accepted, local):
    """Check what composition.json means, from its own data, on every accepted row.

    Each component must be the qualified catalogue instance it names, and the
    owners must partition the state. On an accepted row, the occurrences whose
    guards hold select the component steps, and no component steps twice. A
    stepping component's projected input lies in its instance's domains and
    selects the named command; the local reference accepts it and yields the
    application successor seen through the same view. A component that does not
    step keeps its projected state and owned fields. Every projected state lies
    in the instance's state domains and determines the fields its component
    owns. The declared effect is the whole outbox and matches the contract's.
    Every view reads the same prestate, so the listed step order has no effect
    here; only the reviewed pin fixes it. Every accepted row fires at least one
    occurrence, so a transition that changes nothing is declared as an
    occurrence with no steps, and every declared occurrence fires on at least
    one accepted row, so no occurrence goes unchecked.
    """
    require(m['authority'] == 'none' and m['owner_adoption'] is False, 'metadata cannot grant authority')
    policy = core.read_json((app / 'v2/policy.json').read_text())
    state_fields = list(m['state_fields'])
    binding = rule_names(policy, state_fields, len(ref.DOMAINS))
    owners = {}
    for owner, fields in [('coordinator', (m.get('coordinator') or {}).get('owns_fields', []))] + \
            [(c['name'], c['owns_fields']) for c in m['components']]:
        for field in fields:
            require(field not in owners, f'field {field} is owned by both {owners.get(field)} and {owner}')
            owners[field] = owner
    require(sorted(owners) == sorted(state_fields), f'fields without exactly one owner: {sorted(set(owners) ^ set(state_fields))}')
    require(len({c['name'] for c in m['components']}) == len(m['components']), 'component names repeat')
    require(sorted(m['stateless_components']) == sorted(c['name'] for c in m['components'] if not c['owns_fields']),
            'stateless_components must name exactly the components that own no field')
    components = {}
    for c in m['components']:
        try:
            files = core.source(c['family'], c['parameters'], include_proof=False)
        except ValueError as error:
            raise ValueError(f"{c['name']}: unqualified instance: {error}") from None
        domains = core.read_json(files['core-instance.json'].decode())['input_domains']
        require(set(c['views']) == {'state', 'command', 'context'}, f"{c['name']}: views need state, command and context")
        views = {part: [parse(text) for text in texts] for part, texts in c['views'].items()}
        require(len(views['state']) == core.definition(c['family'])['state_width']
                and sum(map(len, views.values())) == len(domains), f"{c['name']}: views do not fit the {c['family']} inputs")
        function, keys = LOCAL_STEPS[c['family']]
        components[c['name']] = {
            'family': c['family'], 'domains': domains, 'views': views, 'determined': {},
            'step': functools.partial(getattr(local, function), *(c['parameters'][key] for key in keys)),
            'owned': [state_fields.index(field) for field in c['owns_fields']]}
    occurrences = []
    for o in m['occurrences']:
        for s in o['ordered_steps']:
            require(s['component'] in components, f"{o['name']}: unknown component {s['component']}")
            require(s['command'] in core.COMMANDS[components[s['component']]['family']][1].values(),
                    f"{o['name']}: {s['command']} is not a command of {s['component']}")
        occurrences.append((o['name'], parse(o['when']), [(s['component'], s['command']) for s in o['ordered_steps']]))
    effect = m['effects']
    require(any(effect['owner'] in dict(steps) for name, _, steps in occurrences if name == effect['occurrence']),
            'the effect owner takes no step in the effect occurrence')
    outbox = [entry for case in policy['cases'] for entry in case.get('outbox', [])]
    require(outbox and all((entry['channel'], entry['destination'], sorted(entry['payload'])) ==
                           (effect['channel'], effect['destination'], sorted(effect['payload'])) for entry in outbox),
            'the effect channel, destination or payload fields differ from the contract outbox')
    payload = [parse(effect['payload'][field]) for field in sorted(effect['payload'], key=int)]
    width, steps = len(state_fields), 0
    fires, silent = [0] * len(occurrences), []
    for row in accepted:
        raw, result = row['input'], row['expected']
        before = names(binding, list(raw))
        after = names(binding, list(result['post']) + list(raw[width:]))
        matched = []
        for index, (name, guard, chosen) in enumerate(occurrences):
            if evaluate(guard, before) == 1:
                fires[index] += 1
                matched.append((name, chosen))
        if not matched:
            silent.append(list(raw))
        stepped = {}
        for name, chosen in matched:
            for component, command in chosen:
                require(component not in stepped, f'{component} steps twice in one transition')
                stepped[component] = (name, command)
        for name, c in components.items():
            pre = [evaluate(view, before) for view in c['views']['state']]
            post = [evaluate(view, after) for view in c['views']['state']]
            for label, state, values in (('prestate', pre, raw), ('successor', post, result['post'])):
                require(all(value in domain for value, domain in zip(state, c['domains'])),
                        f'{name}: projected {label} {state} outside its state domain')
                owned = tuple(values[i] for i in c['owned'])
                require(c['determined'].setdefault(tuple(state), owned) == owned, f'{name}: its state view does not determine its fields')
            if name not in stepped:
                require(pre == post and all(raw[i] == result['post'][i] for i in c['owned']), f'{name} changed without a step')
                continue
            occurrence, command = stepped[name]
            values = pre + [evaluate(view, before) for view in c['views']['command'] + c['views']['context']]
            require(all(value in domain for value, domain in zip(values, c['domains'])),
                    f'{occurrence}: {name} input {values} outside its instance domain')
            selected = core.command_name(c['family'], values)
            require(selected == command, f'{occurrence}: {name} view selects {selected}, not {command}')
            local_result = c['step'](values)
            require(local_result['class'] == 'Accept', f"{occurrence}: {name} refuses its {command} step ({local_result['reason']})")
            require(local_result['post'] == post,
                    f"{occurrence}: {name} successor {local_result['post']} differs from the application successor {post}")
            steps += 1
        emitted = [(effect['channel'], [evaluate(part, before) for part in payload])] \
            if any(name == effect['occurrence'] for name, _ in matched) else []
        require(result['deliveries'] == emitted and len(emitted) <= effect['max_per_transition'],
                'deliveries differ from the declared effect')
    # After the rows, so a repeated occurrence with steps is refused above for
    # the component that steps twice.
    require(not silent, f'{len(silent)} accepted inputs fire no occurrence, the first {(silent or [None])[0]}; '
            'declare each transition, with no steps where nothing changes')
    for (name, _, _), count in zip(occurrences, fires):
        require(count > 0, f'{name} fires on no accepted input')
    require(len({name for name, _, _ in occurrences}) == len(occurrences), 'occurrence names repeat')
    return {'components': len(components), 'accepted_inputs': len(accepted), 'component_steps': steps,
            'occurrence_fires': {name: count for (name, _, _), count in zip(occurrences, fires)},
            'inputs_without_occurrence': len(silent)}


def check_reviewed_pins(spec, m):
    """Refuse any change to the reviewed composition, sound or not; this checks no meaning.

    Every field of composition.json except its prose must equal the reviewed
    copy in the spec.
    """
    require(m['schema'] == 'zeno-fcis/core-application-composition/1' and m['application'] == spec.APP
            and m['profile'] == spec.PROFILE, 'unsupported application profile')
    require(m['input_tuples'] == spec.INPUT_TUPLES and m['state_fields'] == list(spec.STATE_FIELDS), 'domain or fields changed')
    require(m.get('coordinator') == spec.COORDINATOR, 'coordinator ownership or guarantee changed')
    require(len(m['components']) == len(spec.COMPONENTS), 'missing or duplicate component')
    for c, (name, family, parameters, fields, views) in zip(m['components'], spec.COMPONENTS):
        require((c['name'], c['family'], c['parameters'], c['owns_fields'], c['views']) ==
                (name, family, parameters, fields, views), 'parameter, ownership or projection changed')
    require(m['stateless_components'] == [name for name, _, _, fields, _ in spec.COMPONENTS if not fields],
            'stateless components changed')
    occurrences = [{'name': name, 'when': guard, 'ordered_steps': [{'component': n, 'command': c} for n, c in steps]}
                   for name, guard, steps in spec.OCCURRENCES]
    require(m['occurrences'] == occurrences, 'changed occurrence guard, order, multiplicity or binding')
    require(m['effects'] == spec.EFFECTS, 'effect binding changed')


def check_instance_sources(app, m):
    """Each component's stored files equal the catalogue instance it names.

    This includes the family certificate reference, so it needs current
    certificates; it runs last.
    """
    for c in m['components']:
        files = core.source(c['family'], c['parameters'])
        relative = f"components/{c['name']}/core-instance.json"
        require(c['instance'] == relative and c['instance_sha256'] == sha(app / relative), 'instance identity changed')
        for path, data in files.items():
            require((app / 'components' / c['name'] / path).read_bytes() == data, 'component declaration/reference drift')
        require(c['source_set_sha256'] == core.read_json(files['core-instance.json'].decode())['source_set_sha256'],
                'component source-set pin differs')


def static_semantics(spec, app):
    """Every static step except the catalogue identity check, which needs current certificates."""
    ref = reference(spec)
    local = module('local_component_reference', LOCAL_REFERENCE, LOCAL_REFERENCE_SHA)
    manifest = core.read_json((app / 'composition.json').read_text())
    journey = list(ref.journey())
    require(all(row['expected'] == ref.decision(row['input']) for row in journey), 'journey disagrees with the reference decision')
    rows = list(example_rows(ref))
    require(len(rows) == spec.INPUT_TUPLES and {tuple(row['input']) for row in rows} == set(itertools.product(*ref.DOMAINS)),
            'declared raw product not covered')
    written = [line for line in (app / 'tests/decision-examples.txt').read_text().splitlines() if line and not line.startswith('#')]
    require(written == [example_line(row) for row in rows], 'written examples differ from the independent reference')
    reproduced = check_rules_binding(core.read_json((app / 'v2/policy.json').read_text()), manifest['state_fields'], rows)
    width = len(spec.STATE_FIELDS)
    accepted, delivering, actions = [], 0, set()
    for row in rows:
        raw, result = row['input'], row['expected']
        if result['class'] == 'Reject':
            require(result['post'] == list(raw[:width]) and not result['deliveries'], 'reject changed state or emitted a delivery')
            continue
        require(ref.valid(raw[:width]) and ref.valid(result['post']), 'accepted from or into an invalid coupled state')
        spec.check_accepted(raw, result)
        accepted.append(row)
        delivering += bool(result['deliveries'])
        actions.add(raw[width])  # the first command field: the action, where there is one
    require((len(accepted), delivering, actions) == spec.ACCEPTED, 'positive command or boundary witnesses changed')
    composition = check_composition(app, manifest, ref, accepted, local)
    check_reviewed_pins(spec, manifest)
    # Each control must be refused by the composition check for its declared
    # reason, or, where the spec declares no reason, pass that check and be
    # refused only by the reviewed pins.
    semantic, pinned = [], []
    for name, mutate, reason in spec.METADATA_CONTROLS:
        changed = copy.deepcopy(manifest)
        mutate(changed)
        try:
            check_composition(app, changed, ref, accepted, local)
        except ValueError as error:
            require(reason is not None and reason in str(error), f'metadata control {name} refused for another reason: {error}')
            semantic.append(name)
            continue
        require(reason is None, f'metadata control survived the composition check: {name}')
        try:
            check_reviewed_pins(spec, changed)
        except ValueError:
            pinned.append(name)
        else:
            raise ValueError(f'metadata control survived: {name}')
    return {'input_tuples': spec.INPUT_TUPLES, 'positive_inputs': len(accepted), 'delivery_inputs': delivering,
            'journey_steps': len(journey), 'rule_binding_reproduced': reproduced, 'composition': composition,
            'semantic_controls': semantic, 'pin_only_controls': pinned, 'actual_authority_run': False,
            'evidence': 'static composition semantics and reviewed pins only'}


def static_check(spec, app):
    result = static_semantics(spec, app)
    check_instance_sources(app, core.read_json((app / 'composition.json').read_text()))
    return result


def complete_table_agrees(spec, report):
    """Rebuild the complete row digest, including destination and idempotency.

    The example grammar alone compares only channel and numeric payload. Use
    the actual Authority's full outbox dictionary to close that narrower view.
    Dictionary indices may differ; each value must have the expected meaning.
    """
    ref = reference(spec)
    table = report['decision_table']
    canonical = lambda value: json.dumps(value, sort_keys=True, separators=(',', ':'))
    posts = {canonical(row['fields']): i for i, row in enumerate(table['post_states'])}
    boxes = {canonical(row['deliveries']): i for i, row in enumerate(table['outboxes'])}
    require(len(posts) == len(table['post_states']) and len(boxes) == len(table['outboxes']), 'duplicate result dictionary entries')
    require(table['count'] == spec.INPUT_TUPLES and not table['refusals'], 'technical refusal or incomplete table')
    expected_posts = set(); expected_boxes = set(); lines = []
    digest = bytes(32)
    for row in all_cases(ref):
        e = row['expected']
        fields = [] if e['class'] == 'Reject' else [
            {'field': field, 'value': bool(value) if field in spec.BOOL_FIELDS else str(value)}
            for field, value in zip(spec.STATE_FIELDS, e['post'], strict=True)]
        deliveries = [{'ordinal': 0, 'channel': channel, 'destination': spec.DESTINATION, 'idempotency': '0',
                       'payload': [{'field': field, 'value': str(value)} for field, value in zip(spec.PAYLOAD_FIELDS, payload, strict=True)]}
                      for channel, payload in e['deliveries']]
        post_key, box_key = canonical(fields), canonical(deliveries)
        expected_posts.add(post_key); expected_boxes.add(box_key)
        require(post_key in posts and box_key in boxes,
                'complete state or outbox differs: destination, ordering, idempotency and payload are all required')
        reason = '-' if e['reason'] is None else str(e['reason'])
        line = f"{' '.join(map(str, row['input']))} | {e['class']} {reason} {posts[post_key]} {boxes[box_key]}"
        digest = hashlib.sha256(digest + (line + '\n').encode()).digest()
        lines.append(line)
    require(set(posts) == expected_posts and set(boxes) == expected_boxes, 'unexpected state or delivery in complete table')
    require(table['rows_digest']['sha256'] == digest.hex(), 'complete actual Authority decision table differs from the reference')
    if not table['rows_omitted']:
        require(table['rows'] == lines, 'inline table differs')
    return digest.hex()


def generate_mutant(cli, app, folder, policy, log, env):
    (folder / 'v2').mkdir(parents=True)
    shutil.copyfile(app / 'project.zeno', folder / 'project.zeno')
    (folder / 'v2/policy.json').write_text(json.dumps(policy, indent=2) + '\n')
    components.run([cli, 'generate', 'contract', folder], ROOT, log, env)
    return sha(folder / 'src/v2_contract.rs')


def prepare_native_controls(spec, cli, app, scaffold, work, env):
    """Emit one native test per law mutant and one for a changed destination.

    Each law test runs one named in-domain witness through the actual typed
    Authority, separate from application decisions.
    """
    root = scaffold / 'tests/native-mutants'
    root.mkdir()
    policy = core.read_json((app / 'v2/policy.json').read_text())
    controls = []; modules = []; tests = []
    for name, changed, inputs, law in spec.law_mutants(policy):
        declaration = generate_mutant(cli, app, root / name, changed, work / f'{name}-generate.log', env)
        identifier = name.replace('-', '_'); test_name = 'law_refuses_' + identifier
        modules.append(f'#[path = "native-mutants/{name}/src/v2_contract.rs"] mod {identifier};')
        raw = ', '.join(map(str, inputs))
        tests.append(f'''#[test]
fn {test_name}() {{
    let contract = {identifier}::Contract::new();
    let definition = contract.descriptor();
    let authority = {identifier}::checked_authority(&definition).expect("mutant admission must succeed");
    let [state, command, context] = application::wire(&definition, &[{raw}]).unwrap();
    let raw = c::Raw {{ state: &state, command: &command, context: &context }};
    let original = application::v2_contract::Contract::new(); let original_definition = original.descriptor();
    let original_authority = application::authority(&original_definition).unwrap();
    assert!(!matches!(original_authority.publish(raw), a::PublicationOutcome::Refused {{ .. }}), "the witness must be admitted by the unchanged contract");
    let evaluated = authority.evaluate(raw);
    assert_eq!(evaluated.outcome().result().err(), Some(c::Failure::Law(l::Failure::Violated)));
    assert!(evaluated.outcome().diagnostics().iter().any(|d| d.id == {law} && d.verdict == l::Verdict::Refused(l::Failure::Violated)));
    assert!(matches!(authority.publish(raw), a::PublicationOutcome::Refused {{ error: a::Refusal::Core(c::Failure::Law(l::Failure::Violated)), .. }}));
}}''')
        controls.append({'name': name, 'test_name': test_name, 'inputs': inputs, 'law_id': law,
                         'generated_declaration_sha256': declaration,
                         'scope': 'one named in-domain witness through actual typed Authority evaluation and publication'})
    # Use the original policy: the destination control changes only destination.
    case, inputs = spec.DESTINATION_CONTROL
    changed = copy.deepcopy(policy)
    changed['cases'][case]['outbox'][0]['destination'] = 'another-' + spec.DESTINATION
    declaration = generate_mutant(cli, app, root / 'wrong-destination', changed, work / 'wrong-destination-generate.log', env)
    modules.append('#[path = "native-mutants/wrong-destination/src/v2_contract.rs"] mod wrong_destination;')
    raw = ', '.join(map(str, inputs))
    tests.append(f'''#[test]
fn changed_destination_is_exposed_by_outbox_comparison() {{
    let original = application::v2_contract::Contract::new(); let definition = original.descriptor();
    let authority = application::authority(&definition).unwrap();
    let [state, command, context] = application::wire(&definition, &[{raw}]).unwrap();
    let raw = c::Raw {{ state: &state, command: &command, context: &context }};
    let a::PublicationOutcome::Commit(good) = authority.publish(raw) else {{ panic!("original must commit") }};
    let expected = Value::text_ascii("{spec.DESTINATION}".to_owned()).unwrap().canonical_bytes().unwrap();
    assert_eq!(good.outbox().len(), 1); assert_eq!(good.outbox()[0].destination(), expected);
    let changed = wrong_destination::Contract::new(); let changed_definition = changed.descriptor();
    let changed_authority = wrong_destination::checked_authority(&changed_definition).unwrap();
    let a::PublicationOutcome::Commit(bad) = changed_authority.publish(raw) else {{ panic!("changed destination must reach comparison") }};
    assert_eq!(bad.outbox().len(), 1); assert_ne!(bad.outbox()[0].destination(), expected);
    assert_eq!(bad.outbox()[0].destination(), Value::text_ascii("another-{spec.DESTINATION}".to_owned()).unwrap().canonical_bytes().unwrap());
}}''')
    controls.append({'name': 'wrong-destination', 'test_name': 'changed_destination_is_exposed_by_outbox_comparison',
                     'inputs': list(inputs), 'killed_by': 'complete independent outbox comparison; local laws alone do not pin destination',
                     'generated_declaration_sha256': declaration})
    helper = '''use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_value::Value;
use zeno_fcis_synthesis::finite::{v2_composition as c, v2_laws as l, v2_authority as a};
'''
    (scaffold / f'tests/{spec.TEST_PREFIX}_faults.rs').write_text('\n'.join(modules) + '\n' + helper + '\n' + '\n\n'.join(tests) + '\n')
    (work / 'native-controls.json').write_text(json.dumps(controls, indent=2) + '\n')
    return controls


def review_every_mutant(spec, cli, app, work, env):
    """F2 over the full domain for each law mutant, then a destination substitution.

    F2's example grammar omits destination and idempotency. The destination
    fault must survive that narrow comparison, then fail the full-table one.
    """
    policy = core.read_json((app / 'v2/policy.json').read_text())
    controls = []
    for name, changed, _, _ in spec.law_mutants(policy):
        specimen = work / 'review-mutants' / name
        shutil.copytree(app, specimen)
        (specimen / 'v2/policy.json').write_text(json.dumps(changed, indent=2) + '\n')
        checked = components.review(cli, specimen, work / f'review-{name}.json', work / f'review-{name}.log', env, mutant=True)
        controls.append({'name': name, 'law_refusal_findings': checked['summary']['law_refusal_findings'],
                         'review_sha256': sha(work / f'review-{name}.json')})
    specimen = work / 'review-mutants/wrong-destination'
    shutil.copytree(app, specimen)
    changed = copy.deepcopy(policy)
    changed['cases'][spec.DESTINATION_CONTROL[0]]['outbox'][0]['destination'] = 'another-' + spec.DESTINATION
    (specimen / 'v2/policy.json').write_text(json.dumps(changed, indent=2) + '\n')
    checked = components.review(cli, specimen, work / 'review-wrong-destination.json', work / 'review-wrong-destination.log', env)
    try:
        complete_table_agrees(spec, checked)
    except ValueError as error:
        require('outbox differs' in str(error), 'destination control failed for an unrelated reason')
        controls.append({'name': 'wrong-destination', 'killed_by': 'complete-Authority-outbox-comparison',
                         'review_sha256': sha(work / 'review-wrong-destination.json')})
    else:
        raise ValueError('destination substitution survived complete table comparison')
    return controls


def require_native_tests(log, names):
    counts = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;', log)
    require(counts and sum(int(row[0]) for row in counts) == len(names)
            and all(not any(map(int, row[1:])) for row in counts), 'native tests were omitted, filtered or ignored')
    for name in names:
        require(f'test {name} ... ok' in log, f'native requirement failed: {name}')


def execute(spec, app, cli, work, target):
    static = static_check(spec, app)
    require(cli.is_file() and ROOT != work and ROOT not in work.parents, 'need a CLI and an external work directory')
    require(Path('/dev/shm') != work and Path('/dev/shm') not in work.parents, 'preserve RAM')
    work.mkdir(parents=True, exist_ok=False)
    env = {**os.environ, 'CARGO_TARGET_DIR': str(target), 'CARGO_BUILD_JOBS': '1', 'CARGO_INCREMENTAL': '0',
           'CARGO_PROFILE_DEV_DEBUG': '0', 'CARGO_PROFILE_TEST_DEBUG': '0', 'RUST_TEST_THREADS': '1', 'CARGO_NET_OFFLINE': 'true'}
    source = lambda: {str(p.relative_to(app)): sha(p) for p in app.rglob('*') if p.is_file()}
    before = source(); cli_sha = sha(cli); run = components.run
    run([cli, 'generate', 'contract', app, '--check'], ROOT, work / 'regenerate.log', env)
    scaffold = work / 'app'
    run([cli, 'new', scaffold, '--contract', app, '--source', ROOT], ROOT, work / 'scaffold.log', env)
    journey = f'tests/{spec.TEST_PREFIX}_journey.rs'
    shutil.copyfile(app / journey, scaffold / journey)
    env['ZENO_FCIS_COMPOSITION_TEST_DIR'] = str(work / 'native-journey')
    packages = {tomllib.loads(p.read_text())['package']['name']: p.parent for p in sorted((ROOT / 'crates').glob('*/Cargo.toml'))}
    version = tomllib.loads((ROOT / 'Cargo.toml').read_text())['workspace']['package']['version']
    binding = applications.check_generated_binding(scaffold, packages, version)
    consumer = tomllib.loads((scaffold / 'Cargo.toml').read_text())['package']
    allowed = {(name, version): path / 'Cargo.toml' for name, path in packages.items()}
    allowed[(consumer['name'], consumer['version'])] = scaffold / 'Cargo.toml'
    graph = applications.resolve_reviewed_graph(scaffold, allowed, env)
    # Planted faults use one named lawful witness each through actual Authority
    # publication. Do not rerun the reviewer's unrelated secondary mutant search.
    controls = prepare_native_controls(spec, cli, app, scaffold, work, env)
    # Compile and run the small journey/fault suite before exhaustive comparison.
    # The complete comparison and native corpus remain mandatory.
    run(['cargo', '+1.97.1', 'test', '--locked', '--offline', '--no-run'], scaffold, work / 'native-build.log', env)
    run(['cargo', '+1.97.1', 'test', '--locked', '--offline', '--test', f'{spec.TEST_PREFIX}_faults',
         '--test', f'{spec.TEST_PREFIX}_journey', '--', '--show-output'], scaffold, work / 'native-controls.log', env)
    focused = (*spec.JOURNEY_TESTS, *(row['test_name'] for row in controls))
    require_native_tests((work / 'native-controls.log').read_text(), focused)
    review = components.review(cli, app, work / 'review.json', work / 'review.log', env)
    require(review['inputs']['count'] == spec.INPUT_TUPLES and review['inputs']['domain_size'] == str(spec.INPUT_TUPLES)
            and review['summary']['examples']['compared'] == spec.INPUT_TUPLES, 'F2 did not check the whole independent corpus')
    table_sha = complete_table_agrees(spec, review)
    review_controls = review_every_mutant(spec, cli, app, work, env) if spec.REVIEW_EVERY_MUTANT else []
    # Every test target: generated corpus and commands, library, journey and faults.
    run(['cargo', '+1.97.1', 'test', '--locked', '--offline', '--', '--show-output'], scaffold, work / 'native.log', env)
    log = (work / 'native.log').read_text()
    require(applications.decision_example_counts(log) == [str(spec.INPUT_TUPLES)], 'native examples incomplete')
    native = (*GENERATED_TESTS, *focused)
    require_native_tests(log, native)
    require(before == source() and sha(cli) == cli_sha, 'qualified source or CLI changed')
    return {'static': static, 'actual_authority_run': True, 'input_tuples': spec.INPUT_TUPLES,
            'review_sha256': sha(work / 'review.json'), 'complete_decision_table_sha256': table_sha,
            'native_controls_log_sha256': sha(work / 'native-controls.log'), 'native_log_sha256': sha(work / 'native.log'),
            'native_passed': len(native), 'binding': binding, 'consumer_lock_sha256': graph['lock_sha256'],
            'law_controls': controls, 'review_law_controls': review_controls,
            'source_sha256': before, 'cli_sha256': cli_sha, 'evidence': 'complete finite application replay and native execution',
            'independent_review': False, 'family_wide_theorem': False, 'release_ready': False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--app', required=True, choices=app_names())
    parser.add_argument('--static', action='store_true')
    for name in ('cli', 'work', 'target'):
        parser.add_argument('--' + name, type=Path)
    args = parser.parse_args()
    spec = load_spec(args.app)
    app = ROOT / 'core-apps' / spec.APP
    if args.static:
        require(not any((args.cli, args.work, args.target)), 'static mode does not execute the CLI')
        result = static_check(spec, app)
    else:
        require(all((args.cli, args.work, args.target)), 'execution needs --cli, --work and --target')
        result = execute(spec, app, args.cli.resolve(), args.work.resolve(), args.target.resolve())
        (args.work / 'result.json').write_text(json.dumps(result, indent=2, sort_keys=True) + '\n')
    print(json.dumps(result, sort_keys=True))


if __name__ == '__main__':
    main()
