"""Planted false-success controls for the live operational journey gate."""

import copy
import json
import unittest

import check_operational_journey as journey


class PlantedControls(unittest.TestCase):
    def test_a_silently_skipped_step_fails(self):
        journey.check_steps(list(journey.STEPS))
        for skipped in journey.STEPS:
            with self.subTest(skipped=skipped), self.assertRaisesRegex(RuntimeError, "skipped"):
                journey.check_steps([step for step in journey.STEPS if step != skipped])

    def test_duplicate_effect_fails_even_under_the_same_delivery_id(self):
        record = {"effects": [{"key": "payment"}], "conflicts": 0}
        journey.check_effects(record, ["payment"])
        record["effects"].append({"key": "payment"})
        with self.assertRaisesRegex(RuntimeError, "duplicated"):
            journey.check_effects(record, ["payment"])

    def test_a_migration_changing_any_observation_fails(self):
        original = {"class": "Accept", "reason": None, "state": {"tier": 2},
                    "deliveries": [{"channel": 300, "payload": {"payment_tier": 2}}]}
        journey.check_observations(original, copy.deepcopy(original))
        for key, wrong in (("class", "Reject"), ("reason", 200),
                           ("state", {"tier": 0}), ("deliveries", [])):
            mutant = {**original, key: wrong}
            with self.subTest(key=key), self.assertRaisesRegex(RuntimeError, "changed a decision"):
                journey.check_observations(original, mutant)

    def test_changed_payload_under_the_same_key_fails(self):
        body = {"payload": "original", "destination": "bank"}
        record = {"effects": [{"key": "payment", "body": json.dumps(body)}], "conflicts": 0}
        journey.check_effects(record, ["payment"], {"payment": body})
        record["effects"][0]["body"] = json.dumps({**body, "payload": "wrong"})
        with self.assertRaisesRegex(RuntimeError, "changed destination or payload"):
            journey.check_effects(record, ["payment"], {"payment": body})


if __name__ == "__main__":
    unittest.main()
