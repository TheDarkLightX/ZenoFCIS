#!/usr/bin/env python3
"""Register of every tracked source binding and the producer that rewrites it.

A source binding is a tracked JSON file that pins repository files by SHA-256:
a `files` object of `relative path: sha256`, entries carrying both a path and a
`sha256`, or a `*_sha256` field beside a path field. Pins are resolved against
the binding's own directory, then the repository root; a file counts as a
binding when at least one pin names a tracked file, resolved against the
binding's directory or any of its ancestors. Crate test directories, fixture
directories (`tests/fixtures`, `test_data`, `tests/data`) and the published
examples are not bindings: they are inputs that tests read, not pins that
gates refresh.

`check` fails when a binding exists that the register does not list, when the
register lists a path that is no longer a binding, or when a named producer
does not exist. `list` prints the register with each binding's current
detection. Neither reads any pin's target or runs a producer; the register
says who rewrites a binding, not whether it is current.
"""
from __future__ import annotations

import argparse
import fnmatch
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
HEX = re.compile(r"[0-9a-f]{64}")
NOT_BINDINGS = re.compile(r"(^|/)(tests/fixtures|test_data|tests/data|fixtures)/|^crates/[^/]+/tests/|^(examples|site)/")
PATH_KEYS = ("path", "file", "source", "relative_path")

