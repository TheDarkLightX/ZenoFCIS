"""Pure test reference for the reliable-job bounded requirements table.

Test reference only: no IO, no retained mutable state, no Authority,
proof or settlement claims. Root independently re-derives every row.
"""

from itertools import product

IDLE, RUNNING, WAITING, DONE = range(4)
START, FAIL, SUCCEED = 150, 151, 152
BACKOFF = 4
CHANNEL = 300

# Field order: remaining, phase, deadline_last, reached, clock,
#              action, attempt, now, authorized.
DOMAINS = (
    tuple(range(4)),
    (IDLE, RUNNING, WAITING, DONE),
    tuple(range(5)),
    tuple(range(2)),
    tuple(range(5)),
    (START, FAIL, SUCCEED),
    tuple(range(4)),
    tuple(range(5)),
    tuple(range(2)),
)
GENESIS = (3, IDLE, 0, 0, 0)
STATE_WIDTH = 5
INPUT_WIDTH = 9


def _coerce(values, width):
    if not isinstance(values, (tuple, list)) or len(values) != width:
        raise ValueError("malformed input")
    if any(type(v) is not int for v in values):
        raise ValueError("non-strict-int field")
    return tuple(values)


def _in_domain(values, domains) -> bool:
    return all(v in d for v, d in zip(values, domains))


def _legal(s) -> bool:
    remaining, phase, last, reached, clock = s
    if (reached, last) not in ((0, 0), (1, BACKOFF)):
        return False
    if clock < last:
        return False
    if phase == IDLE:
        return s == GENESIS
    return remaining <= 2


def valid(state) -> bool:
    s = _coerce(state, STATE_WIDTH)
    if not _in_domain(s, DOMAINS[:STATE_WIDTH]):
        raise ValueError("state out of domain")
    return _legal(s)


def _reject(pre, reason) -> dict:
    return {"class": "Reject", "reason": reason,
            "post": list(pre), "deliveries": []}


def _accept(post, deliveries) -> dict:
    return {"class": "Accept", "reason": None,
            "post": list(post), "deliveries": list(deliveries)}


def _blocked(phase, reached, now) -> bool:
    return phase == WAITING and reached == 0 and now < BACKOFF


def _stamped(phase, last, reached):
    if phase == WAITING and reached == 0:
        return BACKOFF, 1
    return last, reached


def _start(pre, attempt, now) -> dict:
    remaining, phase = pre[0], pre[1]
    if phase == RUNNING:
        if attempt == 3 - remaining:
            return _accept(pre, [])
        return _reject(pre, 205)
    if phase == DONE:
        return _reject(pre, 202)
    return _charge(pre, attempt, now)


def _charge(pre, attempt, now) -> dict:
    remaining, phase, last, reached, _ = pre
    if remaining == 0:
        return _reject(pre, 201)
    if attempt != 4 - remaining:
        return _reject(pre, 205)
    if _blocked(phase, reached, now):
        return _reject(pre, 203)
    last, reached = _stamped(phase, last, reached)
    post = [remaining - 1, RUNNING, last, reached, now]
    return _accept(post, [(CHANNEL, [4 - remaining, 1])])


def _fail(pre, attempt, now) -> dict:
    remaining, phase, last, reached, _ = pre
    if phase != RUNNING:
        return _reject(pre, 202)
    if attempt != 3 - remaining:
        return _reject(pre, 205)
    return _accept([remaining, WAITING, last, reached, now], [])


def _succeed(pre, attempt, now) -> dict:
    remaining, phase, last, reached, _ = pre
    if phase not in (RUNNING, DONE):
        return _reject(pre, 202)
    if attempt != 3 - remaining:
        return _reject(pre, 205)
    if phase == DONE:
        return _accept(pre, [])
    return _accept([remaining, DONE, last, reached, now], [])


