#!/usr/bin/env python3
"""Checks the exact Pages artifact in a browser.

Serves site/public as `actions/upload-pages-artifact` uploads it, at a subpath
(`/ZenoFCIS/` by default) of a local server that serves nothing at the root,
and drives headless Chrome through its DevTools pipe, waiting for the
demonstration's completion state before reading the DOM, with a wall-clock
timeout:

0. before the browser: index.html shows exactly one example on load; no
   page file loads a resource from another origin; and every text and
   background pair the page uses, in both of the stylesheet's themes, has
   WCAG AA contrast;
1. the harness, site/tests/harness/, served beside the artifact, once per
   template: it imports the page's own panel and the template's description
   from the artifact, mounts the panel as the page does, runs the README's
   demonstration through it, and prints the results and what the panel
   rendered, which must match the gate's expected summary in
   tools/check_generated_application.py: the decisions, the summary, one
   history entry per decision, the latest-decision box with the last
   decision's verdict, every hash the panel shows by its prefix carrying the
   module's full value, and the proposer's list where the template describes
   one. Then, from genesis again, the harness enters each demonstration step
   into the form and clicks its button: every control must send exactly the
   request the script sends, and reach the script's decision. One template
   per browser capture keeps each result and failure attributable to that
   template;
2. the page itself: the example shown when it loads must have loaded its
   module, run its README's demonstration, said so in its status, rendered
   one history entry per decision with the gate's decision on it, and shown
   the last decision in its latest-decision box; no other example may be
   shown, and no other module fetched;
3. the page in a frame of the same origin (site/tests/harness/viewport.html),
   at 1280 by 800 and at a phone's 390 by 844: after the load-time
   demonstration, the page must not have scrolled itself, its title must be
   inside the viewport, and the document must be no wider than the viewport;
4. every request the browser made, as the DevTools pipe reports them, must
   be for the subpath or the harness. The one exception is Chrome's own probe
   for /favicon.ico at the origin's root, which the server refuses.

A page that loaded from the subpath alone is a page whose relative paths hold.

Usage: python3 site/tests/deploy_check.py [--chrome PATH] [--subpath /ZenoFCIS/] [TEMPLATE...]
(default: every module in site/public)
"""

from __future__ import annotations

import argparse
import html
import json
import re
import shutil
import subprocess
import sys
import tempfile
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import NamedTuple
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
BROWSER_TIMEOUT_MS = 120_000
# The frames the served page is loaded in: a laptop, and a phone.
VIEWPORTS = ((1280, 800), (390, 844))
BADGES = {"Accept": "Accepted", "CommittedFailure": "Committed failure", "Reject": "Rejected"}
BADGE = re.compile(r'<span class="badge [a-z]+">([^<]*)</span>')
# Tags that fetch what they name, and an absolute URL in the attribute that names it.
RESOURCE_TAG = re.compile(r"<(?:link|script|img|iframe|source|video|audio|object|embed|use)\b([^>]*)>", re.IGNORECASE)
ABSOLUTE_URL = re.compile(r"""(?:src|href|data|xlink:href)\s*=\s*["']?\s*(?:[a-z][a-z0-9+.-]*:|//)""", re.IGNORECASE)
CSS_REMOTE = re.compile(r"""@import|url\(\s*["']?\s*(?:[a-z][a-z0-9+.-]*:|//)""", re.IGNORECASE)
# The stylesheet's text-on-background pairs, and the WCAG AA minimum for each:
# 4.5 for text, 3 for a control's border and the focus ring.
TOKEN = re.compile(r"--([a-z-]+):\s*(#[0-9a-fA-F]{6})\s*;")
CONTRAST_PAIRS = (
    ("ink", "paper", 4.5), ("ink", "panel", 4.5), ("ink", "tint", 4.5),
    ("muted", "paper", 4.5), ("muted", "panel", 4.5), ("muted", "tint", 4.5),
    ("link", "paper", 4.5), ("link", "panel", 4.5), ("link", "tint", 4.5),
    ("accent-ink", "accent", 4.5),
    ("accept-ink", "accept-bg", 4.5), ("failure-ink", "failure-bg", 4.5),
    ("reject-ink", "reject-bg", 4.5), ("refused-ink", "refused-bg", 4.5),
    ("line", "paper", 3.0), ("line", "panel", 3.0), ("line", "tint", 3.0),
    ("focus", "paper", 3.0), ("focus", "panel", 3.0), ("focus", "tint", 3.0),
    ("accent", "paper", 3.0), ("accent", "tint", 3.0),
)

