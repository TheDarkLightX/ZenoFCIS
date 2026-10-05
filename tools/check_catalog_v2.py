#!/usr/bin/env python3
"""Offline exact-source catalog proof and translated coverage controls.

Run the entire command under the fleet heavy-check lock. This gate grants no
runtime or release authority. --development retains failures without accepting
an unfrozen coverage manifest; only an ordinary run can produce status passed.
"""
from __future__ import annotations
import argparse
import json
import re
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
from v2_proof_sources import execution_sources, adjust_specimen_source_lengths
from v2_native_dependencies import native_dependency_args
import check_verus as verifier
from verus_coverage import inventory, require_coverage

ROOT = Path(__file__).resolve().parents[1]
HARNESS = Path("verification/verus/catalog_v2.rs")
PROFILE = Path("verification/verus/catalog_v2.json")
BASE = Path("crates/zeno-fcis-synthesis/src/finite")
SUBJECT = BASE / "execution_v2/catalog.rs"
MATCHING = BASE / "execution_v2/catalog/matching.rs"
SPEC = BASE / "execution_v2/catalog/spec.rs"


def sources(root: Path) -> list[Path]:
    paths = [HARNESS, verifier.PIN, Path("tools/verus_coverage.py"), Path("tools/check_verus.py"),
             Path("tools/check_catalog_v2.py"), Path("tools/test_check_catalog_v2.py"),
             Path("docs/V2_CATALOG_BINDING_STAGE.md"), BASE / "mod.rs",
             Path("crates/zeno-fcis-synthesis/tests/v2_catalog.rs"),
             # Native decision tests compile this existing #[path] fixture even
             # when the executed test filter selects only the catalog tests.
             Path("crates/zeno-fcis-cli/templates/order-fulfillment/synthesized/transition.rs"),
             Path("crates/zeno-fcis-synthesis/Cargo.toml"),
             Path("Cargo.toml"), Path("Cargo.lock"), Path(".cargo/config.toml")]
    for directory in ("canonical_v2", "evaluation", "execution_v2"):
        paths += [p.relative_to(root) for p in (root / BASE / directory).rglob("*.rs")]
    if (root / PROFILE).exists():
        paths.append(PROFILE)
    paths += list(execution_sources(root))
    paths += [Path("tools/v2_proof_sources.py"), Path("verification/verus/authority_v2_sources.json")]
    paths.append(Path("tools/v2_native_dependencies.py"))
    return sorted(set(paths))


def snapshot(root: Path) -> dict[str, str]:
    result = {}
    for p in sources(root):
        if (root / p).is_symlink():
            raise ValueError(f"symbolic proof source: {p}")
        result[str(p)] = verifier.digest(root / p)
    return result


def replace_once(text: str, old: str, new: str) -> str:
    """Replace the single token-equal occurrence; whitespace may differ."""
    tokens = re.findall(r"\w+|[^\w\s]", old)
    matches = list(re.finditer(r"\s*".join(re.escape(t) for t in tokens), text))
    if len(matches) != 1:
        raise ValueError(f"mutation anchor count differs from one: {old!r}")
    found = matches[0]
    return text[:found.start()] + new + text[found.end():]


