"""Finite library import versus operational observations and current ZAL views."""
import copy
import json
import unittest
from dataclasses import replace
from hashlib import sha256
from unittest.mock import patch

import behavior
import factory
import semantic_library as adapter
from finite_library.prototype import (
    Budget,
    Entry,
    Library,
    Refused,
    canonical,
    complete_product,
    emit_zal,
    fit,
    loads,
    slot,
)


def operational(state, event, authorized, reusable, guarded):
    occupied = state == "held"
    used = state != "unseen"
    if guarded and not authorized:
        return "reject", state, "unauthorized"
    if event == "release":
        return (("accept", "released", "ok") if occupied
                else ("reject", state, "not_held"))
    if occupied or (used and not reusable):
        return "reject", state, "duplicate"
    return "accept", "held", "ok"


class SemanticLibraryIntegrationTests(unittest.TestCase):
    def setUp(self):
        self.contract = slot(reusable=False)

    def reject_data(self, data):
        with self.assertRaises(behavior.Refusal):
            adapter.admit(json.dumps(data))

    def test_four_variants_against_operational_oracle_and_two_views(self):
        for reusable in (False, True):
            for guarded in (False, True):
                contract = slot(reusable=reusable, guarded=guarded)
                model, report = adapter.admit(canonical(contract.data()).decode())
                self.assertEqual(report["input_tuples"], 12)
                for state in ("held", "released", "unseen"):
                    for event in ("release", "reserve"):
                        for authorized in (False, True):
                            observed = model.decide(state, event, {"authorized": authorized})
                            self.assertEqual(
                                (observed["class"].lower(), observed["state"],
                                 "ok" if observed["class"] == "Accept" else observed["reason"]),
                                operational(state, event, authorized, reusable, guarded),
                            )
                for english in (False, True):
                    self.assertEqual(behavior.parse(model.render(english),
                                                    "english" if english else "symbolic"), model)
                files = factory.lower(model)
                mapping = json.loads(files["zal-source-map.json"])
                self.assertEqual(mapping["revision"], model.revision)
                self.assertEqual(len(files["tests/zal-authority-examples.txt"].splitlines()), 12)
                self.assertEqual(report["authority"], "none")
                self.assertEqual(report["native_fcis_qualification"], "not-performed")

    def test_exact_source_retained_beside_semantic_revision(self):
        first = json.dumps(self.contract.data(), indent=1)
        second = json.dumps(self.contract.data(), sort_keys=True)
        left, a = adapter.admit(first)
        right, b = adapter.admit(second)
        self.assertEqual(left, right)
        self.assertEqual(a["source_revision"], b["source_revision"])
        self.assertNotEqual(a["source_sha256"], b["source_sha256"])
        self.assertEqual(a["source_text"], first)
        self.assertEqual(a["source_sha256"], sha256(first.encode()).hexdigest())

    def test_initial_state_is_preserved(self):
        data = self.contract.data()
        data["state"][0][2] = "held"
        model, report = adapter.admit(json.dumps(data))
        self.assertEqual(model.initial, "held")
        self.assertEqual(report["initial_state"], "held")
        self.assertEqual(model.decide("held", "release", {"authorized": True})["state"], "released")

    def test_no_domain_completion_or_nondeterministic_choice(self):
        source = self.contract.data()
        mutations = []
        data = copy.deepcopy(source)
        data["rows"].pop()
        mutations.append(data)
        data = copy.deepcopy(source)
        data["rows"][0], data["rows"][1] = data["rows"][1], data["rows"][0]
        mutations.append(data)
        data = copy.deepcopy(source)
        data["rows"][1] = copy.deepcopy(data["rows"][0])
        mutations.append(data)
        for choices in ([], [source["rows"][0]["outputs"][0]] * 2):
            data = copy.deepcopy(source)
            data["rows"][0]["outputs"] = choices
            mutations.append(data)
        for data in mutations:
            self.reject_data(data)

    def test_roles_units_decision_frame_and_approval_are_closed(self):
        source = self.contract.data()
        for group, index, key, value in (
            ("inputs", 2, "role", "authenticated_identity"),
            ("inputs", 2, "unit", "credential"),
            ("outputs", 0, "role", "publication_permission"),
        ):
            data = copy.deepcopy(source)
            data[group][index][key] = value
            self.reject_data(data)
        data = copy.deepcopy(source)
        data["rows"][0]["outputs"] = [["reject", "unseen", "unauthorized"]]
        self.reject_data(data)
        data = copy.deepcopy(source)
        data["rows"][0]["outputs"] = [["accept", "held", "duplicate"]]
        self.reject_data(data)
        for key in ("authority", "human_agreement", "imports", "evidence"):
            data = copy.deepcopy(source)
            data[key] = "approved"
            self.reject_data(data)

    def test_untrusted_json_refuses_with_typed_error(self):
        for source in (
            None, b"{}", "\ud800", "{}" * (adapter.MAX_BYTES // 2 + 1),
            '[' * 2000 + ']' * 2000, '{"profile":1,"profile":2}',
            '{"profile":NaN}', '{"profile":Infinity}',
        ):
            with self.subTest(source_type=type(source).__name__), self.assertRaises(behavior.Refusal):
                adapter.admit(source)
        with self.assertRaises(Refused):
            loads("\ud800")

    def test_native_identifier_and_ownership_limits(self):
        for machine in ("state", "a" * 33, "Bad", "one; machine two"):
            with self.assertRaises(behavior.Refusal):
                adapter.admit(json.dumps(self.contract.data()), machine)
        for owns in (["slot", "slot"], ["z", "a"], ["not a resource"]):
            data = self.contract.data()
            data["owns"] = owns
            self.reject_data(data)

    def test_pinned_entry_selection_does_not_imply_fit_or_agreement(self):
        entry = Entry("seat", "Reusable seat", ("seat",), slot(reusable=True))
        library = Library((entry,))
        self.assertEqual(library.discover("seat")[0]["semantic_fit"], "not-checked")
        self.assertEqual(fit(entry.meaning, self.contract)["status"], "counterexample")
        model, report = adapter.admit_library(library, "seat", entry.revision)
        self.assertEqual(report["library_entry"]["revision"], entry.revision)
        self.assertEqual(report["human_agreement"], "pending")
        self.assertEqual(model.decide("released", "reserve", {"authorized": True})["class"], "Accept")
        for pin in (None, "0" * 64, entry.revision.upper()):
            with self.assertRaises(behavior.Refusal):
                adapter.admit_library(library, "seat", pin)
        with self.assertRaises(behavior.Refusal):
            adapter.admit_library(library, "missing", entry.revision)

    def test_shared_dependencies_are_resolved_once_per_call(self):
        entries = [Entry("leaf", "Leaf", (), self.contract)]
        previous = (entries[0],)
        for level in range(18):
            dependencies = tuple((e.id, e.revision) for e in previous)
            previous = tuple(Entry(f"level_{level}_{suffix}", "Shared dependency", (),
                                   self.contract, dependencies) for suffix in ("a", "b"))
            entries.extend(previous)
        library = Library(tuple(entries))
        root = previous[0]
        expected = root.revision
        original = Entry.revision.fget
        counted = []

        def measured(entry):
            counted.append(entry.id)
            return original(entry)

        with patch.object(Entry, "revision", property(measured)):
            self.assertEqual(library.resolve(root.id, expected), root)
        self.assertEqual(len(counted), len(set(counted)))
        self.assertEqual(len(counted), len(entries) - 1)

    def test_shared_cached_entry_still_checks_each_pin(self):
        leaf = Entry("leaf", "Leaf", (), self.contract)
        left = Entry("left", "Left", (), self.contract, ((leaf.id, leaf.revision),))
        right = Entry("right", "Right", (), self.contract, ((leaf.id, "0" * 64),))
        root = Entry("root", "Root", (), self.contract,
                     ((left.id, left.revision), (right.id, right.revision)))
        library = Library((leaf, left, right, root))
        with self.assertRaises(behavior.Refusal):
            adapter.admit_library(library, root.id, root.revision)
        # A later resolve rechecks the current library rather than a retained cache.
        library.entries = (replace(leaf, meaning=slot(reusable=True)), left)
        with self.assertRaises(Refused):
            library.resolve(left.id, left.revision)

    def test_mutated_library_cannot_hide_duplicates_or_exceed_limits(self):
        first = Entry("seat", "First", (), self.contract)
        last = replace(first, meaning=slot(reusable=True))
        library = Library((first,))
        for entries in ((first, last), [last], (object(),)):
            library.entries = entries
            with self.assertRaises(behavior.Refusal):
                adapter.admit_library(library, "seat", last.revision)
            with self.assertRaises(Refused):
                library.discover("Entry")
            with self.assertRaises(Refused):
                complete_product(self.contract, self.contract, library)
        entries = tuple(Entry(f"entry_{i}", "Entry", (), self.contract) for i in range(129))
        library.entries = entries
        with self.assertRaises(behavior.Refusal):
            adapter.admit_library(library, entries[-1].id, entries[-1].revision)
        with self.assertRaises(Refused):
            library.discover("Entry")
        with self.assertRaises(Refused):
            complete_product(self.contract, self.contract, library)

    def test_malformed_dependency_pairs_have_typed_refusals(self):
        for dependency in (("leaf",), ([], "0" * 64), ["leaf", "0" * 64],
                           ("leaf", 1), ("leaf", "0" * 63)):
            with self.assertRaises(Refused):
                Entry("seat", "Seat", (), self.contract, (dependency,))

    def test_legacy_emitter_refuses_changed_native_port_meaning(self):
        for position, changes in ((2, {"role": "temperature", "unit": "celsius"}),
                                  (1, {"unit": "ticks"})):
            inputs = list(self.contract.inputs)
            inputs[position] = replace(inputs[position], **changes)
            changed = replace(self.contract, inputs=tuple(inputs))
            with self.assertRaises(Refused):
                emit_zal(changed)
            self.reject_data(changed.data())
        outputs = list(self.contract.outputs)
        outputs[0] = replace(outputs[0], role="publication_permission")
        changed = replace(self.contract, outputs=tuple(outputs))
        with self.assertRaises(Refused):
            emit_zal(changed)
        self.reject_data(changed.data())
        outputs = list(self.contract.outputs)
        outputs[2] = replace(outputs[2], values=tuple(sorted((*outputs[2].values, "state"))))
        changed = replace(self.contract, outputs=tuple(outputs))
        with self.assertRaises(Refused):
            emit_zal(changed)
        self.reject_data(changed.data())

    def test_work_counter_cannot_be_rewound_or_incremented_by_boolean(self):
        budget = Budget(3)
        budget.spend(2)
        for amount in (-1, True, 0.5):
            with self.assertRaises(Refused):
                budget.spend(amount)
            self.assertEqual(budget.used, 2)
        budget.spend(0)
        budget.spend()
        self.assertEqual(budget.used, 3)


if __name__ == "__main__":
    unittest.main()
