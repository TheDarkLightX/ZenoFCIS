"""Pure reference model: one equipment booking in a capacity-two pool.

State layout: (available, held, version, phase, last, reached).
Pool owns available/held; register owns version/phase; deadline owns
last/reached with fixed absolute due=4. Test reference only: no IO, no
clock, no mutable global state.
"""
from itertools import product

FREE = (2, 0, 0, 0, 0, 0)
RESERVED = (1, 1, 1, 1, 0, 0)
CONFIRMED = (1, 1, 2, 2, 0, 0)
EXPIRED = (2, 0, 2, 3, 4, 1)
LEGAL = frozenset((FREE, RESERVED, CONFIRMED, EXPIRED))

STATE_DOMAINS = (
    range(3),  # available
    range(3),  # held
    range(3),  # version
    range(4),  # phase
    range(5),  # last
    range(2),  # reached
)
DOMAINS = STATE_DOMAINS + (
    (150, 151, 152),  # action: Reserve, Confirm, Timeout
    range(3),         # expected version
    range(5),         # now
    range(2),         # authorized
)

GENESIS = FREE
RESERVE, CONFIRM, TIMEOUT = 150, 151, 152
DUE = 4
INTENT_CHANNEL = 300
INTENT_PAYLOAD = (2, 1)


def _check(values, count, domains):
    if not isinstance(values, (tuple, list)) or len(values) != count:
        raise ValueError("expected tuple/list of %d fields" % count)
    if any(type(v) is not int for v in values):
        raise ValueError("fields must be strict integers")
    row = tuple(values)
    if any(v not in d for v, d in zip(row, domains)):
        raise ValueError("field out of domain")
    return row


def valid(state):
    return _check(state, 6, STATE_DOMAINS) in LEGAL


def _reject(reason, pre):
    return {'class': 'Reject', 'reason': reason,
            'post': list(pre), 'deliveries': []}


def _accept(post, deliveries=()):
    return {'class': 'Accept', 'reason': None, 'post': list(post),
            'deliveries': [(c, list(p)) for c, p in deliveries]}


def _reserve(state, now):
    if state != FREE:
        return _reject(202, state)
    if now >= DUE:
        return _reject(203, state)
    return _accept(RESERVED)


def _confirm(state, now):
    if state != RESERVED:
        return _reject(202, state)
    if now >= DUE:
        return _reject(203, state)
    return _accept(CONFIRMED, ((INTENT_CHANNEL, INTENT_PAYLOAD),))


def _timeout(state, now):
    if state != RESERVED:
        return _reject(202, state)
    if now < DUE:
        return _reject(203, state)
    return _accept(EXPIRED)


def decision(values):
    row = _check(values, 10, DOMAINS)
    state = row[:6]
    action, expected, now, auth = row[6], row[7], row[8], row[9]
    if state not in LEGAL:
        return _reject(204, state)
    if auth == 0:
        return _reject(200, state)
    if now < row[4]:
        return _reject(206, state)
    if action == CONFIRM and state == CONFIRMED:
        return _accept(state) if expected == 1 else _reject(205, state)
    if action == TIMEOUT and state == EXPIRED:
        return _accept(state) if expected == 1 else _reject(205, state)
    if expected != row[2]:
        return _reject(205, state)
    if action == RESERVE:
        return _reserve(state, now)
    if action == CONFIRM:
        return _confirm(state, now)
    return _timeout(state, now)


def cases():
    for row in product(*DOMAINS):
        yield {'input': row, 'expected': decision(row)}


def journey():
    def acc(post, deliveries=()):
        return {'class': 'Accept', 'reason': None, 'post': list(post),
                'deliveries': [(c, list(p)) for c, p in deliveries]}

    def rej(reason, post):
        return {'class': 'Reject', 'reason': reason,
                'post': list(post), 'deliveries': []}

    def row(pre, action, expected, now, auth, result):
        return {'input': pre + (action, expected, now, auth),
                'expected': result}

    return [
        # Sequence A (starts at GENESIS)
        row(FREE, CONFIRM, 0, 0, 1, rej(202, FREE)),
        row(FREE, RESERVE, 0, 0, 0, rej(200, FREE)),
        row(FREE, RESERVE, 0, 0, 1, acc(RESERVED)),
        row(RESERVED, TIMEOUT, 1, 3, 1, rej(203, RESERVED)),
        row(RESERVED, CONFIRM, 1, 3, 1,
            acc(CONFIRMED, ((INTENT_CHANNEL, INTENT_PAYLOAD),))),
        row(CONFIRMED, CONFIRM, 1, 3, 1, acc(CONFIRMED)),
        row(CONFIRMED, TIMEOUT, 1, 4, 1, rej(205, CONFIRMED)),
        row(CONFIRMED, TIMEOUT, 2, 4, 1, rej(202, CONFIRMED)),
        # Sequence B (starts at a separate fresh GENESIS)
        row(FREE, RESERVE, 0, 4, 1, rej(203, FREE)),
        row(FREE, RESERVE, 0, 0, 1, acc(RESERVED)),
        row(RESERVED, CONFIRM, 1, 4, 1, rej(203, RESERVED)),
        row(RESERVED, TIMEOUT, 1, 4, 1, acc(EXPIRED)),
        row(EXPIRED, TIMEOUT, 1, 4, 1, acc(EXPIRED)),
        row(EXPIRED, CONFIRM, 1, 4, 1, rej(205, EXPIRED)),
        row(EXPIRED, TIMEOUT, 1, 3, 1, rej(206, EXPIRED)),
    ]