def mutations() -> list[tuple[str, Path, str, str, str]]:
    return [
        ("omit_policy_equality", SUBJECT, "if !super::util::bytes_equal(&actual, original_contract)", "if false", "proof"),
        ("omit_original_state_root", MATCHING, "description.root == f.state.root\n        && matches!", "matches!", "proof"),
        ("narrow_original_i128_bound", MATCHING, "a == *c as i128 && b == *d as i128", "a <= *c as i128 && b >= *d as i128", "proof"),
        ("omit_unused_outbox", MATCHING, " || !deliveries(defs, bs[i].outbox, ls)", "", "proof"),
        ("discard_text_upper_bound", MATCHING, "\n                    && b.len() as u64 <= max as u64\n                    && super::super::util::ascii(b)", "\n                    && super::super::util::ascii(b)", "proof"),
        ("omit_channel_links", SUBJECT, "if !matching::channels(description.definitions, descriptor.channels, channel_roots)", "if false", "proof"),
        ("weaken_getter_contract", SUBJECT, "ensures result@ == self.view().1,", "ensures true,", "coverage"),
        ("narrow_getter_domain", SUBJECT, "ensures result@ == self.view().1,", "requires self.view().1.len()>0, ensures result@ == self.view().1,", "coverage"),
        ("add_uncontracted_work", MATCHING, "//! Total correspondence checks", "pub fn uncovered_catalog_work()->u64{1}\n// Total correspondence checks", "coverage"),
        ("cached_policy_before_admission", SUBJECT, "    let checked = match schema::admit(original_schema, description, limits.schema) {", "    let _cached = authority::policy_bytes(descriptor, original_schema, framing, channel_roots);\n    let checked = match schema::admit(original_schema, description, limits.schema) {", "coverage"),
    ]


def run(root: Path, directory: Path, command: list[str], environment: dict) -> dict:
    directory.mkdir(parents=True, exist_ok=False)
    cmd = [*command, "--log-dir", str(directory), str(HARNESS)]
    started = time.monotonic()
    process = subprocess.run(cmd, cwd=root, env=environment, capture_output=True, text=True, timeout=1200)
    (directory / "stdout.json").write_text(process.stdout)
    (directory / "stderr.log").write_text(process.stderr)
    try:
        report = json.loads(process.stdout)
    except json.JSONDecodeError:
        report = {}
    return {"command": cmd, "exit_code": process.returncode,
            "elapsed_seconds": time.monotonic()-started, "report": report,
            "stdout": str(directory/"stdout.json"), "stderr": str(directory/"stderr.log")}


def native_tests(root: Path, directory: Path, environment: dict) -> dict:
    directory.mkdir(parents=True, exist_ok=False)
    executable = directory / "catalog-tests"
    commands = [
        ["rustc", "+1.97.1", "--edition=2024", "--check-cfg", "cfg(verus_keep_ghost)",
         "--check-cfg", "cfg(test)", "--crate-name", "catalog_v2", "--test", str(HARNESS),
         "-o", str(executable)],
        [str(executable), "execution_v2::catalog::tests::", "--test-threads=1"],
    ]
    commands[0] += native_dependency_args(ROOT, directory, environment)
    result = {"compile_exit": None, "test_exit": None, "commands": commands,
              "stdout": str(directory / "test.stdout"), "stderr": str(directory / "test.stderr")}
    for label, command in zip(("compile", "test"), commands):
        process = subprocess.run(command, cwd=root, env=environment, capture_output=True, text=True, timeout=120)
        (directory / (label + ".stdout")).write_text(process.stdout)
        (directory / (label + ".stderr")).write_text(process.stderr)
        result[label + "_exit"] = process.returncode
        if label == "compile" and process.returncode != 0:
            break
    return result


def qualified(run: dict, pin: dict) -> bool:
    report = run["report"]
    count = report.get("verification-results", {}).get("verified")
    return (run["exit_code"] == 0 and isinstance(count, int) and count > 0
            and verifier.accepted(report, {**pin, "expected_verified": count,
                "target_functions": ["catalog_v2::execution_v2::catalog::bind_original"]}))


def proof_control(result: dict, name: str, path: Path, diagnostics: str) -> bool:
    """Require the semantic proof obligation challenged by this control."""
    verdict = result["report"].get("verification-results", {})
    obligation = "postcondition not satisfied"
    if name == "omit_unused_outbox":
        obligation = "invariant not satisfied at end of loop body"
        if "spec::branch(defs@,bs@[j],ls@)" not in diagnostics:
            return False
    return (result["exit_code"] != 0 and verdict.get("success") is False
            and type(verdict.get("errors")) is int and verdict["errors"] > 0
            and verdict.get("encountered-vir-error") is False
            and verdict.get("is-verifying-entire-crate") is True
            and "resource limit" not in diagnostics.lower()
            and obligation in diagnostics and str(path) in diagnostics)


