#!/usr/bin/env python3
"""Checks the exact Pages artifact in a browser.

Serves site/public as `actions/upload-pages-artifact` uploads it, at a subpath
(`/ZenoFCIS/` by default) of a local server that serves nothing at the root,
and drives headless Chrome over it with `--dump-dom` under a virtual-time
budget, which lets the page finish its fetches and timers before the DOM is
printed:

1. the harness, site/tests/harness/, served beside the artifact: it imports
   the page's own loader and demonstration script from the artifact, runs the
   README's demonstration through the module, and prints the results, which
   must match the gate's expected summary in tools/check_generated_application.py;
2. the page itself: it must have loaded its module and rendered the genesis.

A page that loaded from the subpath alone is a page whose relative paths hold.

Usage: python3 site/tests/deploy_check.py [--chrome PATH] [--subpath /ZenoFCIS/]
"""

from __future__ import annotations

import argparse
import html
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parents[2]
PUBLIC = ROOT / "site" / "public"
HARNESS = ROOT / "site" / "tests" / "harness"
HARNESS_PREFIX = "/harness/"
CHROME_NAMES = ("google-chrome", "google-chrome-stable", "chromium", "chromium-browser")
CONTENT_TYPES = {
    ".html": "text/html; charset=utf-8",
    ".js": "text/javascript; charset=utf-8",
    ".css": "text/css; charset=utf-8",
    ".wasm": "application/wasm",
    ".json": "application/json",
}
# Chrome prints the DOM once the budget is spent or the page is idle; the
# budget is virtual time, so timers on the page cost nothing.
VIRTUAL_TIME_BUDGET_MS = 30_000

sys.path.insert(0, str(ROOT / "tools"))
import check_generated_application as gate  # noqa: E402


class DeployCheckError(Exception):
    pass


def artifact_handler(routes: dict[str, Path], served: list[tuple[str, int]]) -> type[BaseHTTPRequestHandler]:
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:  # noqa: N802 (the http.server hook)
            path = urlsplit(self.path).path
            status, body, content_type = self.lookup(path)
            served.append((path, status))
            self.send_response(status)
            self.send_header("Content-Type", content_type)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        @staticmethod
        def lookup(path: str) -> tuple[int, bytes, str]:
            for prefix, directory in routes.items():
                if not path.startswith(prefix):
                    continue
                relative = path[len(prefix):] or "index.html"
                file = (directory / relative).resolve()
                if directory.resolve() in file.parents and file.is_file():
                    content_type = CONTENT_TYPES.get(file.suffix, "application/octet-stream")
                    return 200, file.read_bytes(), content_type
            return 404, b"not found", "text/plain; charset=utf-8"

        def log_message(self, format: str, *args: object) -> None:  # noqa: A002 (the http.server hook)
            pass

    return Handler


def find_chrome(requested: str | None) -> str:
    candidates = [requested] if requested else [shutil.which(name) for name in CHROME_NAMES]
    for candidate in candidates:
        if candidate and Path(candidate).is_file():
            return candidate
    raise DeployCheckError("no Chrome binary found; pass --chrome PATH")


def dump_dom(chrome: str, profile: Path, url: str) -> str:
    command = [chrome, "--headless=new", "--disable-gpu", "--no-first-run", "--no-default-browser-check",
               "--disable-extensions", f"--user-data-dir={profile}",
               f"--virtual-time-budget={VIRTUAL_TIME_BUDGET_MS}", "--dump-dom", url]
    if os.geteuid() == 0:
        command.insert(1, "--no-sandbox")
    result = subprocess.run(command, capture_output=True, text=True, timeout=300, check=False)
    if result.returncode != 0 or not result.stdout:
        raise DeployCheckError(f"chrome could not dump {url}: exit {result.returncode}: {result.stderr[-2000:]}")
    return result.stdout


def element_text(dom: str, element_id: str) -> str:
    match = re.search(rf'<[a-z]+[^>]*\sid="{re.escape(element_id)}"[^>]*>(.*?)</[a-z]+>', dom, re.DOTALL)
    if match is None:
        raise DeployCheckError(f"no element with id {element_id!r} in the dumped DOM")
    return html.unescape(re.sub(r"<[^>]+>", "", match.group(1))).strip()


