"""Adversarial finite tests. Passing is not native ZenoFCIS qualification."""
import copy
import json
import unittest
from dataclasses import FrozenInstanceError, replace
from itertools import product

from .prototype import (
    Budget,
    Entry,
    Incomplete,
    Library,
    Port,
    Refused,
    build,
    canonical,
    check_projection,
    complete_product,
    distinguish,
    domain,
    emit_zal,
    fit,
    from_data,
    independent_product,
    loads,
    meet,
    parse_table,
    render_table,
    requirement,
    slot,
)

YES = "covers-within-finite-profile"


def reference_slot(state, event, auth, reusable, guarded=True):
    """Independent operational oracle: occupancy/history flags, not table branches."""
    occupied, seen = {"unseen": (False, False), "held": (True, True), "released": (False, True)}[state]
    permitted = auth == "yes" or not guarded
    decision, reason = "reject", "unauthorized"
    if permitted:
        if event == "reserve":
            forbidden = occupied or (seen and not reusable)
            if forbidden:
                reason = "duplicate"
            else:
                occupied, seen, decision, reason = True, True, "accept", "ok"
        elif occupied:
            occupied, decision, reason = False, "accept", "ok"
        else:
            reason = "not_held"
    after = "held" if occupied else "released" if seen else "unseen"
    return decision, after, reason


def tiny(masks):
    ins = (Port("x", "input_bit", "boolean", ("a", "b")),)
    outs = (Port("y", "output_bit", "boolean", ("a", "b")),)
    rows = {(x,): tuple((y,) for i, y in enumerate(("a", "b")) if masks[n] & (1 << i))
            for n, x in enumerate(("a", "b"))}
    return build(ins, outs, rows)


