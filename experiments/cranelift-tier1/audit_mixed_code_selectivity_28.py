#!/usr/bin/env python3
"""Offline per-function x64 code selectivity audit of the CI-produced AOT ZIP.

Independent of Cranelift compilation, replay source, cost and finite-output
receipts first using the existing dedicated rechecker. This additional pass
compares exact archived per-function instruction byte strings. It refuses
changed code in unrelated integer controls rather than assuming that the
whole-function cost ledger proves targeting. This is byte evidence, not a
proof of source-level or target-level semantics.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import zipfile
from pathlib import Path

HEADER = re.compile(r"^wasm\[0\]::function\[(\d+)\]::sat(\d+):$")
BYTE_LINE = re.compile(r"^ {12}([0-9a-f]{2}(?: [0-9a-f]{2})*)(?: {2,}(.*)|$)")
CASES = {"mixed32": (32, False), "mixed128": (128, False),
         "controls32": (32, True)}
UPSTREAM = "b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49"
RECEIPT_SCHEMA = "zeno-fcis/tier1-wasmtime-heterogeneous-kernels/1"
INDEPENDENT_SCHEMA = "zeno-fcis/tier1-wasmtime-heterogeneous-independent/1"


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def parse_functions(data: bytes, count: int) -> dict[int, dict]:
    result = {}
    current = None
    buf = bytearray()
    first_return = None

    def complete():
        if current is None:
            return
        if first_return is None or not buf or first_return > len(buf):
            raise ValueError("function has no valid return or bytes")
        if current in result:
            raise ValueError("duplicate emitted function")
        result[current] = {"machine_bytes": bytes(buf),
                           "through_ret": first_return, "region": len(buf)}

    for ln in data.decode("utf-8").splitlines():
        h = HEADER.fullmatch(ln)
        if h:
            number = int(h.group(1))
            if number != int(h.group(2)):
                raise ValueError("ambiguous function identity in disassembly")
            complete()
            current, buf, first_return = number, bytearray(), None
            continue
        if current is None:
            if ln.strip():
                raise ValueError("disassembly contains non-function text")
            continue
        if not ln.strip():
            continue
        inst = BYTE_LINE.fullmatch(ln)
        if not inst:
            raise ValueError("unexpected noninstruction in function disassembly")
        encoded = bytes.fromhex(inst.group(1))
        if not encoded:
            raise ValueError("empty encoded instruction")
        buf.extend(encoded)
        mnemonic = (inst.group(2) or "").strip()
        if mnemonic == "retq" or mnemonic.startswith("retq "):
            if first_return is not None:
                raise ValueError("duplicate return inside emitted function")
            first_return = len(buf)
    complete()
    if sorted(result) != list(range(count)):
        raise ValueError("missing, extra or reordered functions")
    return result


def audit(path: Path, expected_sha256: str | None) -> dict:
    raw = path.read_bytes()
    zip_sha = digest(raw)
    if expected_sha256 and zip_sha != expected_sha256:
        raise ValueError("workflow ZIP identity does not match expected SHA-256")
    with zipfile.ZipFile(path) as z:
        names = z.namelist()
        if len(names) != len(set(names)):
            raise ValueError("duplicate ZIP members could shadow genuine objects")
        producer_raw = z.read("cases/qualification.json")
        independent_raw = z.read("independent-recheck.json")
        producer, independent = json.loads(producer_raw), json.loads(independent_raw)
        if (producer.get("schema") != RECEIPT_SCHEMA
                or producer.get("upstream_commit") != UPSTREAM
                or producer.get("target") != "x86_64-unknown-linux"
                or independent.get("schema") != INDEPENDENT_SCHEMA
                or independent.get("status")
                   != "native-AOT-remeasured-and-independent-finite-reexecution-passed"
                or independent.get("producer_receipt_sha256") != digest(producer_raw)
                or len(producer.get("cases", [])) != len(CASES)
                or len(independent.get("cases", [])) != len(CASES)):
            raise ValueError("missing required source or independent qualification")
        report = []
        for position, (case, (count, control_only)) in enumerate(CASES.items()):
            row = producer["cases"][position]
            cross = independent["cases"][position]
            if (row.get("case") != case or row.get("functions") != count
                    or row.get("controls_only") is not control_only
                    or cross.get("case") != case
                    or cross.get("functions") != count
                    or cross.get("measured_deltas") != row.get("baseline_minus_patched")
                    or not cross.get("independent_finite_checks", {}).get("baseline", {}).get("passed")
                    or not cross.get("independent_finite_checks", {}).get("patched", {}).get("passed")):
                raise ValueError("case metadata or second finite checker missing")
            per = {}
            for label in ("baseline", "patched"):
                prefix = f"cases/{case}/{label}/"
                original = z.read(prefix + "functions.cwasm")
                dis = z.read(prefix + "disassembly.txt")
                meta = row["results"][label]
                if (digest(original) != meta.get("aot_sha256")
                        or digest(dis) != meta.get("disassembly_sha256")
                        or len(original) != meta.get("aot_bytes")):
                    raise ValueError("archived object/disassembly identity differs")
                per[label] = parse_functions(dis, count)
                if (sum(v["region"] for v in per[label].values())
                        != meta["sum_function_costs"]["symbol_region_bytes"]
                        or sum(v["through_ret"] for v in per[label].values())
                        != meta["sum_function_costs"]["bytes_through_first_return"]):
                    raise ValueError("independent aggregate emitted function cost mismatch")
            changed, unchanged, groups = [], [], {}
            for index in range(count):
                kind = 4 + index % 4 if control_only else index % 8
                a, b = per["baseline"][index], per["patched"][index]
                delta = a["through_ret"] - b["through_ret"]
                region_delta = a["region"] - b["region"]
                actual_change = a["machine_bytes"] != b["machine_bytes"]
                if kind >= 4 and actual_change:
                    raise ValueError(f"unrelated integer control changed emitted code: {case}:{index}")
                groups.setdefault(str(kind), {"functions": 0, "changed": 0,
                                               "prefix_delta_bytes": 0,
                                               "region_delta_bytes": 0})
                group = groups[str(kind)]
                group["functions"] += 1
                group["changed"] += int(actual_change)
                group["prefix_delta_bytes"] += delta
                group["region_delta_bytes"] += region_delta
                (changed if actual_change else unchanged).append(index)
            total_prefix = sum(v["prefix_delta_bytes"] for v in groups.values())
            total_region = sum(v["region_delta_bytes"] for v in groups.values())
            if (total_prefix != row["baseline_minus_patched"]["executable_prefix_bytes"]
                    or total_region != row["baseline_minus_patched"]["function_region_bytes"]):
                raise ValueError("per-function changes do not sum to full cost receipt")
            report.append({
                "case": case, "count": count, "changed": len(changed),
                "unchanged": len(unchanged), "control_code_unchanged": True,
                "by_shape": groups, "observed_deltas": row["baseline_minus_patched"]
            })
    return {
        "schema": "zeno-fcis/tier1-heterogeneous-offline-selectivity/1",
        "status": "per-function-emitted-code-changes-independent-of-compiler-source",
        "source_workflow_zip_sha256": zip_sha,
        "source_producer_receipt_sha256": digest(producer_raw),
        "upstream_commit": UPSTREAM,
        "cases": report,
        "limitations": [
            "uses byte-disassembly from the same archived CI, not an independent disassembler",
            "control bytes unchanged is a narrow selectivity check, not a whole-state proof",
            "the earlier named VeriISLE proof assumes a locally added scalar select spec",
            "one synthetic heterogeneous workload cannot establish application prevalence or speed"
        ],
    }


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--artifact", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--expected-sha256")
    a = p.parse_args()
    if a.out.exists():
        p.error("refusing to overwrite existing audit")
    result = audit(a.artifact.resolve(), a.expected_sha256)
    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_text(json.dumps(result, sort_keys=True, indent=2) + "\n")
    print(json.dumps({"status": result["status"], "by_case": [
        {"case": r["case"], "changed": r["changed"],
         "controls_unchanged": r["control_code_unchanged"],
         "shape_costs": r["by_shape"]} for r in result["cases"]]}, sort_keys=True))


if __name__ == "__main__":
    main()
