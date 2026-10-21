"""Exercise backdated source copies and restored mutants in one Cargo target."""
from __future__ import annotations

import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from cargo_source_scope import source_environment


class SourceScopeTest(unittest.TestCase):
    def test_backdated_copy_and_restored_mutant(self) -> None:
        environment = dict(os.environ, CARGO_NET_OFFLINE="true")
        self.assertIn("CARGO_TARGET_DIR", environment)
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
