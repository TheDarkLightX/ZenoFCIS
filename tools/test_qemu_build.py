"""Exercise nested build-directory locking without installing an emulator."""

from pathlib import Path
import os
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import qemu_demo


class QemuBuildTests(unittest.TestCase):
    @unittest.skipUnless(os.name == "posix", "the QEMU runner targets Unix hosts")
    def test_shared_target_does_not_lock_out_the_nested_installer(self) -> None:
        with tempfile.TemporaryDirectory(prefix="zeno-qemu-build-") as directory:
            root = Path(directory)
            target = root / "shared target"
            cargo = root / "cargo"
            cargo.write_text(
                f"#!{sys.executable}\n"
                "import fcntl, os, subprocess, sys\n"
                "from pathlib import Path\n"
                "args = sys.argv[1:]\n"
                "target = Path(args[args.index('--target-dir') + 1] if '--target-dir' in args else os.environ['CARGO_TARGET_DIR'])\n"
                "target.mkdir(parents=True, exist_ok=True)\n"
                "with (target / 'build.lock').open('w') as lock:\n"
                "    fcntl.flock(lock, fcntl.LOCK_EX)\n"
                "    child = '''import fcntl, os, sys\n"
                "from pathlib import Path\n"
                "target = os.environ.get('CARGO_TARGET_DIR')\n"
                "if target is None: sys.exit(0)\n"
                "with (Path(target) / 'build.lock').open('w') as lock:\n"
                "    try: fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)\n"
                "    except BlockingIOError: sys.exit(74)\n"
                "'''\n"
                "    result = subprocess.run([sys.executable, '-c', child])\n"
                "    if result.returncode: sys.exit(result.returncode)\n"
                "image = target / 'guest.img'\n"
                "image.write_bytes(b'build path test')\n"
                "print(image)\n",
                encoding="utf-8",
            )
            cargo.chmod(0o700)
            with patch.dict(os.environ, {"CARGO_TARGET_DIR": str(target)}):
                # The unmediated nested build demonstrably contends for the
                # parent lock, without leaving an indefinitely blocked process.
                control = subprocess.run([str(cargo)], check=False, timeout=10)
                self.assertEqual(control.returncode, 74)
                with patch.object(qemu_demo, "executable", return_value=cargo):
                    image = qemu_demo.build_image()
                self.assertEqual(image, target / "guest.img")
                self.assertEqual(image.read_bytes(), b"build path test")
                self.assertEqual(os.environ["CARGO_TARGET_DIR"], str(target))


if __name__ == "__main__":
    unittest.main()
