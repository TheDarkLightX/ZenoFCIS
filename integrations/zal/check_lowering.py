"""Run native lowering regressions and the buildable weakened-property control."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


HERE = Path(__file__).resolve().parent
TEST = "test_lowering.NativeLoweringTests.test_revoked_mail_contract_refines"


def run(cli, logs):
    logs.mkdir(parents=True, exist_ok=True)
    env = {**os.environ, "ZAL_LOWERING_TEST_CLI": str(cli.resolve())}
    env.pop("ZAL_LOWERING_CONTROL", None)
    reports = []
    for label, target, control in [("normal", "test_lowering", False), ("weakened", TEST, True), ("restored", TEST, False)]:
        current = {**env, **({"ZAL_LOWERING_CONTROL": "weaken-revocation"} if control else {})}
        checked = subprocess.run([sys.executable, "-B", "-m", "unittest", target, "-q"], cwd=HERE, env=current,
                                 stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, timeout=120)
        (logs / f"{label}.log").write_text(checked.stdout)
        if control:
            if checked.returncode != 1 or "FAIL: test_revoked_mail_contract_refines" not in checked.stdout or "AssertionError" not in checked.stdout:
                raise RuntimeError("control did not fail its named refinement assertion")
        elif checked.returncode:
            raise RuntimeError(f"{label} lowering tests failed; see {logs / (label + '.log')}")
        report = next((json.loads(line) for line in checked.stdout.splitlines() if line.startswith('{"authority"')), None)
        if not report or report["schema"] != "zal/contract-refinement/1":
            raise RuntimeError("missing native refinement report")
        reports.append({"case": label, "exit": checked.returncode, "result": report})
    result = {"schema": "zal/lowering-control/1", "test": TEST, "results": reports, "authority": "none"}
    (logs / "results.json").write_text(json.dumps(result, indent=2) + "\n")
    print("PASS: native lowering, buildable weakened revocation assertion, restored baseline; authority: none")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    root = HERE.parents[1]
    target = Path(os.environ.get("CARGO_TARGET_DIR", str(root / "target")))
    if not target.is_absolute():
        target = root / target
    parser.add_argument("--cli", type=Path, default=target / "debug/zeno-fcis")
    parser.add_argument("--logs", type=Path)
    options = parser.parse_args()
    if options.logs:
        run(options.cli, options.logs)
    else:
        with tempfile.TemporaryDirectory(prefix="zal-lowering-controls-") as tmp:
            run(options.cli, Path(tmp))


if __name__ == "__main__":
    main()