# path pattern -> (producer argv or None, reason). Patterns use fnmatch over the repository-relative path.
REGISTER: dict[str, tuple[str | None, str]] = {
    "verification/verus/arithmetic-typed-proof.json": (
        "python3 tools/record_arithmetic_proof_sources.py --write",
        "typed arithmetic proof source_sha256 pins; --check lists stale pins and exits 1"),
    "verification/verus/arithmetic-emission.json": (
        "python3 tools/record_arithmetic_proof_sources.py --write",
        "emission source_sha256 pins; the same producer and closure as the typed profile"),
    "integrations/zal/category_profile/provider/SOURCE_MANIFEST.json": (
        "python3 tools/record_category_capsule.py --write",
        "ZAL category capsule chain: provider sources"),
    "integrations/zal/category_profile/DEPENDENCIES.json": (
        "python3 tools/record_category_capsule.py --write",
        "ZAL category capsule chain: adapter, host and the provider manifest"),
    "integrations/zal/finite_library/UPSTREAM.json": (
        "python3 tools/record_finite_library_components.py --write",
        "ZAL finite-library component list, pinned in turn by the category capsule"),
    "core-apps/*/components/*/core-instance.json": (
        "python3 tools/record_core_app_references.py --write",
        "coupled-application component references to a family certificate"),
    "core-apps/zenodex-spot/composition.json": (
        None, "zenodex-spot is not a coupled application in tools/core_app_specs, so the reference producer does not cover it; its composition pins change by a reviewed edit beside its SOURCE_PINS manifest"),
    "core-apps/*/composition.json": (
        "python3 tools/record_core_app_references.py --write",
        "coupled-application composition pinning each core-instance.json"),
    "test-data/v1-compatibility/baseline.json": (
        "python3 tools/record_v1_baseline.py --write",
        "1.x compatibility baseline of path, bytes and sha256"),
    "core-components/proofs/runtime-source.json": (
        "python3 tools/prove_core_families.py",
        "runtime source set bound by every core family certificate; rewritten only by a certificate refresh on the qualification host, which the lead runs at merge"),
    "core-components/proofs/*.json": (
        "python3 tools/prove_core_families.py",
        "core family certificates; rewritten only by the certificate refresh on the qualification host"),
    "release/historical-24-copy.json": (
        "python3 tools/check_historical_24_copy.py --record",
        "record of the retained 2.4 copy used by the upgrade checks"),
    "release/packaged-test-inputs.json": (
        "python3 tools/rc_package.py propose-test-inputs",
        "reviewed list of test inputs packaged with a release; release custody rechecks its digest"),
    "release/knowledge-base-includes.json": (
        None, "one reviewed public knowledge example and its digest; tools/check_knowledge_files.py refuses any other content, so the file is edited with that check, not regenerated"),
    "verification/verus/u128_profile_extensions.json": (
        "python3 tools/u128_profile_inventory.py",
        "inventory of the u128 profile extension sources"),
    "experiments/256_foundations_27/qualification.json": (
        "python3 tools/check_arithmetic_handoff.py --out experiments/256_foundations_27/qualification.json",
        "qualification record of the 256-bit foundations experiment, written by the hand-off check"),
    "experiments/ontology_arithmetic_handoff/qualification.json": (
        "python3 tools/check_arithmetic_handoff.py --out experiments/ontology_arithmetic_handoff/qualification.json",
        "qualification record of the ontology arithmetic hand-off, written by the hand-off check"),
    "integrations/zal/arithmetic_handoff_sources.json": (
        None, "hand-off source list read by tools/check_arithmetic_handoff.py; a source change is a reviewed edit of this list, and the check refuses a stale digest"),
    "core-apps/zenodex-spot/SOURCE_PINS.json": (
        "python3 core-apps/zenodex-spot/tools/write_manifest.py",
        "frozen input pins of the zenodex-spot application, written by its own manifest tool"),
    "experiments/ontology_arithmetic_handoff/selection.obligation.json": (
        None, "research record: an obligation statement with the digests of the sources it was stated over; a change is a new record"),
    "experiments/ontology_arithmetic_handoff/type-mismatch.json": (
        None, "research record of a found type mismatch over pinned sources; a change is a new record"),
    "experiments/zal_registry_batch_27/feasibility.json": (
        None, "research record of a feasibility run over pinned sources; a change is a new record"),
    "experiments/zal_registry_batch_27/pins.template.json": (
        None, "template of the registry batch pins, filled by the batch run; not a gate input"),
    "integrations/zal/examples/patterns/*.json": (
        None, "ZAL pattern examples that name the library source they were written against; the ZAL checks compare the digest and report drift, and a change is a reviewed edit of the example"),
    "integrations/zal/zal_notation/ONTOLOGY_QUALIFICATION.json": (
        None, "qualification record of the ZAL notation ontology over pinned sources; a change is a new record"),
    "security/reviews/v1/*.json": (
        None, "reviewed source records of the 1.x security review; frozen history, never regenerated"),
    "test-data/bounded-completion/mutation-results.json": (
        None, "recorded mutation results over pinned inputs for the bounded-completion controls; a rerun writes a new record"),
    "test-data/zal-observation-contracts/obligation.json": (
        None, "recorded observation obligations over pinned sources; the scenario compares them, a rerun writes a new record"),
    "crates/zeno-fcis-cli/templates/*/v2/schema-origin.json": (
        "python3 tools/refresh_template_rules.py --cli CLI CONTRACT",
        "template schema origin: the digests of the proposal the template's rules were generated from"),
    "verification/kernel-laws/src/oracle/templates/*/v2/schema-origin.json": (
        "python3 tools/refresh_template_rules.py --cli CLI CONTRACT --oracle",
        "retained copy of a template's schema origin for the kernel-laws oracle"),
    "verification/kernel-laws/src/oracle/templates/*/original/v2/schema-origin.json": (
        "python3 tools/refresh_template_rules.py --cli CLI CONTRACT --oracle",
        "retained copy of the original template's schema origin for the kernel-laws oracle"),
    "crates/zeno-fcis-cli/templates/*/synthesized/manifest.json": (
        "zeno-fcis synth run",
        "finite-synthesis manifest of the template's synthesized step, written by the CLI into the template; tools/check_synthesis.py exercises it"),
    "verification/kernel-laws/src/oracle/templates/*/synthesized/manifest.json": (
        "python3 tools/refresh_template_rules.py --cli CLI CONTRACT --oracle",
        "retained copy of a template's synthesized manifest for the kernel-laws oracle"),
    "verification/kernel-laws/src/oracle/templates/*/original/synthesized/manifest.json": (
        "python3 tools/refresh_template_rules.py --cli CLI CONTRACT --oracle",
        "retained copy of the original template's synthesized manifest for the kernel-laws oracle"),
    "verification/verus/numeric256_profile.json": (
        "python3 tools/record_numeric256_profile.py --out DIR",
        "256-bit numeric profile source bindings"),
    "verification/verus/ordering256_profile.json": (
        "python3 tools/record_numeric256_profile.py --out DIR --refresh-ordering-bindings",
        "ordering256 profile source bindings, refreshed with the numeric profile"),
    "verification/verus/ordering_types256_profile.json": (
        "python3 tools/record_numeric256_profile.py --out DIR --refresh-ordering-bindings",
        "ordering-types256 profile source bindings, refreshed with the numeric profile"),
    "verification/verus/qualification_sources.json": (
        "python3 tools/qualification_sources.py PREVIOUS CURRENT",
        "Verus qualification source policy: the production/test partition of every qualified source"),
    "experiments/256_foundations_27/ordering256-qualification.json": (
        None, "retained copy of a tools/check_ordering256.py --out run; a rerun writes a new record"),
    "experiments/256_foundations_27/ordering-types256-qualification.json": (
        None, "retained copy of a tools/check_ordering_types256.py --out run; a rerun writes a new record"),
    "experiments/proof-challenge/results/*/receipt.json": (
        None, "dated proof-challenge result record; a new run is a new dated directory"),
    "experiments/proof-challenge/results/*/public-logs.json": (
        None, "dated proof-challenge log digest record; a new run is a new dated directory"),
    "experiments/zal_registry_batch_27/qualification.json": (
        None, "record of the registry batch qualification run described in its README; a rerun writes a new record"),
    "integrations/zal/relational_profile/vendor/VALIDATION.json": (
        None, "historical vendor validation record kept unchanged (relational_profile/README.md)"),
    "integrations/zal/zal_notation/LENS_UPSTREAM.json": (
        None, "upstream pins of the integrated lens package; a change is a reviewed re-integration"),
    "integrations/zal/zal_notation/ONTOLOGY_UPSTREAM.json": (
        None, "upstream pins of the integrated ontology package; a change is a reviewed re-integration"),
    "integrations/zal/zal_notation/READING_QUALIFICATION.json": (
        None, "qualification record of the notation reading over pinned sources; a change is a new record"),
}


