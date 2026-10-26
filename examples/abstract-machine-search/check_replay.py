"""Replay the bounded arithmetic example under normal and optimized Python.

This checks reproducibility against the reviewed expected results. It does not
provide a proof certificate or qualify a ZenoFCIS runtime or machine profile.
"""

from pathlib import Path
import json
import subprocess
import sys


def main() -> int:
    root = Path(__file__).resolve().parent
    expected = json.loads((root / "expected.json").read_text(encoding="utf-8"))
    program = root / "machine_microcode_search.py"
    for options in ([], ["-O"]):
        result = subprocess.run(
            [sys.executable, *options, str(program)],
            check=True,
            capture_output=True,
            text=True,
            timeout=30,
        )
        actual = json.loads(result.stdout)
        elapsed = actual.pop("elapsed_seconds", None)
        if not isinstance(elapsed, (int, float)) or elapsed < 0:
            raise ValueError("missing or invalid diagnostic elapsed time")
        if actual != expected:
            mode = "optimized" if options else "normal"
            raise ValueError(f"{mode} replay differs from expected results")

    checks = [
        target["independent_modular_check"]
        for row in expected["results"]
        for target in row["targets"].values()
        if "independent_modular_check" in target
    ]
    cases = sum(check["input_pairs"] for check in checks)
    print(
        "bounded arithmetic replay: PASS "
        f"(normal/-O, {len(checks)} programs, {cases:,} modular cases per run, "
        f"{len(expected['controls'])} negative controls)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
