"""Static controls for the shared core-application checker; no CLI execution."""
import copy
import hashlib
import json
import shutil
import tempfile
import unittest
from pathlib import Path

import check_core_app as checks

# Inputs, positive inputs, delivering inputs and journey steps: the first four
# were recorded by the per-application checkers this module replaced, the last
# two by the first runs of their independent references. Then the component
# steps the composition check derives, and the metadata controls it refuses
# against those refused only by the reviewed pins.
EXPECTED = {
    'warehouse-reservation': (14400, 30, 6, 10, 54, 3, 2),
    'expense-approval': (55296, 39, 12, 11, 78, 3, 2),
    'reliable-jobs': (96000, 209, 17, 17, 284, 4, 2),
    'equipment-booking': (97200, 15, 4, 15, 24, 6, 2),
    'api-quota': (63504, 5544, 4221, 10, 9765, 5, 2),
    'document-release': (46656, 1359, 252, 14, 1107, 6, 1),
}
# The accepted inputs on which each declared occurrence fires. In
# reliable-jobs, ten inputs fire both start and deadline.
FIRES = {
    'warehouse-reservation': {'reserve': 6, 'ship': 6, 'retry': 18},
    'expense-approval': {'enqueue': 3, 'vote': 12, 'execute': 12, 'retry': 12},
    'reliable-jobs': {'start': 17, 'retry-start': 48, 'deadline': 10, 'fail': 48, 'succeed': 48, 'retry-success': 48},
    'equipment-booking': {'reserve': 4, 'confirm': 4, 'timeout': 1, 'confirm-retry': 5, 'timeout-retry': 1},
    'api-quota': {'retry': 1323, 'admit': 4221},
    'document-release': {'edit': 36, 'submit': 63, 'approve': 756, 'release': 252, 'retry': 252},
}


def copied_app(name, directory):
    app = Path(directory) / name
    shutil.copytree(checks.ROOT / 'core-apps' / name, app)
    return app