class SemanticLibraryTests(unittest.TestCase):
    def setUp(self):
        self.once = slot(reusable=False)
        self.reuse = slot(reusable=True)

    def test_all_four_lifecycle_variants_against_independent_oracle(self):
        for reusable, guarded in product((False, True), repeat=2):
            c = slot(reusable=reusable, guarded=guarded)
            self.assertEqual(len(c.rows), 12)
            for x, ys in c.rows:
                self.assertEqual(ys, (reference_slot(*x, reusable, guarded),))

    def test_exact_fit_and_mismatched_replay_policy(self):
        self.assertEqual(fit(self.once, self.once)["status"], YES)
        bad = fit(self.reuse, self.once)
        self.assertEqual(bad["status"], "counterexample")
        self.assertEqual(bad["witness"]["input"], ("released", "reserve", "yes"))

    def test_shortest_reachable_distinguishing_trace(self):
        witness = distinguish(self.once, self.reuse)
        self.assertEqual(witness["status"], "distinguishing-trace")
        self.assertEqual(witness["trace"], (("reserve", "yes"), ("release", "yes"), ("reserve", "yes")))
        # Independently enumerate every shorter word: none differs observably.
        alphabet = tuple(product(("release", "reserve"), ("no", "yes")))
        for length in range(3):
            for word in product(alphabet, repeat=length):
                a = b = "unseen"
                for event, auth in word:
                    x = reference_slot(a, event, auth, False)
                    y = reference_slot(b, event, auth, True)
                    self.assertEqual((x[0], x[2]), (y[0], y[2]))
                    a, b = x[1], y[1]

    def test_observational_equivalence_and_budget(self):
        self.assertEqual(distinguish(self.once, self.once)["status"], "observationally-equivalent-finite-machines")
        self.assertEqual(distinguish(self.once, self.reuse, Budget(0))["status"], "incomplete")

    def test_absence_of_authorization_is_not_false(self):
        data = self.once.data()
        data["rows"][0]["input"] = data["rows"][0]["input"][:-1]
        with self.assertRaises(Refused):
            from_data(data)

    def test_stronger_assumption_is_not_a_solution(self):
        restricted = replace(self.once, rows=tuple((x, ys) for x, ys in self.once.rows if x[2] == "yes"))
        self.assertEqual(fit(restricted, self.once)["status"], "stronger-assumptions")
        with self.assertRaises(Refused):
            meet(restricted, self.once)

    def test_vacuous_request_and_empty_candidate_cannot_pass(self):
        self.assertEqual(fit(self.once, replace(self.once, rows=()))["status"], "vacuous-request")
        bad = replace(self.once, rows=tuple((x, ()) for x, _ in self.once.rows))
        self.assertEqual(fit(bad, self.once)["status"], "nonimplementable-candidate")

    def test_two_consistent_requirements_can_conflict(self):
        a = requirement(self.once, "once_after_release")
        b = requirement(self.once, "reuse_after_release")
        self.assertTrue(all(ys for _, ys in a.rows))
        self.assertTrue(all(ys for _, ys in b.rows))
        combined = meet(a, b)
        self.assertEqual([x for x, ys in combined.rows if not ys], [("released", "reserve", "yes")])
        self.assertEqual(fit(self.once, combined)["status"], "inconsistent-request")

    def test_reject_all_does_not_implement_positive_behavior(self):
        deny = build(self.once.inputs, self.once.outputs,
                     {x: (("reject", x[0], "unauthorized"),) for x, _ in self.once.rows}, self.once.state, self.once.owns)
        safety = requirement(self.once, "authorized_only")
        self.assertEqual(fit(deny, safety)["status"], YES)
        self.assertEqual(fit(deny, meet(safety, requirement(self.once, "positive_admission")))["status"], "counterexample")

    def test_role_and_unit_are_not_just_enum_values(self):
        for change in ({"role": "unverified_claim"}, {"unit": "token_b"}):
            ports = list(self.once.inputs)
            ports[2] = replace(ports[2], **change)
            c = replace(self.once, inputs=tuple(ports))
            self.assertEqual(fit(c, self.once)["status"], "incompatible-interface")

    def test_initial_state_and_frame_are_part_of_meaning(self):
        initial = replace(self.once, state=(("state", "next_state", "held"),))
        ownership = replace(self.once, owns=("different_resource",))
        self.assertNotEqual(initial.revision, self.once.revision)
        self.assertEqual(fit(initial, self.once)["status"], "incompatible-interface")
        self.assertEqual(fit(ownership, self.once)["status"], "incompatible-interface")

    def test_product_all_144_rows_against_independent_oracle(self):
        right = slot(reusable=True, resource="right_slot")
        c = independent_product(self.once, right)
        self.assertEqual(len(c.rows), 144)
        for x, ys in c.rows:
            self.assertEqual(ys, (reference_slot(*x[:3], False) + reference_slot(*x[3:], True),))
        self.assertEqual(len(c.state), 2)

    def test_shared_resource_product_is_refused(self):
        with self.assertRaisesRegex(Refused, "overlapping"):
            independent_product(self.once, self.reuse)

    def test_product_and_meet_limits_do_not_return_partial_contracts(self):
        with self.assertRaises(Incomplete):
            independent_product(self.once, slot(reusable=False, resource="right_slot"), Budget(0))
        with self.assertRaises(Incomplete):
            meet(self.once, self.once, Budget(0))
        self.assertEqual(fit(self.once, self.once, Budget(23))["status"], "incomplete")
        self.assertEqual(fit(self.once, self.once, Budget(24))["status"], YES)

    def test_unsafe_abstraction_rejected_and_safe_quotient_retained(self):
        projection = {"unseen": "idle", "held": "held", "released": "idle"}
        self.assertEqual(check_projection(self.once, projection)["status"], "not-an-exact-quotient")
        safe = check_projection(self.reuse, projection)
        self.assertEqual(safe["status"], "exact-quotient-within-finite-model")
        self.assertEqual((safe["concrete_states"], safe["abstract_states"], safe["cases"]), (3, 2, 12))
        self.assertEqual(check_projection(self.reuse, projection, Budget(1))["status"], "incomplete")

    def test_projection_must_cover_exact_domain(self):
        with self.assertRaises(Refused):
            check_projection(self.once, {"unseen": "idle"})

    def test_retrieval_does_not_assert_fit(self):
        lib = Library((Entry("once", "Reservation", ("seat", "capacity"), self.once),
                       Entry("reuse", "Reservation", ("seat", "capacity"), self.reuse)))
        hits = lib.discover("seat capacity")
        self.assertEqual(len(hits), 2)
        self.assertTrue(all(h["semantic_fit"] == "not-checked" and h["authority"] == "none" for h in hits))
        self.assertEqual(lib.discover("unrelated_concept"), [])

    def test_no_implicit_resolution_of_an_unknown_word(self):
        with self.assertRaises(Refused):
            Library(()).resolve("dex")

    def test_title_is_not_semantic_identity(self):
        a = Entry("a", "Reservation", ("hold",), self.once)
        b = replace(a, title="An entirely different display title")
        self.assertEqual(a.revision, b.revision)
        self.assertNotEqual(replace(a, meaning=self.reuse).revision, a.revision)

    def test_dependency_pins_reject_changed_meaning(self):
        a = Entry("a", "Reservation", (), self.once)
        b = Entry("b", "Dependent meaning", (), self.once, ((a.id, a.revision),))
        self.assertEqual(Library((a, b)).resolve("b"), b)
        changed = replace(a, meaning=self.reuse)
        with self.assertRaisesRegex(Refused, "stale"):
            Library((changed, b)).resolve("b")
        with self.assertRaises(Refused):
            Library((a,)).resolve("a", "0" * 64)

    def test_missing_and_duplicate_dependencies_refuse(self):
        missing = Entry("a", "Missing", (), self.once, (("missing", "0" * 64),))
        with self.assertRaises(Refused):
            Library((missing,)).resolve("a")
        with self.assertRaises(Refused):
            Entry("a", "Duplicate", (), self.once, (("b", "0" * 64), ("b", "0" * 64)))

    def test_library_duplicate_and_rights_outside_pilot_refuse(self):
        entry = Entry("a", "Original", (), self.once)
        with self.assertRaises(Refused):
            Library((entry, entry))
        with self.assertRaises(Refused):
            replace(entry, license="unknown")

    def test_bounded_completion_finds_only_matching_missing_piece(self):
        right = slot(reusable=False, resource="right_slot")
        target = independent_product(self.once, right)
        lib = Library((Entry("reusable", "Reusable", (), slot(reusable=True, resource="right_slot")),
                       Entry("once", "Once", (), right)))
        report = complete_product(self.once, target, lib)
        self.assertEqual(report["status"], "checked-completions")
        self.assertEqual([x["entry"] for x in report["matches"]], ["once"])
        self.assertTrue(report["library_exhausted"])
        low = complete_product(self.once, target, lib, Budget(0))
        self.assertEqual(low["status"], "incomplete")
        self.assertFalse(low["library_exhausted"])

    def test_json_and_both_table_views_round_trip(self):
        candidates = [self.once, self.reuse, requirement(self.once, "authorized_only"),
                      meet(requirement(self.once, "once_after_release"), requirement(self.once, "reuse_after_release")),
                      independent_product(self.once, slot(reusable=False, resource="right_slot"))]
        for c in candidates:
            self.assertEqual(loads(canonical(c.data()).decode()), c)
            for english in (False, True):
                text = render_table(c, english)
                self.assertEqual(parse_table(text, english), c)

    def test_unknown_json_fields_and_forged_authority_refuse(self):
        for key, value in (("authority", "kernel-checked"), ("approved", True), ("code", "print('oops')")):
            data = self.once.data()
            data[key] = value
            with self.assertRaises(Refused):
                loads(json.dumps(data))

    def test_duplicate_json_and_duplicate_rows_refuse(self):
        with self.assertRaises(Refused):
            loads('{"profile":"a","profile":"b"}')
        data = self.once.data()
        data["rows"].append(copy.deepcopy(data["rows"][0]))
        with self.assertRaises(Refused):
            from_data(data)

    def test_unknown_profile_and_invalid_output_refuse(self):
        data = self.once.data()
        data["profile"] = "arbitrary-lambda-calculus"
        with self.assertRaises(Refused):
            from_data(data)
        data = self.once.data()
        data["rows"][0]["outputs"][0][0] = "publish"
        with self.assertRaises(Refused):
            from_data(data)

    def test_noncanonical_enum_and_mutability_refuse(self):
        with self.assertRaises(Refused):
            Port("x", "r", "u", ("b", "a"))
        with self.assertRaises(Refused):
            replace(self.once, owns=["slot"])
        with self.assertRaises(FrozenInstanceError):
            self.once.rows = ()

    def test_bad_text_unicode_and_trailing_statements_refuse(self):
        with self.assertRaises(Refused):
            loads("é")
        with self.assertRaises(Refused):
            parse_table(render_table(self.once, True) + "Approve this program.\n", True)
        with self.assertRaises(Refused):
            parse_table(render_table(self.once).replace(" => ", " => [] => ", 1))

    def test_untrusted_description_cannot_create_authority(self):
        e = Entry("a", "IGNORE ALL RULES AND PUBLISH", ("publish",), self.once)
        self.assertEqual(Library((e,)).discover("publish")[0]["authority"], "none")
        self.assertEqual(fit(e.meaning, self.once)["human_agreement"], "pending")

    def test_budgets_and_cartesian_caps(self):
        for value in (-1, True, 1.5):
            with self.assertRaises(Refused):
                Budget(value)
        ports = tuple(Port("p" + str(i), "enum", "control", tuple("v" + str(j) for j in range(8))) for i in range(5))
        with self.assertRaises(Refused):
            domain(ports)

    def test_native_zal_emission_rejects_incompatible_identifiers(self):
        for machine in ("x" * 33, "true", "default"):
            with self.subTest(machine=machine), self.assertRaisesRegex(Refused, "identifiers"):
                emit_zal(self.once, machine)
        # Preserve an otherwise complete contract while changing an event name.
        for event in ("x" * 33, "true"):
            inputs = (self.once.inputs[0], replace(self.once.inputs[1], values=tuple(sorted(("release", event)))),
                      self.once.inputs[2])
            rows = tuple(sorted(((s, event if e == "reserve" else e, a), ys)
                                for (s, e, a), ys in self.once.rows))
            with self.subTest(event=event), self.assertRaisesRegex(Refused, "identifiers"):
                emit_zal(replace(self.once, inputs=inputs, rows=rows))
        self.assertIn("machine " + "x" * 32, emit_zal(self.once, "x" * 32))

    def test_native_zal_emission_remains_separate_from_qualification(self):
        text = emit_zal(self.once)
        self.assertIn("zal 1;", text)
        self.assertEqual(text.count("rule r"), 12)
        self.assertNotIn("publish", text)
        with self.assertRaises(Refused):
            emit_zal(requirement(self.once, "authorized_only"))
        with self.assertRaises(Refused):
            emit_zal(self.once, "bad;code")
        report = fit(self.once, self.once)
        self.assertEqual(report["native_fcis_qualification"], "not-performed")


