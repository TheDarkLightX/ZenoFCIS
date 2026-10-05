"""The README and release-build checks of the application journey gate."""

import tempfile
import unittest
from pathlib import Path

import check_app_journey as journey


class ReadmeTests(unittest.TestCase):
    def test_the_steps_are_read_from_the_build_and_run_block_and_run_as_written(self):
        readme = (journey.ROOT / "crates/zeno-fcis-cli/contract-app/README.md.in").read_text()
        with tempfile.TemporaryDirectory(prefix="zeno-fcis-readme-") as directory:
            app = Path(directory)
            (app / "README.md").write_text(readme)
            commands = journey.readme_commands(app)
            self.assertEqual(commands, [
                ["cargo", "test", "--offline"],
                ["cargo", "run", "--offline", "--", "NEW_DATABASE_PATH"],
            ])
            self.assertEqual(journey.as_written(commands[1], Path("/new/db.sqlite")),
                             ["cargo", "run", "--offline", "--", "/new/db.sqlite"])
            for changed, message in ((readme.replace("## Build and run", "## Build"), "Build and run"),
                                     (readme.replace("```sh\n", "```\n", 1), "sh block")):
                (app / "README.md").write_text(changed)
                with self.subTest(message=message), self.assertRaisesRegex(RuntimeError, message):
                    journey.readme_commands(app)


class ReleaseBuildScanTests(unittest.TestCase):
    def test_the_scan_finds_the_checkout_path_only_where_it_is(self):
        marker = str(journey.ROOT)
        with tempfile.TemporaryDirectory(prefix="zeno-fcis-scan-") as directory:
            binary = Path(directory) / "zeno-fcis"
            binary.write_bytes(b"\x7fELF\x00" + f"{marker}/crates/zeno-fcis-cli/../..".encode() + b"\x00")
            self.assertTrue(journey.holds(binary, marker))
            binary.write_bytes(b"\x7fELF\x00/zeno-fcis-source/crates/zeno-fcis-cli\x00")
            self.assertFalse(journey.holds(binary, marker))


if __name__ == "__main__":
    unittest.main()