def decision(values) -> dict:
    v = _coerce(values, INPUT_WIDTH)
    if not _in_domain(v, DOMAINS):
        raise ValueError("input out of domain")
    pre = v[:STATE_WIDTH]
    action, attempt, now, authorized = v[STATE_WIDTH:]
    if not _legal(pre):
        return _reject(pre, 204)
    if authorized == 0:
        return _reject(pre, 200)
    if now < pre[4]:
        return _reject(pre, 206)
    if action == START:
        return _start(pre, attempt, now)
    if action == FAIL:
        return _fail(pre, attempt, now)
    return _succeed(pre, attempt, now)


def cases():
    for values in product(*DOMAINS):
        yield {"input": values, "expected": decision(values)}


# Persistent sequences: each row's state is the previous row's post;
# journey two restarts at genesis. Duplicate raw rows are retained.
_JOURNEY_INPUTS = (
    (3, 0, 0, 0, 0, SUCCEED, 0, 0, 1),  # stale completion at genesis
    (3, 0, 0, 0, 0, START, 1, 0, 0),   # unauthorized Start1
    (3, 0, 0, 0, 0, START, 1, 0, 1),   # Start1 now0
    (2, 1, 0, 0, 0, START, 1, 0, 1),   # exact Start1 retry
    (2, 1, 0, 0, 0, FAIL, 1, 0, 1),    # Fail1 now0
    (2, 2, 0, 0, 0, START, 2, 3, 1),   # Start2 now3 early
    (2, 2, 0, 0, 0, START, 2, 4, 1),   # Start2 now4
    (1, 1, 4, 1, 4, SUCCEED, 1, 4, 1),  # stale Succeed1
    (1, 1, 4, 1, 4, SUCCEED, 2, 4, 1),  # Succeed2
    (1, 3, 4, 1, 4, SUCCEED, 2, 4, 1),  # matching Succeed2 retry
    (3, 0, 0, 0, 0, START, 1, 0, 1),   # journey two: Start1
    (2, 1, 0, 0, 0, FAIL, 1, 0, 1),    # Fail1
    (2, 2, 0, 0, 0, START, 2, 4, 1),   # Start2 now4
    (1, 1, 4, 1, 4, FAIL, 2, 4, 1),    # Fail2 now4
    (1, 2, 4, 1, 4, START, 3, 4, 1),   # Start3 now4, last budget slot
    (0, 1, 4, 1, 4, FAIL, 3, 4, 1),    # Fail3 now4
    (0, 2, 4, 1, 4, START, 3, 4, 1),   # Start3, budget exhausted
)


_JOURNEY_EXPECTED = (
    ('Reject', 202, (3, 0, 0, 0, 0), ()),
    ('Reject', 200, (3, 0, 0, 0, 0), ()),
    ('Accept', None, (2, 1, 0, 0, 0), ((300, (1, 1)),)),
    ('Accept', None, (2, 1, 0, 0, 0), ()),
    ('Accept', None, (2, 2, 0, 0, 0), ()),
    ('Reject', 203, (2, 2, 0, 0, 0), ()),
    ('Accept', None, (1, 1, 4, 1, 4), ((300, (2, 1)),)),
    ('Reject', 205, (1, 1, 4, 1, 4), ()),
    ('Accept', None, (1, 3, 4, 1, 4), ()),
    ('Accept', None, (1, 3, 4, 1, 4), ()),
    ('Accept', None, (2, 1, 0, 0, 0), ((300, (1, 1)),)),
    ('Accept', None, (2, 2, 0, 0, 0), ()),
    ('Accept', None, (1, 1, 4, 1, 4), ((300, (2, 1)),)),
    ('Accept', None, (1, 2, 4, 1, 4), ()),
    ('Accept', None, (0, 1, 4, 1, 4), ((300, (3, 1)),)),
    ('Accept', None, (0, 2, 4, 1, 4), ()),
    ('Reject', 201, (0, 2, 4, 1, 4), ()),
)


def journey():
    for values, (kind, reason, post, deliveries) in zip(_JOURNEY_INPUTS, _JOURNEY_EXPECTED, strict=True):
        yield {"input": values, "expected": {"class": kind, "reason": reason,
               "post": list(post), "deliveries": [(channel, list(payload)) for channel, payload in deliveries]}}
