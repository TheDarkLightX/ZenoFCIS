"""Independent executable reference for the closed document release profile.

One document edited under optimistic versioning, submitted once, approved by
two of three reviewer slots and released exactly once. Submission locks the
content: edits during review are refused. decision() maps an in-domain
11-field row to a contract decision; any out-of-domain input raises
ValueError. Pure: no IO, clock or randomness.
"""

DRAFT, REVIEW, RELEASED = 0, 1, 2
EDIT, SUBMIT, APPROVE, RELEASE = 160, 161, 162, 163
MAX_VERSION, QUORUM = 2, 2
CHANNEL_RELEASE = 300

R_AUTH = 200
R_DUPLICATE = 201
R_PHASE = 202
R_QUORUM = 203
R_PRESTATE = 204
R_STALE = 205
R_LIMIT = 206

# [content, version, phase, vote0, vote1, vote2,
#  action, expected_version, new_content, principal, authorized]
DOMAINS = (
    (0, 1, 2),
    (0, 1, 2),
    (DRAFT, REVIEW, RELEASED),
    (0, 1),
    (0, 1),
    (0, 1),
    (EDIT, SUBMIT, APPROVE, RELEASE),
    (0, 1, 2),
    (0, 1, 2),
    (0, 1, 2),
    (0, 1),
)
GENESIS = (0, 0, DRAFT, 0, 0, 0)


def _checked(values, domains):
    if not isinstance(values, (list, tuple)) or len(values) != len(domains):
        raise ValueError("expected a list/tuple of %d fields" % len(domains))
    for value, domain in zip(values, domains):
        if type(value) is not int or value not in domain:
            raise ValueError("field out of domain: %r" % (value,))
    return list(values)


def valid(state):
    """True iff the 6-field coupled state is legal; malformed -> ValueError."""
    content, version, phase, v0, v1, v2 = _checked(state, DOMAINS[:6])
    votes = v0 + v1 + v2
    return ((version != 0 or content == 0)
            and (phase != DRAFT or votes == 0)
            and (phase != RELEASED or votes >= QUORUM))


def _accept(post):
    return {'class': 'Accept', 'reason': None,
            'post': list(post), 'deliveries': []}


def _release(post):
    result = _accept(post)
    result['deliveries'] = [(CHANNEL_RELEASE, [post[0], post[1]])]
    return result


def _reject(pre, reason):
    return {'class': 'Reject', 'reason': reason,
            'post': list(pre), 'deliveries': []}


def _wrong_phase(phase, action):
    if phase == RELEASED:
        return True
    if action in (EDIT, SUBMIT):
        return phase != DRAFT
    return phase != REVIEW


def _apply(pre, action, new_content, principal):
    content, version, phase = pre[:3]
    votes = list(pre[3:6])
    if action == EDIT:
        return _accept([new_content, version + 1, phase] + votes)
    if action == SUBMIT:
        return _accept([content, version, REVIEW, 0, 0, 0])
    if action == APPROVE:
        votes[principal] = 1
        return _accept([content, version, phase] + votes)
    return _release([content, version, RELEASED] + votes)


def decision(values):
    row = _checked(values, DOMAINS)
    pre = row[:6]
    version, phase, votes = pre[1], pre[2], pre[3:6]
    action, expected, new_content, principal, authorized = row[6:]
    if not valid(pre):
        return _reject(pre, R_PRESTATE)
    if authorized == 0:
        return _reject(pre, R_AUTH)
    if phase == RELEASED and action == RELEASE and expected == version:
        return _accept(pre)
    if _wrong_phase(phase, action):
        return _reject(pre, R_PHASE)
    if action != APPROVE and expected != version:
        return _reject(pre, R_STALE)
    if action == EDIT and version == MAX_VERSION:
        return _reject(pre, R_LIMIT)
    if action == APPROVE and votes[principal] == 1:
        return _reject(pre, R_DUPLICATE)
    if action == RELEASE and sum(votes) < QUORUM:
        return _reject(pre, R_QUORUM)
    return _apply(pre, action, new_content, principal)


