#!/usr/bin/env python3
"""Cross-target same-CLIF compiler A/B. Emitted symbol bytes only, never speed."""
from __future__ import annotations
import argparse
import hashlib
import json
import re
import subprocess
from pathlib import Path
from install_isle_candidate import UPSTREAM, SOURCE, ORIGINAL_GIT_BLOB, RULE, RULE_NAME

HERE = Path(__file__).resolve().parent
CLIF_SHA = "579244793d801ad907d3ed3c611b156103379e4d91c68d44c6ad4226ea85312e"
TARGETS = {"x86_64": "Advanced Micro Devices X86-64", "aarch64": "AArch64"}
MODES = ("speed", "speed_and_size")
PHASES = ("baseline", "patched")
SYMBOL = "%tier1_signed_satadd"

def digest(b):
    return hashlib.sha256(b).hexdigest()

def command(*args):
    return subprocess.run(args, check=True, capture_output=True, text=True).stdout

def parse_header(raw, target):
    if not re.search(r"(?m)^\s*Class:\s*ELF64\s*$", raw):
        raise ValueError("expected ELF64")
    machine = re.search(r"(?m)^\s*Machine:\s*(.*?)\s*$", raw)
    if machine is None or machine.group(1) != TARGETS[target]:
        raise ValueError("incorrect target architecture")

def parse_symbol(raw):
    syms = []
    for line in raw.splitlines():
        f = line.split()
        if len(f) == 4 and f[1] in ("T", "t"):
            try:
                syms.append({"name": f[0], "offset": int(f[2], 16), "bytes": int(f[3], 16)})
            except ValueError as error:
                raise ValueError("invalid text symbol size") from error
    if len(syms) != 1 or syms[0]["name"] != SYMBOL or syms[0]["bytes"] < 1:
        raise ValueError("expected one nonempty, exactly named function")
    return syms[0]

def parse_text_size(raw):
    hits = re.findall(r"(?m)^\s*\[\s*\d+\]\s+\.text\s+\S+\s+\S+\s+\S+\s+([0-9a-fA-F]+)\s+", raw)
    if len(hits) != 1 or int(hits[0], 16) <= 0:
        raise ValueError("invalid or duplicated text section")
    return int(hits[0], 16)

def profile(path, target):
    if not path.is_file():
        raise ValueError(f"missing object {path}")
    parse_header(command("readelf", "-h", str(path)), target)
    sym = parse_symbol(command("nm", "-S", "--defined-only", "--format=posix", str(path)))
    text_bytes = parse_text_size(command("readelf", "-W", "-S", str(path)))
    if sym["offset"] + sym["bytes"] > text_bytes:
        raise ValueError("symbol extends beyond section")
    raw = path.read_bytes()
    return {"target": target, "object_sha256": digest(raw), "object_bytes": len(raw),
            "function": sym, "text_section_bytes": text_bytes}

def expected_commands():
    return [
        f"phase={phase} target={target} opt_level={mode} source=experiments/cranelift-tier1/satadd.clif"
        for phase in PHASES for target in TARGETS for mode in MODES
    ]

