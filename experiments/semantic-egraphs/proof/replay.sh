#!/usr/bin/env bash
set -euo pipefail

if [[ $# != 1 ]]; then
  printf '%s\n' 'Usage: bash proof/replay.sh /path/to/matching/mathlib-project' >&2
  exit 2
fi

packet_dir="$(cd "$(dirname "$0")" && pwd)"
dependency_project="$(cd "$1" && pwd)"
source_file="$packet_dir/BooleanSignatures.lean"

lean_version="$(cd "$dependency_project" && lake env lean --version)"
case "$lean_version" in
  *'version 4.30.0-rc2'*'commit 3dc1a088b6d2d8eafe25a7cd7ec7b58d731bd7cc'*) ;;
  *) printf '%s\n' "Unexpected Lean toolchain: $lean_version" >&2; exit 1 ;;
esac

python3 - "$dependency_project/lake-manifest.json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    manifest = json.load(handle)
mathlib = next(package for package in manifest["packages"] if package["name"] == "mathlib")
expected = "9977002c3c9492b622fb469b0d18acc7e73aed3e"
if mathlib.get("rev") != expected:
    raise SystemExit("Mathlib manifest revision does not match the proof environment")
PY

mathlib_dir="$dependency_project/.lake/packages/mathlib"
mathlib_head="$(git -C "$mathlib_dir" rev-parse HEAD)"
if [[ "$mathlib_head" != 9977002c3c9492b622fb469b0d18acc7e73aed3e ]]; then
  printf '%s\n' 'Mathlib checkout revision does not match the proof environment' >&2
  exit 1
fi
if [[ -n "$(git -C "$mathlib_dir" status --porcelain)" ]]; then
  printf '%s\n' 'Mathlib checkout has local changes; use a clean dependency checkout' >&2
  exit 1
fi

if rg -n '\b(sorry|admit|axiom|unsafe|native_decide)\b' "$source_file"; then
  printf '%s\n' 'Rejected proof trust escape' >&2
  exit 1
fi

printf '%s\n' "$lean_version" "Mathlib: $mathlib_head"
(cd "$dependency_project" && lake env lean "$source_file") | tee "$packet_dir/replay.log"
python3 - "$packet_dir/replay.log" <<'PY'
import re
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    replay = handle.read()
lines = [line for line in replay.splitlines() if "depends on axioms:" in line]
if len(lines) != 7:
    raise SystemExit("Expected seven theorem axiom reports")
allowed = {"propext", "Classical.choice", "Quot.sound"}
for line in lines:
    found = re.search(r"depends on axioms: \[(.*)\]$", line)
    if found is None:
        raise SystemExit("Unexpected axiom report format")
    names = {name.strip() for name in found.group(1).split(",") if name.strip()}
    if not names <= allowed:
        raise SystemExit("Nonstandard theorem axiom detected")
print("Seven theorem closures checked; only standard Lean foundations are used.")
PY
