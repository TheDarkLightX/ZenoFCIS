#!/usr/bin/env python3
"""Render exact ZenoFCIS CLI output as deterministic marketing artwork."""

from __future__ import annotations

import html
import os
import shutil
import subprocess
import textwrap
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "docs" / "assets" / "marketing"
WIDTH = 1600
HEIGHT = 1000


@dataclass(frozen=True)
class Capture:
    slug: str
    eyebrow: str
    title: str
    subtitle: str
    prompt: str
    output: str
    badges: tuple[str, ...]
    accent: str
    visual: str | None = None


def run(command: list[str]) -> str:
    environment = os.environ.copy()
    environment["CARGO_TERM_COLOR"] = "never"
    environment["NO_COLOR"] = "1"
    completed = subprocess.run(
        command,
        cwd=ROOT,
        env=environment,
        check=True,
        capture_output=True,
        text=True,
    )
    if completed.stderr:
        raise RuntimeError(
            f"capture command wrote to stderr: {' '.join(command)}\n{completed.stderr}"
        )
    return completed.stdout.rstrip()


def cli(*arguments: str) -> str:
    return run(
        [
            "cargo",
            "+1.97.1",
            "run",
            "--quiet",
            "-p",
            "zeno-fcis-cli",
            "--locked",
            "--",
            *arguments,
        ]
    )


def cli_invalid(*arguments: str) -> str:
    environment = os.environ.copy()
    environment["CARGO_TERM_COLOR"] = "never"
    environment["NO_COLOR"] = "1"
    completed = subprocess.run(
        [
            "cargo",
            "+1.97.1",
            "run",
            "--quiet",
            "-p",
            "zeno-fcis-cli",
            "--locked",
            "--",
            *arguments,
        ],
        cwd=ROOT,
        env=environment,
        check=False,
        capture_output=True,
        text=True,
    )
    if completed.returncode != 1:
        raise RuntimeError(
            "invalid-spec capture did not return stable exit class 1: "
            f"{completed.returncode}"
        )
    if completed.stdout:
        raise RuntimeError(f"invalid-spec capture wrote to stdout: {completed.stdout}")
    if not completed.stderr:
        raise RuntimeError("invalid-spec capture produced no diagnostics")
    return completed.stderr.rstrip()


def mini_determinator() -> str:
    return run(
        [
            "cargo",
            "+1.97.1",
            "run",
            "--quiet",
            "-p",
            "zeno-fcis-spec",
            "--example",
            "mini_determinator",
            "--locked",
        ]
    )


def escaped(value: str) -> str:
    return html.escape(value, quote=True)


def terminal_lines(output: str, width: int = 112) -> list[str]:
    lines: list[str] = []
    for line in output.splitlines():
        if not line:
            lines.append("")
            continue
        lines.extend(
            textwrap.wrap(
                line,
                width=width,
                subsequent_indent="  ",
                break_long_words=False,
                break_on_hyphens=False,
            )
            or [""]
        )
    return lines


def badge_markup(badges: tuple[str, ...]) -> str:
    x = 128
    parts: list[str] = []
    for badge in badges:
        badge_width = 30 + len(badge) * 11
        parts.append(
            f'<rect x="{x}" y="925" width="{badge_width}" height="38" rx="19" '
            'fill="#0d1728" stroke="#263751"/>'
            f'<text x="{x + 15}" y="951" class="badge">{escaped(badge)}</text>'
        )
        x += badge_width + 14
    return "".join(parts)


def diagram_markup() -> str:
    return """
      <g aria-label="Composition graph visualization">
        <path d="M 560 630 C 680 630, 690 630, 810 630" fill="none"
              stroke="#34d399" stroke-width="4"/>
        <path d="M 797 617 L 817 630 L 797 643" fill="none"
              stroke="#34d399" stroke-width="4" stroke-linecap="round"
              stroke-linejoin="round"/>
        <rect x="300" y="565" width="260" height="130" rx="22"
              fill="#0c1827" stroke="#4fd1c5" stroke-width="2"/>
        <text x="332" y="612" class="node-id">COMPONENT 300</text>
        <text x="332" y="657" class="node-title">coordinator</text>
        <rect x="815" y="565" width="300" height="130" rx="22"
              fill="#0c1827" stroke="#4fd1c5" stroke-width="2"/>
        <text x="847" y="612" class="node-id">COMPONENT 301</text>
        <text x="847" y="657" class="node-title">worker_space</text>
      </g>
    """


def check_markup() -> str:
    metrics = (
        ("PROJECT", "2"),
        ("COMPONENTS", "2"),
        ("CLAIMS", "2"),
        ("OBLIGATIONS", "3"),
    )
    parts: list[str] = []
    for index, (label, value) in enumerate(metrics):
        x = 236 + index * 282
        parts.append(
            f'<rect x="{x}" y="570" width="236" height="132" rx="20" '
            'fill="#0c1827" stroke="#254056"/>'
            f'<text x="{x + 24}" y="612" class="metric-label">{label}</text>'
            f'<text x="{x + 24}" y="672" class="metric-value">{value}</text>'
        )
    parts.append(
        '<rect x="236" y="735" width="1082" height="76" rx="18" '
        'fill="#07131f" stroke="#1e4957"/>'
        '<circle cx="270" cy="773" r="9" fill="#5eead4"/>'
        '<text x="298" y="766" class="metric-label">SEMANTIC PROGRAM ID</text>'
        '<text x="298" y="792" class="hash">e6a3d0e0c030…bca316bb</text>'
    )
    return "".join(parts)


