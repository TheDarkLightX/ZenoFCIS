"""The source-pin producers: current pins pass, planted changes are found, rebound or refused."""
import contextlib
import hashlib
import io
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import check_arithmetic_typed_proof as typed
import check_v1_compatibility as v1
import pin_producer as pins
import record_arithmetic_proof_sources as arithmetic
import record_category_capsule as capsule
import record_core_app_references as references
import record_finite_library_components as finite
import record_v1_baseline as baseline

ROOT = pins.ROOT


def run(producer, mode, root=ROOT, apps=None):
    plan = producer.plan if apps is None else (lambda r, b: producer.plan(r, b, apps))
    output = io.StringIO()
    with contextlib.redirect_stdout(output):
        code = pins.main(plan, "", [mode], root=root)
    return code, output.getvalue()


def sha(data):
    return hashlib.sha256(data).hexdigest()


class Fixture:
    """A committed copy of the real binding and its bound files."""

    def __init__(self, test, paths):
        temporary = tempfile.TemporaryDirectory()
        test.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        for name in sorted(set(paths)):
            (self.root / name).parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / name, self.root / name)
        for argv in (["init", "-q"], ["add", "-A"],
                     ["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "base"]):
            subprocess.run(["git", *argv], cwd=self.root, check=True)
        self.before = self.snapshot()

    def path(self, name):
        return self.root / name

    def append(self, name, data=b"\n// planted\n"):
        with self.path(name).open("ab") as stream:
            stream.write(data)

    def edit_json(self, name, change, layout=pins.render):
        value = json.loads(self.path(name).read_bytes())
        change(value)
        self.path(name).write_bytes(layout(value))

    def snapshot(self):
        return {str(p.relative_to(self.root)): p.read_bytes() for p in self.root.rglob("*")
                if p.is_file() and ".git" not in p.relative_to(self.root).parts}

    def changed(self, since):
        now = self.snapshot()
        return sorted(n for n in set(now) | set(since) if now.get(n) != since.get(n))


class ProducerContract:
    """Each producer: clean check, stale bound file, rebind of only that pin, refusal outside the closure."""
    producer = apps = None

    def paths(self):
        raise NotImplementedError

    def fixture(self):
        return Fixture(self, self.paths())

    def check(self, fixture):
        return run(self.producer, "--check", fixture.root, self.apps)

    def write(self, fixture):
        return run(self.producer, "--write", fixture.root, self.apps)

    def test_current_pins_pass(self):
        self.assertEqual(run(self.producer, "--check", ROOT, self.apps), (0, "0 stale pins\n"))

    def stale_then_rebound(self, planted, expected_files):
        """Plant one bound-file change; the check names it, the write rebinds only its pins."""
        fixture = self.fixture()
        planted(fixture)
        code, output = self.check(fixture)
        self.assertEqual(code, 1, output)
        self.assertEqual(fixture.changed(fixture.before), [self.planted_name], "check writes nothing")
        self.assertIn(self.planted_name, output)
        planted_state = fixture.snapshot()
        code, written = self.write(fixture)
        self.assertEqual(code, 0, written)
        self.assertEqual(written.splitlines()[:-1], output.splitlines()[:-1], "write prints the same list")
        self.assertEqual(fixture.changed(planted_state), sorted(expected_files))
        self.assertEqual(self.check(fixture), (0, "0 stale pins\n"))
        return fixture, output

    def refused(self, fixture, fragment):
        state = fixture.snapshot()
        for mode in ("--check", "--write"):
            code, output = run(self.producer, mode, fixture.root, self.apps)
            self.assertEqual(code, 2, output)
            self.assertTrue(output.startswith("refused: "), output)
            self.assertIn(fragment, output)
        self.assertEqual(fixture.changed(state), [], "a refusal writes nothing")

    def test_symlinked_or_missing_bound_file_is_refused(self):
        fixture = self.fixture()
        self.path_of_bound(fixture).unlink()
        self.refused(fixture, "missing bound file")
        fixture = self.fixture()
        target = self.path_of_bound(fixture)
        moved = target.with_name(target.name + ".moved")
        target.rename(moved)
        target.symlink_to(moved.name)
        self.refused(fixture, "symlinked bound path")

    def path_of_bound(self, fixture):
        return fixture.path(self.planted_name)


class ArithmeticProofSources(ProducerContract, unittest.TestCase):
    producer = arithmetic
    planted_name = "crates/zeno-fcis-synthesis/src/finite/evaluation/typed.rs"

    def paths(self):
        names = list(arithmetic.PROFILES)
        for profile in arithmetic.PROFILES:
            names += json.loads((ROOT / profile).read_text())["source_sha256"]
        return names

    def test_planted_source_change_rebinds_exactly_its_pins(self):
        fixture, output = self.stale_then_rebound(lambda f: f.append(self.planted_name), arithmetic.PROFILES)
        self.assertEqual(output.count(self.planted_name), 2, "bound by both profiles")
        fresh = sha(fixture.path(self.planted_name).read_bytes())
        for name in arithmetic.PROFILES:
            before, after = json.loads(fixture.before[name]), json.loads(fixture.path(name).read_bytes())
            before["source_sha256"][self.planted_name] = fresh
            self.assertEqual(after, before)
            typed.require_sources(fixture.root, after)

    def test_closure_or_field_change_is_refused(self):
        fixture = self.fixture()
        fixture.edit_json(arithmetic.PROFILES[0], lambda p: p["source_sha256"].update({"Cargo.toml.extra": "0" * 64}))
        self.refused(fixture, "change outside the declared closure")
        fixture = self.fixture()
        fixture.edit_json(arithmetic.PROFILES[1], lambda p: p.update(expected_verified=p["expected_verified"] + 1))
        self.refused(fixture, "change outside the declared closure")


class V1Baseline(ProducerContract, unittest.TestCase):
    producer = baseline
    planted_name = "crates/zeno-fcis-value/src/lib.rs"

    def paths(self):
        return [baseline.BASELINE, *(f["path"] for f in json.loads((ROOT / baseline.BASELINE).read_text())["files"])]

    def test_planted_source_change_rebinds_exactly_its_pin(self):
        fixture, _ = self.stale_then_rebound(lambda f: f.append(self.planted_name), [baseline.BASELINE])
        data = fixture.path(self.planted_name).read_bytes()
        before, after = json.loads(fixture.before[baseline.BASELINE]), json.loads(fixture.path(baseline.BASELINE).read_bytes())
        for item in before["files"]:
            if item["path"] == self.planted_name:
                item.update(bytes=len(data), sha256=sha(data))
        self.assertEqual(after, before)
        with patch.object(v1, "ROOT", fixture.root):
            self.assertEqual(v1.check(), after)

    def test_closure_or_field_change_is_refused(self):
        fixture = self.fixture()
        fixture.edit_json(baseline.BASELINE, lambda b: b.update(baseline_commit="0" * 40))
        self.refused(fixture, "change outside the declared closure")
        fixture = self.fixture()
        fixture.edit_json(baseline.BASELINE, lambda b: b["files"].pop())
        self.refused(fixture, "change outside the declared closure")


class CategoryCapsule(ProducerContract, unittest.TestCase):
    producer = capsule
    planted_name = "integrations/zal/category_profile/provider/oracle.py"

    def paths(self):
        tracked = subprocess.run(["git", "ls-files", capsule.PROFILE], cwd=ROOT, capture_output=True,
                                 text=True, check=True).stdout.split()
        dependencies = json.loads((ROOT / capsule.DEPENDENCIES).read_text())["files"]
        return [*tracked, *(capsule.ZAL + name for name in dependencies)]

    def path_of_bound(self, fixture):
        # A provider file would first fail the provider-directory closure.
        return fixture.path("integrations/zal/zal_notation/core.py")

    def accepted_by_the_host(self, fixture):
        """The rebound capsule loads through the fixture's own host, once per process as it requires."""
        load = ("import importlib.util, sys\n"
                "spec = importlib.util.spec_from_file_location('host', sys.argv[1])\n"
                "host = importlib.util.module_from_spec(spec); spec.loader.exec_module(host)\n"
                "print(host.load_adapter().PROVIDER_MANIFEST)\n")
        loaded = subprocess.run([sys.executable, "-I", "-B", "-c", load, str(fixture.path(capsule.HOST))],
                                capture_output=True, text=True, timeout=60)
        self.assertEqual(loaded.returncode, 0, loaded.stderr)
        self.assertEqual(loaded.stdout.strip(), sha(fixture.path(capsule.MANIFEST).read_bytes()))

    def test_planted_provider_change_rebinds_the_chain(self):
        fixture, output = self.stale_then_rebound(lambda f: f.append(self.planted_name, b"\n# planted\n"),
            [capsule.MANIFEST, capsule.MAIN, capsule.DEPENDENCIES, capsule.ADAPTER, capsule.HOST])
        for line in ("SOURCE_MANIFEST.json: " + self.planted_name, "main.py MANIFEST_SHA256",
                     "adapter.py PROVIDER_MANIFEST", "DEPENDENCIES.json: " + capsule.MAIN,
                     "adapter.py DEPENDENCIES_SHA256", "host.py ADAPTER_SHA256"):
            self.assertIn(line, output)
        self.accepted_by_the_host(fixture)

    def test_planted_library_change_rebinds_only_the_dependency_chain(self):
        name = "integrations/zal/zal_notation/core.py"
        fixture = self.fixture()
        fixture.append(name, b"\n# planted\n")
        self.assertEqual(self.check(fixture)[0], 1)
        planted = fixture.snapshot()
        self.assertEqual(self.write(fixture)[0], 0)
        self.assertEqual(fixture.changed(planted), sorted([capsule.DEPENDENCIES, capsule.ADAPTER, capsule.HOST]))
        self.accepted_by_the_host(fixture)

    def test_undeclared_provider_file_or_manifest_field_is_refused(self):
        fixture = self.fixture()
        fixture.path(capsule.PROVIDER + "extra.py").write_text("planted = True\n")
        self.refused(fixture, "change outside the declared closure")
        fixture = self.fixture()
        fixture.edit_json(capsule.DEPENDENCIES, lambda d: d.update(scope=d["scope"] + " Planted."))
        self.refused(fixture, "change outside the declared closure")


class FiniteLibraryComponents(ProducerContract, unittest.TestCase):
    producer = finite
    planted_name = "integrations/zal/semantic_library.py"

    def paths(self):
        components = json.loads((ROOT / finite.UPSTREAM).read_text())["components"]
        return [finite.UPSTREAM, *(finite.bound(entry["path"]) for entry in components.values())]

    def test_planted_component_change_rebinds_exactly_its_pin(self):
        fixture, output = self.stale_then_rebound(lambda f: f.append(self.planted_name, b"\n# planted\n"),
                                                  [finite.UPSTREAM])
        self.assertIn("UPSTREAM.json: " + self.planted_name, output)
        before, after = json.loads(fixture.before[finite.UPSTREAM]), json.loads(fixture.path(finite.UPSTREAM).read_bytes())
        before["components"]["../semantic_library.py"]["sha256"] = sha(fixture.path(self.planted_name).read_bytes())
        self.assertEqual(after, before)

    def test_closure_or_field_change_is_refused(self):
        fixture = self.fixture()
        fixture.edit_json(finite.UPSTREAM, lambda u: u.update(scope=u["scope"] + " Planted."))
        self.refused(fixture, "change outside the declared closure")
        fixture = self.fixture()
        fixture.edit_json(finite.UPSTREAM, lambda u: u["components"].pop("LICENSE"))
        self.refused(fixture, "change outside the declared closure")
        fixture = self.fixture()
        fixture.edit_json(finite.UPSTREAM, lambda u: u["components"]["LICENSE"].update(path="../LICENSE"))
        self.refused(fixture, "change outside the declared closure")

    def test_component_paths_stay_inside_the_zal_integration(self):
        self.assertEqual(finite.bound("../semantic_library.py"), "integrations/zal/semantic_library.py")
        for path in ("../../README.md", "/etc/passwd", "../../../x"):
            with self.subTest(path=path), self.assertRaises(pins.Refusal):
                finite.bound(path)


def certificate_layout(value):
    return (json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n").encode()


class CoreAppReferences(ProducerContract, unittest.TestCase):
    producer = references
    apps = references.applications()
    planted_name = "core-components/proofs/rate-limiter.json"

    def paths(self):
        names = subprocess.run(["git", "ls-files", "core-components/proofs",
                                *(f"core-apps/{app}/composition.json" for app in self.apps),
                                *(f"core-apps/{app}/components" for app in self.apps)],
                               cwd=ROOT, capture_output=True, text=True, check=True).stdout.split()
        return names

    def referencing(self, fixture, certificate):
        found = []
        for app in self.apps:
            for entry in json.loads(fixture.path(f"core-apps/{app}/composition.json").read_bytes())["components"]:
                directory = f"core-apps/{app}/components/{entry['name']}"
                instance = json.loads(fixture.path(directory + "/core-instance.json").read_bytes())
                if instance["finite_family_certificate"]["path"] == certificate:
                    found.append((app, directory))
        return found

    def test_refreshed_certificate_rebinds_its_references(self):
        planted = lambda f: f.edit_json(self.planted_name, lambda c: c.update(cli_sha256="1" * 64), certificate_layout)
        fixture = self.fixture()
        users = self.referencing(fixture, self.planted_name)
        self.assertTrue(users)
        expected = [n for app, d in users for n in (d + "/core-instance.json", d + "/project.zeno",
                                                     f"core-apps/{app}/composition.json")]
        fixture, _ = self.stale_then_rebound(planted, sorted(set(expected)))
        fresh = sha(fixture.path(self.planted_name).read_bytes())
        for app, directory in users:
            instance = fixture.path(directory + "/core-instance.json").read_bytes()
            self.assertEqual(json.loads(instance)["finite_family_certificate"]["sha256"], fresh)
            self.assertEqual(fixture.path(directory + "/project.zeno").read_bytes().count(fresh.encode()), 1)
            name = directory.rsplit("/", 1)[1]
            composition = json.loads(fixture.path(f"core-apps/{app}/composition.json").read_bytes())
            self.assertEqual(next(c for c in composition["components"] if c["name"] == name)["instance_sha256"],
                             sha(instance))

    def test_certificate_or_component_change_outside_the_pins_is_refused(self):
        fixture = self.fixture()
        fixture.edit_json(self.planted_name, lambda c: c.update(cli_sha256="1" * 64, family="planted"),
                          certificate_layout)
        self.refused(fixture, "fields ['cli_sha256', 'family']")
        fixture = self.fixture()
        app, directory = self.referencing(fixture, self.planted_name)[0]
        fixture.append(directory + "/v2/policy.json", b" ")
        self.refused(fixture, directory + "/v2/policy.json: change outside the declared closure")
        fixture = self.fixture()
        fixture.edit_json(directory + "/core-instance.json", lambda i: i.update(owner_adoption=True))
        self.refused(fixture, "a field other than the certificate SHA-256")

    def test_a_source_digest_moves_only_with_its_file(self):
        bound = sorted(json.loads((ROOT / self.planted_name).read_bytes())["sources"])[0]
        # Accepted: the bound file's bytes changed, and the certificate records the new digest.
        fixture = self.fixture()
        target = fixture.path(bound)
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes((ROOT / bound).read_bytes() + b"edited for this refresh\n")
        fresh = sha(target.read_bytes())
        fixture.edit_json(self.planted_name, lambda c: (c.update(cli_sha256="1" * 64), c["sources"].update({bound: fresh})),
                          certificate_layout)
        code, output = self.check(fixture)
        self.assertEqual(code, 1, output)  # stale pins listed, nothing refused
        self.assertNotIn("refused", output)
        self.assertIn(self.planted_name, output)
        # Refused: the digest moves although the file still matches the base certificate's record.
        fixture = self.fixture()
        target = fixture.path(bound)
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes((ROOT / bound).read_bytes())
        fixture.edit_json(self.planted_name, lambda c: c["sources"].update({bound: "0" * 64}), certificate_layout)
        self.refused(fixture, "source digest moved for an unchanged or mismatched file")
        # Refused: a bound file that is absent cannot justify a moved digest.
        fixture = self.fixture()
        fixture.edit_json(self.planted_name, lambda c: c["sources"].update({bound: "0" * 64}), certificate_layout)
        self.refused(fixture, "source digest moved for an unreadable file")


if __name__ == "__main__":
    unittest.main()
