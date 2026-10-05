"""Legacy release-engineering mirror fixtures, outside S1 evaluator assurance.

The evaluator digest no longer uses these mirrors. Retained archive-fixture
helpers do not establish current package or compiled-source qualification.
"""
from __future__ import annotations

import hashlib
from pathlib import Path

PACKAGE = Path('crates/zeno-fcis-synthesis')
BUNDLE = PACKAGE / 'src/finite/execution_v2/authority/source_bundle'


def mirrored(name: str) -> bool:
    path = Path(name)
    return not path.is_relative_to(PACKAGE) or path == PACKAGE / 'Cargo.toml'


def transport_path(name: str) -> Path:
    path = Path(name)
    if not path.parts or path.is_absolute() or '..' in path.parts:
        raise ValueError('unsafe source transport name')
    # Cargo omits hidden untracked directories from its default package list.
    # Logical source keys remain exact; only transport directory names change.
    portable = Path(*(('dot-' + p[1:]) if p.startswith('.') else p for p in path.parts))
    return BUNDLE / (portable.as_posix() + '.source') if mirrored(name) else path


def read_regular(path: Path) -> bytes:
    if any(p.is_symlink() for p in (path, *path.parents)) or not path.is_file():
        raise ValueError(f'missing, nonregular or symlink source: {path}')
    return path.read_bytes()


def mirror_names(paths: list[str]) -> list[str]:
    if paths != sorted(set(paths)):
        raise ValueError('source transport requires sorted unique names')
    names = [name for name in paths if mirrored(name)]
    if len({transport_path(name) for name in names}) != len(names):
        raise ValueError('source transport filename collision')
    return names


def check(root: Path, paths: list[str]) -> dict:
    names = mirror_names(paths)
    expected = {transport_path(name) for name in names}
    directory = root / BUNDLE
    if directory.is_symlink():
        raise ValueError('symlink source bundle')
    actual = set()
    if directory.exists():
        for p in directory.rglob('*'):
            if p.is_symlink():
                raise ValueError('symlink source bundle entry')
            if p.is_file():
                actual.add(p.relative_to(root))
    if actual != expected:
        raise ValueError('source bundle has missing or extra entries')
    rows = []
    for name in names:
        original = read_regular(root / name)
        if read_regular(root / transport_path(name)) != original:
            raise ValueError(f'stale or substituted source mirror: {name}')
        rows.append({'path': name, 'bytes': len(original),
                     'sha256': hashlib.sha256(original).hexdigest()})
    return {'mirrored_rows': rows, 'logical_source_count': len(paths)}


def generate(root: Path, paths: list[str]) -> dict:
    # Read every mirrored original before writing transport. Unread data fails.
    rows = [(name, read_regular(root / name)) for name in mirror_names(paths)]
    for name, data in rows:
        target = root / transport_path(name)
        if any(p.is_symlink() for p in (target, *target.parents)):
            raise ValueError('symlink source transport target')
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
    return check(root, paths)


def check_archives(root: Path, paths: list[str], packages: dict[str, Path]) -> dict:
    """Compare actual extracted archives with the frozen source, not another mirror.

    Effective normalized manifests, dependency resolution and rustc/extern/cfg
    provenance must additionally be checked by the release consumer gate.
    """
    transport = check(root, paths)
    synthesis = packages['zeno-fcis-synthesis']
    records = []
    bindings = []
    for name in paths:
        logical = Path(name)
        original = read_regular(root / logical)
        if mirrored(name):
            archived_mirror = synthesis / transport_path(name).relative_to(PACKAGE)
            if read_regular(archived_mirror) != original:
                raise ValueError(f'archive source mirror differs: {name}')
        if logical.parts[0] != 'crates':
            bindings.append(name)
            continue
        crate = logical.parts[1]
        relative = Path(*logical.parts[2:])
        if relative == Path('Cargo.toml'):
            relative = Path('Cargo.toml.orig')
        actual = packages[crate] / relative
        if read_regular(actual) != original:
            raise ValueError(f'compiled archive source differs: {name}')
        records.append({'path': name, 'sha256': hashlib.sha256(original).hexdigest()})
    return {'archive_source_rows': records, 'workspace_build_bindings': bindings,
            'mirrored_row_count': len(transport['mirrored_rows']),
            'effective_manifest_resolver_and_compiler_checks_required': True}
