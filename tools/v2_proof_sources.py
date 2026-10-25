"""Copy the actual registered execution dependencies into proof specimens.

The authority gate independently validates the approved manifest identity.
These helpers provide complete local files, including separately listed test
modules and the generated digest. They do not admit specifications or evidence.
"""
from __future__ import annotations

import json
from pathlib import Path
import re

SOURCE_MANIFEST = Path("verification/verus/authority_v2_sources.json")
TEST_MANIFEST = Path("verification/verus/authority_v2_test_sources.json")
GETTER = Path("crates/zeno-fcis-synthesis/src/finite/execution_v2/authority/evaluator.rs")
NATIVE_TEST_DATA = (Path("tools/test-data/scalar-arithmetic/i128-vectors.tsv"),
                    Path("tools/test-data/signed-arithmetic/vectors.tsv"),
                    Path("crates/zeno-fcis-synthesis/tests/test-data/u128-tables/sums.tsv"),
                    Path("crates/zeno-fcis-synthesis/tests/test-data/u128-tables/nicr.tsv"),
                    Path("crates/zeno-fcis-synthesis/source-snapshots/repository/verification/oracle_crypto_profile.json.txt"))


def execution_sources(root: Path) -> tuple[Path, ...]:
    """Return the complete approved local-source list needed by direct harnesses."""
    manifest = json.loads((root / SOURCE_MANIFEST).read_text())
    names = manifest["paths"]
    test_manifest = json.loads((root / TEST_MANIFEST).read_text())
    tests = test_manifest["paths"]
    if manifest.get("schema") != "zeno-fcis/authority-source-closure/1" or not names or names != sorted(set(names)):
        raise ValueError("invalid execution source manifest")
    required = ("crates/zeno-fcis-synthesis/src/finite/execution_v2/nonbinding_step.rs",
                "crates/zeno-fcis-synthesis/src/finite/ir/program.rs")
    if any(name not in names for name in required):
        raise ValueError("production execution closure omits a required refinement/ownership module")
    if (test_manifest.get("schema") != "zeno-fcis/authority-test-sources/1"
            or not isinstance(tests, list) or not tests
            or any(not isinstance(name, str) for name in tests)
            or tests != sorted(set(tests))):
        raise ValueError("invalid execution test source manifest")
    names = sorted(set(names + tests + list(map(str, NATIVE_TEST_DATA))
                       + [str(GETTER), str(SOURCE_MANIFEST), str(TEST_MANIFEST)]))
    paths = []
    for name in names:
        path = Path(name)
        if path.is_absolute() or ".." in path.parts or not re.fullmatch(r"[A-Za-z0-9_./-]+", name):
            raise ValueError("unsafe execution source path")
        source = root / path
        if not source.is_file() or any(parent.is_symlink() for parent in (source, *source.parents)):
            raise ValueError(f"missing or symlink execution source: {name}")
        paths.append(path)
    return tuple(paths)


def adjust_specimen_source_lengths(specimen: Path) -> list[dict]:
    """The fixed evaluator constant has no embedded lengths to repair."""
    return []