def cases():
    """Specification witness rows covering every reason and every accept."""
    rows = [
        ([1, 0, 0, 0, 0, 0, EDIT, 0, 1, 0, 1], _reject([1, 0, 0, 0, 0, 0], R_PRESTATE)),
        ([0, 0, 0, 1, 0, 0, SUBMIT, 0, 0, 0, 1], _reject([0, 0, 0, 1, 0, 0], R_PRESTATE)),
        ([2, 2, 2, 1, 0, 0, RELEASE, 2, 0, 0, 1], _reject([2, 2, 2, 1, 0, 0], R_PRESTATE)),
        ([0, 0, 0, 0, 0, 0, EDIT, 0, 1, 0, 0], _reject([0, 0, 0, 0, 0, 0], R_AUTH)),
        ([2, 2, 2, 1, 1, 0, RELEASE, 2, 0, 0, 0], _reject([2, 2, 2, 1, 1, 0], R_AUTH)),
        ([2, 2, 2, 1, 1, 0, RELEASE, 2, 1, 2, 1], _accept([2, 2, 2, 1, 1, 0])),
        ([2, 2, 2, 1, 1, 0, RELEASE, 1, 0, 0, 1], _reject([2, 2, 2, 1, 1, 0], R_PHASE)),
        ([2, 2, 2, 1, 1, 0, EDIT, 2, 0, 0, 1], _reject([2, 2, 2, 1, 1, 0], R_PHASE)),
        ([2, 2, 2, 1, 1, 0, APPROVE, 2, 0, 2, 1], _reject([2, 2, 2, 1, 1, 0], R_PHASE)),
        ([1, 1, 1, 0, 0, 0, EDIT, 1, 2, 0, 1], _reject([1, 1, 1, 0, 0, 0], R_PHASE)),
        ([1, 1, 1, 0, 0, 0, SUBMIT, 1, 0, 0, 1], _reject([1, 1, 1, 0, 0, 0], R_PHASE)),
        ([1, 1, 0, 0, 0, 0, APPROVE, 1, 0, 0, 1], _reject([1, 1, 0, 0, 0, 0], R_PHASE)),
        ([1, 1, 0, 0, 0, 0, RELEASE, 1, 0, 0, 1], _reject([1, 1, 0, 0, 0, 0], R_PHASE)),
        ([1, 1, 0, 0, 0, 0, EDIT, 0, 2, 0, 1], _reject([1, 1, 0, 0, 0, 0], R_STALE)),
        ([1, 1, 0, 0, 0, 0, SUBMIT, 2, 0, 0, 1], _reject([1, 1, 0, 0, 0, 0], R_STALE)),
        ([1, 1, 1, 1, 1, 0, RELEASE, 0, 0, 0, 1], _reject([1, 1, 1, 1, 1, 0], R_STALE)),
        ([1, 2, 0, 0, 0, 0, EDIT, 2, 0, 0, 1], _reject([1, 2, 0, 0, 0, 0], R_LIMIT)),
        ([1, 1, 1, 0, 1, 0, APPROVE, 0, 0, 1, 1], _reject([1, 1, 1, 0, 1, 0], R_DUPLICATE)),
        ([1, 1, 1, 0, 1, 0, RELEASE, 1, 0, 0, 1], _reject([1, 1, 1, 0, 1, 0], R_QUORUM)),
        ([1, 1, 1, 0, 0, 0, RELEASE, 1, 0, 0, 1], _reject([1, 1, 1, 0, 0, 0], R_QUORUM)),
        ([0, 0, 0, 0, 0, 0, EDIT, 0, 0, 0, 1], _accept([0, 1, 0, 0, 0, 0])),
        ([1, 1, 0, 0, 0, 0, EDIT, 1, 2, 1, 1], _accept([2, 2, 0, 0, 0, 0])),
        ([0, 0, 0, 0, 0, 0, SUBMIT, 0, 2, 0, 1], _accept([0, 0, 1, 0, 0, 0])),
        ([1, 1, 1, 0, 1, 0, APPROVE, 0, 2, 2, 1], _accept([1, 1, 1, 0, 1, 1])),
        ([1, 1, 1, 1, 0, 1, RELEASE, 1, 2, 2, 1], _release([1, 1, 2, 1, 0, 1])),
    ]
    for row, expected in rows:
        yield {'input': list(row), 'expected': expected}


def journey():
    """Persistent genesis sequence: each input prestate is the prior post."""
    rows = [
        ([0, 0, 0, 0, 0, 0, EDIT, 0, 1, 0, 0], _reject([0, 0, 0, 0, 0, 0], R_AUTH)),
        ([0, 0, 0, 0, 0, 0, EDIT, 0, 1, 0, 1], _accept([1, 1, 0, 0, 0, 0])),
        ([1, 1, 0, 0, 0, 0, EDIT, 0, 2, 0, 1], _reject([1, 1, 0, 0, 0, 0], R_STALE)),
        ([1, 1, 0, 0, 0, 0, EDIT, 1, 2, 0, 1], _accept([2, 2, 0, 0, 0, 0])),
        ([2, 2, 0, 0, 0, 0, SUBMIT, 2, 0, 0, 1], _accept([2, 2, 1, 0, 0, 0])),
        ([2, 2, 1, 0, 0, 0, EDIT, 2, 1, 0, 1], _reject([2, 2, 1, 0, 0, 0], R_PHASE)),
        ([2, 2, 1, 0, 0, 0, APPROVE, 0, 0, 0, 1], _accept([2, 2, 1, 1, 0, 0])),
        ([2, 2, 1, 1, 0, 0, APPROVE, 0, 0, 0, 1], _reject([2, 2, 1, 1, 0, 0], R_DUPLICATE)),
        ([2, 2, 1, 1, 0, 0, RELEASE, 2, 0, 0, 1], _reject([2, 2, 1, 1, 0, 0], R_QUORUM)),
        ([2, 2, 1, 1, 0, 0, APPROVE, 0, 0, 1, 1], _accept([2, 2, 1, 1, 1, 0])),
        ([2, 2, 1, 1, 1, 0, RELEASE, 1, 0, 0, 1], _reject([2, 2, 1, 1, 1, 0], R_STALE)),
        ([2, 2, 1, 1, 1, 0, RELEASE, 2, 0, 0, 1], _release([2, 2, 2, 1, 1, 0])),
        ([2, 2, 2, 1, 1, 0, RELEASE, 2, 0, 0, 1], _accept([2, 2, 2, 1, 1, 0])),
        ([2, 2, 2, 1, 1, 0, EDIT, 2, 0, 0, 1], _reject([2, 2, 2, 1, 1, 0], R_PHASE)),
    ]
    for row, expected in rows:
        yield {'input': list(row), 'expected': expected}