def require(condition: bool, message: str) -> None:
    if not condition:
        raise DeployCheckError(message)


def check_harness(dom: str, subpath: str) -> None:
    results = json.loads(element_text(dom, "results"))
    require(results.get("errors") == [], f"the harness reported errors: {results.get('errors')}")
    expected = gate.EXAMPLE_TEMPLATES["account-lockout"]
    steps = results["steps"]
    require([step["decision"] for step in steps] == expected["decisions"],
            f"demonstration decisions differ from the gate's: {[step['decision'] for step in steps]}")
    for index, step in enumerate(steps, start=1):
        require(step["error"] is None, f"demonstration step {index} was refused: {step['error']}")
        require(all(status == "Satisfied" for _, status in step["laws"]),
                f"demonstration step {index} evaluated a law as other than Satisfied: {step['laws']}")
    account = results["state"]
    require([account["failed_attempts"], account["locked_until"], account["last_seen"]]
            == [expected["failed_attempts"], expected["locked_until"], expected["last_seen"]],
            f"the account after the demonstration differs from the gate's: {account}")
    require(results["bundles"] == expected["bundles"], f"bundles: {results['bundles']}")
    # Nothing delivers in the page, so the alerts the gate delivered stay pending.
    require(results["pending"] == expected["deliveries"], f"pending alerts: {results['pending']}")
    require(results["base"].endswith(subpath), f"the harness loaded from {results['base']}, not {subpath}")


def check_page(dom: str) -> None:
    status = element_text(dom, "status")
    require("Reset to the exact genesis" in status, f"the page did not load its module: status {status!r}")
    require(element_text(dom, "failed-attempts") == "0", "the page did not render the genesis account")
    require("Reset to genesis" in dom and 'id="reset"' in dom, "the page's controls are missing")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--chrome", help="the Chrome or Chromium binary (default: the first found on PATH)")
    parser.add_argument("--subpath", default="/ZenoFCIS/", help="where the artifact is served (default: /ZenoFCIS/)")
    args = parser.parse_args()
    subpath = args.subpath if args.subpath.endswith("/") else args.subpath + "/"
    require(subpath.startswith("/") and subpath != "/" and subpath != HARNESS_PREFIX, f"bad subpath {subpath!r}")
    require((PUBLIC / "account-lockout.wasm").is_file(), "site/public/account-lockout.wasm is missing; run site/build.py")
    chrome = find_chrome(args.chrome)

    served: list[tuple[str, int]] = []
    server = ThreadingHTTPServer(("127.0.0.1", 0), artifact_handler({subpath: PUBLIC, HARNESS_PREFIX: HARNESS}, served))
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        origin = f"http://127.0.0.1:{server.server_address[1]}"
        with tempfile.TemporaryDirectory(prefix="zeno-fcis-deploy-check-") as profile:
            check_harness(dump_dom(chrome, Path(profile), f"{origin}{HARNESS_PREFIX}?base={subpath}"), subpath)
            check_page(dump_dom(chrome, Path(profile), f"{origin}{subpath}"))
    finally:
        server.shutdown()
        server.server_close()

    paths = {path for path, status in served if status == 200}
    require(f"{subpath}account-lockout.wasm" in paths, f"the module was not fetched from {subpath}: {sorted(paths)}")
    require(all(path.startswith((subpath, HARNESS_PREFIX)) for path in paths),
            f"a request outside the subpath succeeded: {sorted(paths)}")
    print(f"deploy check: PASS: the artifact served from {subpath} ran the demonstration in {Path(chrome).name}; "
          f"{len(served)} requests, none served outside the subpath")


if __name__ == "__main__":
    try:
        main()
    except (DeployCheckError, OSError, subprocess.SubprocessError, ValueError, KeyError) as error:
        print(f"deploy check: FAIL: {error}", file=sys.stderr)
        sys.exit(1)
