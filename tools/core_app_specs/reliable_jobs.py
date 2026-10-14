"""Closed single job: three attempts, fixed backoff threshold four, one intent per start."""
import copy

from check_core_app import require

APP = 'reliable-jobs'
PROFILE = 'single-job-three-attempts-fixed-threshold-four/1'
REFERENCE = 'job_reference.py'
REFERENCE_SHA = '1f1e5b185ca6d7d6cc39e65252cd05727c149e270d77a347ab8e2f359cf0220f'
INPUT_TUPLES = 96000
STATE_FIELDS = (110, 111, 112, 113, 114)
BOOL_FIELDS = (113,)
PAYLOAD_FIELDS = (140, 141)
COORDINATOR = {'owns_fields': [111, 114], 'owner': 'application-rules', 'state': ['phase', 'clock'],
               'assurance': 'included in the complete coupled Authority laws and finite comparison'}
COMPONENTS = (
    ('retry', 'retry-budget', {'C': 3}, [110],
     {'state': ['remaining', 'phase == 3'], 'command': ['action == 152'], 'context': ['authorized']}),
    ('memo', 'idempotency-slot', {'C': 3}, [],
     {'state': ['remaining != 3', 'active', '1'], 'command': ['attempt', '1'], 'context': ['authorized']}),
    ('deadline', 'logical-deadline', {'C': 4}, [112, 113],
     {'state': ['4', 'last', 'reached'], 'command': [], 'context': ['now', 'authorized']}),
)
OCCURRENCES = (
    ('start', 'action == 150 && phase != 1', [('memo', 'Record'), ('retry', 'Attempt')]),
    ('retry-start', 'action == 150 && phase == 1', [('memo', 'Record')]),
    ('deadline', 'action == 150 && phase == 2 && !reached', [('deadline', 'Tick')]),
    ('fail', 'action == 151', [('memo', 'Record')]),
    ('succeed', 'action == 152 && phase == 1', [('memo', 'Record'), ('retry', 'Finish')]),
    ('retry-success', 'action == 152 && phase == 3', [('memo', 'Record')]),
)
EFFECTS = {'owner': 'retry', 'occurrence': 'start', 'channel': 300, 'destination': 'worker',
           'payload': {'140': 'active + 1', '141': '1'}, 'max_per_transition': 1}
DESTINATION = 'worker'
ACCEPTED = (209, 17, {150, 151, 152})  # positive inputs, execution intents, successful actions
TEST_PREFIX = 'job'
JOURNEY_TESTS = ('retry_budget_deadline_and_lost_ack_are_durable',)
# (name, change, reason the composition check gives); None: the change fits
# every accepted row, so only the reviewed pins refuse it.
METADATA_CONTROLS = (
    ('aliased-owner', lambda m: m['components'][0]['owns_fields'].append(111), 'field 111 is owned by both coordinator and retry'),
    ('retry-capacity', lambda m: m['components'][0]['parameters'].update(C=4), None),  # a looser budget also fits
    ('tick-map', lambda m: m['components'][2]['views'].update(context=['0', 'authorized']), 'deadline: deadline refuses its Tick step (203)'),
    ('omitted-coordinator', lambda m: m['coordinator'].update(owns_fields=[114]), 'fields without exactly one owner: [111]'),
    ('duplicate-occurrence', lambda m: m['occurrences'].append(copy.deepcopy(m['occurrences'][0])), 'memo steps twice'),
    ('reordered-start', lambda m: m['occurrences'][0]['ordered_steps'].reverse(), None),  # views read one prestate
)
REVIEW_EVERY_MUTANT = False
DESTINATION_CONTROL = (9, [3, 0, 0, 0, 0, 150, 1, 0, 1])


def check_accepted(raw, result):
    """Coupled facts no component states; composition.json yields every component step."""
    remaining, phase, last, reached, clock, action, attempt, now, auth = raw
    require(auth == 1 and now >= clock, 'accepted without authorization or before the clock')
    delivery = [(300, [3 - result['post'][0], 1])] if result['deliveries'] else []
    require(result['deliveries'] == delivery, 'execution intent differs from the charged attempt')


def law_mutants(policy):
    fixtures = (
        ('refund-failure', 12, [1, 1, 4, 1, 4, 151, 2, 4, 1], 502),
        ('stale-completion', 14, [1, 1, 4, 1, 4, 152, 1, 4, 1], 501),
        ('early-retry', 8, [2, 2, 0, 0, 0, 150, 2, 3, 1], 500),
        ('missed-deadline-tick', 9, [2, 2, 0, 0, 0, 150, 2, 4, 1], 504),
        ('duplicate-intent', 9, [3, 0, 0, 0, 0, 150, 1, 0, 1], 503),
        ('wrong-attempt-intent', 9, [3, 0, 0, 0, 0, 150, 1, 0, 1], 503),
        ('charge-exact-retry', 3, [2, 1, 0, 0, 0, 150, 1, 0, 1], 502),
    )
    for name, case, raw, law in fixtures:
        changed = copy.deepcopy(policy)
        if name == 'refund-failure': changed['cases'][case]['post']['110'] = 'remaining + 1'
        elif name in ('stale-completion', 'early-retry'): changed['cases'][case]['when'] = 'false'
        elif name == 'missed-deadline-tick': changed['cases'][case]['post'].update({'112': 'last', '113': 'reached'})
        elif name == 'duplicate-intent':
            extra = copy.deepcopy(changed['cases'][case]['outbox'][0]); extra.update(ordinal=1, idempotency_ordinal=1)
            changed['cases'][case]['outbox'].append(extra)
        elif name == 'wrong-attempt-intent': changed['cases'][case]['outbox'][0]['payload']['140'] = 2
        else: changed['cases'][case]['post']['110'] = 'remaining - 1'
        yield name, changed, raw, law
