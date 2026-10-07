"""Legacy release-only archive/mirror fixtures; not S1 evaluator assurance."""
import shutil
import tempfile
import unittest
from pathlib import Path

import authority_source_bundle as bundle


class SourceBundleTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name) / 'source'
        self.originals = {
            'Cargo.lock': b'qualified original resolver\n',
            'crates/zeno-fcis-codec/Cargo.toml': b'name = "codec"\nversion = "2.0.0"\n',
            'crates/zeno-fcis-codec/src/lib.rs': b'pub fn checked() -> bool { true }\n',
            'crates/zeno-fcis-synthesis/Cargo.toml': b'version.workspace = true\n',
            'crates/zeno-fcis-synthesis/src/lib.rs': b'pub fn execute() -> bool { true }\n',
        }
        self.paths = sorted(self.originals)
        for name, data in self.originals.items():
            target = self.root / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)

    def generate(self):
        return bundle.generate(self.root, self.paths)

    def archives(self):
        self.generate()
        packages = {}
        for crate in ['zeno-fcis-codec', 'zeno-fcis-synthesis']:
            target = Path(self.temporary.name) / 'archives' / crate
            shutil.copytree(self.root / 'crates' / crate, target)
            (target / 'Cargo.toml').rename(target / 'Cargo.toml.orig')
            (target / 'Cargo.toml').write_bytes(b'name = "normalized"\n')
            packages[crate] = target
        return packages

    def test_exact_original_bytes_and_local_sources_are_direct(self):
        result = self.generate()
        self.assertEqual(len(result['mirrored_rows']), 4)
        for name, data in self.originals.items():
            physical = bundle.transport_path(name)
            self.assertEqual((self.root / physical).read_bytes(), data)
            if name.endswith('synthesis/src/lib.rs'):
                self.assertEqual(physical, Path(name))

    def test_regeneration_is_deterministic(self):
        first = self.generate()
        before = {p.relative_to(self.root): p.read_bytes()
                  for p in (self.root / bundle.BUNDLE).rglob('*') if p.is_file()}
        self.assertEqual(self.generate(), first)
        self.assertEqual(before, {p.relative_to(self.root): p.read_bytes()
                                for p in (self.root / bundle.BUNDLE).rglob('*') if p.is_file()})

    def test_unread_original_prevents_transport_writes(self):
        (self.root / self.paths[-1]).unlink()
        # Use an external/raw-manifest row so it is a required generation input.
        (self.root / 'Cargo.lock').unlink()
        with self.assertRaises(ValueError):
            self.generate()
        self.assertFalse((self.root / bundle.BUNDLE).exists())

    def test_stale_and_substituted_mirrors_refused(self):
        self.generate()
        mirror = self.root / bundle.transport_path('Cargo.lock')
        mirror.write_bytes(b'different resolver\n')
        with self.assertRaisesRegex(ValueError, 'stale or substituted'):
            bundle.check(self.root, self.paths)

    def test_missing_and_extra_entries_refused(self):
        self.generate()
        target = self.root / bundle.transport_path('Cargo.lock')
        target.unlink()
        with self.assertRaisesRegex(ValueError, 'missing or extra'):
            bundle.check(self.root, self.paths)
        self.generate()
        (self.root / bundle.BUNDLE / 'unregistered.source').write_bytes(b'extra')
        with self.assertRaisesRegex(ValueError, 'missing or extra'):
            bundle.check(self.root, self.paths)

    def test_symlink_mirror_and_original_refused(self):
        self.generate()
        target = self.root / bundle.transport_path('Cargo.lock')
        target.unlink()
        target.symlink_to(self.root / 'Cargo.lock')
        with self.assertRaisesRegex(ValueError, 'symlink'):
            bundle.check(self.root, self.paths)
        target.unlink()
        original = self.root / 'Cargo.lock'
        original.unlink()
        original.symlink_to(self.root / self.paths[-1])
        with self.assertRaisesRegex(ValueError, 'symlink'):
            self.generate()

    def test_names_must_be_safe_sorted_and_unique(self):
        for paths in [list(reversed(self.paths)), self.paths + [self.paths[-1]]]:
            with self.assertRaisesRegex(ValueError, 'sorted unique'):
                bundle.generate(self.root, paths)
        for name in ['../outside.rs', '/outside.rs', '', '.']:
            with self.assertRaisesRegex(ValueError, 'unsafe'):
                bundle.transport_path(name)

    def test_hidden_directory_transport_is_portable_and_keeps_original_key(self):
        name = '.cargo/config.toml'
        physical = bundle.transport_path(name)
        self.assertEqual(physical, bundle.BUNDLE / 'dot-cargo/config.toml.source')
        self.assertFalse(any(part.startswith('.') for part in physical.parts))
        target = self.root / name
        target.parent.mkdir(parents=True)
        target.write_bytes(b'exact original cargo configuration\n')
        result = bundle.generate(self.root, sorted(self.paths + [name]))
        self.assertIn(name, [row['path'] for row in result['mirrored_rows']])
        self.assertEqual((self.root / physical).read_bytes(), target.read_bytes())

    def test_hidden_directory_alias_collision_refused_before_writes(self):
        with self.assertRaisesRegex(ValueError, 'collision'):
            bundle.generate(self.root, ['.cargo/config.toml', 'dot-cargo/config.toml'])
        self.assertFalse((self.root / bundle.BUNDLE).exists())

    def test_actual_archive_sources_and_raw_manifests_match(self):
        packages = self.archives()
        result = bundle.check_archives(self.root, self.paths, packages)
        self.assertEqual(len(result['archive_source_rows']), 4)
        self.assertEqual(result['workspace_build_bindings'], ['Cargo.lock'])
        self.assertTrue(result['effective_manifest_resolver_and_compiler_checks_required'])

    def test_same_version_dependency_body_substitution_refused(self):
        packages = self.archives()
        (packages['zeno-fcis-codec'] / 'src/lib.rs').write_bytes(b'wrong implementation')
        with self.assertRaisesRegex(ValueError, 'compiled archive source differs'):
            bundle.check_archives(self.root, self.paths, packages)

    def test_archive_raw_manifest_substitution_refused(self):
        packages = self.archives()
        (packages['zeno-fcis-synthesis'] / 'Cargo.toml.orig').write_bytes(b'normalized replacement')
        with self.assertRaisesRegex(ValueError, 'compiled archive source differs'):
            bundle.check_archives(self.root, self.paths, packages)

    def test_archive_mirror_substitution_refused(self):
        packages = self.archives()
        target = packages['zeno-fcis-synthesis'] / bundle.transport_path('Cargo.lock').relative_to(bundle.PACKAGE)
        target.write_bytes(b'unqualified resolver')
        with self.assertRaisesRegex(ValueError, 'archive source mirror differs'):
            bundle.check_archives(self.root, self.paths, packages)


if __name__ == '__main__':
    unittest.main()
