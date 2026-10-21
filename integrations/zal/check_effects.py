#!/usr/bin/env python3
"""Build the registry printer and challenge the types, typed-machine and effects checks; authority: none.

`native-build` builds the shell example that prints the built-in shell-effects/1
registry. `planted` mutates an isolated copy of integrations/zal, one control at a
time: a checker module or the shipped example source. Each mutated copy must import,
its named test must fail, and the unmutated copy must then pass every named suite.
The live source tree is never written.
"""
import argparse
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
ZAL = ROOT / "integrations/zal"
SUITES = ("test_adt", "test_typed2", "test_effects")
OWNED = ("adt.py", "typed2.py", "effects.py", "effects_guide.py", "examples/shell_effects.zal",
         "examples/relay_typed.zal", *(name + ".py" for name in SUITES))
ADT, TYPED, EFFECTS = "test_adt.", "test_typed2.", "test_effects."
HEADLINE = EFFECTS + "ShellRegistryTests.test_shipped_ports_lower_byte_for_byte"
ACKNOWLEDGE = ("operation Acknowledge(store: Store, delivery_id: Hash, payload_sha256: Hash) -> Outcome unit;\n"
               "footprint Acknowledge: capability Relay, reads {durable, volatile}, writes {durable, volatile}, laws [6], "
               "v2 calls [V2SqliteShell::pending_by_id, Pending::relayed, Delivered::acknowledge];\n")
# (label, named test, file, [(original, defect)])
CONTROLS = [
    ("frame-misses-a-variant-case", EFFECTS + "CheckTests.test_frame_must_cover_every_case", "effects.py",
     [("        for case in cases:\n            if case not in covered:", "        for case in []:\n            if case not in covered:")]),
    ("source-answer-misses-a-variant-case", HEADLINE, "examples/shell_effects.zal",
     [(" | not_applied(Fault) | unknown(Fault);", " | not_applied(Fault);")]),
    ("duplicate-case-tag-admitted", ADT + "RefusalTests.test_duplicate_tags_and_fields_refused", "adt.py",
     [("            if name in seen:\n", "            if False:\n")]),
    ("product-computed-as-sum", ADT + "CardinalityTests.test_cardinality_matches_enumeration", "adt.py",
     [('return card_product(counts) if target.form == "record" else card_sum(counts)',
       'return card_sum(counts) if target.form == "record" else card_sum(counts)')]),
    ("footprint-beyond-capability-admitted", EFFECTS + "CheckTests.test_footprint_beyond_capability_fails", "effects.py",
     [("            beyond = [r for r in used if r not in allowed]", "            beyond = []")]),
    ("unknown-law-id-admitted", EFFECTS + "CheckTests.test_unknown_law_fails", "effects.py",
     [("            if law not in ids:\n", "            if False:\n")]),
    ("port-coverage-not-checked", EFFECTS + "CheckTests.test_missing_port_fails", "effects.py",
     [("            if model.lookup(kind, port.name) is None:", "            if False:")]),
    ("source-port-operation-missing", HEADLINE, "examples/shell_effects.zal", [(ACKNOWLEDGE, "")]),
    ("source-port-missing-everywhere", HEADLINE, "examples/shell_effects.zal",
     [(ACKNOWLEDGE, ""), (" Acknowledge = 6, Reopen = 7, Clock = 8, Random = 9, Wait = 10;",
                          " Reopen = 7, Clock = 8, Random = 9, Wait = 10;")]),
    ("source-byte-cap-changed", HEADLINE, "examples/shell_effects.zal",
     [("type Envelope = bytes ..=1048576;", "type Envelope = bytes ..=1048575;")]),
    ("source-capability-allowance-changed", EFFECTS + "ShellRegistryTests.test_declarations_match_the_rust_registry",
     "examples/shell_effects.zal",
     [("capability Clock = 5: reads {clock}, writes {};", "capability Clock = 5: reads {clock}, writes {clock};")]),
    ("lowering-record-and-choice-tags-swapped", HEADLINE, "effects.py",
     [('out.append(5 if target.form == "record" else 6)', 'out.append(6 if target.form == "record" else 5)')]),
    ("recursive-type-admitted", ADT + "RefusalTests.test_recursion_refused", "adt.py",
     [("            raise Refusal(f\"Type {path[0]} is recursive ({cycle}); only finite types are admitted\")",
       "            return")]),
    ("english-byte-cap-read-as-exact", ADT + "RoundTripTests.test_both_surfaces_round_trip", "adt.py",
     [('"bytes_max": lambda: f"at most {args[0]} bytes"', '"bytes_max": lambda: f"exactly {args[0]} bytes"')]),
    ("typed-enumeration-cap-not-named", TYPED + "CapTests.test_enumeration_above_cap_refused_by_name", "typed2.py",
     [("            if count is None or count > MAX_VALUES:", "            if count is None:")]),
    ("typed-cross-type-comparison-admitted", TYPED + "CapTests.test_typed_names_only_compare_for_equality", "typed2.py",
     [('            if op not in {"==", "!="} or name is None or not (right.op == "int" or same):', "            if False:")]),
    ("store-port-without-v2-calls-admitted", EFFECTS + "CheckTests.test_v2_mapping_fails_for_unmapped_store_ports",
     "effects.py", [('        if store and kind != "calls":', "        if False:")]),
    ("construct-without-help", EFFECTS + "GuideTests.test_every_construct_has_help", "effects_guide.py",
     [('    Construct("frame", "statement", "Answer frame",', '    Construct("frames_removed", "statement", "Answer frame",')]),
    ("help-form-drifted-from-the-parser", EFFECTS + "GuideTests.test_every_form_parses_as_its_construct", "effects_guide.py",
     [('"frame ANSWER: CASE applied, CASE unchanged;"', '"frame ANSWER: CASE -> applied, CASE -> unchanged;"')]),
]


