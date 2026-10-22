#!/usr/bin/env python3
"""Replay bounded build-then-assertion controls for ZAL meaning preservation.

Only a temporary source copy is changed. Native contract controls are provided
by test_refinement_binding with ZAL_NATIVE_CLI set; this checks Python defects.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ZAL = Path(__file__).resolve().parent
CONTROLS = (
    ("scope-omitted", "ladder.py",
     "    return (scope_identity(ctx),tree)\n",
     "    return (scope_identity(ctx),tree) if ctx.data['theory']==TEMPORAL_THEORY else tree\n",
     "test_ladder_preservation.PreservationTests.test_file_and_tool_comparison_bind_the_same_declared_scope"),
    ("prime-erased", "ladder.py",
     "    if rung not in ('formula','typed'):\n        n = reading_tree(n, ctx)\n", "",
     "test_ladder_preservation.PreservationTests.test_primed_boolean_definitions_keep_after_state_meaning"),
    ("primitive-state-predicate-admitted", "ladder.py",
     "            if s['kind'] == 'predicate' and s.get('state'):\n                raise Refusal('context:state-predicate-needs-integer-definition')\n", "",
     "test_ladder_preservation.PreservationTests.test_primitive_state_predicates_are_refused_explicitly"),
    ("mutable-policy-index", "behavior2.py",
     '        object.__setattr__(self, "_index", MappingProxyType({key: tuple(rules) for key, rules in index.items()}))',
     '        object.__setattr__(self, "_index", index)',
     "test_refinement_binding.BindingTests.test_policy_indexes_cannot_change_a_revision_bound_decision"),
)


def run(logs):
    logs.mkdir(parents=True, exist_ok=True)
    sources = {name: (ZAL / name).read_bytes() for _, name, *_ in CONTROLS}
    records = []
    with tempfile.TemporaryDirectory(prefix="zal-polish-controls-") as directory:
        work = Path(directory) / "integrations/zal"
        shutil.copytree(ZAL, work, ignore=shutil.ignore_patterns("__pycache__", "node_modules"))
        for label, name, anchor, mutation, test in CONTROLS:
            original = sources[name].decode()
            if original.count(anchor) != 1:
                raise RuntimeError("control anchor changed: " + label)
            target = work / name
            named = test.rsplit(".", 1)[1]
            def invoke(args, phase):
                result = subprocess.run([sys.executable, "-B", *args], cwd=work,
                                        text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=120)
                (logs / (label + "-" + phase + ".log")).write_text(result.stdout)
                return result
            normal = invoke(["-m", "unittest", "-v", test], "normal")
            if normal.returncode:
                raise RuntimeError("normal check failed: " + label)
            target.write_text(original.replace(anchor, mutation))
            built = invoke(["-c", f"import {name.removesuffix('.py')}"], "build")
            if built.returncode:
                raise RuntimeError("mutant failed to import: " + label)
            result = invoke(["-m", "unittest", "-v", test], "mutated")
            if result.returncode == 0 or "FAIL: " + named not in result.stdout or "ERROR:" in result.stdout:
                raise RuntimeError("mutant did not fail its named assertion: " + label)
            records.append({"control": label, "named_test": test, "normal_exit": normal.returncode,
                            "build_exit": built.returncode, "mutated_exit": result.returncode,
                            "source_sha256": hashlib.sha256(sources[name]).hexdigest(),
                            "mutant_sha256": hashlib.sha256(target.read_bytes()).hexdigest()})
            target.write_bytes(sources[name])
            print("KILLED " + label + " by " + named, flush=True)
    if any((ZAL / name).read_bytes() != source for name, source in sources.items()):
        raise RuntimeError("live source changed during controls")
    (logs / "controls.json").write_text(json.dumps(records, indent=2) + "\n")
    print(f"PASS: {len(records)} built controls fail their named assertions; live source unchanged")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--logs", type=Path, required=True)
    run(parser.parse_args().logs)
