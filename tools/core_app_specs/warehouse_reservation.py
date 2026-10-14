"""Closed single-order, capacity-four warehouse composed from pool, memo and phase."""
import copy

from check_core_app import require

APP = 'warehouse-reservation'
PROFILE = 'single-order-capacity-four/1'
REFERENCE = 'warehouse_reference.py'
REFERENCE_SHA = 'b96fa1109c0c7da3b283feca58ee0f5e5fafe8df145995f704b43cf8a320b680'
INPUT_TUPLES = 14400
STATE_FIELDS = (110, 111, 112, 113, 114)
BOOL_FIELDS = ()
PAYLOAD_FIELDS = (140, 141)
COORDINATOR = None
COMPONENTS = (
    ('pool', 'reservation-pool', {'C': 4, 'Q': 3}, [110, 111],
     {'state': ['available', 'held'], 'command': ['choose(action == 150, 150, 152)', 'requested'], 'context': ['authorized']}),
    ('memo', 'idempotency-slot', {'C': 3}, [113, 114],
     {'state': ['phase != 0', 'key', 'quantity'], 'command': ['request_key', 'requested'], 'context': ['authorized']}),
    ('phase', 'finite-phase-machine', {'C': 2}, [112], {'state': ['phase'], 'command': ['false'], 'context': ['authorized']}),
)
OCCURRENCES = (
    ('reserve', 'action == 150 && phase == 0', [('memo', 'Record'), ('pool', 'Reserve'), ('phase', 'Advance')]),
    ('ship', 'action == 151 && phase == 1 && request_key == key && requested == quantity',
     [('memo', 'Record'), ('pool', 'Consume'), ('phase', 'Advance')]),
    ('retry', 'phase != 0 && request_key == key && requested == quantity && (action == 150 || phase == 2)', [('memo', 'Record')]),
)
EFFECTS = {'owner': 'pool', 'occurrence': 'ship', 'channel': 300, 'destination': 'warehouse',
           'payload': {'140': 'quantity', '141': 'key'}, 'max_per_transition': 1}
DESTINATION = 'warehouse'
ACCEPTED = (30, 6, {150, 151})  # positive inputs, shipments, successful actions
TEST_PREFIX = 'warehouse'
JOURNEY_TESTS = ('reservation_shipping_retry_and_lost_ack_are_durable',)
# (name, change, reason the composition check gives); None: the change fits
# every accepted row, so only the reviewed pins refuse it.
METADATA_CONTROLS = (
    ('aliased-owner', lambda m: m['components'][0]['owns_fields'].append(113), 'field 113 is owned by both pool and memo'),
    ('out-of-range-parameter', lambda m: m['components'][0]['parameters'].update(C=5), 'pool: unqualified instance'),
    # Every accepted row is authorized, so a constant true fits them all.
    ('authorization-map', lambda m: m['components'][0]['views'].update(context=['true']), None),
    ('duplicate-occurrence', lambda m: m['occurrences'].append(copy.deepcopy(m['occurrences'][0])), 'memo steps twice'),
    ('reordered-occurrence', lambda m: m['occurrences'][0]['ordered_steps'].reverse(), None),  # views read one prestate
)
# The small domain keeps the earlier full-domain F2 review of every mutant.
REVIEW_EVERY_MUTANT = True
DESTINATION_CONTROL = (4, [1, 3, 1, 3, 0, 151, 3, 0, 1])


def check_accepted(raw, result):
    """Coupled facts no component states; composition.json yields every component step."""
    a, h, p, q, k, action, requested, key, auth = raw
    require(auth == 1, 'accepted without authorization')
    delivery = [(300, [h - result['post'][1], k])] if result['deliveries'] else []
    require(result['deliveries'] == delivery, 'delivery did not match actual consumed holdings and stored key')


def law_mutants(policy):
    """Each witness and law is the first law-consistent refusal F2 reported for that mutant."""
    reserve, ship, retry = 3, 4, 5
    fixtures = (
        ('lost-reservation', [4, 0, 0, 0, 0, 150, 1, 0, 1], 500),
        ('wrong-memo-key', [4, 0, 0, 0, 0, 150, 1, 1, 1], 501),
        ('double-shipment', [1, 3, 1, 3, 0, 151, 3, 0, 1], 503),
        ('wrong-shipment-key', [1, 3, 1, 3, 0, 151, 3, 0, 1], 503),
        ('charge-retry-twice', [2, 0, 2, 2, 0, 150, 2, 0, 1], 500),
    )
    for name, raw, law in fixtures:
        changed = copy.deepcopy(policy)
        if name == 'lost-reservation': changed['cases'][reserve]['post']['111'] = 'held'
        elif name == 'wrong-memo-key': changed['cases'][reserve]['post']['114'] = 0
        elif name == 'double-shipment':
            extra = copy.deepcopy(changed['cases'][ship]['outbox'][0]); extra.update(ordinal=1, idempotency_ordinal=1)
            changed['cases'][ship]['outbox'].append(extra)
        elif name == 'wrong-shipment-key': changed['cases'][ship]['outbox'][0]['payload']['141'] = '1 - key'
        else:
            changed['cases'][retry]['post']['110'] = 'available - requested'
            changed['cases'][retry]['post']['111'] = 'held + requested'
        yield name, changed, raw, law