def replay_markup() -> str:
    slots = (
        ("SLOT 1", "10", "coordinator"),
        ("SLOT 2", "15", "worker 1 returned 15"),
        ("SLOT 3", "20", "worker 2 returned 20"),
    )
    parts: list[str] = []
    for index, (label, value, detail) in enumerate(slots):
        x = 286 + index * 348
        parts.append(
            f'<rect x="{x}" y="650" width="300" height="142" rx="21" '
            'fill="#0c1827" stroke="#2b5260"/>'
            f'<text x="{x + 25}" y="690" class="metric-label">{label}</text>'
            f'<text x="{x + 25}" y="748" class="metric-value">{value}</text>'
            f'<text x="{x + 90}" y="747" class="slot-detail">{escaped(detail)}</text>'
        )
    return "".join(parts)


def visual_markup(kind: str | None) -> str:
    if kind == "graph":
        return diagram_markup()
    if kind == "check":
        return check_markup()
    if kind == "replay":
        return replay_markup()
    return ""


def render(capture: Capture) -> None:
    lines = terminal_lines(capture.output)
    output_markup = []
    start_y = 413
    line_height = 25
    for index, line in enumerate(lines):
        output_markup.append(
            f'<text x="164" y="{start_y + index * line_height}" class="terminal">'
            f"{escaped(line)}</text>"
        )

    visual = visual_markup(capture.visual).strip()
    svg = f"""<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{HEIGHT}"
         viewBox="0 0 {WIDTH} {HEIGHT}" role="img" aria-label="{escaped(capture.title)}">
  <defs>
    <linearGradient id="background" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#050912"/>
      <stop offset="0.62" stop-color="#08111f"/>
      <stop offset="1" stop-color="#101827"/>
    </linearGradient>
    <radialGradient id="glow" cx="0.82" cy="0.06" r="0.78">
      <stop offset="0" stop-color="{capture.accent}" stop-opacity="0.25"/>
      <stop offset="1" stop-color="{capture.accent}" stop-opacity="0"/>
    </radialGradient>
    <pattern id="grid" width="44" height="44" patternUnits="userSpaceOnUse">
      <path d="M 44 0 L 0 0 0 44" fill="none" stroke="#39506a"
            stroke-opacity="0.12" stroke-width="1"/>
    </pattern>
    <filter id="shadow" x="-20%" y="-20%" width="140%" height="150%">
      <feDropShadow dx="0" dy="22" stdDeviation="30" flood-color="#000611"
                    flood-opacity="0.62"/>
    </filter>
  </defs>
  <style>
    .brand {{ font-family: Inter, sans-serif; font-size: 22px; font-weight: 600; letter-spacing: 5px; fill: #dce8f6; }}
    .release {{ font-family: Inter, sans-serif; font-size: 17px; font-weight: 600; letter-spacing: 2px; fill: #83f5d0; }}
    .eyebrow {{ font-family: Inter, sans-serif; font-size: 17px; font-weight: 600; letter-spacing: 3px; fill: {capture.accent}; }}
    .title {{ font-family: 'Inter Display', Inter, sans-serif; font-size: 52px; font-weight: 600; fill: #f4f8fd; }}
    .subtitle {{ font-family: Inter, sans-serif; font-size: 22px; font-weight: 400; fill: #9db0c8; }}
    .terminal {{ font-family: 'DejaVu Sans Mono', monospace; font-size: 20px; font-weight: 400; fill: #dce7f4; }}
    .prompt {{ font-family: 'DejaVu Sans Mono', monospace; font-size: 20px; font-weight: 600; fill: #6ee7c7; }}
    .terminal-title {{ font-family: Inter, sans-serif; font-size: 15px; font-weight: 500; fill: #70849e; letter-spacing: 1px; }}
    .badge {{ font-family: Inter, sans-serif; font-size: 16px; font-weight: 500; fill: #b9c9dc; }}
    .footer {{ font-family: Inter, sans-serif; font-size: 15px; font-weight: 400; fill: #64778f; }}
    .node-id {{ font-family: Inter, sans-serif; font-size: 15px; font-weight: 600; fill: #7de4cb; letter-spacing: 2px; }}
    .node-title {{ font-family: 'DejaVu Sans Mono', monospace; font-size: 25px; font-weight: 600; fill: #ecf7ff; }}
    .metric-label {{ font-family: Inter, sans-serif; font-size: 14px; font-weight: 600; letter-spacing: 2px; fill: #72d9ca; }}
    .metric-value {{ font-family: 'Inter Display', Inter, sans-serif; font-size: 48px; font-weight: 600; fill: #f2f8ff; }}
    .hash {{ font-family: 'DejaVu Sans Mono', monospace; font-size: 17px; font-weight: 400; fill: #c5d5e8; }}
    .slot-detail {{ font-family: Inter, sans-serif; font-size: 14px; font-weight: 400; fill: #8fa5bc; }}
  </style>
  <rect width="1600" height="1000" fill="url(#background)"/>
  <rect width="1600" height="1000" fill="url(#glow)"/>
  <rect width="1600" height="1000" fill="url(#grid)"/>
  <rect x="0" y="0" width="9" height="1000" fill="{capture.accent}"/>

  <text x="128" y="72" class="brand">ZENOFCIS</text>
  <text x="1457" y="72" text-anchor="end" class="release">1.0.0-RC.3</text>
  <text x="128" y="132" class="eyebrow">{escaped(capture.eyebrow.upper())}</text>
  <text x="128" y="197" class="title">{escaped(capture.title)}</text>
  <text x="128" y="238" class="subtitle">{escaped(capture.subtitle)}</text>

  <g filter="url(#shadow)">
    <rect x="128" y="282" width="1344" height="610" rx="24" fill="#080e18"
          stroke="#26364c" stroke-width="2"/>
    <rect x="128" y="282" width="1344" height="70" rx="24" fill="#0d1624"/>
    <rect x="128" y="328" width="1344" height="24" fill="#0d1624"/>
    <circle cx="170" cy="317" r="8" fill="#ff6b6b"/>
    <circle cx="198" cy="317" r="8" fill="#f6c85f"/>
    <circle cx="226" cy="317" r="8" fill="#58d68d"/>
    <text x="800" y="323" text-anchor="middle" class="terminal-title">DETERMINISTIC CLI RENDER</text>
  </g>
  <text x="164" y="382" class="prompt">$ {escaped(capture.prompt)}</text>
  {''.join(output_markup)}
{visual}
  {badge_markup(capture.badges)}
  <text x="1472" y="951" text-anchor="end" class="footer">Executable output · deterministic renderer</text>
</svg>
"""

    OUTPUT.mkdir(parents=True, exist_ok=True)
    svg_path = OUTPUT / f"{capture.slug}.svg"
    png_path = OUTPUT / f"{capture.slug}.png"
    svg_path.write_text(svg, encoding="utf-8")

    converter = shutil.which("convert")
    if converter is None:
        raise RuntimeError("ImageMagick `convert` is required to render PNG assets")
    subprocess.run(
        [converter, "-background", "none", str(svg_path), str(png_path)],
        cwd=ROOT,
        check=True,
    )
    print(f"rendered {svg_path.relative_to(ROOT)}")
    print(f"rendered {png_path.relative_to(ROOT)}")


