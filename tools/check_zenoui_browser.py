#!/usr/bin/env python3
"""Run the generated-app browser matrix using only hashed DevTools fulfilment."""
import argparse
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "site/tests"))
from deploy_check import find_chrome
from socket_free_check import short_profile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True, help="check_zenoui.py evidence directory")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--chrome")
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    chrome = find_chrome(args.chrome)
    manifest = args.out.resolve() / "assets.json"
    subprocess.run(["node", str(ROOT / "site/tests/asset-manifest.mjs"), str(args.root.resolve()), str(manifest)], check=True)
    # Chrome's Unix socket path must fit even when the caller has a long TMPDIR.
    with short_profile() as profile:
        return subprocess.run(["node", str(ROOT / "site/tests/pipe-browser.mjs"), chrome,
            profile, str(args.root.resolve()), str(manifest), "-", str(args.out.resolve() / "browser.json")],
            close_fds=True, timeout=600).returncode


if __name__ == "__main__":
    from telemetry import run_cli
    def _entrypoint():
        sys.exit(main())
    run_cli(_entrypoint)
