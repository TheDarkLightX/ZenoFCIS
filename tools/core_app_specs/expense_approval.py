"""Closed single-expense approval: budget seven, quorum two, one payment."""
import copy

from check_core_app import require

APP = 'expense-approval'
PROFILE = 'single-expense-budget-seven-quorum-two/1'
REFERENCE = 'expense_reference.py'
REFERENCE_SHA = '807cab10122f7ef6e4a890c6ddeead3038599809f991dd099a3ceb497e99d30c'
INPUT_TUPLES = 55296
STATE_FIELDS = (110, 111, 112, 113, 114)
BOOL_FIELDS = (112, 113, 114)
PAYLOAD_FIELDS = (140, 141)
COORDINATOR = None
COMPONENTS = (
    ('budget', 'consumable-budget', {'C': 7}, [110], {'state': ['remaining'], 'command': ['3'], 'context': ['authorized']}),
    ('queue', 'approval-queue', {'K': 2}, [111, 112, 113, 114],
     {'state': ['phase + 150', 'v0', 'v1', 'v2'], 'command': ['action'], 'context': ['principal', 'authorized']}),
    ('memo', 'idempotency-slot', {'C': 7}, [],
     {'state': ['phase != 0', '1', '3'], 'command': ['key', 'requested'], 'context': ['authorized']}),
)
OCCURRENCES = (
    ('enqueue', 'phase == 0 && action == 160', [('memo', 'Record'), ('queue', 'Enqueue')]),
    ('vote', 'phase == 1 && action == 161', [('memo', 'Record'), ('queue', 'Vote')]),
    ('execute', 'phase == 1 && action == 162', [('memo', 'Record'), ('queue', 'Execute'), ('budget', 'Spend')]),
    ('retry', 'phase == 2 && action == 162', [('memo', 'Record')]),
)
EFFECTS = {'owner': 'budget', 'occurrence': 'execute', 'channel': 300, 'destination': 'bank',
           'payload': {'140': '3', '141': '1'}, 'max_per_transition': 1}
DESTINATION = 'bank'
ACCEPTED = (39, 12, {160, 161, 162})  # positive inputs, payments, successful actions
TEST_PREFIX = 'expense'
JOURNEY_TESTS = ('quorum_payment_retry_and_lost_ack_are_durable',)
# (name, change, reason the composition check gives); None: the change fits
# every accepted row, so only the reviewed pins refuse it.
METADATA_CONTROLS = (
    ('aliased-owner', lambda m: m['components'][0]['owns_fields'].append(111), 'field 111 is owned by both budget and queue'),
    ('out-of-range-quorum', lambda m: m['components'][1]['parameters'].update(K=4), 'queue: unqualified instance'),
    # Every accepted row is authorized, so a constant true fits them all.
    ('authorization-map', lambda m: m['components'][1]['views'].update(context=['principal', 'true']), None),
    ('duplicate-occurrence', lambda m: m['occurrences'].append(copy.deepcopy(m['occurrences'][0])), 'memo steps twice'),
    ('reordered-occurrence', lambda m: m['occurrences'][2]['ordered_steps'].reverse(), None),  # views read one prestate
)
REVIEW_EVERY_MUTANT = False
DESTINATION_CONTROL = (9, [7, 1, 1, 1, 0, 162, 3, 1, 0, 1])


def check_accepted(raw, result):
    """Coupled facts no component states; composition.json yields every component step."""
    remaining, phase, v0, v1, v2, action, requested, key, principal, auth = raw
    require(auth == 1 and (requested, key) == (3, 1), 'accepted without authorization or for another subject')
    delivery = [(300, [remaining - result['post'][0], 1])] if result['deliveries'] else []
    require(result['deliveries'] == delivery, 'payment did not equal first spend and fixed subject')


def law_mutants(policy):
    fixtures = (
        ('one-vote-quorum', [7, 1, 1, 0, 0, 162, 3, 1, 0, 1], 500),
        ('wrong-voter', [7, 1, 0, 0, 0, 161, 3, 1, 1, 1], 502),
        ('double-spend', [7, 1, 1, 1, 0, 162, 3, 1, 0, 1], 500),
        ('wrong-payment', [7, 1, 1, 1, 0, 162, 3, 1, 0, 1], 503),
        ('duplicate-payment', [7, 1, 1, 1, 0, 162, 3, 1, 0, 1], 503),
        ('spending-retry', [4, 2, 1, 1, 0, 162, 3, 1, 0, 1], 500),
    )
    for name, raw, law in fixtures:
        changed = copy.deepcopy(policy)
        if name == 'one-vote-quorum': changed['cases'][6]['when'] = 'action == 162 && votes < 1'
        elif name == 'wrong-voter': changed['cases'][8]['post']['113'] = 'v1'
        elif name == 'double-spend': changed['cases'][9]['post']['110'] = 'remaining - 6'
        elif name == 'wrong-payment': changed['cases'][9]['outbox'][0]['payload']['140'] = 4
        elif name == 'duplicate-payment':
            extra = copy.deepcopy(changed['cases'][9]['outbox'][0]); extra.update(ordinal=1, idempotency_ordinal=1)
            changed['cases'][9]['outbox'].append(extra)
        else: changed['cases'][3]['post']['110'] = 'remaining - 3'
        yield name, changed, raw, law
