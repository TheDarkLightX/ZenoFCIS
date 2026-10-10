"""Release version drift must fail before expensive acceptance or packaging."""
from pathlib import Path
import shutil
import tempfile
import tomllib
import unittest

import rc_package as release


class ScaffoldVersions(unittest.TestCase):
    def setUp(self):
        self.version = tomllib.loads((release.ROOT / 'Cargo.toml').read_text())['workspace']['package']['version']

    def test_current_template_and_contract_pins_match_the_release(self):
        release.validate_scaffold_versions(self.version)

    def test_each_template_with_a_stale_pin_is_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            paths = list((release.ROOT / 'crates').glob('*/Cargo.toml'))
            templates = list((release.ROOT / 'crates/zeno-fcis-cli/templates').glob('*/Cargo.toml.in'))
            templates.append(release.ROOT / 'crates/zeno-fcis-cli/contract-app/Cargo.toml.in')
            for path in paths + templates:
                target = root / path.relative_to(release.ROOT)
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(path, target)
            release.validate_scaffold_versions(self.version, root)
            for path in templates:
                target = root / path.relative_to(release.ROOT)
                original = target.read_text()
                token = '={version}' if '={version}' in original else '=' + self.version
                self.assertIn(token, original)
                target.write_text(original.replace(token, '=0.0.0-stale', 1))
                with self.subTest(template=str(path.relative_to(release.ROOT))):
                    with self.assertRaisesRegex(release.RcError, 'dependency pin is invalid'):
                        release.validate_scaffold_versions(self.version, root)
                target.write_text(original)
            # A missing manifest also fails rather than silently skipping a template.
            (root / templates[0].relative_to(release.ROOT)).unlink()
            with self.assertRaises(FileNotFoundError):
                release.validate_scaffold_versions(self.version, root)


if __name__ == '__main__':
    unittest.main()