sys.path.insert(0, str(ROOT / "tools"))
import check_generated_application as gate  # noqa: E402


class DeployCheckError(Exception):
    pass


class Capture(NamedTuple):
    """One browser capture: the document's HTML, and every URL the page requested."""
    html: str
    requests: list[str]


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


def dump_page(chrome: str, profile: Path, url: str, timeout_ms: int = BROWSER_TIMEOUT_MS) -> Capture:
    command = ["node", str(ROOT / "site" / "tests" / "browser.mjs"), chrome,
               str(profile), url, str(timeout_ms)]
    result = subprocess.run(command, capture_output=True, text=True, timeout=timeout_ms / 1000 + 15, check=False)
    if result.returncode != 0 or not result.stdout:
        raise DeployCheckError(f"chrome could not dump {url}: exit {result.returncode}: {result.stderr[-2000:]}")
    try:
        printed = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise DeployCheckError(f"the browser runner printed no capture for {url}: {result.stdout[:300]!r}") from error
    return Capture(printed["html"], list(printed["requests"]))


def dump_dom(chrome: str, profile: Path, url: str, timeout_ms: int = BROWSER_TIMEOUT_MS) -> str:
    return dump_page(chrome, profile, url, timeout_ms).html


def element_html(dom: str, element_id: str) -> str:
    """The inner HTML of the element with `element_id`, up to its own closing tag; the elements read here do not nest."""
    match = re.search(rf'<([a-z]+)[^>]*\sid="{re.escape(element_id)}"[^>]*>(.*?)</\1>', dom, re.DOTALL)
    if match is None:
        raise DeployCheckError(f"no element with id {element_id!r} in the dumped DOM")
    return match.group(2)


def element_text(dom: str, element_id: str) -> str:
    return html.unescape(re.sub(r"<[^>]+>", " ", element_html(dom, element_id))).strip()


def require(condition: bool, message: str) -> None:
    if not condition:
        raise DeployCheckError(message)


def shown_on_load() -> str:
    """The one example index.html shows on load: the section without `hidden`."""
    text = (PUBLIC / "index.html").read_text(encoding="utf-8")
    sections = [match.group(1) for match in re.finditer(r"<section\b([^>]*)>", text) if "data-template=" in match.group(1)]
    require(len(sections) == 6, f"index.html must hold six examples, not {len(sections)}")
    shown = [re.search(r'data-template="([^"]+)"', section).group(1) for section in sections
             if not re.search(r"\shidden(?:=|\s|$)", section)]
    require(len(shown) == 1, f"index.html must show exactly one example on load, not {shown}")
    return shown[0]


def check_resources() -> None:
    """No page file loads anything from another origin: no web font, CDN, or remote image."""
    page = (PUBLIC / "index.html").read_text(encoding="utf-8")
    for tag in RESOURCE_TAG.finditer(page):
        require(ABSOLUTE_URL.search(tag.group(1)) is None,
                f"index.html loads a resource by an absolute URL: {tag.group(0)[:80]}")
    stylesheet = (PUBLIC / "site.css").read_text(encoding="utf-8")
    require(CSS_REMOTE.search(stylesheet) is None, "site.css loads a resource by an absolute URL or an @import")


