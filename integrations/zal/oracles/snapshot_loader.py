"""Load exactly the bytes whose hash is recorded, avoiding import/hash races."""
import hashlib
from pathlib import Path
import sys
import types

def load_behavior(source):
    if not __debug__:
        raise RuntimeError("Oracle checks require assertions; run Python without -O")
    # Load only the checked repository module; ZAL text is never executed as Python.
    raw=Path(source).read_bytes()
    identity=hashlib.sha256(raw).hexdigest()
    name='_zal_review_behavior_'+identity
    module=types.ModuleType(name)
    module.__file__=str(source)
    sys.modules[name]=module
    exec(compile(raw,str(source),'exec'),module.__dict__)
    return module,identity
