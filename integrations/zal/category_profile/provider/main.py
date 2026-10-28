"""Sealed standalone loader; only captured, manifest-pinned trusted bytes run."""
import hashlib
import json
from pathlib import Path
import sys
import types

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parent
MANIFEST_SHA256 = '2628e184504a84753b9a775779e9e9660ea3f6947734e96de64aae41e7c91cec'
manifest_path = ROOT / 'SOURCE_MANIFEST.json'
if manifest_path.is_symlink() or manifest_path.stat().st_size > 1024**2:
    raise SystemExit('ordinary bounded manifest required')
manifest = manifest_path.read_bytes()
if hashlib.sha256(manifest).hexdigest() != MANIFEST_SHA256:
    raise SystemExit('source manifest changed')
pins = json.loads(manifest)['files']
data = {}
for name, digest in pins.items():
    path = ROOT / name
    if path.is_symlink() or not path.is_file() or path.stat().st_size > 1024**2:
        raise SystemExit('ordinary bounded source required')
    blob = path.read_bytes()
    if hashlib.sha256(blob).hexdigest() != digest:
        raise SystemExit('source changed: ' + name)
    data[name] = blob
profile = types.ModuleType('_captured_lens_profile')
profile.__file__ = str(ROOT / 'profile.py')
profile.__manifest_sha256__ = MANIFEST_SHA256
exec(compile(data['profile.py'], profile.__file__, 'exec'), profile.__dict__)
if __name__ == '__main__':
    profile.main()