class CoreApplicationTests(unittest.TestCase):
    def assert_recorded_counts(self, name, result):
        self.assertEqual((result['input_tuples'], result['positive_inputs'], result['delivery_inputs'],
                          result['journey_steps'], result['composition']['component_steps'],
                          len(result['semantic_controls']), len(result['pin_only_controls'])), EXPECTED[name])
        self.assertEqual(result['rule_binding_reproduced'], EXPECTED[name][0])
        self.assertEqual(result['composition']['occurrence_fires'], FIRES[name])
        self.assertEqual(result['composition']['inputs_without_occurrence'], 0)
        self.assertFalse(result['actual_authority_run'])

    def test_every_application_passes_with_its_recorded_counts(self):
        self.assertEqual(checks.app_names(), sorted(EXPECTED))
        for name in EXPECTED:
            with self.subTest(name):
                self.assert_recorded_counts(name, checks.static_check(checks.load_spec(name), checks.ROOT / 'core-apps' / name))

    def test_every_step_before_the_catalogue_identity_check_passes(self):
        # Needs no family certificate, so it runs while certificates are stale.
        for name in EXPECTED:
            with self.subTest(name):
                self.assert_recorded_counts(name, checks.static_semantics(checks.load_spec(name), checks.ROOT / 'core-apps' / name))

    def test_the_rules_reproduce_every_example_through_the_names(self):
        spec = checks.load_spec('warehouse-reservation')
        rows = list(checks.example_rows(checks.reference(spec)))
        policy = json.loads((checks.ROOT / 'core-apps' / spec.APP / 'v2/policy.json').read_text())
        self.assertEqual(checks.check_rules_binding(policy, spec.STATE_FIELDS, rows), spec.INPUT_TUPLES)

        def refused(change, error):
            changed = copy.deepcopy(policy)
            change(changed)
            with self.subTest(error), self.assertRaisesRegex(ValueError, error):
                checks.check_rules_binding(changed, spec.STATE_FIELDS, rows)
        # Two names bound to each other's fields: the rules read the wrong positions.
        refused(lambda p: p['variables'].update(quantity=policy['variables']['key'], key=policy['variables']['quantity']),
                'not as the example')
        refused(lambda p: p['variables'].pop('request_key'), "the contract's names do not cover the raw input")
        refused(lambda p: p['cases'][0].update(failure='true'), 'unsupported rule')

    def test_views_are_rule_expressions(self):
        name = lambda values: values.__getitem__
        value = lambda text, **values: checks.evaluate(checks.parse(text), name(values))
        self.assertEqual(value('a || b && c', a=True, b=False, c=False), True)
        self.assertEqual(value('a -> b -> c', a=False, b=False, c=False), True)  # groups to the right
        self.assertEqual(value('10 - 3 - 2'), 5)
        self.assertEqual(value('!a == 0', a=2), False)  # prefix `!` binds tightest
        self.assertEqual(value('-a + 1 - -2', a=3), 0)
        self.assertEqual(value('choose(action == 150, 150, 152)', action=151), 152)
        # A number used as a boolean holds only when it equals 1.
        self.assertEqual((value('choose(2, 5, 6)'), value('!a', a=2), value('a && true', a=1)), (6, True, True))
        for text, error in (('150 if action == 150 else 152', 'unexpected `if`'), ('choose(a, b)', 'with 2 arguments'),
                            ('a * 2', r'unexpected `\*`'), ('div_floor(a, 2)', 'with 2 arguments'),
                            ('a +', 'ends early'), ('a $ b', 'unsupported text'), ('9223372036854775807 + a', '64 bits')):
            with self.subTest(text), self.assertRaisesRegex(ValueError, error):
                value(text, a=1, b=2, action=150)

    def test_the_composition_check_reads_meaning_not_spelling(self):
        spec = checks.load_spec('warehouse-reservation')
        app = checks.ROOT / 'core-apps' / spec.APP
        ref = checks.reference(spec)
        local = checks.module('local_component_reference', checks.LOCAL_REFERENCE, checks.LOCAL_REFERENCE_SHA)
        accepted = [row for row in checks.all_cases(ref) if row['expected']['class'] == 'Accept']
        manifest = json.loads((app / 'composition.json').read_text())
        compose = lambda m: checks.check_composition(app, m, ref, accepted, local)
        result = compose(manifest)
        self.assertEqual((result['component_steps'], result['occurrence_fires'], result['inputs_without_occurrence']),
                         (EXPECTED[spec.APP][4], FIRES[spec.APP], 0))
        # An equivalent mapping means the same: only the reviewed pin refuses it.
        equivalent = copy.deepcopy(manifest)
        equivalent['components'][0]['views']['command'][0] = 'choose(action != 150, 152, 150)'
        compose(equivalent)
        with self.assertRaisesRegex(ValueError, 'parameter, ownership or projection changed'):
            checks.check_reviewed_pins(spec, equivalent)

        def refused(change, error):
            changed = copy.deepcopy(manifest)
            change(changed)
            with self.subTest(error), self.assertRaisesRegex(ValueError, error):
                compose(changed)
        refused(lambda m: m['components'][0]['views']['command'].__setitem__(0, '150 if action == 150 else 152'),
                'unexpected `if`')
        refused(lambda m: m['components'][1]['views']['state'].__setitem__(1, 'remembered_key'), "unknown name 'remembered_key'")
        refused(lambda m: m['occurrences'][0]['ordered_steps'].pop(1), 'pool changed without a step')
        refused(lambda m: (m['components'][1]['owns_fields'].insert(0, 112), m['components'][2]['owns_fields'].clear(),
                           m['stateless_components'].append('phase')), 'memo: its state view does not determine its fields')
        refused(lambda m: m['components'][2]['owns_fields'].clear(), 'fields without exactly one owner')
        refused(lambda m: m['effects']['payload'].update({'140': 'requested + 1'}), 'deliveries differ from the declared effect')
        refused(lambda m: m['occurrences'][0]['ordered_steps'][1].update(command='Release'), 'pool view selects Reserve, not Release')
        # Every accepted input is a declared transition, and every declared one happens.
        refused(lambda m: m['occurrences'].pop(2), '18 accepted inputs fire no occurrence')
        refused(lambda m: m['occurrences'].append({'name': 'never', 'when': 'false', 'ordered_steps': []}),
                'never fires on no accepted input')
        refused(lambda m: m['occurrences'].append({'name': 'reserve', 'when': 'action == 150 && phase == 0', 'ordered_steps': []}),
                'occurrence names repeat')

    def test_metadata_cannot_grant_authority(self):
        for name in EXPECTED:
            with self.subTest(name), tempfile.TemporaryDirectory() as directory:
                app = copied_app(name, directory)
                manifest = json.loads((app / 'composition.json').read_text())
                manifest['owner_adoption'] = True
                (app / 'composition.json').write_text(json.dumps(manifest, indent=2) + '\n')
                with self.assertRaisesRegex(ValueError, 'metadata cannot grant authority'):
                    checks.static_check(checks.load_spec(name), app)

    def test_one_changed_written_example_is_refused(self):
        for name in EXPECTED:
            with self.subTest(name), tempfile.TemporaryDirectory() as directory:
                app = copied_app(name, directory)
                path = app / 'tests/decision-examples.txt'
                lines = path.read_text().splitlines()
                last = max(i for i, line in enumerate(lines) if line and not line.startswith('#'))
                lines[last] = lines[last].replace(' | reject ', ' | accept ', 1) if ' | reject ' in lines[last] \
                    else lines[last].replace(' | accept ', ' | reject ', 1)
                path.write_text('\n'.join(lines) + '\n')
                with self.assertRaisesRegex(ValueError, 'written examples differ'):
                    checks.static_check(checks.load_spec(name), app)

    def test_complete_table_needs_the_declared_destination(self):
        spec = checks.load_spec('warehouse-reservation')
        report = authority_table(spec, spec.DESTINATION)
        self.assertEqual(checks.complete_table_agrees(spec, report), report['decision_table']['rows_digest']['sha256'])
        with self.assertRaisesRegex(ValueError, 'outbox differs'):
            checks.complete_table_agrees(spec, authority_table(spec, 'another-' + spec.DESTINATION))
        truncated = copy.deepcopy(report)
        truncated['decision_table']['count'] -= 1
        with self.assertRaisesRegex(ValueError, 'incomplete table'):
            checks.complete_table_agrees(spec, truncated)


