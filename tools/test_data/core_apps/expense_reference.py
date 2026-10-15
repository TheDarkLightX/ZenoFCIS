"""Independent executable reference for the closed expense-approval profile.

One bounded request: amount 3, key 1, budget 7, quorum 2 of 3 principals.
decision() maps an in-domain 10-field row to a contract decision; any
out-of-domain input raises ValueError. Pure: no IO, clock or randomness.
"""

EMPTY, PENDING, EXECUTED = 0, 1, 2
ENQ, VOT, EXE = 160, 161, 162
AMOUNT, KEY, BUDGET, SPENT_REMAINING, QUORUM = 3, 1, 7, 4, 2
CHANNEL_PAYMENT = 300

R_AUTH = 200
R_DUPLICATE = 201
R_PHASE = 202
R_QUORUM = 203
R_PRESTATE = 204
R_SUBJECT = 205

DOMAINS = (
    tuple(range(8)),
    (EMPTY, PENDING, EXECUTED),
    (0, 1),
    (0, 1),
    (0, 1),
    (ENQ, VOT, EXE),
    tuple(range(8)),
    (0, 1),
    (0, 1, 2),
    (0, 1),
)
GENESIS = (BUDGET, EMPTY, 0, 0, 0)


def _checked(values, domains):
    if not isinstance(values, (list, tuple)) or len(values) != len(domains):
        raise ValueError("expected a list/tuple of %d fields" % len(domains))
    for value, domain in zip(values, domains):
        if type(value) is not int or value not in domain:
            raise ValueError("field out of domain: %r" % (value,))
    return list(values)


def valid(state):
    """True iff the 5-field coupled state is legal; malformed -> ValueError."""
    rem, phase, v0, v1, v2 = _checked(state, DOMAINS[:5])
    if phase == EMPTY:
        return rem == BUDGET and v0 == v1 == v2 == 0
    if phase == PENDING:
        return rem == BUDGET
    return rem == SPENT_REMAINING and v0 + v1 + v2 >= QUORUM


def _accept(post):
    return {'class': 'Accept', 'reason': None,
            'post': list(post), 'deliveries': []}


def _pay(post):
    result = _accept(post)
    result['deliveries'] = [(CHANNEL_PAYMENT, [AMOUNT, KEY])]
    return result


def _reject(pre, reason):
    return {'class': 'Reject', 'reason': reason,
            'post': list(pre), 'deliveries': []}


def _vote(pre, principal):
    if pre[1] != PENDING:
        return _reject(pre, R_PHASE)
    if pre[2 + principal] != 0:
        return _reject(pre, R_DUPLICATE)
    post = list(pre)
    post[2 + principal] = 1
    return _accept(post)


def _execute(pre):
    if pre[1] != PENDING:
        return _reject(pre, R_PHASE)
    if sum(pre[2:5]) < QUORUM:
        return _reject(pre, R_QUORUM)
    return _pay([SPENT_REMAINING, EXECUTED] + list(pre[2:5]))


def _dispatch(pre, action, principal):
    if pre[1] == EXECUTED:
        return _accept(pre) if action == EXE else _reject(pre, R_PHASE)
    if action == ENQ:
        if pre[1] != EMPTY:
            return _reject(pre, R_PHASE)
        return _accept([BUDGET, PENDING, 0, 0, 0])
    if action == VOT:
        return _vote(pre, principal)
    return _execute(pre)


def decision(values):
    row = _checked(values, DOMAINS)
    pre = row[:5]
    action, requested, key, principal, authorized = row[5:]
    if not valid(pre):
        return _reject(pre, R_PRESTATE)
    if authorized == 0:
        return _reject(pre, R_AUTH)
    if requested != AMOUNT or key != KEY:
        return _reject(pre, R_SUBJECT)
    return _dispatch(pre, action, principal)


