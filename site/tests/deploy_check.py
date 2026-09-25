#!/usr/bin/env python3
"""Checks the exact Pages artifact in a browser.

Serves site/public as `actions/upload-pages-artifact` uploads it, at a subpath
(`/ZenoFCIS/` by default) of a local server that serves nothing at the root,
and drives headless Chrome over it with `--dump-dom` under a virtual-time
budget, which lets a page finish its fetches and timers before the DOM is
printed:

1. the harness, site/tests/harness/, served beside the artifact, once per
   template: it imports the page's own panel and the template's description
   from the artifact, mounts the panel as the page does, runs the README's
   demonstration through it, and prints the results and what the panel
   rendered, which must match the gate's expected summary in
   tools/check_generated_application.py. One template per dump, because the
   virtual-time budget is spent across module rounds: a single round
   finishes within a small budget, several do not;
2. the page itself: the panel that is open when it loads must have loaded
   its module, run its README's demonstration, said so in its status, and
   rendered one timeline entry per decision with the gate's decision on it;
   no other panel's module may have been fetched;
3. the page in a viewport-sized frame (site/tests/harness/viewport.html):
   after the load-time demonstration, the page must not have scrolled
   itself: its scroll position must be 0 and its banner inside the viewport,
   on a document taller than the viewport.

A page that loaded from the subpath alone is a page whose relative paths hold.

Usage: python3 site/tests/deploy_check.py [--chrome PATH] [--subpath /ZenoFCIS/] [TEMPLATE...]
(default: every module in site/public)
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
# budget is virtual time, so timers on the page cost nothing, and a budget
# the page does not use costs nothing either.
VIRTUAL_TIME_BUDGET_MS = 120_000

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
    """The text of the element with `element_id`, up to its own closing tag; the elements read here do not nest."""
    match = re.search(rf'<([a-z]+)[^>]*\sid="{re.escape(element_id)}"[^>]*>(.*?)</\1>', dom, re.DOTALL)
    if match is None:
        raise DeployCheckError(f"no element with id {element_id!r} in the dumped DOM")
    return html.unescape(re.sub(r"<[^>]+>", " ", match.group(2))).strip()


def require(condition: bool, message: str) -> None:
    if not condition:
        raise DeployCheckError(message)


def open_panel() -> str:
    """The one panel the page opens on load, from index.html."""
    text = (PUBLIC / "index.html").read_text(encoding="utf-8")
    panels = [match.group(1) for match in re.finditer(r'<details\b([^>]*)>', text) if "data-template=" in match.group(1)]
    opened = [re.search(r'data-template="([^"]+)"', panel).group(1) for panel in panels if re.search(r"\sopen(?:\s|$)", panel)]
    require(len(opened) == 1, f"index.html must open exactly one panel on load, not {opened}")
    return opened[0]


def check_harness(dom: str, subpath: str, templates: list[str]) -> None:
    printed = element_text(dom, "results")
    try:
        results = json.loads(printed)
    except json.JSONDecodeError as error:
        raise DeployCheckError(f"the harness printed no results, only {printed[:300]!r}") from error
    require(results["errors"] == [], f"the harness reported errors: {results['errors']}")
    require(results["base"].endswith(subpath), f"the harness loaded from {results['base']}, not {subpath}")
    for name in templates:
        result = results["results"].get(name)
        require(result is not None, f"{name}: the harness reported nothing")
        require(result["errors"] == [], f"{name}: the harness reported errors: {result['errors']}")
        expected = gate.EXAMPLE_TEMPLATES[name]
        decisions = [step["decision"] for step in result["steps"]]
        require(decisions == expected["decisions"], f"{name}: demonstration decisions differ from the gate's: {decisions}")
        for index, step in enumerate(result["steps"], start=1):
            require(step["error"] is None, f"{name}: demonstration step {index} was refused: {step['error']}")
            require(all(status == "Satisfied" for _, status in step["laws"]),
                    f"{name}: demonstration step {index} evaluated a law as other than Satisfied: {step['laws']}")
        summary = result["summary"]
        require(summary == {key: expected[key] for key in summary},
                f"{name}: the demonstration's end differs from the gate's summary: {summary}")
        require(result["steps_counted"] == len(expected["decisions"]), f"{name}: decisions counted: {result['steps_counted']}")
        require("decided in this browser" in result["status"], f"{name}: the panel's status after the demonstration: {result['status']!r}")
        require(result["timeline"] == len(expected["decisions"]),
                f"{name}: the panel rendered {result['timeline']} decisions, not {len(expected['decisions'])}")
        require(result["proposals"] == result["proposals_described"],
                f"{name}: the panel listed {result['proposals']} proposals, not {result['proposals_described']}")


BADGES = {"Accept": "Accepted", "CommittedFailure": "Committed failure", "Reject": "Rejected"}


def check_page(dom: str, first: str) -> None:
    """The open section ran its README demonstration when the page loaded, and says so."""
    expected = gate.EXAMPLE_TEMPLATES[first]
    status = element_text(dom, f"{first}-status")
    require("decided in this browser when the page loaded" in status,
            f"the open panel did not run its demonstration on load: status {status!r}")
    require(f": {len(expected['decisions'])} requests" in status, f"the status does not count the requests: {status!r}")
    state = element_text(dom, f"{first}-state")
    require("Committed bundles" in state and "State root" in state, f"the open panel did not render its state: {state!r}")
    require("Reset to genesis" in dom, "the open panel's controls are missing")
    badges = re.findall(r'<span class="badge [a-z]+">([^<]*)</span>', dom)
    require(badges == [BADGES[decision] for decision in expected["decisions"]],
            f"the timeline shows {badges}, not the gate's decisions")


def check_viewport(dom: str) -> None:
    """The served page stayed at its top after deciding its demonstration on load."""
    printed = element_text(dom, "results")
    try:
        results = json.loads(printed)
    except json.JSONDecodeError as error:
        raise DeployCheckError(f"the viewport harness printed no results, only {printed[:300]!r}") from error
    require("error" not in results, f"the viewport harness failed: {results.get('error')}")
    require(results["status"] is not None, "the framed page never said its demonstration was decided on load")
    require(results["documentHeight"] > results["viewportHeight"],
            f"the framed page is not taller than its viewport ({results['documentHeight']} against "
            f"{results['viewportHeight']}), so scrolling cannot be checked")
    require(results["scrollY"] == 0, f"the page scrolled itself on load: scrollY {results['scrollY']}")
    banner = results["banner"]
    require(banner is not None, "the page has no banner heading")
    require(0 <= banner["top"] and banner["bottom"] <= results["viewportHeight"],
            f"the banner {banner['text']!r} is outside the viewport: top {banner['top']}, bottom {banner['bottom']}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--chrome", help="the Chrome or Chromium binary (default: the first found on PATH)")
    parser.add_argument("--subpath", default="/ZenoFCIS/", help="where the artifact is served (default: /ZenoFCIS/)")
    parser.add_argument("templates", nargs="*", help="the templates to run (default: every module in site/public)")
    args = parser.parse_args()
    subpath = args.subpath if args.subpath.endswith("/") else args.subpath + "/"
    require(subpath.startswith("/") and subpath != "/" and subpath != HARNESS_PREFIX, f"bad subpath {subpath!r}")
    templates = args.templates or sorted(path.stem for path in PUBLIC.glob("*.wasm"))
    require(bool(templates), "no module in site/public; run site/build.py")
    for name in templates:
        require((PUBLIC / f"{name}.wasm").is_file(), f"site/public/{name}.wasm is missing; run site/build.py")
    first = open_panel()
    chrome = find_chrome(args.chrome)

    served: list[tuple[str, int]] = []
    server = ThreadingHTTPServer(("127.0.0.1", 0), artifact_handler({subpath: PUBLIC, HARNESS_PREFIX: HARNESS}, served))
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        origin = f"http://127.0.0.1:{server.server_address[1]}"
        with tempfile.TemporaryDirectory(prefix="zeno-fcis-deploy-check-") as profile:
            for name in templates:
                harness_url = f"{origin}{HARNESS_PREFIX}?base={subpath}&templates={name}"
                check_harness(dump_dom(chrome, Path(profile), harness_url), subpath, [name])
            page_start = len(served)
            check_page(dump_dom(chrome, Path(profile), f"{origin}{subpath}"), first)
            page_end = len(served)
            check_viewport(dump_dom(chrome, Path(profile), f"{origin}{HARNESS_PREFIX}viewport.html?page={subpath}"))
    finally:
        server.shutdown()
        server.server_close()

    succeeded = {path for path, status in served if status == 200}
    require(all(path.startswith((subpath, HARNESS_PREFIX)) for path in succeeded),
            f"a request outside the subpath succeeded: {sorted(succeeded)}")
    for name in templates:
        require(f"{subpath}{name}.wasm" in succeeded, f"{name}: the module was not fetched from {subpath}")
    page_modules = sorted({path for path, status in served[page_start:page_end] if status == 200 and path.endswith(".wasm")})
    require(page_modules == [f"{subpath}{first}.wasm"],
            f"the page must fetch the open panel's module and no other, not {page_modules}")
    print(f"deploy check: PASS: the artifact served from {subpath} ran {len(templates)} demonstration(s) in "
          f"{Path(chrome).name}; the page fetched only {first}.wasm and stayed at its top; "
          f"{len(served)} requests, none served outside the subpath")


if __name__ == "__main__":
    try:
        main()
    except (DeployCheckError, OSError, subprocess.SubprocessError, ValueError, KeyError, AttributeError) as error:
        print(f"deploy check: FAIL: {error}", file=sys.stderr)
        sys.exit(1)
