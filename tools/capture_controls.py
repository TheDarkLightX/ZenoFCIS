"""Non-vacuous runtime controls for black-box capture; run after a heavy grant.

Every expected refutation/refusal/budget outcome is checked explicitly. Missing
runtime support fails Linux qualification rather than skipping all controls.
--stub modes MUST fail. This harness never decides candidate acceptance.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile

import capture_reference as R


def compare(actual, expected):
    return actual == expected


def harness_self_test(stub=None):
    equal = (lambda *_: True) if stub == "comparison" else compare
    return equal("captured-exact", "captured-exact") and not equal("captured-exact", "refuted") and \
        not equal("refused", "captured-exact") and not equal("inconclusive", "captured-exact")


class Controls:
    def __init__(self, cli, directory, stub=None):
        self.cli, self.directory, self.stub = str(cli), Path(directory), stub
        self.rows = []
        self.sequence = 0
        (self.directory / "parent-only.txt").write_text("parent descriptor control", encoding="ascii")

    def call(self, *args):
        with (self.directory / "parent-only.txt").open("rb") as inherited:
            done = subprocess.run([self.cli, *map(str, args)], capture_output=True, timeout=35,
                                  env={**os.environ, "CAPTURE_CONTROL_SECRET": "parent-environment-control"},
                                  pass_fds=(inherited.fileno(),))
        if len(done.stdout) > 8 << 20:
            raise ValueError("output bound")
        return done.returncode, json.loads(done.stdout)

    def write(self, path, data):
        path.write_text(data if type(data) is str else json.dumps(data), encoding="ascii")
        return path

    def open(self, source, spec=None, **limits):
        self.sequence += 1
        folder = self.directory / str(self.sequence)
        folder.mkdir()
        spec = spec or {"inputs": [{"kind": "Int", "min": "0", "max": "3"}], "outputs": [{"kind": "Bool"}]}
        self.write(folder / "spec.json", spec)
        self.write(folder / "reference.py", source)
        flags = [item for k, v in limits.items() for item in ("--" + k.replace("_", "-"), str(v))]
        code, reply = self.call("loop", "capture", "open", "--spec", folder / "spec.json", "--reference",
                                folder / "reference.py", "--session", folder / "session", *flags)
        if self.stub == "reference" and reply.get("detail", {}).get("reason") == "capture-reference-nondeterministic":
            reply["status"] = "opened"
        return folder, code, reply

    def candidate(self, folder, fixture):
        before = json.loads((folder / "session" / "capture.json").read_text())
        path = self.write(folder / "candidate.json", fixture)
        code, reply = self.call("loop", "capture", "candidate", "--session", folder / "session", "--candidate-json", path)
        if self.stub == "exact":
            reply["status"] = "captured-exact"
        if reply.get("status") in {"refuted", "precheck-refuted"}:
            selected = before["examples"]
            expected = R.policies(before["spec"], before["rows"], fixture, selected,
                                  anchor=selected[-1] if selected else None)
            ordinal = expected["precheck_failure"] if reply["status"] == "precheck-refuted" else expected["chosen"]
            detail = reply.get("detail", {})
            ps = R.points(before["spec"])
            passed = detail.get("feedback") == "example-budget" and detail.get("example") is None
            passed = passed or ordinal is not None and detail.get("example") == {
                "input": list(ps[ordinal]), "output": before["rows"][ordinal]}
            if "coverage" in detail:
                after = json.loads((folder / "session" / "capture.json").read_text())
                covered = R.policies(before["spec"], before["rows"], fixture, after["examples"])["coverage"]
                expected_outcomes = [{k: v for k, v in row.items() if k != "first"} for row in covered]
                passed = passed and detail["coverage"].get("outcomes") == expected_outcomes
            self.row(f"independent-example-policy-{folder.name}-{before['invocations']}", passed,
                     {"status": reply["status"], "chosen": ordinal})
        return code, reply

    def row(self, name, passed, evidence):
        self.rows.append({"control": name, "passed": bool(passed), "evidence": evidence})

    def semantic_cases(self):
        # Fixed semantic controls, not a live proposer or a capture-rate study.
        integer = {"kind": "Int", "min": "0", "max": "7"}
        cases = []
        for lo, hi in ((0, 0), (0, 7), (2, 5), (7, 7)):
            spec = {"inputs": [integer], "outputs": [{"kind": "Bool"}]}
            nodes = [["Input", 0], ["Int", str(lo)], ["Int", str(hi)], ["Lt", 0, 1],
                     ["Not", 3], ["Lt", 2, 0], ["Not", 5], ["And", 4, 6]]
            source = f"def reference(p): return [int({lo} <= p[0] <= {hi})]"
            cases.append((f"interval-{lo}-{hi}", spec, source, nodes, [7],
                          [[int(lo <= x <= hi)] for x in range(8)]))
        for center in (0, 3, 7):
            spec = {"inputs": [integer], "outputs": [integer]}
            nodes = [["Input", 0], ["Int", str(center)], ["Lt", 0, 1], ["Sub", 1, 0],
                     ["Sub", 0, 1], ["Select", 2, 3, 4]]
            cases.append((f"absolute-{center}", spec, f"def reference(p): return [abs(p[0]-{center})]",
                          nodes, [5], [[abs(x - center)] for x in range(8)]))
        for threshold in (1, 3, 7):
            spec = {"inputs": [integer, {"kind": "Bool"}],
                    "outputs": [{"kind": "Int", "min": "0", "max": "8"}]}
            nodes = [["Input", 0], ["Input", 1], ["Int", str(threshold)], ["Lt", 0, 2],
                     ["And", 1, 3], ["Int", "1"], ["Add", 0, 5], ["Select", 4, 6, 0]]
            source = f"def reference(p): return [p[0]+1 if p[1]==1 and p[0]<{threshold} else p[0]]"
            expected = [[x + int(a == 1 and x < threshold)] for x in range(8) for a in range(2)]
            cases.append((f"guarded-{threshold}", spec, source, nodes, [7], expected))
        for name, spec, source, nodes, roots, expected in cases:
            fresh, code, opened = self.open(source, spec)
            if code != 0:
                self.row("semantic-" + name, False, opened)
                continue
            code, captured = self.candidate(fresh, {**spec, "nodes": nodes, "roots": roots})
            _, owner = self.call("loop", "capture", "status", "--full", "--session", fresh / "session")
            passed = code == 0 and compare(captured.get("status"), "captured-exact") and \
                owner.get("detail", {}).get("session", {}).get("rows") == expected and R.check_owner_report(owner)
            self.row("semantic-" + name, passed, {"status": captured.get("status"), "domain": len(expected)})

    def run(self):
        spec = {"inputs": [{"kind": "Int", "min": "0", "max": "3"}], "outputs": [{"kind": "Bool"}]}
        source = "def reference(p): return [int(p[0] < 2)]\n"
        right = {**spec, "nodes": [["Input", 0], ["Int", "2"], ["Lt", 0, 1]], "roots": [2]}
        wrong = {**spec, "nodes": [["Bool", False]], "roots": [0]}
        folder, code, reply = self.open(source)
        self.row("complete-acquisition", code == 0 and compare(reply.get("status"), "opened"), reply)
        if code != 0:
            # Fail, do not turn an unavailable runtime into an all-skipped PASS.
            return False
        _, first = self.candidate(folder, wrong)
        self.row("examples-equivalent-is-not-capture", first.get("detail", {}).get("precheck") == "examples-equivalent"
                 and compare(first.get("status"), "refuted"), first)
        _, repeated = self.candidate(folder, wrong)
        self.row("precheck-every-candidate", compare(repeated.get("status"), "precheck-refuted") and
                 repeated.get("detail", {}).get("exact") == "not-run", repeated)
        code, captured = self.candidate(folder, right)
        self.row("exact-positive", code == 0 and compare(captured.get("status"), "captured-exact") and
                 captured["detail"]["check"]["replay"] == "matched", captured)
        code, terminal = self.candidate(folder, wrong)
        self.row("terminal-session-never-credits-new-proposal", code != 0 and
                 compare(terminal.get("status"), "refused"), terminal)
        _, owner = self.call("loop", "capture", "status", "--full", "--session", folder / "session")
        checked = R.check_owner_report(owner)
        if self.stub == "replay":
            checked = True
        self.row("independent-capture-replay", checked, {"status": owner.get("status")})
        planted = json.loads(json.dumps(owner))
        planted["detail"]["session"]["candidate"] = wrong
        bad_replay = True if self.stub == "replay" else R.check_owner_report(planted)
        self.row("independent-replay-planted-fault", not bad_replay, {})
        # Recompute a receipt with the ordinary transform commands, byte for byte.
        original_json = self.write(folder / "table.json", owner["detail"]["table_program"])
        candidate_json = self.write(folder / "captured.json", right)
        for label, path in (("table", original_json), ("captured", candidate_json)):
            code, encoded = self.call("loop", "encode", "--program", path, "--out", folder / (label + ".zcve"))
            if code != 0 or encoded.get("status") != "encoded":
                raise ValueError("encode refused")
        receipt = folder / "receipt.json"
        code, exact = self.call("transform", "check", "--original", folder / "table.zcve", "--candidate", folder / "captured.zcve",
                                "--max-input-tuples", 4096, "--receipt", receipt)
        expected_receipt = (json.dumps(owner["detail"]["session"]["receipt"], sort_keys=True,
                                      separators=(",", ":"), ensure_ascii=True) + "\n").encode("ascii")
        self.row("separate-transform-receipt", code == 0 and receipt.read_bytes() == expected_receipt, exact)
        code, replay = self.call("transform", "replay", "--original", folder / "table.zcve", "--candidate", folder / "captured.zcve",
                                 "--max-input-tuples", 4096, "--receipt", receipt)
        self.row("separate-transform-replay", code == 0 and replay.get("status") == "replayed", replay)
        # Single-point faults in the first, middle and last tuples must refute.
        for ordinal in (0, 2, 3):
            fresh, _, opened = self.open(source)
            fixture = {**spec, "nodes": right["nodes"] + [["Int", str(ordinal)], ["Eq", 0, 3],
                ["Not", 2], ["Select", 4, 5, 2]], "roots": [6]}
            _, refuted = self.candidate(fresh, fixture)
            self.row(f"single-point-fault-{ordinal}", compare(refuted.get("status"), "refuted") and
                     refuted.get("detail", {}).get("example", {}).get("input") == [ordinal], refuted)
        # Distant failure is chosen from the last selected example, not the first
        # checker witness or an input the model supplies.
        fresh, _, _ = self.open(source)
        self.candidate(fresh, wrong)
        _, varied = self.candidate(fresh, {**spec, "nodes": [["Bool", True]], "roots": [0]})
        self.row("vary-counterexamples", varied.get("detail", {}).get("example", {}).get("input") == [3], varied)
        integer = {"kind": "Int", "min": "0", "max": "3"}
        coverage_spec = {"inputs": [integer], "outputs": [integer]}
        fresh, _, _ = self.open("def reference(p): return [p[0]]", coverage_spec)
        _, coverage = self.candidate(fresh, {**coverage_spec, "nodes": [["Input", 0], ["Int", "2"], ["Lt", 0, 1],
            ["Int", "0"], ["Int", "3"], ["Select", 2, 3, 4]], "roots": [5]})
        outcomes = coverage.get("detail", {}).get("coverage", {}).get("outcomes", [])
        self.row("cover-both-reachable-decisions", len(outcomes) == 2 and all(o["reachable"] and o["covered"] for o in outcomes), coverage)
        # Budgets stop without a candidate attempt; intake reservations persist.
        for label, limits in (("round", {"rounds": 0}), ("work", {"node_visits": 0})):
            fresh, _, opened = self.open(source, **limits)
            code, stopped = self.candidate(fresh, wrong)
            state = json.loads((fresh / "session" / "capture.json").read_text())
            self.row(label + "-budget-no-attempt", code == 2 and compare(stopped.get("status"), "inconclusive") and
                     state["attempts"] == 0, {"result": stopped, "reserved_visits": state["reserved_visits"]})
        fresh, _, _ = self.open(source, max_examples=0)
        code, result = self.candidate(fresh, wrong)
        state = json.loads((fresh / "session" / "capture.json").read_text())
        self.row("decided-refutation-example-budget-spends-attempt", code == 1 and
                 result.get("status") == "refuted" and result.get("detail", {}).get("feedback") == "example-budget" and
                 state["attempts"] == 1, result)
        fresh, code, domain = self.open("invalid source must never execute", max_tuples=3)
        self.row("domain-budget-no-execution", code == 2 and compare(domain.get("status"), "inconclusive") and
                 not (fresh / "session").exists(), domain)
        nondeterministic = "n=0\ndef reference(p):\n global n\n n+=1\n return [int(n>4)]\n"
        failures = {"crash": "def reference(p): raise ValueError('hidden')", "nondeterministic": nondeterministic,
                    "Boolean-alias": "def reference(p): return [True]", "float": "def reference(p): return [0.0]",
                    "output-domain": "def reference(p): return [2]", "arity": "def reference(p): return [0,1]",
                    "late-crash": "def reference(p):\n if p[0]==3: raise ValueError()\n return [0]",
                    "timeout": "def reference(p):\n while True: pass", "memory": "def reference(p): return [len(bytearray(256<<20))]",
                    "native-crash": "import ctypes\ndef reference(p): return [ctypes.string_at(0)]",
                    "file-size": "def reference(p):\n with open('scratch.txt','w') as f: f.write('x'*65537)\n return [0]",
                    "stdout-bound": "def reference(p):\n print('x'*(3<<20))\n return [0]"}
        for label, reference in failures.items():
            fresh, code, refused = self.open(reference)
            self.row("reference-" + label, code != 0 and compare(refused.get("status"), "refused") and
                     not (fresh / "session").exists(), refused)
        fresh, code, unsupported = self.open(source, reference_format="native-binary")
        self.row("unsupported-format", code != 0 and compare(unsupported.get("status"), "refused"), unsupported)
        # Negative capability control: outside content/write and raw network,
        # process/exec/memory syscalls must all be denied, without leaking paths.
        secret = self.write(self.directory / "private-content.txt", "PRIVATE-CONTENT")
        outside = self.directory / "forbidden-write.txt"
        confined = "import ctypes\nimport os\ndef reference(p):\n denied=0\n"
        confined += f" for path,mode in [({str(secret)!r},'r'),({str(outside)!r},'w'),('/proc/self/environ','r')]:\n"
        confined += "  try: open(path,mode)\n  except PermissionError: denied+=1\n"
        confined += " libc=ctypes.CDLL(None,use_errno=True)\n for number,args in [(41,(2,1,0)),(57,()),(59,(0,0,0)),(101,(0,0,0,0)),(310,(0,0,0,0,0,0)),(72,(1,8,1))]:\n"
        confined += "  if libc.syscall(number,*args)==-1 and ctypes.get_errno()==1: denied+=1\n return [denied]\n"
        spec9 = {"inputs": [], "outputs": [{"kind": "Int", "min": "0", "max": "9"}]}
        fresh, code, denied = self.open(confined, spec9)
        if code == 0:
            _, owner_denied = self.call("loop", "capture", "status", "--full", "--session", fresh / "session")
            sandboxed = owner_denied["detail"]["session"]["rows"] == [[9]]
        else:
            sandboxed = False
        self.row("kernel-content-network-process-denials", sandboxed and not outside.exists(), {"status": denied.get("status")})
        # Positive capability and inheritance controls prevent a blanket worker
        # refusal from counting as successful containment qualification.
        scratch_source = "def reference(p):\n with open('scratch.txt','w') as f: f.write('safe')\n with open('scratch.txt') as f: return [len(f.read())]\n"
        inheritance = "import os\ndef reference(p):\n for fd in range(3,4096):\n  try: os.fstat(fd)\n  except OSError: continue\n  return [0]\n return [int('CAPTURE_CONTROL_SECRET' not in os.environ)]\n"
        for name, reference, expected in (("scratch-write-read", scratch_source, 4),
                                          ("no-parent-descriptor-or-environment", inheritance, 1)):
            fresh, code, opened = self.open(reference, spec9)
            if code == 0:
                _, observed = self.call("loop", "capture", "status", "--full", "--session", fresh / "session")
                passed = observed["detail"]["session"]["rows"] == [[expected]]
            else:
                passed = False
            self.row(name, passed, {"status": opened.get("status")})
        public = first["detail"]
        self.row("public-feedback-no-reference", "receipt_sha256" not in public.get("check", {}) and
                 "source_sha256" not in json.dumps(first) and "table_sha256" not in json.dumps(first), {"fields": sorted(public)})
        self.semantic_cases()
        return bool(self.rows) and all(r["passed"] for r in self.rows)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--stub", choices=["comparison", "exact", "reference", "replay"])
    args = parser.parse_args(argv)
    if args.self_test:
        passed = harness_self_test(args.stub)
        print(json.dumps({"harness_self_test": "PASS" if passed else "FAIL", "stub": args.stub}))
        return 0 if passed else 1
    if args.stub == "comparison":
        parser.error("comparison is a harness-only stub; use --self-test. Product seams are in Rust tests")
    if args.cli is None or args.out is None or args.out.exists():
        parser.error("give --cli and a fresh --out directory")
    args.out.mkdir(parents=True)
    with tempfile.TemporaryDirectory(prefix="capture-controls-") as scratch:
        controls = Controls(args.cli, scratch, args.stub)
        try:
            passed = controls.run()
        except (ValueError, KeyError, TypeError, OSError, subprocess.TimeoutExpired) as error:
            controls.row("harness-completed", False, {"error_type": type(error).__name__})
            passed = False
        result = {"status": "PASS" if passed else "FAIL", "authority": "none", "rows": controls.rows,
                  "applicable": len(controls.rows), "failed": sum(not r["passed"] for r in controls.rows), "stub": args.stub}
        (args.out / "controls.json").write_text(json.dumps(result, sort_keys=True, indent=2) + "\n")
    print(json.dumps({k: v for k, v in result.items() if k != "rows"}))
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