def luminance(color: str) -> float:
    def channel(value: int) -> float:
        fraction = value / 255
        return fraction / 12.92 if fraction <= 0.03928 else ((fraction + 0.055) / 1.055) ** 2.4
    red, green, blue = (channel(int(color[index:index + 2], 16)) for index in (1, 3, 5))
    return 0.2126 * red + 0.7152 * green + 0.0722 * blue


def contrast(foreground: str, background: str) -> float:
    lighter, darker = sorted((luminance(foreground), luminance(background)), reverse=True)
    return (lighter + 0.05) / (darker + 0.05)


def check_contrast() -> None:
    """Both themes' tokens, as the stylesheet's two `:root` blocks declare them, meet WCAG AA on every pair the page uses."""
    stylesheet = (PUBLIC / "site.css").read_text(encoding="utf-8")
    themes = [dict(TOKEN.findall(block)) for block in re.findall(r":root\s*\{([^}]*)\}", stylesheet)]
    require(len(themes) == 2, f"site.css must declare a light and a dark :root block, not {len(themes)}")
    require(set(themes[0]) == set(themes[1]), "the dark theme must redefine exactly the light theme's colors")
    for theme, tokens in zip(("light", "dark"), themes):
        for foreground, background, minimum in CONTRAST_PAIRS:
            require(foreground in tokens and background in tokens, f"site.css names no --{foreground} or --{background}")
            ratio = contrast(tokens[foreground], tokens[background])
            require(ratio >= minimum,
                    f"{theme} theme: --{foreground} on --{background} has contrast {ratio:.2f}, below {minimum}")


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
        require(result["latest"] == BADGES[expected["decisions"][-1]],
                f"{name}: the latest-decision box shows {result['latest']!r}, not the last decision")
        require(result["proposals"] == result["proposals_described"],
                f"{name}: the panel listed {result['proposals']} proposals, not {result['proposals_described']}")
        hashes = result["hashes"]
        require(hashes["mismatches"] == [] and hashes["checked"] == len(expected["decisions"]) + 2,
                f"{name}: a hash shown differs from the module's, or lacks its full value: {hashes}")
        controls = result["controls"]
        require(controls["mismatches"] == [] and controls["checked"] == len(expected["decisions"]),
                f"{name}: a control sent a request other than the script's: {controls}")


def check_page(dom: str, first: str) -> None:
    """The example shown on load ran its README demonstration when the page loaded, and says so."""
    expected = gate.EXAMPLE_TEMPLATES[first]
    decisions = [BADGES[decision] for decision in expected["decisions"]]
    status = element_text(dom, f"{first}-status")
    require("decided in this browser when the page loaded" in status,
            f"the shown example did not run its demonstration on load: status {status!r}")
    require(f": {len(decisions)} requests" in status, f"the status does not count the requests: {status!r}")
    state = element_text(dom, f"{first}-state")
    require("Decisions committed" in state and "State fingerprint" in state and "state root" in state,
            f"the shown example did not render its state: {state!r}")
    require("Start over" in dom and "Run the demonstration" in dom, "the shown example's controls are missing")
    latest = BADGE.findall(element_html(dom, f"{first}-latest"))
    require(latest == decisions[-1:], f"the latest-decision box shows {latest}, not the last decision {decisions[-1]!r}")
    timeline = BADGE.findall(element_html(dom, f"{first}-timeline"))
    require(timeline == decisions, f"the history shows {timeline}, not the gate's decisions")
    shown = [match.group(1) for match in re.finditer(r'<section class="example" id="([^"]+)"([^>]*)>', dom)
             if not re.search(r"\shidden(?:=|\s|$)", match.group(2))]
    require(shown == [first], f"the page shows {shown} on load, not only {first}")