def check_source_and_proof(root, upstream):
    if digest((HERE / "satadd.clif").read_bytes()) != CLIF_SHA:
        raise ValueError("original qualified CLIF input changed")
    if command("git", "-C", str(upstream), "rev-parse", "HEAD").strip() != UPSTREAM:
        raise ValueError("upstream revision mismatch")
    if command("git", "-C", str(upstream), "rev-parse", f"HEAD:{SOURCE}").strip() != ORIGINAL_GIT_BLOB:
        raise ValueError("unreviewed upstream rule source")
    original = command("git", "-C", str(upstream), "show", f"HEAD:{SOURCE}")
    if (upstream / SOURCE).read_text() != original.rstrip("\n") + "\n" + RULE:
        raise ValueError("unreviewed ISLE rule patch")
    if command("git", "-C", str(upstream), "diff", "--name-only").splitlines() != [SOURCE]:
        raise ValueError("unexpected modified upstream source")
    patch = json.loads((root / "patch-receipt.json").read_text())
    proof = json.loads((root / "rule-proof.json").read_text())
    rule_hash = digest(RULE.encode())
    if not (patch.get("status") == "source-pinned-patch-applied"
            and patch.get("upstream_commit") == UPSTREAM
            and patch.get("rule_name") == RULE_NAME
            and patch.get("rule_sha256") == rule_hash
            and proof.get("status") == "unsat-counterexample-query"
            and proof.get("rule") == RULE_NAME
            and proof.get("upstream_commit") == UPSTREAM
            and proof.get("rule_sha256") == rule_hash):
        raise ValueError("incomplete or forged rule proof/patch receipts")
    if (root / "commands.txt").read_text().splitlines() != expected_commands():
        raise ValueError("missing, extra or reordered compiler invocations")
    return {"clif_sha256": CLIF_SHA, "rule_sha256": rule_hash,
            "patch_receipt_sha256": digest((root / "patch-receipt.json").read_bytes()),
            "proof_receipt_sha256": digest((root / "rule-proof.json").read_bytes()),
            "commands_sha256": digest((root / "commands.txt").read_bytes())}

def summarize(rows):
    seen = set()
    wins, regressions, ties = [], [], []
    for r in rows:
        key = (r["target"], r["opt_level"])
        if key not in {(t, m) for t in TARGETS for m in MODES} or key in seen:
            raise ValueError("unexpected or duplicate target/optimization pair")
        seen.add(key)
        if r["baseline"]["target"] != r["target"] or r["patched"]["target"] != r["target"]:
            raise ValueError("architecture disagreement")
        d = r["baseline"]["function"]["bytes"] - r["patched"]["function"]["bytes"]
        if d != r["symbol_delta_baseline_minus_patched"]:
            raise ValueError("inconsistent reported byte delta")
        (wins if d > 0 else regressions if d < 0 else ties).append(
            {"target": r["target"], "mode": r["opt_level"], "delta": d})
    if seen != {(t, m) for t in TARGETS for m in MODES}:
        raise ValueError("incomplete target/optimization matrix")
    anchor = next(r for r in rows if (r["target"], r["opt_level"]) == ("x86_64", "speed_and_size"))
    if (anchor["baseline"]["function"]["bytes"], anchor["patched"]["function"]["bytes"]) != (56, 39):
        raise ValueError("previously qualified x64 positive-control result drifted")
    return {"wins": wins, "regressions": regressions, "ties": ties}

def qualify(root, upstream):
    source = check_source_and_proof(root, upstream)
    rows = []
    for target in TARGETS:
        for mode in MODES:
            a = profile(root / target / mode / "baseline.o", target)
            b = profile(root / target / mode / "patched.o", target)
            rows.append({"target": target, "opt_level": mode, "baseline": a,
                         "patched": b, "symbol_delta_baseline_minus_patched":
                         a["function"]["bytes"] - b["function"]["bytes"]})
    return {"schema": "zeno-fcis/tier1-cross-target-28-code-size/1",
            "status": "measured-four-same-clif-patched-vs-unpatched-pairs",
            "upstream_pin": UPSTREAM,
            "experiment_pin": command("git", "rev-parse", "HEAD").strip(),
            "proof_and_source": source, "pairs": rows, "summary": summarize(rows),
            "metric": "same-target whole ELF text-symbol bytes (may include literal pool and padding)",
            "nonclaims": ["no latency or throughput improvement measured",
                          "AArch64 code was cross-compiled, not native-executed",
                          "provisional VeriISLE select specification not upstream-certified",
                          "no compiler-wide, poison, target-ABI or flags-liveness proof",
                          "not an upstream PR or a general application speedup"]}

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", type=Path, required=True)
    ap.add_argument("--upstream", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    args = ap.parse_args()
    if args.out.exists():
        ap.error("new output required")
    result = qualify(args.root.resolve(), args.upstream.resolve())
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, sort_keys=True, indent=2) + "\n")
    print(json.dumps({"status": result["status"], "summary": result["summary"]}, sort_keys=True))

if __name__ == "__main__":
    main()
