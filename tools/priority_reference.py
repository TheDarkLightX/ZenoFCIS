#!/usr/bin/env python3
"""Independent map reference for the closed construction-r2 priority profile."""
from __future__ import annotations

import argparse
import hashlib
import itertools
import json
from pathlib import Path
import sys

REVISION = 'construction-r2'
SCHEMA = 'zeno-fcis/priority-relation/1'
LIMIT = 1024 * 1024


def encoded(value):
    return (json.dumps(value, sort_keys=True, separators=(',', ':'), allow_nan=False) + '\n').encode()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def integer(value, low, high):
    return type(value) is int and low <= value <= high


def state(n, entries=()):
    """Return an immutable mathematical map state, checking its entire carrier."""
    require(integer(n, 0, 2), 'counter is outside the carrier')
    entries = tuple(tuple(entry) for entry in entries)
    require(len(entries) <= 2, 'capacity exceeded')
    require(all(len(entry) == 3 and integer(entry[0], 0, 2) and
                integer(entry[1], 0, 1) and integer(entry[2], 0, 1)
                and entry[2] < n for entry in entries), 'invalid map entry')
    require(tuple(sorted(entries)) == entries, 'entries must be in identity order')
    require(len({x[0] for x in entries}) == len(entries), 'duplicate identity')
    require(len({x[2] for x in entries}) == len(entries), 'duplicate arrival')
    return n, entries


def state_order(value):
    n, entries = value
    contents = {d: 1 + 2 * t + p for d, p, t in entries}
    return n, tuple(contents.get(d, 0) for d in range(3))


def carrier():
    result = []
    for n in range(3):
        for size in range(min(n, 2) + 1):
            for identities in itertools.combinations(range(3), size):
                for arrivals in itertools.permutations(range(n), size):
                    for priorities in itertools.product(range(2), repeat=size):
                        result.append(state(n, zip(identities, priorities, arrivals)))
    result.sort(key=state_order)
    require(len(result) == 45 and len(set(result)) == 45, 'carrier count changed')
    return tuple(result)


def command(q):
    require(integer(q, 0, 15), 'command outside the declared domain')
    if q < 6:
        kind, identity, priority = 'Insert', q // 2, q % 2
    elif q < 9:
        kind, identity, priority = 'Cancel', q - 6, None
    elif q < 15:
        kind, identity, priority = 'Change', (q - 9) // 2, (q - 9) % 2
    else:
        kind, identity, priority = 'RemoveLowest', None, None
    return dict(q=q, kind=kind, id=identity, priority=priority)


def step(value, q):
    """Apply the four operations directly to a private copy of the input map."""
    n, entries = state(*value)
    request = command(q)
    contents = {d: (p, t) for d, p, t in entries}
    kind, identity, priority = request['kind'], request['id'], request['priority']
    reason = None
    if kind == 'Insert':
        if identity in contents:
            reason = 201
        elif len(contents) == 2:
            reason = 202
        elif n == 2:
            reason = 203
    elif kind in ('Cancel', 'Change'):
        if identity not in contents:
            reason = 204
    elif not contents:
        reason = 205
    if reason is not None:
        return dict(successor=(n, entries), **{'class': 'Reject'}, reason=reason,
                    effects=[], outbox=[])
    if kind == 'Insert':
        contents[identity] = priority, n
        event = (0, identity, priority, n)
        n += 1
    elif kind == 'Cancel':
        del contents[identity]
        event = (1, identity, 2, 2)
    elif kind == 'Change':
        contents[identity] = priority, contents[identity][1]
        event = (2, identity, priority, 2)
    else:
        identity = min(contents, key=lambda d: contents[d])
        del contents[identity]
        event = (3, identity, 2, 2)
    successor = state(n, sorted((d, p, t) for d, (p, t) in contents.items()))
    delivery = dict(ordinal=0, channel=300, destination='priority-events', idempotency=0,
                    payload=[[140 + i, value] for i, value in enumerate(event)])
    return dict(successor=successor, **{'class': 'Accept'}, reason=None,
                effects=[], outbox=[delivery])


def relation():
    states = carrier()
    ordinals = {value: index for index, value in enumerate(states)}
    rows = []
    for index, value in enumerate(states):
        for q in range(16):
            result = step(value, q)
            result['successor'] = ordinals[result['successor']]
            rows.append(dict(state=index, input=q, **result))
    return dict(schema=SCHEMA, revision=REVISION, genesis=0,
                states=[dict(n=n, entries=[list(entry) for entry in entries]) for n, entries in states],
                commands=[command(q) for q in range(16)], rows=rows)


def pairs(items):
    result = {}
    for key, value in items:
        require(key not in result, 'duplicate JSON key: ' + key)
        result[key] = value
    return result


def read_json(path):
    with path.open('rb') as stream:
        data = stream.read(LIMIT + 1)
    require(len(data) <= LIMIT, 'reference document exceeds 1 MiB')
    return data, json.loads(data, object_pairs_hook=pairs,
                           parse_constant=lambda _: (_ for _ in ()).throw(ValueError('nonfinite number')))