def check_viewport(dom: str, width: int, height: int) -> None:
    """The served page stayed at its top after deciding its demonstration on load, and fits the frame's width."""
    printed = element_text(dom, "results")
    try:
        results = json.loads(printed)
    except json.JSONDecodeError as error:
        raise DeployCheckError(f"the viewport harness printed no results, only {printed[:300]!r}") from error
    require("error" not in results, f"the viewport harness failed: {results.get('error')}")
    require(results["status"] is not None, "the framed page never said its demonstration was decided on load")
    require((results["viewportWidth"], results["viewportHeight"]) == (width, height),
            f"the frame is {results['viewportWidth']} by {results['viewportHeight']}, not {width} by {height}")
    require(results["documentHeight"] > results["viewportHeight"],
            f"the framed page is not taller than its viewport ({results['documentHeight']} against "
            f"{results['viewportHeight']}), so scrolling cannot be checked")
    require(results["scrollY"] == 0 and results["scrollX"] == 0,
            f"the page scrolled itself on load at {width} px: scrollX {results['scrollX']}, scrollY {results['scrollY']}")
    title = results["title"]
    require(title is not None, "the page has no title heading")
    require(0 <= title["top"] and title["bottom"] <= results["viewportHeight"],
            f"at {width} px the title {title['text']!r} is outside the viewport: top {title['top']}, bottom {title['bottom']}")
    require(results["documentWidth"] <= results["clientWidth"] and results["wide"] == [],
            f"at {width} px the page overflows sideways: document {results['documentWidth']} against "
            f"{results['clientWidth']} visible; past the edge: {results['wide']}")


def check_requests(requests: list[str], origin: str, subpath: str) -> None:
    """Every request the browser made stayed within the subpath or the harness, but for Chrome's favicon probe."""
    allowed = (f"{origin}{subpath}", f"{origin}{HARNESS_PREFIX}")
    outside = sorted({url for url in requests if not url.startswith(allowed) and url != f"{origin}/favicon.ico"})
    require(outside == [], f"the browser requested something outside the subpath: {outside}")


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
    first = shown_on_load()
    check_resources()
    check_contrast()
    chrome = find_chrome(args.chrome)

    served: list[tuple[str, int]] = []
    requested: list[str] = []
    server = ThreadingHTTPServer(("127.0.0.1", 0), artifact_handler({subpath: PUBLIC, HARNESS_PREFIX: HARNESS}, served))
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        origin = f"http://127.0.0.1:{server.server_address[1]}"
        with tempfile.TemporaryDirectory(prefix="zeno-fcis-deploy-check-") as profile:
            for name in templates:
                capture = dump_page(chrome, Path(profile), f"{origin}{HARNESS_PREFIX}?base={subpath}&templates={name}")
                requested.extend(capture.requests)
                check_harness(capture.html, subpath, [name])
            page_start = len(served)
            capture = dump_page(chrome, Path(profile), f"{origin}{subpath}")
            requested.extend(capture.requests)
            check_page(capture.html, first)
            page_end = len(served)
            for width, height in VIEWPORTS:
                capture = dump_page(chrome, Path(profile),
                                    f"{origin}{HARNESS_PREFIX}viewport.html?page={subpath}&width={width}&height={height}")
                requested.extend(capture.requests)
                check_viewport(capture.html, width, height)
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
            f"the page must fetch the shown example's module and no other, not {page_modules}")
    check_requests(requested, origin, subpath)
    widths = " and ".join(f"{width} px" for width, _ in VIEWPORTS)
    print(f"deploy check: PASS: the artifact served from {subpath} ran {len(templates)} demonstration(s) in "
          f"{Path(chrome).name}, and every control sent the script's request; the page showed only {first}, "
          f"fetched only {first}.wasm, and stayed at its top without sideways overflow at {widths}; "
          f"{len(served)} requests served, none outside the subpath; {len(requested)} requests made by the "
          f"browser, none outside it")


if __name__ == "__main__":
    try:
        main()
    except (DeployCheckError, OSError, subprocess.SubprocessError, ValueError, KeyError, AttributeError) as error:
        print(f"deploy check: FAIL: {error}", file=sys.stderr)
        sys.exit(1)