def coverage_control(vir: str, profile: dict, name: str) -> tuple[bool, str | None, dict]:
    """An unrelated inventory change cannot satisfy the intended control."""
    refusal = None
    try:
        require_coverage(vir, profile)
    except ValueError as error:
        refusal = str(error)
    actual = inventory(vir, profile["namespace"], tuple(profile["body_covered_functions"]))
    expected = profile["functions"]
    prefix = "catalog_v2::execution_v2::catalog::"
    if name == "add_uncontracted_work":
        target, field = prefix + "matching::uncovered_catalog_work", "inventory"
        intended = target in actual and target not in expected and refusal is not None and target in refusal
    else:
        if name in ("weaken_getter_contract", "narrow_getter_domain"):
            getters = [n for n, f in expected.items()
                       if n.startswith(prefix) and n.endswith("::original_contract") and f["mode"] == "Exec"]
            if len(getters) != 1:
                raise ValueError("reviewed catalog getter is missing or ambiguous")
            target = getters[0]
            field = "ensures_sha256" if name == "weaken_getter_contract" else "requires_sha256"
        elif name == "cached_policy_before_admission":
            target, field = prefix + "bind_original", "body_sha256"
        else:
            raise ValueError("unknown coverage control: " + name)
        intended = (target in actual and target in expected and refusal is not None and target in refusal
                    and actual[target][field] != expected[target][field])
        if name == "cached_policy_before_admission" and intended:
            intended = all(actual[target][f] == expected[target][f]
                           for f in ("signature_sha256", "requires_sha256", "ensures_sha256"))
    return intended, refusal, {"function": target, "field": field, "intended_refusal": intended}


