#!/usr/bin/env python3
"""Source-bound review tripwires, not proofs of arbitrary shell behavior."""
import ast
import hashlib
import json
from pathlib import Path

APP = Path(__file__).resolve().parents[1]
ROOT = APP.parents[1]
BOUND = "crates/zeno-fcis-synthesis/src/finite/execution_v2/authority/bound.rs"
PUBLICATION = "crates/zeno-fcis-synthesis/src/finite/execution_v2/authority/publication.rs"


def check_shell(source):
    tree = ast.parse(source)
    probe = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == "probe")
    # The transport may encode the whole command, never inspect its economics
    # or replace it. The independent reference/comparator is a separate checker.
    for node in ast.walk(probe):
        if isinstance(node, ast.Attribute) and isinstance(node.value, ast.Name) and node.value.id in ("world", "command"):
            raise ValueError("shell_decision: transport inspects a decision field")
        if isinstance(node, (ast.Assign, ast.AnnAssign, ast.AugAssign)):
            targets = node.targets if isinstance(node, ast.Assign) else [node.target]
            if any(isinstance(t, ast.Name) and t.id in ("world", "command", "expected") for t in targets):
                raise ValueError("shell_decision: transport replaces an authoritative input")


def check_identity(bound, publication):
    if 'framing::identity_bytes(catalog.original_contract(), &evaluator::EVALUATOR)' not in bound:
        raise ValueError("receipt_core_identity: checked policy/build binding missing")
    seals = [line.strip() for line in bound.splitlines() if "outcome::seal(" in line]
    if len(seals) != 2 or any("&self.identity" not in line for line in seals):
        raise ValueError("receipt_core_identity: invocation seal omits core identity")
    if "identity: &'a [u8]" not in publication:
        raise ValueError("receipt_core_identity: publication loses borrowed identity")


def audit():
    sources = {name: (ROOT / name).read_text() for name in (BOUND, PUBLICATION)}
    transport = APP / "tools/native_check.py"
    check_identity(sources[BOUND], sources[PUBLICATION])
    check_shell(transport.read_text())
    paths = [ROOT / name for name in sources] + [transport, APP / "tools/spot_reference.py",
            APP / "project.zeno", APP / "v2/policy.json", APP / "components/pool/project.zeno",
            APP / "components/pool/v2/policy.json", APP / "components/balance/project.zeno"]
    return {"scope": "source-bound Spot probe and native policy/publication paths", "authority": "none",
            "files": {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in paths},
            "findings": ["No Spot mount/publication adapter exists; probe output and Python Decision are observations, not receipts.",
                         "Context closed_shape is an external admission assertion; raw-byte/authenticated identity admission remains unqualified."]}


if __name__ == "__main__":
    print(json.dumps(audit(), indent=2))
