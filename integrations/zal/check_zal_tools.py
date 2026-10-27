#!/usr/bin/env python3
"""Planted-defect controls for the explanation, guide and ladder tools; authority: none.

Each control mutates an isolated copy of integrations/zal, requires its named test
to fail there, and then requires every named test to pass on the unmutated copy.
The live source tree is never written.
"""
import hashlib
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ZAL = Path(__file__).resolve().parent
MINIMAL = "test_explain.WhyNotTests.test_why_not_minimal_against_brute_force"
APPLIED = "test_explain.WhatIfTests.test_what_if_applies_and_reports_every_named_input"
BACKED = "test_explain.NarrativeTests.test_narrative_sentences_are_backed_by_the_witness"
REAL = "test_explain.RegistryHelpTests.test_every_help_topic_names_a_real_construct"
COVERED = "test_explain.RegistryHelpTests.test_every_construct_has_help"
STALE = "test_ladder_tools.LadderToolTests.test_stale_ladder_revision_is_refused"
MINIMAL2 = "test_explain2.WhyNot2Tests.test_why_not_minimal_against_brute_force"
APPLIED2 = "test_explain2.WhatIf2Tests.test_what_if_applies_every_named_change"
BACKED2 = "test_explain2.Narrative2Tests.test_narrative_sentences_are_backed_by_the_witness"
CYCLE = '''    parts += step_parts(model, report, [*W, "cycle"], "cycle")'''
TRAP = '''trap = sentence(model, "trap", {key: witness[key] for key in ("rule", "variable", "value", "range")},'''
WRAPPED = TRAP.replace('{key: witness[key] for key in ("rule", "variable", "value", "range")}',
                       '{**{key: witness[key] for key in ("rule", "variable", "range")}, "value": (witness["value"] - witness["range"][0])'
                       ' % (witness["range"][1] - witness["range"][0] + 1) + witness["range"][0]}')
VIOLATES = '''parts.append(sentence(model, "violates", {"invariant": report["invariant"], "state": witness["state"]},'''
VERIFIER = '''    if problems:
        raise Refusal("Narrative sentence not backed by the witness: " + problems[0])'''
FRAME = '''    Construct("frame", "statement", "Step and frame",
              "An Accept may change only the control state. Reject preserves it. This profile has no effects."),
'''
CONTROLS = [
    ("why-not-not-minimal", MINIMAL, "explain.py",
     [("    minimal = [item for item in found if item[0] == distance]", "    minimal = found[:1]")]),
    ("what-if-ignores-an-input", APPLIED, "explain.py",
     [("    after_context = {name: facts.get(name, value) for name, value in context.items()}",
       "    after_context = {name: value if name == model.facts[-1] else facts.get(name, value) for name, value in context.items()}"),
      ('[("context:" + name, context.get(name), facts[name]) for name in model.facts if name in facts]',
       '[("context:" + name, context.get(name), facts[name]) for name in model.facts if name in facts and name != model.facts[-1]]')]),
    ("unbacked-sentence-runtime-check", BACKED, "explain.py",
     [(VIOLATES, VIOLATES.replace('"state": witness["state"]}', '"state": model.initial}'))]),
    ("unbacked-sentence-independent-oracle", BACKED, "explain.py",
     [(VIOLATES, VIOLATES.replace('"state": witness["state"]}', '"state": model.initial}')),
      (VERIFIER, VERIFIER.replace("if problems:", "if False:"))]),
    ("help-for-a-missing-construct", REAL, "constructs.py",
     [('    Construct("true", "operator", "Boolean true",',
       '    Construct("=>", "operator", "Implication", "Not in this profile.", "P => Q", "P implies Q", formula_op="or"),\n'
       '    Construct("true", "operator", "Boolean true",')]),
    ("construct-without-help", COVERED, "constructs.py", [(FRAME, "")]),
    ("stale-ladder-revision-accepted", STALE, "ladder_tools.py",
     [("    if revision != current:\n        raise l.Refusal(STALE)", "    if False:\n        raise l.Refusal(STALE)")]),
    ("profile2-why-not-not-minimal", MINIMAL2, "explain2.py",
     [("    minimal = [item for item in found if item[0] == distance]", "    minimal = found[:1]")]),
    ("profile2-what-if-ignores-a-variable", APPLIED2, "explain2.py",
     [("    after_values = {name: stored.get(name, value) for name, value in variables.items()}", "    after_values = dict(variables)"),
      ('[("variable:" + v.name, variables.get(v.name), stored[v.name]) for v in model.variables if v.name in stored]', "[]")]),
    ("lasso-without-its-cycle-runtime-check", BACKED2, "explain2.py", [(CYCLE, CYCLE + "[:-1]")]),
    ("lasso-without-its-cycle-independent-oracle", BACKED2, "explain2.py",
     [(CYCLE, CYCLE + "[:-1]"), (VERIFIER, VERIFIER.replace("if problems:", "if False:"))]),
    ("range-story-wraps-around", BACKED2, "explain2.py",
     [(TRAP, WRAPPED), (VERIFIER, VERIFIER.replace("if problems:", "if False:"))]),
]


def run(work, test):
    result = subprocess.run([sys.executable, "-B", "-m", "unittest", test], cwd=work, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=600,
                            env={"PATH": "/usr/bin:/bin", "PYTHONDONTWRITEBYTECODE": "1"})
    return result.returncode, result.stdout


def digest():
    return {path.name: hashlib.sha256(path.read_bytes()).hexdigest() for path in ZAL.iterdir() if path.is_file()}


def main():
    before = digest()
    with tempfile.TemporaryDirectory(prefix="zal-tool-defects-") as directory:
        work = Path(directory) / "zal"
        shutil.copytree(ZAL, work, ignore=shutil.ignore_patterns("__pycache__", "*.lock"))
        for label, test, filename, replacements in CONTROLS:
            path = work / filename
            original = path.read_text()
            mutated = original
            for anchor, replacement in replacements:
                if mutated.count(anchor) != 1:
                    raise SystemExit(f"STOP: mutation anchor drift for {label}")
                mutated = mutated.replace(anchor, replacement)
            path.write_text(mutated)
            try:
                code, output = run(work, test)
            finally:
                path.write_text(original)
            name = test.rsplit(".", 1)[-1]
            if code == 0 or f"FAIL: {name}" not in output:
                print(output[-3000:])
                raise SystemExit(f"STOP: planted defect survived or failed outside its named test: {label}")
            print(f"KILLED {label} by {test}", flush=True)
        for test in sorted({control[1] for control in CONTROLS}):
            code, output = run(work, test)
            if code != 0:
                print(output[-3000:])
                raise SystemExit(f"STOP: baseline fails {test}")
            print(f"BASELINE PASS {test}", flush=True)
    if digest() != before:
        raise SystemExit("STOP: live source changed during isolated defect controls")
    print(f"PASS: {len(CONTROLS)} named planted controls killed; baseline passes; live source unchanged")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