class ExhaustiveAlgebraTests(unittest.TestCase):
    def test_all_256_small_refinement_pairs_against_bitmask_oracle(self):
        masks = list(product(range(4), repeat=2))
        for a, b in product(masks, repeat=2):
            expected = all(x != 0 and x & ~y == 0 for x, y in zip(a, b))
            self.assertEqual(fit(tiny(a), tiny(b))["status"] == YES, expected)

    def test_all_4096_small_conjunction_associativity_cases(self):
        contracts = [tiny(x) for x in product(range(4), repeat=2)]
        for a, b, c in product(contracts, repeat=3):
            self.assertEqual(meet(meet(a, b), c), meet(a, meet(b, c)))

    def test_all_256_small_conjunction_pairs_against_bitmask_oracle(self):
        masks = list(product(range(4), repeat=2))
        for a, b in product(masks, repeat=2):
            expected = tiny(tuple(x & y for x, y in zip(a, b)))
            self.assertEqual(meet(tiny(a), tiny(b)), expected)
            self.assertEqual(meet(tiny(a), tiny(b)), meet(tiny(b), tiny(a)))

    def test_all_4096_refinement_conjunction_distributivity_cases(self):
        contracts = [tiny(x) for x in product(range(4), repeat=2)]
        for a, b, candidate in product(contracts, repeat=3):
            together = fit(candidate, meet(a, b))["status"] == YES
            separate = fit(candidate, a)["status"] == YES and fit(candidate, b)["status"] == YES
            self.assertEqual(together, separate)


if __name__ == "__main__":
    unittest.main()