def cases():
    """Specification witness rows covering every reason and every accept."""
    rows = [
        ([5, 0, 0, 0, 0, ENQ, 4, 0, 0, 0], _reject([5, 0, 0, 0, 0], R_PRESTATE)),
        ([4, 2, 1, 0, 0, EXE, 3, 1, 0, 1], _reject([4, 2, 1, 0, 0], R_PRESTATE)),
        ([7, 0, 0, 0, 0, ENQ, 4, 0, 0, 0], _reject([7, 0, 0, 0, 0], R_AUTH)),
        ([4, 2, 1, 1, 0, EXE, 3, 1, 0, 0], _reject([4, 2, 1, 1, 0], R_AUTH)),
        ([7, 0, 0, 0, 0, EXE, 4, 1, 0, 1], _reject([7, 0, 0, 0, 0], R_SUBJECT)),
        ([7, 1, 1, 0, 0, VOT, 3, 0, 1, 1], _reject([7, 1, 1, 0, 0], R_SUBJECT)),
        ([7, 0, 0, 0, 0, ENQ, 3, 1, 0, 1], _accept([7, 1, 0, 0, 0])),
        ([7, 1, 0, 0, 0, ENQ, 3, 1, 0, 1], _reject([7, 1, 0, 0, 0], R_PHASE)),
        ([4, 2, 1, 1, 0, ENQ, 3, 1, 0, 1], _reject([4, 2, 1, 1, 0], R_PHASE)),
        ([7, 0, 0, 0, 0, VOT, 3, 1, 0, 1], _reject([7, 0, 0, 0, 0], R_PHASE)),
        ([4, 2, 1, 1, 0, VOT, 3, 1, 2, 1], _reject([4, 2, 1, 1, 0], R_PHASE)),
        ([7, 1, 0, 0, 0, VOT, 3, 1, 1, 1], _accept([7, 1, 0, 1, 0])),
        ([7, 1, 1, 0, 0, VOT, 3, 1, 0, 1], _reject([7, 1, 1, 0, 0], R_DUPLICATE)),
        ([7, 0, 0, 0, 0, EXE, 3, 1, 0, 1], _reject([7, 0, 0, 0, 0], R_PHASE)),
        ([7, 1, 0, 0, 0, EXE, 3, 1, 0, 1], _reject([7, 1, 0, 0, 0], R_QUORUM)),
        ([7, 1, 1, 0, 0, EXE, 3, 1, 0, 1], _reject([7, 1, 1, 0, 0], R_QUORUM)),
        ([7, 1, 1, 1, 0, EXE, 3, 1, 0, 1], _pay([4, 2, 1, 1, 0])),
        ([4, 2, 1, 1, 0, EXE, 3, 1, 0, 1], _accept([4, 2, 1, 1, 0])),
        ([4, 2, 1, 1, 0, EXE, 3, 0, 0, 1], _reject([4, 2, 1, 1, 0], R_SUBJECT)),
    ]
    for row, expected in rows:
        yield {'input': list(row), 'expected': expected}


def journey():
    """Persistent genesis sequence: each input prestate is the prior post."""
    rows = [
        ([7, 0, 0, 0, 0, EXE, 3, 1, 0, 1], _reject([7, 0, 0, 0, 0], R_PHASE)),
        ([7, 0, 0, 0, 0, ENQ, 3, 1, 0, 0], _reject([7, 0, 0, 0, 0], R_AUTH)),
        ([7, 0, 0, 0, 0, ENQ, 3, 1, 0, 1], _accept([7, 1, 0, 0, 0])),
        ([7, 1, 0, 0, 0, VOT, 3, 1, 0, 1], _accept([7, 1, 1, 0, 0])),
        ([7, 1, 1, 0, 0, VOT, 3, 1, 0, 1], _reject([7, 1, 1, 0, 0], R_DUPLICATE)),
        ([7, 1, 1, 0, 0, EXE, 3, 1, 0, 1], _reject([7, 1, 1, 0, 0], R_QUORUM)),
        ([7, 1, 1, 0, 0, VOT, 3, 1, 1, 1], _accept([7, 1, 1, 1, 0])),
        ([7, 1, 1, 1, 0, EXE, 4, 1, 0, 1], _reject([7, 1, 1, 1, 0], R_SUBJECT)),
        ([7, 1, 1, 1, 0, EXE, 3, 1, 0, 1], _pay([4, 2, 1, 1, 0])),
        ([4, 2, 1, 1, 0, EXE, 3, 1, 0, 1], _accept([4, 2, 1, 1, 0])),
        ([4, 2, 1, 1, 0, EXE, 3, 0, 0, 1], _reject([4, 2, 1, 1, 0], R_SUBJECT)),
    ]
    for row, expected in rows:
        yield {'input': list(row), 'expected': expected}
