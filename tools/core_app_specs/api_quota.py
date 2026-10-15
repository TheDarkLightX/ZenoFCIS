"""Closed single-client API quota: three new requests per epoch, epochs 0..6, memory of one."""
import copy

from check_core_app import require

APP = 'api-quota'
PROFILE = 'single-client-three-per-epoch-seven-epochs/1'
REFERENCE = 'api_quota_reference.py'
REFERENCE_SHA = 'ecaa342d4f2802991af9c7ea7b7d0b83eef8933fc222a757c8dad4dd021176a3'
INPUT_TUPLES = 63504
STATE_FIELDS = (110, 111, 112, 113, 114)
BOOL_FIELDS = (112,)
PAYLOAD_FIELDS = (140, 141, 142)
COORDINATOR = None
COMPONENTS = (
    # W=1: the supplied epoch is the window number (see the README).
    ('quota', 'rate-limiter', {'W': 1, 'N': 3}, [110, 111],
     {'state': ['window_start', 'used'], 'command': ['150'], 'context': ['epoch', 'authorized']}),
    ('memo', 'idempotency-slot', {'C': 2}, [112, 113, 114],
     {'state': ['seen', 'last_key', 'last_payload'], 'command': ['request_key', 'request_payload'], 'context': ['authorized']}),
)
OCCURRENCES = (
    ('retry', 'seen && last_key == request_key && last_payload == request_payload', [('memo', 'Record')]),
    ('admit', '!(seen && last_key == request_key) && epoch >= window_start', [('memo', 'Record'), ('quota', 'Acquire')]),
)
EFFECTS = {'owner': 'quota', 'occurrence': 'admit', 'channel': 300, 'destination': 'service',
           'payload': {'140': 'epoch', '141': 'request_key', '142': 'request_payload'}, 'max_per_transition': 1}
DESTINATION = 'service'
ACCEPTED = (5544, 4221, {0, 1, 2})  # positive inputs, service intents, request keys of accepted inputs
TEST_PREFIX = 'quota'
JOURNEY_TESTS = ('epoch_quota_retry_and_lost_ack_are_durable',)
# (name, change, reason the composition check gives); None: the change fits
# every accepted row, so only the reviewed pins refuse it.
METADATA_CONTROLS = (
    ('aliased-owner', lambda m: m['components'][0]['owns_fields'].append(112), 'field 112 is owned by both quota and memo'),
    ('wider-window', lambda m: m['components'][0]['parameters'].update(W=2), 'admit: quota successor [0, 1] differs'),
    # Every accepted row is authorized, so a constant true fits them all.
    ('authorization-map', lambda m: m['components'][1]['views'].update(context=['true']), None),
    ('charged-retry', lambda m: m['occurrences'][0]['ordered_steps'].append({'component': 'quota', 'command': 'Acquire'}),
     'retry: quota successor [0, 2] differs'),
    ('duplicate-occurrence', lambda m: m['occurrences'].append(copy.deepcopy(m['occurrences'][0])), 'memo steps twice'),
    ('reordered-admit', lambda m: m['occurrences'][1]['ordered_steps'].reverse(), None),  # views read one prestate
    ('effect-destination', lambda m: m['effects'].update(destination='another-service'), 'differ from the contract outbox'),
)
REVIEW_EVERY_MUTANT = False
DESTINATION_CONTROL = (7, [0, 0, 0, 0, 0, 1, 1, 0, 1])


def check_accepted(raw, result):
    """Coupled facts no component states; composition.json yields every component step."""
    window_start, used, seen, last_key, last_payload, key, payload, epoch, auth = raw
    require(auth == 1, 'accepted without authorization')
    charged = result['post'][:2] != [window_start, used]
    require(bool(result['deliveries']) == charged, 'an intent without a charge, or a charge without an intent')


def law_mutants(policy):
    retry, stale, later, current = 3, 4, 6, 7
    fixtures = (
        ('charged-retry', retry, [0, 1, 1, 1, 1, 1, 1, 0, 1], 502),
        ('retry-emits-intent', retry, [0, 1, 1, 1, 1, 1, 1, 0, 1], 503),
        ('uncharged-admit', current, [0, 1, 1, 1, 1, 2, 0, 0, 1], 502),
        ('no-window-reset', later, [0, 3, 1, 0, 0, 1, 0, 2, 1], 502),
        ('stale-epoch-admitted', stale, [2, 1, 1, 1, 0, 2, 2, 1, 1], 502),
        ('forgotten-request', later, [0, 3, 1, 0, 0, 1, 0, 2, 1], 500),
        ('unrecorded-key', current, [0, 1, 1, 1, 1, 2, 0, 0, 1], 501),
        ('unauthorized-admit', 1, [0, 0, 0, 0, 0, 1, 1, 0, 0], 501),
        ('duplicate-intent', current, [0, 0, 0, 0, 0, 1, 1, 0, 1], 503),
        ('wrong-intent-payload', current, [0, 0, 0, 0, 0, 1, 2, 0, 1], 503),
    )
    for name, case, raw, law in fixtures:
        changed = copy.deepcopy(policy)
        target = changed['cases'][case]
        if name == 'charged-retry': target['post']['111'] = 'used + 1'
        elif name == 'retry-emits-intent': target['outbox'] = copy.deepcopy(changed['cases'][current]['outbox'])
        elif name == 'uncharged-admit': target['post']['111'] = 'used'
        elif name == 'no-window-reset': target['post']['110'] = 'window_start'
        elif name in ('stale-epoch-admitted', 'unauthorized-admit'): target['when'] = 'false'
        elif name == 'forgotten-request': target['post']['112'] = False
        elif name == 'unrecorded-key': target['post']['113'] = 'last_key'
        elif name == 'duplicate-intent':
            extra = copy.deepcopy(target['outbox'][0]); extra.update(ordinal=1, idempotency_ordinal=1)
            target['outbox'].append(extra)
        else: target['outbox'][0]['payload']['142'] = 'request_key'
        yield name, changed, raw, law
