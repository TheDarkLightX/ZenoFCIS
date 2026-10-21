"""Behavioral telemetry checks shared by the CLI and Python entrypoints."""
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import telemetry


class TelemetryTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.file = self.root / "input.zeno"
        self.file.write_text("source text must never enter telemetry\n")
        self.log = self.root / "telemetry.jsonl"

    def invoke_python(self, destination):
        env = {**os.environ, "ZENO_FCIS_TELEMETRY": str(destination), "PYTHONPATH": str(Path(__file__).parent)}
        return subprocess.run([sys.executable, "-B", "-c",
            "from telemetry import run_cli; run_cli(lambda: (_ for _ in ()).throw(SystemExit(7)))",
            str(self.file)], capture_output=True, env=env, check=False)

    def test_python_unwritable_preserves_exit(self):
        result = self.invoke_python(self.root)
        self.assertEqual(result.returncode, 7)
        self.assertEqual(result.stdout, b"")
        self.assertEqual(result.stderr, b"")

    def test_python_records_digests_not_contents(self):
        result = self.invoke_python(self.log)
        self.assertEqual(result.returncode, 7)
        row = json.loads(self.log.read_text())
        self.assertEqual(row["inputs"][0]["sha256"], hashlib.sha256(self.file.read_bytes()).hexdigest())
        self.assertNotIn(self.file.read_text().strip(), self.log.read_text())
        self.assertEqual(row["exit_code"], 7)

    def test_disabled_does_no_io(self):
        with patch.dict(os.environ, {}, clear=True), patch.object(telemetry, "input_digests", side_effect=AssertionError):
            self.assertEqual(telemetry.run_cli(lambda: 42), 42)

    def test_single_append_write(self):
        with patch.object(telemetry.os, "write", wraps=os.write) as write:
            telemetry.append(self.log, {"schema": telemetry.SCHEMA})
        self.assertEqual(write.call_count, 1)
        self.assertEqual(json.loads(self.log.read_text())["schema"], telemetry.SCHEMA)

    def test_reported_measured_work_is_observed_without_result_data(self):
        import io
        observer = telemetry.ObservationStream(io.StringIO())
        observer.write(json.dumps({"status": "UNKNOWN", "measured": {"work": "17", "source": "private"}}) + "\n")
        self.assertEqual(observer.work, {"measured.work": 17})
        self.assertEqual(observer.verdict, "unknown")

    def test_python_concurrent_lines(self):
        with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
            results = list(pool.map(lambda _: self.invoke_python(self.log), range(16)))
        self.assertTrue(all(result.returncode == 7 for result in results))
        self.assertEqual(len([json.loads(line) for line in self.log.read_text().splitlines()]), 16)


@unittest.skipUnless(os.environ.get("ZENO_FCIS_CLI"), "set ZENO_FCIS_CLI to the built binary")
class CliTelemetryTests(TelemetryTests):
    def cli(self, arguments, destination=None):
        env = {key: value for key, value in os.environ.items() if key not in ("ZENO_FCIS_TELEMETRY", "ZENO_FCIS_KB")}
        if destination is not None:
            env["ZENO_FCIS_TELEMETRY"] = str(destination)
        return subprocess.run([os.environ["ZENO_FCIS_CLI"], "--no-kb", *arguments], env=env,
                              capture_output=True, check=False)

    def test_cli_unwritable_preserves_exit_and_output(self):
        for args in [["check", str(self.file), "--format", "json"], ["--help"], ["not-a-command"], [b"\xff"]]:
            plain = self.cli(args)
            measured = self.cli(args, self.root)
            self.assertEqual((measured.returncode, measured.stdout, measured.stderr),
                             (plain.returncode, plain.stdout, plain.stderr))

    def test_cli_records_digests_not_contents(self):
        self.cli(["check", str(self.file), "--format", "json"], self.log)
        row = json.loads(self.log.read_text())
        self.assertEqual(row["schema"], telemetry.SCHEMA)
        self.assertRegex(row["timestamp"], r"T[0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]{3}Z$")
        self.assertEqual(row["command"], ["check"])
        self.assertEqual(row["inputs"][0]["sha256"], hashlib.sha256(self.file.read_bytes()).hexdigest())
        self.assertNotIn(self.file.read_text().strip(), self.log.read_text())
        self.assertNotIn(str(self.file), self.log.read_text())

    def test_cli_distribution_preserves_reported_work(self):
        source = Path(__file__).resolve().parents[1] / "examples/distribution-laws/modulo-bias"
        result = self.cli(["contract", "distribution", str(source), "--max-work", "1", "--format", "json"], self.log)
        self.assertEqual(result.returncode, 2, result.stderr)
        report = json.loads(result.stdout)
        row = json.loads(self.log.read_text())
        self.assertEqual(row["verdict"], "unknown")
        self.assertIsInstance(row["work_units"], dict)
        self.assertEqual(row["work_units"]["measured.work"], report["measured"]["work"])

    def test_cli_concurrent_lines(self):
        with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
            results = list(pool.map(lambda _: self.cli(["--help"], self.log), range(16)))
        self.assertTrue(all(result.returncode == 0 for result in results))
        self.assertEqual(len([json.loads(line) for line in self.log.read_text().splitlines()]), 16)


if __name__ == "__main__":
    unittest.main()
