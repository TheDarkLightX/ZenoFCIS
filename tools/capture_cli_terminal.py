#!/usr/bin/env python3
"""Capture real ZenoFCIS CLI sessions from an xterm window."""

from __future__ import annotations

import argparse
import os
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "docs" / "assets" / "marketing"
WINDOW_ID = re.compile(r"Window id:\s+(0x[0-9a-fA-F]+)")


@dataclass(frozen=True)
class Session:
    slug: str
    title: str
    geometry: str
    arguments: tuple[str, ...]


SESSIONS = {
    session.slug: session
    for session in (
        Session("cli-overview", "ZenoFCIS RC3 CLI", "112x30", ("--help",)),
        Session(
            "mini-determinator-check",
            "ZenoFCIS Mini Determinator Check",
            "112x18",
            ("check", "examples/mini-determinator/project.zeno"),
        ),
        Session(
            "accumulated-diagnostics",
            "ZenoFCIS Accumulated Diagnostics",
            "112x20",
            ("check", "examples/diagnostics-tour/project.zeno"),
        ),
        Session(
            "composition-graph",
            "ZenoFCIS Composition Graph",
            "112x18",
            (
                "graph",
                "examples/mini-determinator/project.zeno",
                "--format",
                "mermaid",
            ),
        ),
    )
}


def require_program(name: str) -> str:
    path = shutil.which(name)
    if path is None:
        raise RuntimeError(f"required program is unavailable: {name}")
    return path


def run_session(slug: str, ready: Path) -> int:
    session = SESSIONS[slug]
    executable = ROOT / "target" / "debug" / "zeno-fcis"
    if not executable.is_file():
        raise RuntimeError("build target/debug/zeno-fcis before capturing")

    command = [str(executable), *session.arguments]
    shown = ["zeno-fcis", *session.arguments]
    sys.stdout.write("\x1b[2J\x1b[H")
    sys.stdout.write(f"\x1b[1;36m$\x1b[0m {shlex.join(shown)}\n\n")
    sys.stdout.flush()
    environment = os.environ.copy()
    environment["CARGO_TERM_COLOR"] = "never"
    environment["NO_COLOR"] = "1"
    completed = subprocess.run(command, cwd=ROOT, env=environment, check=False)
    sys.stdout.write(f"\n\x1b[2mexit status {completed.returncode}\x1b[0m\n")
    sys.stdout.flush()
    ready.write_text("complete\n", encoding="ascii")
    time.sleep(20)
    return completed.returncode


def window_id(xwininfo: str, title: str) -> str:
    for _ in range(50):
        completed = subprocess.run(
            [xwininfo, "-name", title],
            check=False,
            capture_output=True,
            text=True,
        )
        match = WINDOW_ID.search(completed.stdout)
        if completed.returncode == 0 and match is not None:
            return match.group(1)
        time.sleep(0.1)
    raise RuntimeError(f"xterm window did not appear: {title}")


def wait_until_complete(ready: Path, process: subprocess.Popen[bytes]) -> None:
    for _ in range(100):
        if ready.is_file():
            time.sleep(0.2)
            return
        if process.poll() is not None:
            raise RuntimeError("xterm session exited before completing the CLI command")
        time.sleep(0.1)
    raise RuntimeError("xterm session did not complete within 10 seconds")


def capture_one(
    session: Session,
    xterm: str,
    xwininfo: str,
    importer: str,
    converter: str,
) -> None:
    title = f"{session.title} [{os.getpid()}]"
    with tempfile.TemporaryDirectory(prefix="zeno-fcis-terminal-") as directory:
        ready = Path(directory) / "ready"
        process = subprocess.Popen(
            [
                xterm,
                "-T",
                title,
                "-geometry",
                session.geometry,
                "-fa",
                "DejaVu Sans Mono",
                "-fs",
                "14",
                "-bg",
                "#070d17",
                "-fg",
                "#dce7f4",
                "-cr",
                "#5eead4",
                "-xrm",
                "XTerm*scrollBar:false",
                "-e",
                sys.executable,
                str(Path(__file__).resolve()),
                "session",
                session.slug,
                str(ready),
            ],
            cwd=ROOT,
        )
        try:
            identity = window_id(xwininfo, title)
            wait_until_complete(ready, process)
            raw = Path(directory) / "raw.png"
            destination = OUTPUT / f"terminal-{session.slug}.png"
            subprocess.run([importer, "-window", identity, str(raw)], check=True)
            subprocess.run(
                [
                    converter,
                    str(raw),
                    "-resize",
                    "1600x1600>",
                    "-strip",
                    str(destination),
                ],
                check=True,
            )
            print(f"captured {destination.relative_to(ROOT)}")
        finally:
            process.terminate()
            try:
                process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=3)


def capture(slugs: list[str]) -> int:
    if not os.environ.get("DISPLAY"):
        raise RuntimeError("DISPLAY is required for xterm capture")
    xterm = require_program("xterm")
    xwininfo = require_program("xwininfo")
    importer = require_program("import")
    converter = require_program("convert")
    subprocess.run(
        [
            "cargo",
            "+1.97.1",
            "build",
            "-p",
            "zeno-fcis-cli",
            "--locked",
        ],
        cwd=ROOT,
        check=True,
    )
    OUTPUT.mkdir(parents=True, exist_ok=True)
    for slug in slugs:
        capture_one(SESSIONS[slug], xterm, xwininfo, importer, converter)
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    capture_parser = subparsers.add_parser("capture")
    capture_parser.add_argument(
        "sessions",
        nargs="*",
        choices=sorted(SESSIONS),
    )
    session_parser = subparsers.add_parser("session")
    session_parser.add_argument("name", choices=sorted(SESSIONS))
    session_parser.add_argument("ready", type=Path)
    arguments = parser.parse_args()
    if arguments.command == "session":
        return run_session(arguments.name, arguments.ready)
    selected = arguments.sessions or sorted(SESSIONS)
    return capture(selected)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, subprocess.SubprocessError) as error:
        raise SystemExit(f"terminal-capture: FAIL: {error}") from error
