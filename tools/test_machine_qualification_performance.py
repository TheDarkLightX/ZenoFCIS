"""Exact-rational performance analysis on fabricated rows only; nothing is measured or executed."""

from contextlib import redirect_stdout
import copy
from fractions import Fraction
import io
import json
from pathlib import Path
import tempfile
import unittest

from machine_qualification import performance as perf

ONE = Fraction(1)
LOW, HIGH = 1 - perf.MARGIN, 1 + perf.MARGIN


def plan(**changes):
    value = {"schema": perf.PLAN_SCHEMA, "plan_id": "fabricated-plan", "candidate": "candidate-kernel",
             "baseline": "gcc-o2-baseline", "baseline_label": "compiler-trusted comparator", "sessions": 20,
             "pairs_per_session": 6, "ab_pairs_per_session": 3, "ba_pairs_per_session": 3, "call_count": "1000000",
             "comparisons": 1, "alpha": {"numerator": 1, "denominator": 20},
             "practical_margin": {"numerator": 1, "denominator": 50}, "schedule_seed": "20261030",
             "schedule": [["AB", "BA", "BA", "AB", "BA", "AB"] if s % 2 else ["BA", "AB", "AB", "BA", "AB", "BA"]
                          for s in range(20)]}
    value.update(changes)
    return value


def observations(frozen, sessions):
    """Rows whose pair ratios are exactly the given fractions; sessions is 20 lists of 6 ratios."""
    rows = []
    for s, ratios in enumerate(sessions):
        for p, ratio in enumerate(ratios):
            rows.append({"observation_id": f"run-{s}-{p}", "session": s, "session_id": f"process-{s}", "pair": p,
                         "order": frozen["schedule"][s][p], "candidate_calls": frozen["call_count"],
                         "baseline_calls": frozen["call_count"],
                         "candidate_elapsed_ns": str(ratio.numerator * 1000),
                         "baseline_elapsed_ns": str(ratio.denominator * 1000),
                         "outputs_agreed": True, "artifact_unchanged": True})
    return {"schema": perf.OBSERVATIONS_SCHEMA, "plan_sha256": perf.digest(frozen), "rows": rows}


def uniform(summaries):
    """Sessions whose six ratios all equal the given session summaries."""
    return [[Fraction(z)] * 6 for z in summaries]


def endpoints(lower, upper):
    """Twenty session summaries whose 6th and 15th order statistics are exactly lower and upper."""
    return [lower / 2] * 5 + [lower] + [(lower + upper) / 2] * 8 + [upper] + [upper * 2] * 5


class Arithmetic(unittest.TestCase):
    def test_single_comparison_coverage_is_1005176_over_1048576(self):
        self.assertEqual(perf.rank(20, 1), 6)
        self.assertEqual(perf.coverage(20, 6), Fraction(1005176, 1048576))
        self.assertEqual(perf.coverage(20, 6), Fraction(125647, 131072))

    def test_family_ranks_match_the_design_and_a_direct_count(self):
        self.assertEqual((perf.rank(20, 1), perf.rank(20, 7), perf.rank(20, 14)), (6, 4, 4))
        for m in (1, 2, 3, 5, 7, 14, 50, 1000, 26214):
            admissible = [k for k in range(1, 11)
                          if Fraction(2 * sum(perf.comb(20, j) for j in range(k)), 2 ** 20) <= Fraction(1, 20 * m)]
            self.assertEqual(perf.rank(20, m), max(admissible), m)
        self.assertEqual(perf.rank(20, 26214), 1)
        self.assertIsNone(perf.rank(20, 26215))

    def test_classification_boundaries_are_exact(self):
        tiny = Fraction(1, 10 ** 12)
        cases = [(LOW - 1, LOW, "inconclusive"),             # upper exactly 49/50 is not an improvement
                 (LOW, LOW, "practically_equivalent"),        # ... but can be equivalent
                 (LOW - 1, LOW - tiny, "improved"),
                 (LOW, HIGH, "practically_equivalent"),
                 (LOW - tiny, HIGH, "inconclusive"),
                 (LOW, HIGH + tiny, "inconclusive"),
                 (HIGH, HIGH, "practically_equivalent"),     # lower exactly 51/50 is not a regression
                 (HIGH, HIGH * 2, "inconclusive"),
                 (HIGH + tiny, HIGH * 2, "regressed"),
                 (Fraction(1, 2), Fraction(2), "inconclusive"),
                 (None, None, "inconclusive")]
        for lower, upper, expected in cases:
            with self.subTest(lower=lower, upper=upper):
                self.assertEqual(perf.classify(lower, upper), expected)

    def test_decimal_presentation_rounds_outward_and_never_decides(self):
        third = Fraction(1, 3)
        self.assertEqual((perf.decimal(third, "down"), perf.decimal(third, "up")), ("0.333333", "0.333334"))
        self.assertEqual(perf.decimal(Fraction(2, 3), "nearest"), "0.666667")
        self.assertEqual(perf.decimal(Fraction(49, 50), "up"), "0.980000")
        almost = LOW - Fraction(1, 10 ** 12)
        self.assertEqual(perf.decimal(almost, "up"), "0.980000")
        self.assertEqual(perf.classify(Fraction(1, 2), almost), "improved")