def authority_table(spec, destination):
    """A decision table shaped like the actual Authority's, built from the reference."""
    posts, boxes, rows = {}, {}, []
    digest = bytes(32)
    for row in checks.all_cases(checks.reference(spec)):
        e = row['expected']
        fields = [] if e['class'] == 'Reject' else [{'field': f, 'value': str(v)} for f, v in zip(spec.STATE_FIELDS, e['post'])]
        deliveries = [{'ordinal': 0, 'channel': channel, 'destination': destination, 'idempotency': '0',
                       'payload': [{'field': f, 'value': str(v)} for f, v in zip(spec.PAYLOAD_FIELDS, payload)]}
                      for channel, payload in e['deliveries']]
        post = posts.setdefault(json.dumps(fields, sort_keys=True), len(posts))
        box = boxes.setdefault(json.dumps(deliveries, sort_keys=True), len(boxes))
        reason = '-' if e['reason'] is None else str(e['reason'])
        line = f"{' '.join(map(str, row['input']))} | {e['class']} {reason} {post} {box}"
        digest = hashlib.sha256(digest + (line + '\n').encode()).digest()
        rows.append(line)
    return {'decision_table': {
        'count': len(rows), 'refusals': [], 'rows_omitted': False, 'rows': rows, 'rows_digest': {'sha256': digest.hex()},
        'post_states': [{'fields': json.loads(key)} for key in posts],
        'outboxes': [{'deliveries': json.loads(key)} for key in boxes]}}


if __name__ == '__main__':
    unittest.main()