def environment():
    target = Path(os.environ.get("CARGO_TARGET_DIR", str(ROOT / "target")))
    return dict(os.environ, CARGO_TARGET_DIR=str((target if target.is_absolute() else ROOT / target).resolve()),
                ZENO_FCIS_ROOT=str(ROOT), CARGO_PROFILE_DEV_DEBUG="0", CARGO_BUILD_JOBS="2", CARGO_NET_OFFLINE="true")


def native_build(env):
    if shutil.disk_usage("/").free < 6 * 1024 ** 3:
        raise RuntimeError("STOP: less than 6 GiB free")
    subprocess.run(["cargo", "+1.97.1", "build", "-p", "zeno-fcis-shell", "--example", "shell_effects_registry",
                    "--locked", "--offline", "-j", "2"], cwd=ROOT, env=env, check=True)


def run(work, targets, env, log):
    result = subprocess.run([sys.executable, "-B", "-m", "unittest", *targets], cwd=work, env=env, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=600)
    log.write_text(result.stdout)
    return result


def planted(env, logs):
    logs.mkdir(parents=True, exist_ok=True)
    before = {name: hashlib.sha256((ZAL / name).read_bytes()).hexdigest() for name in OWNED}
    with tempfile.TemporaryDirectory(prefix="zal-effects-defects-") as directory:
        work = Path(directory) / "zal"
        shutil.copytree(ZAL, work, ignore=shutil.ignore_patterns("__pycache__", "node_modules"))
        for label, test, name, edits in CONTROLS:
            original = (work / name).read_text()
            mutated = original
            for anchor, defect in edits:
                if mutated.count(anchor) != 1:
                    raise RuntimeError("mutation anchor drift: " + label)
                mutated = mutated.replace(anchor, defect)
            (work / name).write_text(mutated)
            built = subprocess.run([sys.executable, "-B", "-c", "import adt, typed2, effects, effects_guide"], cwd=work,
                                   capture_output=True, text=True)
            if built.returncode:
                raise RuntimeError("planted defect did not import: " + label + "\n" + built.stderr[-2000:])
            result = run(work, [test], env, logs / (label + ".log"))
            if result.returncode == 0 or ("FAIL: " + test.rsplit(".", 1)[-1]) not in result.stdout:
                raise RuntimeError("planted defect survived or failed outside its named check: " + label)
            print(f"KILLED {label} by {test.rsplit('.', 1)[-1]}", flush=True)
            (work / name).write_text(original)
        result = run(work, list(SUITES), env, logs / "baseline.log")
        if result.returncode:
            raise RuntimeError("baseline suites failed after restoring the copy")
    after = {name: hashlib.sha256((ZAL / name).read_bytes()).hexdigest() for name in OWNED}
    if after != before:
        raise RuntimeError("live source changed during the planted run")
    print(f"PASS: {len(CONTROLS)} named planted controls; baseline passes; live source unchanged")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("native-build", "planted"))
    parser.add_argument("--logs", type=Path)
    args = parser.parse_args()
    env = environment()
    if args.command == "native-build":
        native_build(env)
    elif args.logs is not None:
        planted(env, args.logs)
    else:
        with tempfile.TemporaryDirectory(prefix="zal-effects-defect-logs-") as logs:
            planted(env, Path(logs))


if __name__ == "__main__":
    main()
