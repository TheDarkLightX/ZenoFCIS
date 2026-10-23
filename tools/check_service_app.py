#!/usr/bin/env python3
"""Build a fresh service and check its complete byte-stream journey."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
LOW_PRIORITY = ["nice", "-n", "19", "ionice", "-c3"]


def main():
    with tempfile.TemporaryDirectory(prefix="service-app-check-") as temporary:
        work = Path(temporary)
        application = work / "application"
        subprocess.run(LOW_PRIORITY + [sys.executable, str(ROOT / "tools/service_app_factory.py"),
            "--service", str(ROOT / "examples/composition-service/runtime/service.json"),
            "--out", str(application)], cwd=ROOT, check=True)
        target = Path(os.environ.get("CARGO_TARGET_DIR", application / "target"))
        if not target.is_absolute():
            target = application / target
        subprocess.run(LOW_PRIORITY + [sys.executable, str(ROOT / "tools/test_service_app.py"),
            "--transport", "stdio", "--app", str(target / "debug/composition-service-app"),
            "--work", str(work / "journey")], cwd=ROOT, check=True)
    print("service app: complete stdio journey passed; no live socket claim")


if __name__ == "__main__":
    main()