def main() -> int:
    captures = (
        Capture(
            slug="zeno-fcis-cli-overview",
            eyebrow="Authoring CLI",
            title="One interface. Closed boundaries.",
            subtitle="Check, generate, explain, graph, and prove bounded ZenoFCIS projects.",
            prompt="zeno-fcis --help",
            output=cli("--help"),
            badges=("Versioned .zeno", "Deterministic output", "Fail closed"),
            accent="#5eead4",
        ),
        Capture(
            slug="accumulated-diagnostics",
            eyebrow="Authoring diagnostics",
            title="Three blockers. One bounded check.",
            subtitle="Independent specification defects arrive together in stable source order.",
            prompt="zeno-fcis check examples/diagnostics-tour/project.zeno",
            output=cli_invalid(
                "check",
                "examples/diagnostics-tour/project.zeno",
                "--format",
                "human",
            ),
            badges=("Accumulated", "Stable codes", "Actionable remediation"),
            accent="#fbbf24",
        ),
        Capture(
            slug="mini-determinator-check",
            eyebrow="Mini Determinator",
            title="A complete project check in one command.",
            subtitle="Typed components, claims, obligations, and semantic identity are reported together.",
            prompt="zeno-fcis check examples/mini-determinator/project.zeno",
            output=cli("check", "examples/mini-determinator/project.zeno"),
            badges=("2 components", "2 claims", "Content-bound identity"),
            accent="#67e8f9",
            visual="check",
        ),
        Capture(
            slug="mini-determinator-replay",
            eyebrow="Executable semantics",
            title="Private work. Canonical merge.",
            subtitle="Worker completion order changes; the accepted logical result does not.",
            prompt="cargo run -p zeno-fcis-spec --example mini_determinator --locked",
            output=mini_determinator(),
            badges=("Shared nothing", "Checked arithmetic", "Stable conflict rejection"),
            accent="#a7f3d0",
            visual="replay",
        ),
        Capture(
            slug="composition-graph",
            eyebrow="Derived view",
            title="Composition you can inspect.",
            subtitle="Graphs are deterministic projections for review; they grant no production authority.",
            prompt="zeno-fcis graph examples/mini-determinator/project.zeno --format mermaid",
            output=cli(
                "graph",
                "examples/mini-determinator/project.zeno",
                "--format",
                "mermaid",
            ),
            badges=("Mermaid", "Stable IDs", "Diagnostic view only"),
            accent="#c4b5fd",
            visual="graph",
        ),
    )

    for capture in captures:
        render(capture)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
