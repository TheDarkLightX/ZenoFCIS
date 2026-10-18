"""Regression for the independent paused unrelated-record writer witness.

Use the actual reader functions with isolated records. Foreign partial JSON
must not break a completed invocation; corrupt matching JSON must still fail.
"""
import ast
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import threading


def load(source):
    tree = ast.parse(source.read_text(), filename=str(source))
    nodes = [n for n in tree.body if isinstance(n, (ast.Import, ast.ImportFrom, ast.FunctionDef))]
    ns = {}
    exec(compile(ast.Module(body=nodes, type_ignores=[]), str(source), "exec"), ns)
    return ns


def exercise(source, host=False):
    ns = load(source)
    with tempfile.TemporaryDirectory(prefix="zeno-record-race-") as tmp:
        root = Path(tmp)
        unit = "zeno-checker-42-0.service"
        supervisor = dict(observer="worker-supervisor", pid=42, process_start_ticks=123,
                          unit=unit, worker_budget_ms=25000, worker_observed_ms=250,
                          ack_budget_ms=10000, worker_pid=43, worker_start_ticks=124,
                          manager_query_budget_ms=5000, manager_query_ms=10, startup_ack_sent=True)
        startup = dict(observer="worker-startup", pid=44, process_start_ticks=125,
                       unit=unit, ack_budget_ms=10000, ack_wait_ms=10)
        (root/"42-supervisor.json").write_text(json.dumps(supervisor))
        own = root/("44-unit-"+hashlib.sha256(unit.encode()).hexdigest()+"-record.json")
        own.write_text(json.dumps(startup))
        ns.update(operator=root, backend="systemd")
        call = (lambda: ns["observations"](42)) if host else (lambda: ns["phases"](42,123))
        call()
        opened, resume = threading.Event(), threading.Event()
        def writer():
            with (root/"99999999-in-progress.json").open("w") as f:
                opened.set()
                assert resume.wait(5)
                f.write('{"observer":"worker-startup","unit":"other-unit"}')
        thread = threading.Thread(target=writer); thread.start()
        assert opened.wait(5)
        try:
            call()  # The exact independent witness: unrelated final-name file is empty.
        finally:
            resume.set(); thread.join()
        call()
        own.write_text("{")  # Unit-bearing filename selects before decoding.
        try:
            call()
        except json.JSONDecodeError:
            pass
        else:
            raise AssertionError("matching malformed JSON was ignored")
        own.unlink()
        try:
            call()
        except AssertionError:
            pass
        else:
            raise AssertionError("missing matching startup record was ignored")
        # Legacy baseline: select its unit from text before parsing JSON.
        own = root/"44-legacy.json"; own.write_text(json.dumps(startup)); call()
        own.write_text('{"unit": "'+unit+'", "broken":')
        try:
            call()
        except json.JSONDecodeError:
            pass
        else:
            raise AssertionError("matching malformed legacy JSON was ignored")
    print("operator record race and matching-corruption refusals passed:",source)


if __name__ == "__main__":
    exercise(Path(__file__).with_name("startup_contention.py"))
    if len(sys.argv) > 1:
        exercise(Path(sys.argv[1]), host=True)