def check(args) -> dict:
    before = snapshot(ROOT)
    pin = json.loads((ROOT/verifier.PIN).read_text())
    tools = verifier.prepare_tools(args.cache, pin, False)  # Never download.
    environment = {k:v for k,v in os.environ.items() if not k.startswith(("VERUS_","VARGO_"))
                   and k not in ("RUSTFLAGS","CARGO_ENCODED_RUSTFLAGS")}
    environment.update(RUSTUP_TOOLCHAIN=pin["rust_toolchain"], VERUS_Z3_PATH=str(tools/"z3"))
    command = [str(tools/"verus"), "--crate-type=lib", "--edition=2024", "--no-cheating",
               "--no-external-by-default", "--num-threads", "2", "-V", "spinoff-all", "--rlimit", "20",
               "--output-json", "--triggers-mode", "silent", "--log", "vir",
               "--log", "vir-option=no_span+no_type+no_fn_details"]
    directory = args.out.parent / (args.out.stem+"-artifacts")
    directory.mkdir(parents=True, exist_ok=False)
    positive = run(ROOT, directory/"positive", command, environment)
    receipt = {"schema":"zeno-fcis/catalog-evidence/1", "status":"unfinished",
               "source_sha256":before, "toolchain":pin, "positive":positive, "mutations":[],
               "nonclaims":["combined V2 qualification", "frontend intent", "recomputed schema SHA", "release authority"]}
    if not qualified(positive,pin):
        receipt["failure"] = "Whole-source positive proof did not qualify; inspect retained diagnostics."
        return receipt
    vir = (directory/"positive/crate.vir").read_text()
    functions = inventory(vir,"catalog_v2::")
    bodies = sorted(n for n,v in functions.items() if v["mode"]=="Exec")
    observed = inventory(vir,"catalog_v2::",tuple(bodies))
    proposal = {"namespace":"catalog_v2::", "expected_verified":positive["report"]["verification-results"]["verified"],
                "target_functions":["catalog_v2::execution_v2::catalog::bind_original"],
                "body_covered_functions":bodies, "functions":observed}
    (directory/"observed-coverage.json").write_text(json.dumps(proposal,indent=2,sort_keys=True)+"\n")
    receipt["observed_coverage"] = str(directory/"observed-coverage.json")
    verifier.verify_tool_files(tools,pin)
    if snapshot(ROOT)!=before:
        receipt["failure"]="Source drift during positive proof"
        return receipt
    if args.development:
        receipt["status"]="development_positive_unfrozen_coverage"
        return receipt
    profile = json.loads((ROOT/PROFILE).read_text())
    require_coverage(vir,profile)
    if profile["expected_verified"] != positive["report"]["verification-results"]["verified"]:
        raise ValueError("verification count differs from reviewed profile")
    receipt["positive_native"] = native_tests(ROOT, directory/"positive-native", environment)
    native_positive = receipt["positive_native"]
    if (native_positive["compile_exit"] != 0 or native_positive["test_exit"] != 0
            or "test result: ok. 8 passed; 0 failed; 0 ignored" not in Path(native_positive["stdout"]).read_text()):
        receipt["failure"] = "Positive native catalog fixtures did not qualify"
        return receipt
    for name,path,old,new,expected in mutations():
        specimen = directory/name
        for unit in before:
            target = specimen/unit;target.parent.mkdir(parents=True,exist_ok=True)
            shutil.copyfile(ROOT/unit,target)
        target=specimen/path;target.write_text(replace_once(target.read_text(),old,new))
        adjust_specimen_source_lengths(specimen)
        native = native_tests(specimen, specimen/"native", environment) if expected=="proof" else None
        if native is not None and (native["compile_exit"] != 0 or native["test_exit"] != 101
                                   or "test result: FAILED." not in Path(native["stdout"]).read_text()):
            receipt["mutations"].append({"name":name,"expected":expected,"killed":False,"native":native})
            receipt["failure"] = "Runtime mutation survived or failed to compile: " + name
            return receipt
        result=run(specimen,specimen/"proof",command,environment)
        refused=None
        intended=None
        if expected=="coverage":
            caught,refused,intended=coverage_control((specimen/"proof/crate.vir").read_text(),profile,name)
        diagnostics=Path(result["stderr"]).read_text()
        killed=proof_control(result,name,path,diagnostics) if expected=="proof" else (qualified(result,pin) and caught)
        receipt["mutations"].append({"name":name,"expected":expected,"killed":killed,"coverage_refusal":refused,
                                     "intended_coverage_control":intended,"native":native,**result})
        if not killed:
            receipt["failure"]="Control survived or failed for an unrelated reason: "+name
            return receipt
        print(f"catalog: {name}: {expected} control caught",flush=True)
    verifier.verify_tool_files(tools,pin)
    if snapshot(ROOT)!=before:
        receipt["failure"]="Source drift during proof/control suite"
        return receipt
    receipt["status"]="passed"
    return receipt


def main() -> int:
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out",type=Path,required=True)
    parser.add_argument("--cache",type=Path,default=Path.home()/".cache/zeno-fcis/verus")
    parser.add_argument("--development",action="store_true")
    args=parser.parse_args()
    if args.out.exists() or args.out.is_symlink():
        parser.error("receipt path must be new; retained evidence is never overwritten")
    try:
        receipt=check(args)
    except (OSError,ValueError,RuntimeError,KeyError,subprocess.SubprocessError) as error:
        receipt={"schema":"zeno-fcis/catalog-evidence/1","status":"unfinished","failure":str(error)}
    args.out.write_text(json.dumps(receipt,indent=2,sort_keys=True)+"\n")
    print(json.dumps({"status":receipt["status"],"receipt":str(args.out),"failure":receipt.get("failure")}))
    return 0 if receipt["status"] in ("passed","development_positive_unfrozen_coverage") else 1

if __name__=="__main__":
    sys.exit(main())
