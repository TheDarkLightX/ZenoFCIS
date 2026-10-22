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
import sys
import unicodedata

ROOT = Path(__file__).resolve().parents[1]
# Default-ignorable code points (Unicode DerivedCoreProperties), such as the
# zero-width characters, bidirectional controls, fillers and variation selectors.
IGNORABLE = ((0x00AD, 0x00AD), (0x034F, 0x034F), (0x061C, 0x061C), (0x115F, 0x1160), (0x17B4, 0x17B5),
             (0x180B, 0x180F), (0x200B, 0x200F), (0x202A, 0x202E), (0x2060, 0x206F), (0x3164, 0x3164),
             (0xFE00, 0xFE0F), (0xFEFF, 0xFEFF), (0xFFA0, 0xFFA0), (0xFFF0, 0xFFF8), (0x1BCA0, 0x1BCA3),
             (0x1D173, 0x1D17A), (0xE0000, 0xE0FFF))


def unique(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"UiDuplicateKey:{key}")
        result[key] = value
    return result


def invisible(character):
    """A character that can make text read differently from its code points."""
    code = ord(character)
    return (code < 0x20 or 0x7F <= code <= 0x9F
            or unicodedata.category(character) in {"Cf", "Cs", "Co", "Zl", "Zp"}
            or any(low <= code <= high for low, high in IGNORABLE))


def visible(value, path="ui.json"):
    """Refuse C0 and C1 controls, bidirectional controls, zero-width and other
    invisible characters, separators, surrogates and private use anywhere in
    the metadata: every label, unit, value label, message and key."""
    if isinstance(value, dict):
        for key, item in value.items():
            where = f"{path}.{json.dumps(key)[1:-1]}"
            visible(key, where)
            visible(item, where)
    elif isinstance(value, list):
        for index, item in enumerate(value):
            visible(item, f"{path}[{index}]")
    elif isinstance(value, str):
        for character in value:
            if invisible(character):
                raise ValueError(f"UiInvisibleCharacter:{path}:U+{ord(character):04X}")
    return value


def save(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def run(command, **kwargs):
    try:
        return subprocess.run(list(map(str, command)), check=True, **kwargs)
    except subprocess.CalledProcessError as error:
        details = error.stderr or str(error)
        if isinstance(details, bytes):
            details = details.decode("utf-8", errors="replace")
        if "U128SubjectUnsupported" in details:
            raise ValueError("UiOracleRefused:U128SubjectUnsupported") from None
        lines = [line.strip() for line in details.splitlines() if line.strip()]
        raise ValueError("UiOracleRefused:" + " ".join((lines[-1] if lines else "checker refused").split())) from None


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
    visible(json.loads(metadata.read_text(encoding="utf-8"), object_pairs_hook=unique))
    raw = json.loads(run([args.oracle.resolve(), project], capture_output=True, text=True).stdout)
    # The server sets a declared clock field; the interface compares replies without it.
    clock = json.loads((project / "v2/policy.json").read_text(), object_pairs_hook=unique).get("clock")
    if clock is not None:
        marked = [f for f in raw["descriptor"]["fields"] if f["path"] == f"context.102.{clock['field']}"]
        if len(marked) != 1 or marked[0]["source"] != "context" or marked[0]["domain"]["kind"] != "int":
            raise ValueError("UiClockField")
        marked[0]["clock"] = True
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
    from commons_hook import failure, report
    subject = Path(sys.argv[1]).name if len(sys.argv) > 1 else "unnamed"
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        report("zenoui", "refused", subject, failure(error))
        print(f"UI build refused: {error}", file=sys.stderr)
        sys.exit(1)
    except BaseException as error:
        report("zenoui", "refused", subject, failure(error))
        raise
    report("zenoui", "synthesized", subject, {})
