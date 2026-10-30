"""Evidence admission must distinguish semantic test failures from missing runs."""
import unittest
from pathlib import Path
import tempfile
import check_arithmetic_handoff as gate


class NativeEvidenceTests(unittest.TestCase):
    def result(self, failed=None):
        text = "\n".join(f"test {name} ... {'FAILED' if name == failed else 'ok'}" for name in sorted(gate.TESTS))
        text += ("\nassertion `left == right` failed\ntest result: FAILED. 2 passed; 1 failed;" if failed else
                 "\ntest result: ok. 3 passed; 0 failed;")
        text += " 0 ignored; 0 measured; 0 filtered out; finished in 0.03s"
        return {"exit": 101 if failed else 0, "timed_out": False}, text, ""

    def test_complete_success(self):
        self.assertTrue(gate.native_outcome(*self.result()))

    def test_expected_semantic_refutation(self):
        record, out, _ = self.result("schema_ranges_and_nominal_refusals")
        self.assertTrue(gate.native_outcome(record, out, "error: test failed, to rerun pass `-p zeno-fcis-schema`",
                                           failed_test="schema_ranges_and_nominal_refusals"))

    def test_timeout_and_nonzero_success_are_open(self):
        record, out, err = self.result()
        for altered in ({**record, "timed_out": True}, {**record, "exit": -9}, {**record, "exit": 101}):
            self.assertFalse(gate.native_outcome(altered, out, err))

    def test_compile_failure_is_not_a_killed_mutant(self):
        record, out, _ = self.result("schema_ranges_and_nominal_refusals")
        for diagnostic in ("error[E0000]: wrong type", "error: could not compile crate", "error: linker failed"):
            self.assertFalse(gate.native_outcome(record, out, diagnostic, failed_test="schema_ranges_and_nominal_refusals"))

    def test_missing_or_unexpected_test_is_not_success(self):
        record, out, err = self.result()
        self.assertFalse(gate.native_outcome(record, out.replace("test independent_vectors ... ok", ""), err))
        self.assertFalse(gate.native_outcome(record, out + "\ntest another ... ok", err))

    def test_wrong_assertion_failure_does_not_qualify_control(self):
        self.assertFalse(gate.native_outcome(*self.result("independent_vectors"),
                                            failed_test="schema_ranges_and_nominal_refusals"))

    def test_runtime_error_without_assertion_is_not_semantic_control(self):
        record, out, err = self.result("independent_vectors")
        self.assertFalse(gate.native_outcome(record, out.replace("assertion", "allocation failure"), err,
                                            failed_test="independent_vectors"))

    def test_duplicate_event_or_summary_cannot_erase_failure(self):
        record, out, err = self.result()
        self.assertFalse(gate.native_outcome(record, "test independent_vectors ... FAILED\n" + out, err))
        self.assertFalse(gate.native_outcome(record, out + "\n" + out.splitlines()[-1], err))

    def test_failed_prerequisites_or_controls_keep_obligations_open(self):
        packet = gate.adapter.handoff(gate.example(), gate.bindings())
        passed = {"status": "satisfied"}
        controls = [{"name": str(i), "status": "refuted"} for i in range(4)]
        for prerequisites, current in ((False, controls), (True, controls[:-1]),
                                       (True, controls[:-1] + [{"name": "3", "status": "open"}])):
            results = gate.assess(packet, passed, current, passed, prerequisites)
            self.assertTrue(all(r["status"] == "open" for r in results if r["kind"] == "native-corpus"))
            if not prerequisites:
                self.assertTrue(all(r["status"] == "open" for r in results))
        results = gate.assess(packet, passed, controls, passed, True)
        self.assertTrue(all(r["status"] == "open" for r in results if r["kind"].startswith("schema-")))

    def test_unavailable_executable_records_open_outcome(self):
        with tempfile.TemporaryDirectory() as temporary:
            record, out, err = gate.contained_run([str(Path(temporary) / "unavailable")],
                gate.ROOT, Path(temporary), "missing", seconds=1)
            self.assertEqual(record["startup_error"], "FileNotFoundError")
            self.assertIsNone(record["exit"])
            self.assertFalse(gate.native_outcome(record, out, err))


if __name__ == "__main__":
    unittest.main()
