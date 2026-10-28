"""Captured product host and explicit immutable operator context, outside JSON."""
from dataclasses import dataclass
import hashlib
from pathlib import Path
import sys
import threading
import types

ROOT = Path(__file__).resolve().parent
ADAPTER_SHA256 = '4cac1609013e5c13c191a1ba57b72f367db4c8be0943c66b8581891eec9d6fad'
_loaded = None
_lock = threading.Lock()


@dataclass(frozen=True)
class OperatorContext:
    runtime: object = None
    evidence: object = None


def load_adapter():
    global _loaded
    with _lock:
        path = ROOT/'adapter.py'
        if path.is_symlink() or not path.is_file() or path.stat().st_size > 128*1024:
            raise ValueError('ordinary bounded adapter source required')
        data = path.read_bytes()
        if hashlib.sha256(data).hexdigest() != ADAPTER_SHA256:
            raise ValueError('category adapter source changed')
        if _loaded is not None:
            _loaded._capture()
            return _loaded
        module = types.ModuleType('_zenofcis_captured_category_adapter')
        module.__file__ = str(path)
        module.__verified_source_sha256__ = ADAPTER_SHA256
        sys.modules[module.__name__] = module  # Frozen dataclass definitions bind this exact module.
        try:
            exec(compile(data, str(path), 'exec'), module.__dict__)
        except BaseException:
            sys.modules.pop(module.__name__, None)
            raise
        _loaded = module
        return module


def operator_context(*, runtime_config=None, evidence_catalog=None, evidence_sha256=None):
    """Operator file options, never forwarded from JSON tool fields."""
    adapter = load_adapter()
    runtime = adapter.RuntimeApproval.from_operator_file(runtime_config) if runtime_config is not None else None
    if (evidence_catalog is None) != (evidence_sha256 is None):
        raise ValueError('catalog path and approved SHA256 must be supplied together')
    evidence = adapter.EvidenceApproval.from_pinned_file(evidence_catalog, evidence_sha256) if evidence_catalog is not None else None
    return OperatorContext(runtime, evidence)


def dispatch_tool(name, arguments, *, context=None):
    if context is not None and type(context) is not OperatorContext:
        return {'status': 'Invalid', 'authority': 'none', 'reason': 'operator context is not a JSON argument'}
    adapter = load_adapter()
    return adapter.dispatch(name, arguments, runtime=context.runtime if context else None,
                            evidence=context.evidence if context else None)


def prepare_runtime(directory):
    """Capture provider sources into a fresh ordinary directory; compile nothing."""
    adapter = load_adapter()
    _, blobs = adapter._capture()
    lexical = Path(directory).absolute()
    if lexical.is_symlink() or any(p.is_symlink() for p in lexical.parents):
        raise ValueError('runtime source destination must be fresh, separate and without symlink parents')
    destination = lexical.resolve()
    source = adapter.ZAL_ROOT.resolve()
    if (destination.exists() or destination.is_relative_to(source) or source.is_relative_to(destination)):
        raise ValueError('runtime source destination must be fresh, separate and without symlink parents')
    prefix = 'category_profile/provider/'
    selected = {name[len(prefix):]: data for name, data in blobs.items() if name.startswith(prefix)}
    if len(selected) != 10:
        raise ValueError('provider source capsule inventory changed')
    destination.mkdir()
    for name, data in selected.items():
        path = destination/name; path.parent.mkdir(parents=True, exist_ok=True)
        with path.open('xb') as handle:
            handle.write(data)
    return {'schema': 'zal/category-runtime-sources/1', 'status': 'Prepared', 'authority': 'none',
            'provider_manifest_sha256': adapter.PROVIDER_MANIFEST, 'files': len(selected),
            'worker_built': False, 'scope': 'Captured source files only; no compiler or runtime approval.'}
