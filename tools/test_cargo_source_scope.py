"""Exercise backdated source copies and restored mutants in one Cargo target."""
from __future__ import annotations

import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from cargo_source_scope import source_environment


class SourceScopeTest(unittest.TestCase):
    def test_binary_environment_and_arguments_are_preserved(self) -> None:
        with tempfile.TemporaryDirectory(prefix="compiler-scope-env-") as directory:
            environment=source_environment(Path(directory),dict(os.environ,
                CARGO_TARGET_DIR=str(Path(directory) / "target")))
            environment["CARGO_BIN_EXE_app-server"]="test-sentinel"
            result=subprocess.run([environment["RUSTC_WORKSPACE_WRAPPER"],sys.executable,"-c",
                "import os,sys; print(os.environ.get('CARGO_BIN_EXE_app-server')); print(repr(sys.argv[1:]))",
                "argument with spaces", "", "literal-$value"],env=environment,capture_output=True,text=True,check=False)
            self.assertEqual(result.returncode,0,result.stderr)
            self.assertEqual(result.stdout.splitlines(),["test-sentinel","['argument with spaces', '', 'literal-$value']"])

    def test_backdated_copy_and_restored_mutant(self) -> None:
        cargo = ["cargo", "+1.97.1", "test", "--locked", "--offline",
                 "--lib", "exact_source", "--", "--exact", "--nocapture"]

        def code(answer: int, expected: int) -> str:
            return (f"fn answer() -> u8 {{ {answer} }}\n"
                    f"#[test] fn exact_source() {{ assert_eq!(answer(), {expected}, "
                    f"\"expected source {expected}\"); println!(\"verified source {expected}\"); }}\n")

        def run(root: Path, expected: int, status: int = 0) -> None:
            result = subprocess.run(cargo, cwd=root, env=source_environment(root, environment),
                                    text=True, stdout=subprocess.PIPE,
                                    stderr=subprocess.STDOUT, timeout=120)
            self.assertEqual(result.returncode, status, result.stdout)
            if status == 0:
                self.assertIn("1 passed", result.stdout)
                self.assertIn(f"verified source {expected}", result.stdout)
            else:
                self.assertIn(f"expected source {expected}", result.stdout)
                self.assertIn("test result: FAILED", result.stdout)

        with tempfile.TemporaryDirectory(prefix="cargo-source-scope-") as directory:
            environment = dict(os.environ, CARGO_NET_OFFLINE="true",
                               CARGO_TARGET_DIR=str(Path(directory) / "target"))
            roots = [Path(directory) / name for name in ("first", "second")]
            for expected, root in enumerate(roots, 1):
                (root / "src").mkdir(parents=True)
                (root / "Cargo.toml").write_text(
                    '[package]\nname="source-scope-sample"\nversion="0.0.0"\n'
                    'edition="2024"\n[workspace]\n')
                (root / "Cargo.lock").write_text(
                    'version = 4\n[[package]]\nname = "source-scope-sample"\nversion = "0.0.0"\n')
                (root / "src/lib.rs").write_text(code(expected, expected))
                os.utime(root / "src/lib.rs", (1, 1))
            run(roots[0], 1)
            run(roots[1], 2)
            source = roots[1] / "src/lib.rs"
            source.write_text(code(3, 2))
            run(roots[1], 2, 101)
            source.write_text(code(2, 2))
            run(roots[1], 2)


if __name__ == "__main__":
    unittest.main()
