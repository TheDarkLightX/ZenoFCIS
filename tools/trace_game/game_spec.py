"""Strict finite game input. This module has no solver or decision authority."""
import itertools
import json
from pathlib import Path

SCHEMA = 'zeno-fcis/game/1'
SCOPE = ('safe on every finite history of the declared game: these actors, '
         "these capabilities, this contract's declared domain. "
         'It does not mean safe against every attack.')


def need(condition, message):
    if not condition:
        raise ValueError(message)


def unique(pairs):
    result = {}
    for key, value in pairs:
        need(key not in result, 'duplicate JSON key: ' + key)
        result[key] = value
    return result


def reject_number(value):
    raise ValueError('integral JSON numbers required: ' + value)


def decode(raw):
    need(len(raw) <= 65536, 'game file exceeds 64 KiB')
    return json.loads(raw, object_pairs_hook=unique, parse_float=reject_number,
                      parse_constant=reject_number)


def validate(game):
    need(type(game) is dict, 'game must be an object')
    required = {'schema', 'contract', 'actors', 'coalition_bound',
                'compromised_actor_bound', 'objective'}
    need(required <= game.keys() <= required | {'guards', 'K', 'observation'}, 'game fields')
    need(game['schema'] == SCHEMA, 'game schema')
    need(type(game['contract']) is str and game['contract'], 'contract directory')
    need(game.get('observation', 'perfect') == 'perfect', 'partial observation unsupported')
    actors = game['actors']
    need(type(actors) is list and 1 <= len(actors) <= 16, 'actor count: 1..16')
    names = set()
    for actor in actors:
        need(type(actor) is dict and set(actor) == {'id', 'compromised', 'command', 'context'}, 'actor fields')
        need(type(actor['id']) is str and actor['id'] and actor['id'] not in names, 'unique actor id')
        names.add(actor['id'])
        need(type(actor['compromised']) is bool, 'compromised flag')
        for part in ('command', 'context'):
            need(type(actor[part]) is dict, 'capability object')
            for name, values in actor[part].items():
                need(name.startswith(part + '.'), 'capability position')
                need(type(values) is list and values and all(type(v) is int for v in values), 'integral capability values')
                need(len(set(values)) == len(values), 'duplicate capability value')
    for field, minimum in (('coalition_bound', 1), ('compromised_actor_bound', 0)):
        need(type(game[field]) is int and minimum <= game[field] <= len(actors), field)
    objective = game['objective']
    need(type(objective) is dict and {'kind', 'formula'} <= objective.keys() <= {'kind', 'formula', 'on'}, 'objective fields')
    need(objective['kind'] in ('safety', 'reachability'), 'unsupported objective')
    formula(objective['formula'])
    on = objective.get('on', ['Accept', 'Reject', 'CommittedFailure', 'Refused', 'Blocked'])
    need(type(on) is list and on and all(type(v) is str for v in on) and len(set(on)) == len(on) and
         set(on) <= {'Accept', 'Reject', 'CommittedFailure', 'Refused', 'Blocked'}, 'objective decision classes')
    guards = game.get('guards', ['true'])
    need(type(guards) is list and 1 <= len(guards) <= 32 and all(type(g) is str for g in guards)
         and len(set(guards)) == len(guards), 'guard candidate set')
    for guard in guards:
        formula(guard)
        need(not any(word in guard for word in ('post.', 'outbox.', 'decision_reason')), 'guards read only pre, command and context')
    if 'K' in game:
        need(type(game['K']) is int and 0 <= game['K'] <= 1000, 'K: 0..1000')
    return game


def formula(value):
    need(type(value) is str and 0 < len(value) <= 8192, 'formula size')
    need(not any(x in value for x in (';', '\n', '\r', '//', '/*', '{', '}')), 'one law expression required')


def coalitions(game):
    return tuple(mask for mask in range(1, 1 << len(game['actors']))
                 if mask.bit_count() <= game['coalition_bound'] and
                 sum(bool(mask & (1 << i)) and a['compromised']
                     for i, a in enumerate(game['actors'])) <= game['compromised_actor_bound'])


def expand(game, positions):
    inputs = [p for p in positions if not p['name'].startswith('pre.')]
    expected = {p['name'] for p in inputs}
    events = []
    for actor, capability in enumerate(game['actors']):
        values = dict(capability['command'], **capability['context'])
        need(set(values) == expected, 'capabilities must specify every command/context position exactly')
        lists = [values[p['name']] for p in inputs]
        count = 1
        for values_ in lists:
            count *= len(values_)
        need(len(events) + count <= 100000, 'event budget')
        events.extend(dict(actor=actor, request=list(row)) for row in itertools.product(*lists))
    return events


def load(path):
    with Path(path).open('rb') as source:
        game = validate(decode(source.read(65537)))
    return game, (Path(path).resolve().parent / game['contract']).resolve()


def description(game, native, guard=0, horizon=None):
    return dict(model='contract', game=game, positions=native['positions'],
                genesis=native['genesis'], coalitions=list(coalitions(game)),
                events=expand(game,native['positions']), guards=game.get('guards',['true']),
                guard=guard, objective=game['objective'], horizon=horizon, empty=False)


def native_numbers(packet):
    """Decode the driver's decimal strings without narrowing signed 128-bit values."""
    packet['genesis']=[int(v) for v in packet['genesis']]
    for row in packet.get('rows',[]):
        row['pre']=[int(v) for v in row['pre']]
        row['post']=[int(v) for v in row['post']]
    return packet
