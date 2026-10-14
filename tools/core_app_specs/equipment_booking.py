"""Closed single booking: capacity two, fixed deadline four, one confirmation intent."""
import copy

from check_core_app import require

APP = 'equipment-booking'
PROFILE = 'single-booking-capacity-two-fixed-deadline-four/1'
REFERENCE = 'booking_reference.py'
REFERENCE_SHA = 'a9372b561571733f25617a1204c1035e7988cdb59c55a488f35400cba5c8f7f1'
INPUT_TUPLES = 97200
STATE_FIELDS = (110, 111, 112, 113, 114, 115)
BOOL_FIELDS = (115,)
PAYLOAD_FIELDS = (140, 141)
COORDINATOR = {'owns_fields': [], 'owner': 'application-rules',
               'assurance': 'complete coupled state/lifecycle/subject/intent laws and full finite comparison required'}
COMPONENTS = (
    ('pool', 'reservation-pool', {'C': 2, 'Q': 1}, [110, 111],
     {'state': ['available', 'held'], 'command': ['choose(action == 150, 150, 151)', '1'], 'context': ['authorized']}),
    # C=3: the register owns phase, and Timeout writes phase 3.
    ('register', 'versioned-register', {'C': 3}, [112, 113],
     {'state': ['phase', 'version'], 'command': ['expected', 'choose(action == 150, 1, choose(action == 151, 2, 3))'],
      'context': ['authorized']}),
    ('deadline', 'logical-deadline', {'C': 4}, [114, 115],
     {'state': ['4', 'last', 'reached'], 'command': [], 'context': ['now', 'authorized']}),
    ('counter', 'bounded-counter', {'C': 2}, [], {'state': ['version'], 'command': ['150'], 'context': ['authorized']}),
)
OCCURRENCES = (
    ('reserve', 'action == 150 && phase == 0', [('pool', 'Reserve'), ('register', 'Write'), ('counter', 'Inc')]),
    ('confirm', 'action == 151 && phase == 1', [('register', 'Write'), ('counter', 'Inc')]),
    ('timeout', 'action == 152 && phase == 1', [('deadline', 'Tick'), ('pool', 'Release'), ('register', 'Write'), ('counter', 'Inc')]),
    # The winning command's exact retries change nothing, so they take no step.
    ('confirm-retry', 'action == 151 && phase == 2 && expected == 1', []),
    ('timeout-retry', 'action == 152 && phase == 3 && expected == 1', []),
)
EFFECTS = {'owner': 'register', 'occurrence': 'confirm', 'channel': 300, 'destination': 'booking',
           'payload': {'140': 'version + 1', '141': '1'}, 'max_per_transition': 1}
DESTINATION = 'booking'
ACCEPTED = (15, 4, {150, 151, 152})  # positive inputs, confirmation intents, successful actions
TEST_PREFIX = 'booking'
JOURNEY_TESTS = ('booking_journeys_reject_without_writes_and_redeliver_once', 'confirm_and_timeout_candidates_share_one_store_cas')
# (name, change, reason the composition check gives); None: the change fits
# every accepted row, so only the reviewed pins refuse it.
METADATA_CONTROLS = (
    ('aliased-owner', lambda m: m['components'][0]['owns_fields'].append(112), 'field 112 is owned by both pool and register'),
    ('pool-capacity', lambda m: m['components'][0]['parameters'].update(C=3), None),  # a looser pool also fits
    # The qualified C=2 register cannot hold phase 3, which Timeout writes.
    ('register-capacity-two', lambda m: m['components'][1]['parameters'].update(C=2),
     'register: projected successor [3, 2] outside its state domain'),
    ('release-map', lambda m: m['components'][0]['views'].update(command=['action', '1']),
     'timeout: pool view selects Consume, not Release'),
    ('writable-counter', lambda m: m['components'][3]['owns_fields'].append(112), 'field 112 is owned by both register and counter'),
    ('duplicate-occurrence', lambda m: m['occurrences'].append(copy.deepcopy(m['occurrences'][0])), 'pool steps twice'),
    # Timeout after confirmation is always refused, so this occurrence, and the
    # double register step it lists, is never examined row by row.
    ('never-fires', lambda m: m['occurrences'].append(
        {'name': 'late-timeout', 'when': 'action == 152 && phase == 2',
         'ordered_steps': [{'component': 'register', 'command': 'Write'}, {'component': 'register', 'command': 'Write'}]}),
     'late-timeout fires on no accepted input'),
    ('reordered-timeout', lambda m: m['occurrences'][2]['ordered_steps'].reverse(), None),  # views read one prestate
)
REVIEW_EVERY_MUTANT = False
DESTINATION_CONTROL = (13, [1, 1, 1, 1, 0, 0, 151, 1, 3, 1])


def check_accepted(raw, result):
    """Coupled facts no component states; composition.json yields every component step."""
    available, held, version, phase, last, reached, action, expected, now, auth = raw
    require(auth == 1 and now >= last, 'accepted without authorization or before the last tick')
    delivery = [(300, [result['post'][2], 1])] if result['deliveries'] else []
    require(result['deliveries'] == delivery, 'confirmation intent differs from the confirmed version')


def law_mutants(policy):
    fixtures = (
        ('over-reserve', 10, [2, 0, 0, 0, 0, 0, 150, 0, 0, 1], 500),
        ('lost-held-on-confirm', 13, [1, 1, 1, 1, 0, 0, 151, 1, 3, 1], 500),
        ('stale-version', 7, [1, 1, 1, 1, 0, 0, 151, 0, 3, 1], 501),
        ('early-timeout', 15, [1, 1, 1, 1, 0, 0, 152, 1, 3, 1], 500),
        ('late-confirm', 12, [1, 1, 1, 1, 0, 0, 151, 1, 4, 1], 502),
        ('duplicate-intent', 13, [1, 1, 1, 1, 0, 0, 151, 1, 3, 1], 503),
        ('wrong-intent-version', 13, [1, 1, 1, 1, 0, 0, 151, 1, 3, 1], 503),
        ('changed-confirm-retry', 3, [1, 1, 2, 2, 0, 0, 151, 1, 3, 1], 500),
        ('missed-timeout-tick', 16, [1, 1, 1, 1, 0, 0, 152, 1, 4, 1], 500),
    )
    for name, case, raw, law in fixtures:
        changed = copy.deepcopy(policy)
        if name == 'over-reserve': changed['cases'][case]['post']['111'] = 'held + 2'
        elif name == 'lost-held-on-confirm': changed['cases'][case]['post'].update({'110': 2, '111': 0})
        elif name in ('stale-version', 'early-timeout', 'late-confirm'): changed['cases'][case]['when'] = 'false'
        elif name == 'duplicate-intent':
            extra = copy.deepcopy(changed['cases'][case]['outbox'][0]); extra.update(ordinal=1, idempotency_ordinal=1)
            changed['cases'][case]['outbox'].append(extra)
        elif name == 'wrong-intent-version': changed['cases'][case]['outbox'][0]['payload']['140'] = 1
        elif name == 'changed-confirm-retry': changed['cases'][case]['post']['112'] = 1
        else: changed['cases'][case]['post'].update({'114': 'last', '115': 'reached'})
        yield name, changed, raw, law
