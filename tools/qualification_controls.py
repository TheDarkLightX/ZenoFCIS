"""Bound independent controls without changing their evidence or input order.

Workers own their specimen, subprocesses and result row. Only the coordinator
publishes ordered result lists. All submitted controls finish, including after
a refusal; the first exception in declared order is then raised. Timing is
optional, separate from the deterministic evidence receipt.
"""
from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor
import json
import os
from pathlib import Path
import time


def control_jobs(environment=None) -> int:
    settings = os.environ if environment is None else environment
    value = settings.get("ZENO_FCIS_CONTROL_JOBS", "1")
    if value not in tuple(str(i) for i in range(1, 9)):
        raise ValueError("ZENO_FCIS_CONTROL_JOBS must be an integer from one to eight")
    return int(value)


class ControlRefused(RuntimeError):
    """A completed control whose refusal row must survive the failed gate."""
    def __init__(self, result, message):
        super().__init__(message)
        self.result = result


def run_controls(worker, items, *, timings: Path | None = None, publish=None):
    """Run each item exactly once; return rows in input order or refuse.

    No change to proof threads, solver limits, classification or control sets.
    The caller must isolate writable paths and avoid mutating shared inputs.
    The serial default uses the same all-controls refusal accounting as a pool.
    """
    jobs = control_jobs()
    items = tuple(items)
    records = None
    if timings is not None:
        records = timings.parent / (timings.stem + "-records")
        records.mkdir(parents=True, exist_ok=True)

    def completed(indexed):
        index, item = indexed
        started = time.monotonic_ns()
        try:
            row, failure = worker(item), None
        except Exception as error:
            row, failure = getattr(error, "result", None), error
            if row is None:
                row = {"name": item[0], "killed": False, "error": str(error),
                       "error_type": type(error).__name__}
        elapsed = time.monotonic_ns() - started
        if records is not None:
            # Preserve completed work immediately, even while earlier controls
            # are still running. One writer owns each file; the aggregate stays
            # coordinator-owned and ordered. This is evidence, not a reuse cache.
            (records / f"{index:04d}.json").write_text(json.dumps({
                "index": index, "name": item[0], "elapsed_ns": elapsed,
                "result": row, "error": None if failure is None else str(failure),
            }, indent=2) + "\n")
        return row, failure, elapsed

    started = time.monotonic_ns()
    if jobs == 1:
        outcomes = [completed(item) for item in enumerate(items)]
    else:
        with ThreadPoolExecutor(max_workers=jobs) as pool:
            outcomes = list(pool.map(completed, enumerate(items)))
    if timings is not None:
        timings.write_text(json.dumps({
            "jobs": jobs, "elapsed_ns": time.monotonic_ns() - started,
            "controls": [{"index": i, "name": item[0], "elapsed_ns": elapsed,
                          "completed": error is None,
                          "error": None if error is None else str(error)}
                         for i, (item, (_, error, elapsed)) in enumerate(zip(items, outcomes))],
        }, indent=2) + "\n")
    if publish is not None:
        for row, _, _ in outcomes:
            if row is not None:
                publish(row)
    for _, error, _ in outcomes:
        if error is not None:
            raise error
    return [row for row, _, _ in outcomes]