def keys(value, expected):
    require(type(value) is dict and set(value) == set(expected), 'unknown or missing object field')


def validate(data):
    keys(data, ('schema', 'revision', 'genesis', 'states', 'commands', 'rows'))
    require(data['schema'] == SCHEMA and data['revision'] == REVISION, 'wrong reference version')
    require(type(data['genesis']) is int and data['genesis'] == 0, 'wrong genesis')
    require(type(data['states']) is list and len(data['states']) == 45, 'wrong state count')
    for item in data['states']:
        keys(item, ('n', 'entries'))
        require(type(item['entries']) is list and all(type(x) is list for x in item['entries']), 'entries must be arrays')
        state(item['n'], item['entries'])
    require(type(data['commands']) is list and len(data['commands']) == 16, 'wrong command count')
    for item in data['commands']:
        keys(item, ('q', 'kind', 'id', 'priority'))
        require(integer(item['q'], 0, 15), 'wrong command number')
        require(item['id'] is None or integer(item['id'], 0, 2), 'wrong command identity')
        require(item['priority'] is None or integer(item['priority'], 0, 1), 'wrong requested priority')
        require(item == command(item['q']), 'command meaning differs')
    require(type(data['rows']) is list and len(data['rows']) == 720, 'wrong row count')
    for index, row in enumerate(data['rows']):
        keys(row, ('state', 'input', 'successor', 'class', 'reason', 'effects', 'outbox'))
        require(integer(row['state'], 0, 44) and integer(row['input'], 0, 15), 'wrong row coordinate')
        require((row['state'], row['input']) == divmod(index, 16), 'row reordered, missing or duplicated')
        require(integer(row['successor'], 0, 44), 'wrong successor ordinal')
        require(row['class'] in ('Accept', 'Reject'), 'unsupported decision class')
        require(row['reason'] is None or integer(row['reason'], 201, 205), 'wrong reason kind')
        require(type(row['effects']) is list and row['effects'] == [], 'unexpected effects')
        require(type(row['outbox']) is list, 'outbox must be an array')
        require(len(row['outbox']) == (1 if row['class'] == 'Accept' else 0), 'wrong delivery count')
        for delivery in row['outbox']:
            keys(delivery, ('ordinal', 'channel', 'destination', 'idempotency', 'payload'))
            require(type(delivery['ordinal']) is int and delivery['ordinal'] == 0, 'wrong ordinal')
            require(type(delivery['channel']) is int and delivery['channel'] == 300, 'wrong channel')
            require(type(delivery['idempotency']) is int and delivery['idempotency'] == 0, 'wrong idempotency')
            require(delivery['destination'] == 'priority-events', 'wrong destination')
            require(type(delivery['payload']) is list and len(delivery['payload']) == 4, 'wrong event width')
            for i, pair in enumerate(delivery['payload']):
                require(type(pair) is list and len(pair) == 2 and type(pair[0]) is int
                        and pair[0] == 140 + i and integer(pair[1], 0, 3 if i == 0 else 2), 'wrong event field')
    require(data == relation(), 'reference meaning differs from independent recomputation')


def report(data):
    observations = {encoded({key: row[key] for key in ('class', 'reason', 'effects', 'outbox')})
                    for row in data['rows']}
    require(len(observations) == 29, 'observation count differs')
    return dict(schema='zeno-fcis/priority-reference-report/1', revision=REVISION,
                states=45, commands=16, rows=720, observations=29,
                relation_sha256=hashlib.sha256(encoded(data)).hexdigest(),
                reference_source_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                authority='none', scope='complete declared finite map relation')


def emit(directory):
    data = relation()
    validate(data)
    summary = report(data)
    directory.mkdir(parents=True, exist_ok=False)
    (directory / 'relation.json').write_bytes(encoded(data))
    (directory / 'reference-report.json').write_bytes(encoded(summary))
    (directory / 'source.sha256').write_text(summary['reference_source_sha256'] + '  priority_reference.py\n')
    return summary


def check(directory):
    raw, data = read_json(directory / 'relation.json')
    validate(data)
    require(raw == encoded(relation()), 'retained relation bytes differ')
    raw, summary = read_json(directory / 'reference-report.json')
    require(summary == report(data) and raw == encoded(summary), 'retained reference report differs')
    require((directory / 'source.sha256').read_text() == summary['reference_source_sha256'] + '  priority_reference.py\n', 'reference source pin differs')
    return summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_mutually_exclusive_group(required=True)
    modes.add_argument('--out', type=Path)
    modes.add_argument('--check', type=Path)
    args = parser.parse_args()
    try:
        summary = emit(args.out) if args.out is not None else check(args.check)
    except (ValueError, TypeError, KeyError) as error:
        print('priority reference refused: ' + str(error), file=sys.stderr)
        return 65
    except OSError as error:
        print('priority reference I/O failed: ' + str(error), file=sys.stderr)
        return 3
    print(json.dumps(summary, sort_keys=True))
    return 0


if __name__ == '__main__':
    sys.exit(main())