class Refusal(ValueError):
    """A register or detection failure."""


def tracked(root: Path) -> set[str]:
    out = subprocess.run(["git", "-C", str(root), "ls-files", "-z"], capture_output=True, check=True)
    return {name for name in out.stdout.decode().split("\0") if name}


def _resolves(root: Path, binding: str, pin: str, files: set[str]) -> bool:
    if not isinstance(pin, str) or not pin or pin.startswith("/"):
        return False
    bases = [Path(binding).parent, *Path(binding).parent.parents]
    for base in bases:
        candidate = (root / base / pin).resolve()
        try:
            relative = candidate.relative_to(root.resolve()).as_posix()
        except ValueError:
            continue
        if relative in files:
            return True
    return False


def pins(value, out: list[str]) -> None:
    """Collect every (path) a value pins with a 64-hex SHA-256."""
    if isinstance(value, dict):
        keys = set(value)
        if any(key.endswith("sha256") and isinstance(value[key], str) and HEX.fullmatch(value[key]) for key in keys):
            for key in PATH_KEYS:
                if isinstance(value.get(key), str):
                    out.append(value[key])
        for key, child in value.items():
            if isinstance(child, dict) and child and all(
                    isinstance(digest, str) and HEX.fullmatch(digest) for digest in child.values()):
                out.extend(child)
            pins(child, out)
    elif isinstance(value, list):
        for child in value:
            pins(child, out)


def detect(root: Path = ROOT) -> dict[str, int]:
    """Every tracked binding with the number of tracked files it pins."""
    files = tracked(root)
    found = {}
    for name in sorted(files):
        if not name.endswith(".json") or NOT_BINDINGS.search(name):
            continue
        try:
            document = json.loads((root / name).read_bytes())
        except (OSError, ValueError):
            continue
        candidates: list[str] = []
        pins(document, candidates)
        count = sum(1 for pin in set(candidates) if _resolves(root, name, pin, files))
        if count:
            found[name] = count
    return found


def entry(name: str, register=REGISTER):
    for pattern, row in register.items():
        if fnmatch.fnmatchcase(name, pattern):
            return pattern, row
    return None


def check(root: Path = ROOT, register=REGISTER) -> list[str]:
    problems = []
    found = detect(root)
    for name in found:
        if entry(name, register) is None:
            problems.append(f"unregistered binding: {name} pins {found[name]} tracked file(s); add it to tools/binding_register.py with its producer or the reason it has none")
    for pattern, (producer, reason) in register.items():
        if not any(fnmatch.fnmatchcase(name, pattern) for name in found):
            problems.append(f"stale register entry: {pattern} matches no tracked binding")
        if not reason.strip():
            problems.append(f"register entry without a reason: {pattern}")
        if producer is not None and producer.startswith("python3 "):
            script = producer.split()[1]
            if not (root / script).is_file():
                problems.append(f"producer missing: {pattern} names {script}")
    return problems


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("command", choices=("check", "list"))
    parser.add_argument("--root", type=Path, default=ROOT)
    args = parser.parse_args(argv)
    if args.command == "list":
        found = detect(args.root)
        for name in sorted(found):
            matched = entry(name)
            producer, reason = matched[1] if matched else (None, "UNREGISTERED")
            print(json.dumps({"binding": name, "pins": found[name], "producer": producer, "reason": reason}))
        return 0
    problems = check(args.root)
    for problem in problems:
        print("binding-register: FAIL: " + problem)
    if problems:
        return 1
    print(f"binding-register: PASS ({len(detect(args.root))} bindings, {len(REGISTER)} register entries)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
