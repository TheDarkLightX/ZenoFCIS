"""Real startup: eight same-core burners, independent preparation/ACK clocks.

The 180s outer guard is only a harness bound (unoptimized preparation was 54s).
Every product refusal still fails the run, with its private phase observations.
"""
import json
import hashlib
import re
import os
from pathlib import Path
import subprocess
import sys
import time
import tempfile
import shutil

cli, contract, backend = sys.argv[1:]
cpu = min(os.sched_getaffinity(0))
os.sched_setaffinity(0, {cpu})
operator = Path(f"/tmp/zeno-fcis-operator-{os.geteuid()}")

def start_ticks(pid):
    return int(Path(f"/proc/{pid}/stat").read_text().rsplit(") ", 1)[1].split()[19])

def records(pid, ticks):
    assert isinstance(ticks, int) and ticks > 0, (pid, ticks)
    rows = [json.loads(p.read_text()) for p in operator.glob(f"{pid}-*.json")]
    return [r for r in rows if r.get("process_start_ticks") == ticks]

def unit_records(unit):
    tag = hashlib.sha256(unit.encode()).hexdigest()
    rows = []
    for p in operator.glob("*.json"):
        # New records carry the unit in the filename; corrupt matching JSON
        # must raise. Legacy records need a textual unit filter before parsing.
        if "-unit-" in p.name:
            if f"-unit-{tag}-" not in p.name:
                continue
            row = json.loads(p.read_text())
            assert row.get("unit") == unit, (p, row)
        else:
            text = p.read_text()
            if not re.search(r'"unit"\s*:\s*' + re.escape(json.dumps(unit)), text):
                continue
            row = json.loads(text)
        if row.get("observer") == "worker-startup":
            rows.append(row)
    return rows

def phases(pid, ticks):
    caller = [r for r in records(pid, ticks) if r["observer"] == "caller"]
    if backend == "posix":
        assert len(caller) == 1, caller
        caller = caller[0]
        assert caller["guardian_budget_ms"] == 30000, caller
        assert caller["guardian_observed_ms"] <= 31000, caller
        supervisor_pid = caller["guardian_pid"]
        supervisor_ticks = caller["guardian_start_ticks"]
        assert caller["preparation_ms"] >= 0, caller
        preparation = [r for r in records(supervisor_pid, supervisor_ticks) if r["observer"] == "guardian-preparation"]
        assert len(preparation) == 1, preparation
        assert preparation[0]["preparation_ms"] < 30000, preparation
        assert preparation[0]["invocation_sha256"] == caller["invocation_sha256"], (preparation, caller)
    else:
        caller = None
        supervisor_pid = pid
        supervisor_ticks = ticks
    supervisor = [r for r in records(supervisor_pid, supervisor_ticks) if r["observer"] == "worker-supervisor"]
    assert len(supervisor) == 1, supervisor
    supervisor = supervisor[0]
    assert supervisor["worker_budget_ms"] == 25000, supervisor
    assert supervisor["worker_observed_ms"] <= 26000, supervisor
    assert supervisor["ack_budget_ms"] == 10000, supervisor
    if backend == "posix":
        startup = [r for r in records(supervisor["worker_pid"], supervisor["worker_start_ticks"]) if r["observer"] == "worker-startup"]
    else:
        startup = unit_records(supervisor["unit"])
        assert supervisor["manager_query_budget_ms"] == 5000, supervisor
        assert supervisor["manager_query_ms"] <= 6000, supervisor
    assert len(startup) == 1, (caller, supervisor, startup)
    startup = startup[0]
    assert startup["ack_budget_ms"] == 10000, startup
    assert startup["ack_wait_ms"] <= 11000, startup
    return {"caller": caller, "guardian_preparation": preparation[0] if backend == "posix" else None, "supervisor": supervisor, "startup": startup}

burners = []
results = []
try:
    for _ in range(8):
        burners.append(subprocess.Popen([sys.executable, "-I", "-S", "-B", "-c", "while True: pass"],
                                        stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                        stderr=subprocess.DEVNULL))
    temporary = tempfile.TemporaryDirectory(prefix="zeno-startup-contention-")
    fixture = Path(temporary.name)
    # Keep the counter's complete 64-input scalar domain for the startup test.
    # Delivery-law gap enumeration is separately tested and is not this test's
    # subject. Use an owned fixture rather than changing the shipped contract.
    scalar_contract = fixture / "scalar-counter"
    shutil.copytree(contract, scalar_contract)
    policy_path = scalar_contract / "v2" / "policy.json"
    policy = json.loads(policy_path.read_text())
    delivery_ids = set(policy["delivery_laws"])
    project = scalar_contract / "project.zeno"
    project.write_text("\n".join(line for line in project.read_text().splitlines()
        if not any(line.startswith(f"law {law_id} ") for law_id in delivery_ids)) + "\n")
    policy["law_kinds"] = {key: value for key, value in policy["law_kinds"].items()
                           if key not in delivery_ids}
    policy["delivery_laws"] = {}
    policy_path.write_text(json.dumps(policy))
    spec = fixture / "spec.json"
    reference = fixture / "reference.py"
    spec.write_text(json.dumps({"inputs": [{"kind": "Int", "min": "0", "max": "3"}], "outputs": [{"kind": "Bool"}]}))
    reference.write_text("def reference(p): return [int(p[0] < 2)]\n")
    for ordinal in range(4):
        started = time.monotonic()
        args = [cli, "--no-kb", "--containment-backend", backend, "loop"]
        if backend == "posix":
            args += ["question", "--contract", str(scalar_contract)]
        else:
            args += ["capture", "open", "--spec", str(spec), "--reference", str(reference),
                     "--session", str(fixture / f"session-{ordinal}")]
        child = subprocess.Popen(args, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        ticks = start_ticks(child.pid)
        try:
            stdout, stderr = child.communicate(timeout=180)
        except BaseException:
            if child.poll() is None:
                child.kill()
            child.wait()
            raise
        try:
            observed = phases(child.pid, ticks)
        except AssertionError as error:
            raise AssertionError({"exit": child.returncode, "stdout": stdout, "stderr": stderr,
                                  "operator_records": records(child.pid, ticks), "phase_error": str(error)}) from error
        results.append({"whole_cli_seconds": time.monotonic() - started, **observed})
        assert child.returncode == 0, (child.returncode, stdout, stderr, results)
        report = json.loads(stdout)
        assert report["status"] == ("open" if backend == "posix" else "opened"), report
        assert report["authority"] == "none", report
        assert report["backend"] == backend, report
        if backend == "posix":
            assert report["containment"]["evidence"] == "private-append-only-journal", report
            assert observed["caller"]["supervisor_reason"] is None, observed
        assert observed["supervisor"]["supervisor_reason"] is None, observed
        assert observed["supervisor"]["startup_ack_sent"] is True, observed
        assert observed["supervisor"]["cleanup_verified"] is True, observed
        assert observed["startup"]["ack_accepted"] is True, observed
        assert observed["startup"]["startup_reason"] is None, observed
        assert observed["startup"]["ack_wait_ms"] < 10000, observed
        assert not stderr, (stderr, observed)
        assert all(p.poll() is None for p in burners), "contention exited prematurely"
finally:
    if "temporary" in locals():
        temporary.cleanup()
    # Only these owned direct children; no process-group signals after reap.
    for p in burners:
        if p.poll() is None:
            p.terminate()
    for p in burners:
        p.wait(timeout=5)
print(json.dumps(results, sort_keys=True))
print(f"4 actual {backend.upper()} startup runs passed with 8 same-core CPU burners")
