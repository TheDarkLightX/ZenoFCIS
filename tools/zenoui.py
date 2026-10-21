#!/usr/bin/env python3
"""Build a checked local UI from project.zeno, v2/policy.json and ui.json.

The build checks every declared finite input and all eight UI laws before
writing the application. Only shared components and contract data are emitted.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def unique(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"UiDuplicateKey:{key}")
        result[key] = value
    return result


def save(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def run(command, **kwargs):
    return subprocess.run(list(map(str, command)), check=True, **kwargs)


def hashes(directory):
    return {str(p.relative_to(directory)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in sorted(directory.rglob("*")) if p.is_file()}


def tuple_at(fields, ordinal):
    values = []
    for f in reversed(fields):
        d = f["domain"]
        size = int(d["max"]) - int(d["min"]) + 1 if d["kind"] == "int" else len(f["options"])
        ordinal, digit = divmod(ordinal, size)
        values.append(str(int(d["min"]) + digit) if d["kind"] == "int" else f["options"][digit]["value"])
    return list(reversed(values))


def samples(fields, action, classes):
    seen = set()
    selected = []
    for ordinal, classification in enumerate(classes):
        values = tuple_at(fields, ordinal)
        key = (values[action], classification)
        initial = all(values[i] == f["initial"] for i, f in enumerate(fields) if f["source"] == "state")
        initial_key = (values[action], "genesis-accept")
        if key not in seen or len(classes) <= 100 or (initial and classification == "A" and initial_key not in seen):
            selected.append(values)
            seen.add(key)
            if initial and classification == "A":
                seen.add(initial_key)
    return selected


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("contract", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, help="retain checks outside the emitted application")
    parser.add_argument("--oracle", type=Path, default=Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "release/ui_contract")
    parser.add_argument("--runtime", type=Path, default=ROOT / "site/public/factory.wasm")
    args = parser.parse_args()
    if args.evidence and args.evidence.resolve().is_relative_to(args.out.resolve()):
        raise ValueError("UiEvidenceInsideApplication")
    project = args.contract.resolve()
    metadata = project / "ui.json"
    if not metadata.is_file() or metadata.is_symlink():
        raise ValueError("UiMissingMetadata:ui.json")
    json.loads(metadata.read_text(), object_pairs_hook=unique)
    raw = json.loads(run([args.oracle.resolve(), project], capture_output=True, text=True).stdout)
    name = project.name
    with tempfile.TemporaryDirectory(prefix="zenoui-") as temporary:
        work = Path(temporary)
        case = work / name
        case.mkdir()
        save(case / "raw.json", raw)
        shutil.copyfile(metadata, case / "metadata.json")
        # Validation precedes exhaustive work and precedes writing the requested output.
        run(["node", ROOT / "site/zenoui/generate.mjs", case / "raw.json", metadata,
             case / "app", args.runtime.resolve()])
        finite = json.loads(run([args.oracle.resolve(), project, "finite"], capture_output=True, text=True).stdout)
        save(case / "finite.json", finite)
        app = json.loads((case / "app/app.json").read_text())["description"]
        selected = samples(app["fields"], app["action"], finite["classes"])
        native = json.loads(run([args.oracle.resolve(), project, "decisions"], input=json.dumps(selected), capture_output=True, text=True).stdout)
        save(case / "samples.json", [{"input":v,"reply":r} for v,r in zip(selected,native,strict=True)])
        run(["node", ROOT / "site/tests/zenoui-laws.test.mjs", work, name])
        run(["node", "--experimental-vm-modules", ROOT / "site/tests/zenoui-runtime.test.mjs", work, name])
        generated = hashes(case / "app")
        if args.out.exists() and set(hashes(args.out)) - set(generated):
            raise ValueError("UiOutputContainsUnknownFiles")
        shutil.copytree(case / "app", args.out, dirs_exist_ok=True)
        if args.evidence:
            args.evidence.mkdir(parents=True, exist_ok=True)
            for file in ["raw.json", "metadata.json", "finite.json", "samples.json"]:
                shutil.copyfile(case / file, args.evidence / file)
            shutil.copyfile(work / "laws.json", args.evidence / "laws.json")
            save(args.evidence / "assets.json", generated)
        print(f"{name}: checked application written; {finite['domain']} Authority tuples", flush=True)


if __name__ == "__main__":
    main()