class Analysis(unittest.TestCase):
    def analyze(self, frozen, sessions):
        report = perf.analyze(frozen, observations(frozen, sessions))
        self.assertEqual(report["authority"], "none")
        return report

    def test_session_summary_point_and_interval_use_the_frozen_order_statistics(self):
        frozen = plan()
        sessions = uniform([Fraction(80 + s, 100) for s in range(20)])
        sessions[0] = [Fraction(1, 2), Fraction(2), ONE, Fraction(3, 2), Fraction(100), ONE]
        report = self.analyze(frozen, sessions)
        self.assertEqual(report["status"], "valid")
        self.assertEqual(report["session_summaries"][0], "5/4")
        ordered = sorted([Fraction(5, 4)] + [Fraction(80 + s, 100) for s in range(1, 20)])
        self.assertEqual(report["point_estimate"]["exact"], str((ordered[9] + ordered[10]) / 2))
        interval = report["interval"]
        self.assertEqual(interval["ranks"], [6, 15])
        self.assertEqual((Fraction(interval["lower"]), Fraction(interval["upper"])), (ordered[5], ordered[14]))
        self.assertEqual(interval["coverage_per_comparison"], "125647/131072")
        self.assertEqual(report["classification"], perf.classify(ordered[5], ordered[14]))
        self.assertEqual(len(report["pair_ratios"]), 20)
        self.assertEqual(report["baseline_label"], "compiler-trusted comparator")

    def test_rows_reach_each_classification(self):
        tiny = Fraction(1, 10 ** 9)
        for lower, upper, expected in ((Fraction(1, 2), LOW - tiny, "improved"), (LOW, HIGH, "practically_equivalent"),
                                       (HIGH + tiny, Fraction(2), "regressed"), (Fraction(9, 10), Fraction(11, 10), "inconclusive"),
                                       (Fraction(1, 2), LOW, "inconclusive")):
            with self.subTest(expected=expected):
                report = self.analyze(plan(), uniform(endpoints(lower, upper)))
                self.assertEqual((Fraction(report["interval"]["lower"]), Fraction(report["interval"]["upper"])),
                                 (lower, upper))
                self.assertEqual(report["classification"], expected)

    def test_ties_and_identical_timings_are_valid(self):
        report = self.analyze(plan(), uniform([1] * 20))
        self.assertEqual((report["status"], report["point_estimate"]["exact"]), ("valid", "1/1"))
        self.assertEqual((report["interval"]["lower"], report["interval"]["upper"]), ("1/1", "1/1"))
        self.assertEqual(report["classification"], "practically_equivalent")
        tied = [[ONE, ONE, ONE, ONE, Fraction(1, 2), Fraction(3)]] * 20
        self.assertEqual(self.analyze(plan(), tied)["session_summaries"], ["1/1"] * 20)

    def test_a_larger_comparison_family_widens_the_interval(self):
        # Sorted, ranks 6 and 15 are 3/5 and 9/10 (improved); ranks 4 and 17 are 1/2 and 1 (inconclusive).
        tenths = [5] * 5 + [6] + [7] * 8 + [9, 9.5, 10] + [11] * 3
        summaries = [Fraction(str(t)) / 10 for t in tenths]
        one = self.analyze(plan(), uniform(summaries))
        seven = self.analyze(plan(comparisons=7), uniform(summaries))
        self.assertEqual((one["interval"]["ranks"], seven["interval"]["ranks"]), ([6, 15], [4, 17]))
        self.assertEqual((one["classification"], seven["classification"]), ("improved", "inconclusive"))
        none = self.analyze(plan(comparisons=26215), uniform(summaries))
        self.assertIsNone(none["interval"]["ranks"])
        self.assertEqual(none["classification"], "inconclusive")


