"""Closed single-document release: two edits at most, quorum two of three, released once."""
import copy

from check_core_app import require

APP = 'document-release'
PROFILE = 'single-document-two-edits-quorum-two/1'
REFERENCE = 'document_release_reference.py'
REFERENCE_SHA = '2109b623cd3e8610c90f97b50b43ba3ba6e07835d8b998e103a2f0a877176765'
INPUT_TUPLES = 46656
STATE_FIELDS = (110, 111, 112, 113, 114, 115)
BOOL_FIELDS = (113, 114, 115)
PAYLOAD_FIELDS = (140, 141)
COORDINATOR = None
COMPONENTS = (
    ('register', 'versioned-register', {'C': 2}, [110, 111],
     {'state': ['content', 'version'], 'command': ['expected_version', 'new_content'], 'context': ['authorized']}),
    # Submit 161 -> Enqueue 160, Approve 162 -> Vote 161, Release 163 -> Execute 162.
    # Edit 160 would map to 159, outside the queue; no occurrence steps the queue on Edit.
    ('review', 'approval-queue', {'K': 2}, [112, 113, 114, 115],
     {'state': ['phase + 150', 'v0', 'v1', 'v2'], 'command': ['action - 1'], 'context': ['principal', 'authorized']}),
)
OCCURRENCES = (
    ('edit', 'action == 160 && phase == 0', [('register', 'Write')]),
    ('submit', 'action == 161 && phase == 0', [('review', 'Enqueue')]),
    ('approve', 'action == 162 && phase == 1', [('review', 'Vote')]),
    ('release', 'action == 163 && phase == 1', [('review', 'Execute')]),
    ('retry', 'action == 163 && phase == 2 && expected_version == version', []),
)
EFFECTS = {'owner': 'review', 'occurrence': 'release', 'channel': 300, 'destination': 'publisher',
           'payload': {'140': 'content', '141': 'version'}, 'max_per_transition': 1}
DESTINATION = 'publisher'
ACCEPTED = (1359, 252, {160, 161, 162, 163})  # positive inputs, release intents, successful actions
TEST_PREFIX = 'release'
JOURNEY_TESTS = ('review_locks_release_once_and_lost_ack_is_durable',)
# (name, change, reason the composition check gives); None: the change fits
# every accepted row, so only the reviewed pins refuse it.
METADATA_CONTROLS = (
    ('aliased-owner', lambda m: m['components'][0]['owns_fields'].append(112), 'field 112 is owned by both register and review'),
    # The rules refuse version 3 themselves (206), so a C=3 register also fits.
    ('out-of-range-version', lambda m: m['components'][0]['parameters'].update(C=3), None),
    ('unmapped-review-command', lambda m: m['components'][1]['views'].update(command=['action']),
     'submit: review view selects Vote, not Enqueue'),
    ('edit-reaches-review', lambda m: m['occurrences'][0]['ordered_steps'].append({'component': 'review', 'command': 'Enqueue'}),
     'edit: review input [150, 0, 0, 0, 159, 0, 1] outside its instance domain'),
    ('retry-transition', lambda m: m['occurrences'][4]['ordered_steps'].append({'component': 'review', 'command': 'Execute'}),
     'retry: review refuses its Execute step'),
    ('duplicate-occurrence', lambda m: m['occurrences'].append(copy.deepcopy(m['occurrences'][0])), 'register steps twice'),
    ('extra-phase-machine', lambda m: m['components'].append(copy.deepcopy(m['components'][1])),
     'field 112 is owned by both review and review'),
)
REVIEW_EVERY_MUTANT = False
DESTINATION_CONTROL = (11, [2, 2, 1, 1, 1, 0, 163, 2, 0, 0, 1])


def check_accepted(raw, result):
    """Coupled facts no component states; composition.json yields every component step."""
    content, version, phase, v0, v1, v2, action, expected, new_content, principal, auth = raw
    require(auth == 1 and (action == 162 or expected == version), 'accepted without authorization or a current version')
    require(phase == 0 or result['post'][:2] == [content, version], 'locked content or version changed')
    require(phase != 2 or (action == 163 and not result['deliveries']), 'a released document accepted more than its retry')


def law_mutants(policy):
    wrong_phase, stale, quorum, approve, release = 3, 4, 7, 10, 11
    fixtures = (
        ('edit-during-review', wrong_phase, [2, 1, 1, 0, 0, 0, 160, 1, 0, 0, 1], 502),
        ('stale-edit', stale, [1, 1, 0, 0, 0, 0, 160, 0, 2, 0, 1], 501),
        ('unauthorized-edit', 1, [0, 0, 0, 0, 0, 0, 160, 0, 1, 0, 0], 501),
        ('one-vote-release', quorum, [2, 2, 1, 1, 0, 0, 163, 2, 0, 0, 1], 500),
        ('approve-moves-content', approve, [2, 2, 1, 0, 0, 0, 162, 0, 1, 0, 1], 502),
        ('wrong-voter', approve, [2, 2, 1, 1, 0, 0, 162, 0, 0, 1, 1], 504),
        ('release-changes-content', release, [2, 2, 1, 1, 1, 0, 163, 2, 0, 0, 1], 502),
        ('wrong-release-content', release, [2, 2, 1, 1, 1, 0, 163, 2, 0, 0, 1], 503),
        ('duplicate-release', release, [2, 2, 1, 1, 1, 0, 163, 2, 0, 0, 1], 503),
        ('retry-releases-again', 2, [2, 2, 2, 1, 1, 0, 163, 2, 0, 0, 1], 503),
    )
    for name, case, raw, law in fixtures:
        changed = copy.deepcopy(policy)
        target = changed['cases'][case]
        if name == 'edit-during-review':
            target['when'] = 'phase == 2 || (action == 161 && phase != 0) || ((action == 162 || action == 163) && phase != 1)'
        elif name in ('stale-edit', 'unauthorized-edit'): target['when'] = 'false'
        elif name == 'one-vote-release': target['when'] = 'action == 163 && votes < 1'
        elif name == 'approve-moves-content': target['post']['110'] = 'new_content'
        elif name == 'wrong-voter': target['post']['114'] = 'v1'
        elif name == 'release-changes-content': target['post']['110'] = 'new_content'
        elif name == 'wrong-release-content': target['outbox'][0]['payload']['140'] = 'new_content'
        elif name == 'duplicate-release':
            extra = copy.deepcopy(target['outbox'][0]); extra.update(ordinal=1, idempotency_ordinal=1)
            target['outbox'].append(extra)
        else: target['outbox'] = copy.deepcopy(changed['cases'][release]['outbox'])
        yield name, changed, raw, law
