"""Build and invoke the actual Rust scoped source checker; no semantic rules here."""

from __future__ import annotations

import json
import os
import subprocess
import tomllib
from pathlib import Path

from check_generated_application import validate_resolved_graph


ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "tools/resolved-purity/Cargo.toml"


class ResolutionError(RuntimeError):
    """The actual checker or its reviewed graph was unavailable/inconsistent."""


class ResolvedChecker:
    def __init__(self) -> None:
        self.environment = dict(os.environ, CARGO_INCREMENTAL="0", CARGO_BUILD_JOBS="1")
        self.binary: Path | None = None

    def prepare(self) -> None:
        metadata = self._cargo("metadata", "--format-version", "1")
        try:
            decoded = json.loads(metadata.stdout)
            validate_resolved_graph(
                tomllib.loads((ROOT / "Cargo.lock").read_text()),
                tomllib.loads(MANIFEST.with_name("Cargo.lock").read_text()),
                decoded,
                {("zeno-fcis-resolved-purity", "0.0.0"): MANIFEST},
            )
            binary = Path(decoded["target_directory"]) / "debug" / (
                "zeno-fcis-resolved-purity.exe" if os.name == "nt" else "zeno-fcis-resolved-purity"
            )
        except (OSError, ValueError, KeyError, TypeError, RuntimeError) as error:
            raise ResolutionError(f"unreviewed/unreadable checker graph: {error}") from error
        self._cargo("build", "-j", "1")
        if not binary.is_file():
            raise ResolutionError(f"compiled checker binary absent: {binary}")
        self.binary = binary

    def _cargo(self, operation: str, *arguments: str) -> subprocess.CompletedProcess[str]:
        command = ["cargo", "+1.97.1", operation, "--manifest-path", str(MANIFEST),
                   "--locked", "--offline", "-vv", *arguments]
        try:
            result = subprocess.run(command, cwd=ROOT, env=self.environment,
                                    stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                    text=True, check=False)
        except OSError as error:
            raise ResolutionError(f"checker command unavailable: {error}") from error
        if result.returncode:
            raise ResolutionError(f"checker {operation} failed ({result.returncode}): {result.stderr.strip()}")
        return result

    def check(self, paths: list[Path]) -> dict:
        if self.binary is None:
            raise ResolutionError("actual checker has not been built")
        try:
            result = subprocess.run([str(self.binary), *(str(p) for p in paths)],
                                    cwd=ROOT, env=self.environment, text=True,
                                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False)
            report = json.loads(result.stdout)
        except (OSError, ValueError) as error:
            raise ResolutionError(f"checker invocation/report failed: {error}") from error
        status = report.get("status")
        expected = {"clean": 0, "confined": 0, "violations": 1, "unreadable": 2}.get(status)
        if (report.get("schema") != "zeno-fcis/purity-report/2" or
                not isinstance(report.get("resolution"), dict) or
                expected is None or result.returncode != expected or
                not isinstance(report.get("findings"), list) or
                not isinstance(report.get("unreadable"), list)):
            raise ResolutionError(f"inconsistent/non-resolved checker result (exit {result.returncode})")
        return report