class Refusals(unittest.TestCase):
    def setUp(self):
        self.plan = plan()
        self.rows = observations(self.plan, uniform([1] * 20))

    def invalid(self, mutate, reason):
        rows = copy.deepcopy(self.rows)
        mutate(rows["rows"])
        report = perf.analyze(self.plan, rows)
        self.assertEqual(report["status"], "invalid")
        self.assertTrue(any(reason in text for text in report["reasons"]), report["reasons"])
        self.assertNotIn("classification", report)

    def test_invalid_measurements_are_reported_and_never_classified(self):
        cases = [
            (lambda r: r[1].update(observation_id="run-0-0"), "observation identity reused: run-0-0"),
            (lambda r: [row.update(session_id="process-0") for row in r if row["session"] == 1],
             "session identity reused across sessions: process-0"),
            (lambda r: r[1].update(session_id="other"), "session 0: more than one session identity"),
            (lambda r: r.pop(7), "session 1 pair 1: missing"),
            (lambda r: r[2].update(candidate_elapsed_ns="0"), "session 0 pair 2: zero duration"),
            (lambda r: r[2].update(baseline_elapsed_ns="0"), "session 0 pair 2: zero duration"),
            (lambda r: r[3].update(candidate_calls="999999"), "session 0 pair 3: unequal call counts"),
            (lambda r: [r[4].update(candidate_calls="7"), r[4].update(baseline_calls="7")],
             "session 0 pair 4: unequal call counts"),
            (lambda r: r[0].update(order="BA" if r[0]["order"] == "AB" else "AB"), "order differs from the frozen schedule"),
            (lambda r: r[5].update(outputs_agreed=False), "session 0 pair 5: wrong outputs"),
            (lambda r: r[6].update(artifact_unchanged=False), "session 1 pair 0: artifact changed"),
            (lambda r: r.append(dict(r[0], observation_id="again")), "session 0 pair 0: recorded more than once"),
        ]
        for mutate, reason in cases:
            with self.subTest(reason=reason):
                self.invalid(mutate, reason)

    def test_malformed_or_changed_inputs_are_refused(self):
        bad_plans = [plan(extra=1), plan(sessions=19), plan(pairs_per_session=7), plan(ab_pairs_per_session=4),
                     plan(call_count="01"), plan(call_count="0"), plan(call_count=1000000), plan(comparisons=0),
                     plan(comparisons=True), plan(alpha={"numerator": 1, "denominator": 10}),
                     plan(practical_margin={"numerator": 1, "denominator": 100}),
                     plan(schedule=[["AB"] * 4 + ["BA"] * 2] * 20), plan(schedule=[["AB", "BA"] * 3] * 19),
                     plan(plan_id="has space"), plan(schema="zeno-fcis/machine-performance-plan/2")]
        for value in bad_plans:
            with self.subTest(plan=value), self.assertRaises(ValueError):
                perf.analyze(value, self.rows)
        changed = plan(practical_margin={"numerator": 1, "denominator": 50}, comparisons=2)
        with self.assertRaisesRegex(ValueError, "different plan"):
            perf.analyze(changed, self.rows)
        for mutate in (lambda o: o.update(extra=1), lambda o: o["rows"][0].update(extra=1),
                       lambda o: o["rows"][0].pop("artifact_unchanged"), lambda o: o["rows"][0].update(session=20),
                       lambda o: o["rows"][0].update(pair=True), lambda o: o["rows"][0].update(candidate_elapsed_ns=12),
                       lambda o: o["rows"][0].update(outputs_agreed=1), lambda o: o.update(schema="other"),
                       lambda o: o["rows"].extend([o["rows"][0]] * 241)):
            rows = copy.deepcopy(self.rows); mutate(rows)
            with self.subTest(rows=str(mutate)), self.assertRaises(ValueError):
                perf.analyze(self.plan, rows)

    def test_loader_refuses_duplicate_keys_floats_and_oversize_input(self):
        for data in (b'{"a":1,"a":2}', b'{"a":1.5}', b'{"a":NaN}', b'[1]', b" " * (perf.MAX_INPUT_BYTES + 1)):
            with self.subTest(data=data[:20]), self.assertRaises(ValueError):
                perf.load(data)
        self.assertEqual(perf.load(perf.canonical(self.plan)), self.plan)

    def test_cli_exit_codes(self):
        with tempfile.TemporaryDirectory(prefix="zeno-perf-analysis-") as folder:
            root = Path(folder)
            (root / "plan.json").write_bytes(perf.canonical(self.plan))
            (root / "rows.json").write_bytes(perf.canonical(self.rows))
            invalid = copy.deepcopy(self.rows); invalid["rows"].pop()
            (root / "invalid.json").write_bytes(perf.canonical(invalid))
            (root / "duplicate.json").write_bytes(b'{"schema":"a","schema":"b"}')
            for rows, code, status in (("rows.json", 0, "valid"), ("invalid.json", 1, "invalid"),
                                       ("duplicate.json", 65, "refused")):
                with self.subTest(rows=rows), redirect_stdout(io.StringIO()) as output:
                    self.assertEqual(perf.main(["--plan", str(root / "plan.json"), "--observations", str(root / rows)]), code)
                self.assertEqual(json.loads(output.getvalue())["status"], status)


if __name__ == "__main__":
    unittest.main()
