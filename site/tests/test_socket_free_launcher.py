"""Check short browser profiles independently of inherited temporary paths."""
import os
from pathlib import Path
import unittest
from unittest.mock import patch

from socket_free_check import short_profile


class ShortProfileTests(unittest.TestCase):
    def test_long_tmpdir_does_not_lengthen_chrome_singleton_path(self):
        with patch.dict(os.environ, {'TMPDIR': '/tmp/' + 'long-parent-' * 30}):
            with short_profile() as profile:
                path = Path(profile)
                self.assertEqual(path.parent, Path('/tmp'))
                self.assertTrue(path.is_dir())
                self.assertLess(len(os.fsencode(profile)) + 60, 108)
            self.assertFalse(path.exists())


if __name__ == '__main__':
    unittest.main()
