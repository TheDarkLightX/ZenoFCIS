"""Strict contract-control classifier regressions; fixtures are not proof results."""
import json
import unittest

import verus_contract_controls as controls

PIN = {"version": "0.pinned", "commit": "exact"}
FRAMED = "crates/zeno-fcis-synthesis/src/finite/execution_v2/composition/framed.rs"
# Excerpt of the retained hosted envelope-frame omit_frame_contract stderr; the error,
# location and note lines are verbatim, the caret and elided source lines are shortened.
HOSTED = """error: postcondition not satisfied
  --> /home/runner/work/_temp/v2-evidence/envelope-frame/envelope-frame-run-fvnh2rsx/omit_frame_contract/verification/verus/../../crates/zeno-fcis-synthesis/src/finite/execution_v2/composition/framed.rs:42:5
   |
42 |       ((match result{Ok(p)=>Ok(p.view()),Err(e)=>Err(e)}),final(meter).used.counters@)==spec::project(bytes@,binding,old(meter).limits.counters@,old(meter).used.counters@),))]
   |       ^^^^^^^^^^^ failed this postcondition
...
56 | /     match envelope::frame(bytes, binding.root, &binding.schema, binding.max_bytes) {
   | |_____- at the end of the function body

note: automatically chose triggers for this expression:
error: aborting due to 1 previous error
"""


def report(**changes) -> str:
    results = {"success": False, "errors": 1, "verified": 834, "encountered-error": True,
               "encountered-vir-error": False, "is-verifying-entire-crate": True, **changes}
    return json.dumps({"verus": dict(PIN), "verification-results": results})


def stderr_for(rows) -> str:
    return "".join(f"error: {kind}\n   --> /work/specimen/verification/verus/../../{path}:{line}:5\n    |\n"
                   for kind, path, line in rows) + "error: aborting due to 1 previous error\n"


def assert_strict_classification(case: unittest.TestCase, expected: frozenset) -> None:
    """Admit exactly the demonstrated caller rejection and refuse every hostile variant."""
    accepted = stderr_for(sorted(expected))
    outcome = controls.contract_rejected(1, report(), accepted, PIN, expected)
    case.assertTrue(outcome["accepted"], outcome)
    unrelated = [("postcondition not satisfied", "crates/unrelated/src/lib.rs", 7)]
    kind, path, line = sorted(expected)[0]
    hostile = {
        "resource exhaustion": (1, report(), accepted + "error: Resource limit (rlimit) exceeded\n"),
        "timeout": (1, report(), accepted + "note: verification timed out\n"),
        "out of memory": (1, report(), accepted + "memory allocation of 4096 bytes failed\n"),
        "unknown solver result": (1, report(), accepted + "note: solver returned unknown\n"),
        "signal termination": (-9, report(), accepted),
        "verified successfully": (0, report(success=True, errors=0), ""),
        "VIR error": (1, report(**{"encountered-vir-error": True}), accepted),
        "type error": (1, report(), "error[E0308]: mismatched types\n  --> " + path + ":1:1\n"),
        "partial crate": (1, report(**{"is-verifying-entire-crate": False}), accepted),
        "unpinned verifier": (1, report().replace('"exact"', '"other"'), accepted),
        "missing report": (1, "", accepted),
        "boolean error count": (1, report(errors=True), accepted),
        "missing diagnostics": (1, report(), ""),
        "unrelated caller": (1, report(), stderr_for(unrelated)),
        "expected plus unrelated": (1, report(), stderr_for(sorted(expected) + unrelated)),
        "other obligation kind": (1, report(), stderr_for([("assertion failed", path, line)])),
        "other line": (1, report(), stderr_for([(kind, path, line + 1)])),
    }
    if len(expected) > 1:
        hostile["only part of the demonstration"] = (1, report(), stderr_for(sorted(expected)[:1]))
    for label, (code, stdout, stderr) in hostile.items():
        with case.subTest(label):
            case.assertFalse(controls.contract_rejected(code, stdout, stderr, PIN, expected)["accepted"])


class ContractClassifier(unittest.TestCase):
    def test_parser_reads_the_retained_hosted_caller_diagnostic(self):
        self.assertEqual(controls.diagnostics(HOSTED), [("postcondition not satisfied", FRAMED, 42)])
        outcome = controls.contract_rejected(1, report(), HOSTED, PIN,
                                             frozenset({("postcondition not satisfied", FRAMED, 42)}))
        self.assertEqual((outcome["accepted"], outcome["observed"]),
                         (True, [["postcondition not satisfied", FRAMED, 42]]))
        # The receipt binds the classification, exit, verifier identity and whole-crate flag.
        self.assertEqual((outcome["category"], outcome["exit_code"], outcome["verifier"], outcome["whole_crate"]),
                         ("verified caller rejected the changed contract", 1, PIN, True))
        self.assertIsNone(controls.contract_rejected(None, "", HOSTED, PIN, frozenset())["verifier"])

    def test_inconclusive_outcomes_refuse_before_any_expected_diagnostic(self):
        assert_strict_classification(self, frozenset({("postcondition not satisfied", FRAMED, 42)}))
        assert_strict_classification(self, frozenset({("postcondition not satisfied", FRAMED, 42),
                                                      ("precondition not satisfied", FRAMED, 56)}))


if __name__ == "__main__":
    unittest.main()
