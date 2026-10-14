"""Independent executable reference for the closed API quota profile.

At most three distinct new requests per epoch; an exact retry of the most
recent request is answered from memory and never charged twice.
decision() maps an in-domain 9-field row to a contract decision; any
out-of-domain input raises ValueError. Pure: no IO, clock or randomness.
"""

QUOTA = 3
CHANNEL_INTENT = 300

R_AUTH = 200
R_CONFLICT = 201
R_STALE = 202
R_EXHAUSTED = 203
R_PRESTATE = 204

# [window_start, used, seen, last_key, last_payload,
#  request_key, request_payload, epoch, authorized]
DOMAINS = (
    tuple(range(7)),
    tuple(range(4)),
    (0, 1),
    (0, 1, 2),
    (0, 1, 2),
    (0, 1, 2),
    (0, 1, 2),
    tuple(range(7)),
    (0, 1),
)
GENESIS = (0, 0, 0, 0, 0)


def _checked(values, domains):
    if not isinstance(values, (list, tuple)) or len(values) != len(domains):
        raise ValueError("expected a list/tuple of %d fields" % len(domains))
    for value, domain in zip(values, domains):
        if type(value) is not int or value not in domain:
            raise ValueError("field out of domain: %r" % (value,))
    return list(values)


def valid(state):
    """True iff the 5-field coupled state is legal; malformed -> ValueError."""
    window_start, used, seen, last_key, last_payload = _checked(state, DOMAINS[:5])
    if seen == 0:
        return window_start == used == last_key == last_payload == 0
    return 1 <= used <= QUOTA


def _accept(post):
    return {'class': 'Accept', 'reason': None,
            'post': list(post), 'deliveries': []}


def _admit(post, epoch, key, payload):
    result = _accept(post)
    result['deliveries'] = [(CHANNEL_INTENT, [epoch, key, payload])]
    return result


def _reject(pre, reason):
    return {'class': 'Reject', 'reason': reason,
            'post': list(pre), 'deliveries': []}


def decision(values):
    row = _checked(values, DOMAINS)
    pre = row[:5]
    window_start, used, seen, last_key, last_payload = pre
    key, payload, epoch, authorized = row[5:]
    if not valid(pre):
        return _reject(pre, R_PRESTATE)
    if authorized == 0:
        return _reject(pre, R_AUTH)
    if seen and last_key == key and last_payload != payload:
        return _reject(pre, R_CONFLICT)
    if seen and last_key == key and last_payload == payload:
        return _accept(pre)
    if epoch < window_start:
        return _reject(pre, R_STALE)
    if epoch == window_start and used == QUOTA:
        return _reject(pre, R_EXHAUSTED)
    if epoch > window_start:
        window_start, used = epoch, 1
    else:
        used += 1
    return _admit([window_start, used, 1, key, payload], epoch, key, payload)


def cases():
    """Specification witness rows covering every reason and every accept."""
    rows = [
        ([0, 1, 0, 0, 0, 0, 0, 0, 1], _reject([0, 1, 0, 0, 0], R_PRESTATE)),
        ([2, 0, 0, 0, 0, 0, 0, 2, 1], _reject([2, 0, 0, 0, 0], R_PRESTATE)),
        ([0, 0, 1, 0, 0, 1, 1, 0, 1], _reject([0, 0, 1, 0, 0], R_PRESTATE)),
        ([0, 0, 0, 0, 0, 1, 1, 0, 0], _reject([0, 0, 0, 0, 0], R_AUTH)),
        ([0, 1, 1, 1, 1, 1, 2, 0, 0], _reject([0, 1, 1, 1, 1], R_AUTH)),
        ([0, 1, 1, 1, 1, 1, 2, 0, 1], _reject([0, 1, 1, 1, 1], R_CONFLICT)),
        ([3, 2, 1, 0, 0, 0, 1, 1, 1], _reject([3, 2, 1, 0, 0], R_CONFLICT)),
        ([0, 1, 1, 1, 1, 1, 1, 0, 1], _accept([0, 1, 1, 1, 1])),
        ([3, 1, 1, 1, 1, 1, 1, 0, 1], _accept([3, 1, 1, 1, 1])),
        ([6, 3, 1, 2, 2, 2, 2, 6, 1], _accept([6, 3, 1, 2, 2])),
        ([2, 1, 1, 1, 0, 2, 2, 1, 1], _reject([2, 1, 1, 1, 0], R_STALE)),
        ([0, 3, 1, 0, 0, 1, 0, 0, 1], _reject([0, 3, 1, 0, 0], R_EXHAUSTED)),
        ([0, 0, 0, 0, 0, 1, 1, 0, 1], _admit([0, 1, 1, 1, 1], 0, 1, 1)),
        ([0, 1, 1, 1, 1, 2, 0, 0, 1], _admit([0, 2, 1, 2, 0], 0, 2, 0)),
        ([0, 3, 1, 0, 0, 1, 0, 2, 1], _admit([2, 1, 1, 1, 0], 2, 1, 0)),
        ([0, 0, 0, 0, 0, 2, 1, 5, 1], _admit([5, 1, 1, 2, 1], 5, 2, 1)),
    ]
    for row, expected in rows:
        yield {'input': list(row), 'expected': expected}


def journey():
    """Persistent genesis sequence: each input prestate is the prior post."""
    rows = [
        ([0, 0, 0, 0, 0, 1, 1, 0, 0], _reject([0, 0, 0, 0, 0], R_AUTH)),
        ([0, 0, 0, 0, 0, 1, 1, 0, 1], _admit([0, 1, 1, 1, 1], 0, 1, 1)),
        ([0, 1, 1, 1, 1, 1, 1, 0, 1], _accept([0, 1, 1, 1, 1])),
        ([0, 1, 1, 1, 1, 1, 2, 0, 1], _reject([0, 1, 1, 1, 1], R_CONFLICT)),
        ([0, 1, 1, 1, 1, 2, 0, 0, 1], _admit([0, 2, 1, 2, 0], 0, 2, 0)),
        ([0, 2, 1, 2, 0, 0, 0, 0, 1], _admit([0, 3, 1, 0, 0], 0, 0, 0)),
        ([0, 3, 1, 0, 0, 1, 0, 0, 1], _reject([0, 3, 1, 0, 0], R_EXHAUSTED)),
        ([0, 3, 1, 0, 0, 1, 0, 2, 1], _admit([2, 1, 1, 1, 0], 2, 1, 0)),
        ([2, 1, 1, 1, 0, 2, 2, 1, 1], _reject([2, 1, 1, 1, 0], R_STALE)),
        ([2, 1, 1, 1, 0, 1, 0, 2, 1], _accept([2, 1, 1, 1, 0])),
    ]
    for row, expected in rows:
        yield {'input': list(row), 'expected': expected}
