"""Every fuzz harness is registered, built by the workflow and states its oracle.

The harnesses compile only under the pinned nightly with cargo-fuzz, which
the fuzz-build workflow installs; this test keeps the three registrations
(source file, Cargo binary, workflow build step) consistent without building.
"""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[1]
FUZZ = ROOT / "fuzz"
WORKFLOW = ROOT / ".github/workflows/fuzz.yml"
# The untrusted-input decoders the 2.7 gap register (F27-26) names.
REQUIRED = {"codec_roundtrip", "candidate_bundle", "zeno_parser", "codec2_envelope", "schema2_envelope", "worker_channel_frames"}
# Harnesses with a differential oracle; zeno_parser is a crash-only harness by design.
DIFFERENTIAL = {"codec_roundtrip", "candidate_bundle", "codec2_envelope", "schema2_envelope", "worker_channel_frames"}


def sources():
    return {path.stem: path for path in (FUZZ / "fuzz_targets").glob("*.rs")}


def registered_bins():
    manifest = (FUZZ / "Cargo.toml").read_text()
    names = re.findall(r'^\[\[bin\]\]\nname = "([a-z0-9_]+)"\npath = "fuzz_targets/\1\.rs"\ntest = false\ndoc = false\nbench = false$',
                       manifest, re.M)
    return set(names), manifest.count("[[bin]]")


class FuzzTargets(unittest.TestCase):
    def test_every_source_is_a_registered_binary_and_vice_versa(self):
        names, count = registered_bins()
        self.assertEqual(count, len(names), "a [[bin]] entry deviates from the fixed shape")
        self.assertEqual(names, set(sources()))

    def test_required_decoders_have_harnesses(self):
        self.assertLessEqual(REQUIRED, set(sources()))

    def test_workflow_builds_every_harness(self):
        built = set(re.findall(r"cargo \+nightly-\d{4}-\d{2}-\d{2} fuzz build ([a-z0-9_]+)", WORKFLOW.read_text()))
        self.assertEqual(built, set(sources()))

    def test_every_harness_states_its_oracle_and_uses_no_unwrap_on_input(self):
        for name, path in sorted(sources().items()):
            text = path.read_text()
            with self.subTest(target=name):
                self.assertTrue(text.startswith("#![no_main]\n"))
                self.assertIn("fuzz_target!(", text)
                if name in DIFFERENTIAL:
                    self.assertIn("assert", text, "a differential harness compares against its oracle")
                    self.assertNotIn(".unwrap()", text, "refusals on fuzz input are expected; use let-else with a message")

    def test_new_harnesses_depend_only_on_workspace_crates(self):
        manifest = (FUZZ / "Cargo.toml").read_text()
        dependencies = re.findall(r"^([a-z0-9-]+) = \{", manifest.split("[dependencies]", 1)[1].split("\n\n", 1)[0], re.M)
        for name in dependencies:
            with self.subTest(dependency=name):
                self.assertTrue(name == "libfuzzer-sys" or (ROOT / "crates" / name).is_dir(), "no new external dependency")
        self.assertRegex(manifest, r'libfuzzer-sys = "=0\.4\.13"', "the fuzz runtime stays pinned")


if __name__ == "__main__":
    unittest.main()
