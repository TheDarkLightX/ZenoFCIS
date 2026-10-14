"""Test expectations for a capacity-four, single-order warehouse.

Written from the coupled application requirements before its decision table.
This is a test oracle; the shipped core is data interpreted by ZenoFCIS.
The same author writes the table: independent implementations are useful, but
they are not an independent review or owner adoption.
"""
import itertools

DOMAINS = (range(5), range(5), range(3), range(4), range(2),
           (150, 151), range(1, 4), range(2), (0, 1))
STATE_WIDTH = 5
GENESIS = (4, 0, 0, 0, 0)


def valid(state):
    available, held, phase, quantity, key = state
    if phase == 0:
        return tuple(state) == GENESIS
    if phase == 1:
        return quantity > 0 and held == quantity and available + held == 4
    if phase == 2:
        return quantity > 0 and held == 0 and available + quantity == 4
    return False


def decision(values):
    state = tuple(values[:STATE_WIDTH])
    available, held, phase, quantity, key = state
    action, requested, request_key, authorized = values[STATE_WIDTH:]

    def reject(reason):
        return {"class": "Reject", "reason": reason, "post": list(state), "deliveries": []}

    def accept(post, deliveries=()):
        return {"class": "Accept", "reason": None, "post": list(post), "deliveries": list(deliveries)}

    if not valid(state):
        return reject(204)
    if not authorized:
        return reject(200)
    if phase != 0 and request_key == key and requested != quantity:
        return reject(201)
    if action == 150:
        if phase:
            return accept(state) if (request_key, requested) == (key, quantity) else reject(202)
        return accept((available - requested, held + requested, 1, requested, request_key))
    if action == 151:
        if phase == 0:
            return reject(202)
        if (request_key, requested) != (key, quantity):
            return reject(202)
        if phase == 2:
            return accept(state)
        # One complete shipment intent, in the declared payload field order.
        return accept((available, held - quantity, 2, quantity, key), [(300, [quantity, key])])
    raise ValueError("command outside the declared closed domain")


def cases():
    for values in itertools.product(*DOMAINS):
        yield {"input": list(values), "expected": decision(values)}


JOURNEY = (
    ((151, 3, 1, 1), 202),  # ship before reserving
    ((150, 3, 1, 0), 200),  # supplied authorization false
    ((150, 3, 1, 1), None), # reserve three of four units
    ((150, 3, 1, 1), None), # retry the exact request
    ((150, 2, 1, 1), 201),  # same key, different payload
    ((151, 3, 0, 1), 202),  # another key cannot ship this order
    ((151, 3, 1, 1), None), # one shipment consumes the reservation
    ((151, 3, 1, 1), None), # retry never emits a second intent
    ((150, 3, 1, 1), None), # reserve retry after shipping remains harmless
    ((150, 2, 0, 1), 202),  # this closed profile cannot open a second order
)


def journey():
    state = GENESIS
    for command, reason in JOURNEY:
        values = state + command
        expected = decision(values)
        assert expected['reason'] == reason
        yield {'input': list(values), 'expected': expected}
        state = tuple(expected['post'])
    assert state == (1, 0, 2, 3, 1)


def example_rows():
    """Complete product, ordered so the generated persistent test ships once."""
    first = list(journey())
    used = {tuple(row['input']) for row in first}
    assert len(first) == len(used)
    yield from first
    for row in cases():
        if tuple(row['input']) not in used:
            yield row
