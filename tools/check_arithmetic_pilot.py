#!/usr/bin/env python3
"""Check a closed unsigned arithmetic pilot on exact sources and pinned tools.

Produces candidate evidence, not profile promotion, business acceptance,
publication authority, release qualification or Lean KernelChecked evidence.
No tool installation or network access is performed.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import time
from pathlib import Path

import check_verus as verifier
import verus_coverage

ROOT = Path(__file__).resolve().parents[1]
HARNESS = Path("verification/verus/arithmetic_pilot.rs")
SPOT = Path("verification/arithmetic/spot.rs")
BOUNDS = Path("verification/arithmetic/spot_bounds.rs")
WIDE = Path("crates/zeno-fcis-synthesis/src/finite/value/wide.rs")
PROBE = Path("verification/arithmetic/probe.rs")
GRAPH = Path("verification/arithmetic/graphs.rs")
TEST = Path("crates/zeno-fcis-synthesis/tests/arithmetic_pilot.rs")
SOURCES = (
    HARNESS,
    SPOT,
    BOUNDS,
    WIDE,
    PROBE,
    GRAPH,
    TEST,
    verifier.PIN,
    Path("tools/check_arithmetic_pilot.py"),
    Path("tools/check_verus.py"),
    Path("tools/verus_coverage.py"),
    Path("Cargo.toml"),
    Path("Cargo.lock"),
    Path("crates/zeno-fcis-synthesis/Cargo.toml"),
)
RUNTIME_ROOT = Path("crates/zeno-fcis-synthesis/src/finite")
TARGETS = {
    "wide": (
        "multiply",
        "multiply_three",
        "products_ge",
        "mul_div_floor",
        "mul_div_ceil",
    ),
    "spot": (
        "floor_interval",
        "ceil_interval",
        "fee_bound",
        "fee_monotonic",
        "fee_split",
        "output_bound",
        "exact_in",
        "burn_bound",
        "lp_burn",
        "minimum",
        "maximum",
        "saturated_subtraction",
        "min_bounds",
        "max_bounds",
        "select_bounds",
        "is_minimum",
        "selection_bounds_sound",
    ),
    "spot_bounds": (
        "analyse",
        "swap_nodes_defined",
        "liquidity_nodes_defined",
    ),
}
COUNTS = {
    "fee_reduced": 640064,
    "swap_scaled": 5120,
    "burn_scaled": 4096,
    "mint_scaled": 4096,
}


def snapshot() -> dict[str, str]:
    # Bind the complete runtime tree used by the native driver, not just its helper.
    paths = (
        set(SOURCES)
        | {path.relative_to(ROOT) for path in (ROOT / RUNTIME_ROOT).rglob("*.rs")}
        | {Path("crates/zeno-fcis-core/src/resource.rs")}
    )
    result = {}
    for path in sorted(paths):
        full = ROOT / path
        if full.is_symlink() or not full.is_file():
            raise RuntimeError(f"missing or aliased source: {path}")
        result[str(path)] = verifier.digest(full)
    return result


def once(source: str, before: str, after: str) -> str:
    tokens = re.findall(r"\w+|[^\w\s]", before)
    pattern = r"\s*".join(
        (r",?\s*" if token in ("}", ")", "]") else "") + re.escape(token)
        for token in tokens
    )
    matches = list(re.finditer(pattern, source))
    if len(matches) != 1:
        raise RuntimeError(f"mutation anchor is not unique: {before}")
    match = matches[0]
    return source[: match.start()] + after + source[match.end() :]


def mutations() -> dict[str, tuple[Path, str, str]]:
    spot = (ROOT / SPOT).read_text()
    bounds = (ROOT / BOUNDS).read_text()
    return {
        "fee_floor": (
            SPOT,
            once(
                spot,
                "let fee = wide::mul_div_ceil(gross,rate,BPS)?;",
                "let fee = wide::mul_div_floor(gross,rate,BPS)?;",
            ),
            "fee_split",
        ),
        "fee_denominator": (
            SPOT,
            once(
                spot,
                "let fee = wide::mul_div_ceil(gross,rate,BPS)?;",
                "let fee = wide::mul_div_ceil(gross,rate,BPS + 1)?;",
            ),
            "fee_split",
        ),
        "net_omitted": (
            SPOT,
            once(spot, "let net = gross.checked_sub(fee)?;", "let net = gross;"),
            "fee_split",
        ),
        "fee_always_refuses": (
            SPOT,
            once(spot, "if rate > BPS { return None; }", "if true { return None; }"),
            "fee_split",
        ),
        "swap_rounds_up": (
            SPOT,
            once(
                spot,
                "let output=wide::mul_div_floor(rout,net,input_reserve)?;",
                "let output=wide::mul_div_ceil(rout,net,input_reserve)?;",
            ),
            "exact_in",
        ),
        "swap_zero_output": (
            SPOT,
            once(
                spot,
                "let output=wide::mul_div_floor(rout,net,input_reserve)?;",
                "let output=0u128;",
            ),
            "exact_in",
        ),
        "burn_rounds_up": (
            SPOT,
            once(
                spot,
                "wide::mul_div_floor(lp,reserve,supply)",
                "wide::mul_div_ceil(lp,reserve,supply)",
            ),
            "lp_burn",
        ),
        "minimum_arms_swapped": (
            SPOT,
            once(
                spot,
                "pub fn minimum(a:u128,b:u128)->u128 { if a<b {a} else {b} }",
                "pub fn minimum(a:u128,b:u128)->u128 { if a<b {b} else {a} }",
            ),
            "minimum",
        ),
        "min_bound_uses_max": (
            SPOT,
            once(
                spot, "upper:minimum(a.upper,b.upper)", "upper:maximum(a.upper,b.upper)"
            ),
            "min_bounds",
        ),
        "wrong_output_bound": (
            BOUNDS,
            once(
                bounds,
                "swap_output:Range{lower:0,upper:reserve-1}",
                "swap_output:Range{lower:0,upper:0}",
            ),
            "analyse",
        ),
    }


def process(
    command: list[str],
    cwd: Path,
    environment: dict[str, str],
    directory: Path,
    timeout: int = 900,
):
    directory.mkdir(parents=True, exist_ok=True)
    start = time.monotonic()
    proc = verifier.run(command, cwd, environment, timeout=timeout)
    (directory / "stdout.txt").write_text(proc.stdout)
    (directory / "stderr.txt").write_text(proc.stderr)
    (directory / "process.json").write_text(
        json.dumps(
            {
                "command": command,
                "cwd": str(cwd),
                "exit_code": proc.returncode,
                "seconds": time.monotonic() - start,
            },
            indent=2,
            sort_keys=True,
        )
        + "\n"
    )
    return proc


def require_targets(report: dict) -> None:
    functions = report.get("func-details", {})
    required = {
        f"arithmetic_pilot::{module}::{name}"
        for module, names in TARGETS.items()
        for name in names
    }
    missing = required - set(functions)
    if missing:
        raise RuntimeError("missing proof targets: " + ", ".join(sorted(missing)))


def compare_native(
    observed: dict[str, str], baseline: dict[str, str], stub: bool = False
) -> bool:
    return True if stub else observed == baseline


def probe_values(stdout: str) -> dict[str, str]:
    pairs = [line.split(" ", 1) for line in stdout.splitlines() if line.strip()]
    if (
        any(len(row) != 2 for row in pairs)
        or len(pairs) != 7
        or len({row[0] for row in pairs}) != 7
    ):
        raise RuntimeError("missing, duplicated or malformed native witnesses")
    return dict(pairs)


def oracle_rows(stdout: str) -> dict:
    counts = {}
    rows = []
    for line in stdout.splitlines():
        if "ARITHMETIC" in line:
            line = line[line.index("ARITHMETIC") :]
        if line.startswith("ARITHMETIC_COUNT "):
            _, kind, count = line.split()
            if kind in counts:
                raise RuntimeError(f"duplicate count {kind}")
            counts[kind] = int(count)
        elif line.startswith("ARITHMETIC "):
            _, kind, *values = line.split()
            values = list(map(int, values))
            if kind == "fee":
                gross, rate, fee, net = values
                expected = (gross * rate + 9999) // 10000
                if (fee, net) != (expected, gross - expected):
                    raise RuntimeError("full-width fee oracle mismatch")
            elif kind == "swap":
                gross, rate, rin, rout, fee, net, out, postin, postout = values
                expected_fee = (gross * rate + 9999) // 10000
                expected_net = gross - expected_fee
                expected_out = rout * expected_net // (rin + expected_net)
                if (fee, net, out, postin, postout) != (
                    expected_fee,
                    expected_net,
                    expected_out,
                    rin + expected_net,
                    rout - expected_out,
                ):
                    raise RuntimeError("exact-in oracle mismatch")
                if postin * postout < rin * rout:
                    raise RuntimeError("k oracle mismatch")
            elif kind == "burn":
                lp, reserve, supply, out = values
                if out != lp * reserve // supply:
                    raise RuntimeError("full-width burn oracle mismatch")
            else:
                raise RuntimeError(f"unknown oracle row {kind}")
            rows.append({"kind": kind, "values": list(map(str, values))})
    if counts != COUNTS or len(rows) != 57:
        raise RuntimeError(
            f"empty/incomplete native scope: counts={counts}, oracle rows={len(rows)}"
        )
    return {
        "counts": counts,
        "arbitrary_precision_rows": rows,
        "bridge": "native testing, not a compiler or Authority theorem",
    }


def copy_specimen(directory: Path, path: Path, changed: str) -> None:
    for source in (HARNESS, SPOT, BOUNDS, WIDE, PROBE):
        target = directory / source
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(changed) if source == path else target.write_bytes(
            (ROOT / source).read_bytes()
        )


def check(args) -> dict:
    pin = json.loads((ROOT / verifier.PIN).read_text())
    before = snapshot()
    for source in (HARNESS, SPOT, BOUNDS, WIDE):
        if re.search(
            r"\b(?:assume|admit|external_body|external_fn_specification)\b",
            (ROOT / source).read_text(),
        ):
            raise RuntimeError(f"proof hygiene refused: {source}")
    environment = {
        k: v
        for k, v in os.environ.items()
        if not k.startswith(("VERUS_", "VARGO_"))
        and k not in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS")
    }
    environment["TMPDIR"] = str(args.work)
    environment["CARGO_BUILD_JOBS"] = "6"
    args.work.mkdir(parents=True, exist_ok=True)
    args.out.mkdir(parents=True, exist_ok=True)
    proof = None
    inventory = None
    proof_faults = []
    command = []
    if not args.native_controls_only:
        tools = verifier.prepare_tools(args.cache, pin, False)
        environment.update(
            RUSTUP_TOOLCHAIN=pin["rust_toolchain"], VERUS_Z3_PATH=str(tools / "z3")
        )
        command = [
            str(tools / "verus"),
            "--crate-type=lib",
            "--edition=2024",
            "--no-cheating",
            "--no-external-by-default",
            "--num-threads",
            "1",
            "--output-json",
            "--log",
            "vir",
            "--log",
            "vir-option=no_span+no_type+no_fn_details",
        ]
        positive = process(
            [*command, "--log-dir", str(args.out / "positive/logs"), str(HARNESS)],
            ROOT,
            environment,
            args.out / "positive",
        )
        verifier.require_success(positive)
        proof = json.loads(positive.stdout)
        if not verifier.proof_succeeded(proof, pin):
            raise RuntimeError("incomplete or unpinned positive proof")
        require_targets(proof)
        vir = (args.out / "positive/logs/crate.vir").read_text()
        observed = verus_coverage.inventory(vir, "arithmetic_pilot::")
        inventory = verus_coverage.inventory(
            vir, "arithmetic_pilot::", body_functions=tuple(observed)
        )
        if any(
            row["mode"] == "Exec" and row["ensures"] < 1 for row in inventory.values()
        ):
            raise RuntimeError("executable body missing its arithmetic contract")
        print(
            "Arithmetic pilot: pinned whole-harness candidate proof passed", flush=True
        )
        for name, (path, changed, function) in mutations().items():
            specimen = args.work / "specimens" / name
            copy_specimen(specimen, path, changed)
            proc = process(
                [*command, "--log-dir", str(args.out / name / "logs"), str(HARNESS)],
                specimen,
                environment,
                args.out / name,
            )
            report = json.loads(proc.stdout)
            region = verifier.function_lines(changed, function)
            if not verifier.semantic_refusal(
                proc, report, pin, path, specimen, region=region
            ):
                raise RuntimeError(
                    f"proof control {name} survived or did not reach its semantic obligation"
                )
            proof_faults.append(
                {
                    "name": name,
                    "function": function,
                    "region": region,
                    "exit_code": proc.returncode,
                    "semantic_refusal": True,
                }
            )
            print(f"Arithmetic pilot: semantic proof refusal {name}", flush=True)
        verifier.verify_tool_files(tools, pin)
        native = process(
            [
                "cargo",
                "+1.97.1",
                "test",
                "-p",
                "zeno-fcis-synthesis",
                "--locked",
                "--offline",
                "--test",
                "arithmetic_pilot",
                "--test",
                "v2_numeric_wide_arithmetic",
                "--test",
                "v2_numeric_execution",
                "--",
                "--nocapture",
                "--test-threads=1",
            ],
            ROOT,
            environment,
            args.out / "native",
        )
        verifier.require_success(native)
        oracle = oracle_rows(native.stdout)
    else:
        oracle = None
    rust = [
        "rustc",
        f"+{pin['runtime_rust']}",
        "--edition=2024",
        "--check-cfg",
        "cfg(verus_keep_ghost)",
        "--check-cfg",
        "cfg(test)",
        "-A",
        "dead_code",
    ]
    binary = args.work / "baseline-probe"
    verifier.require_success(
        process(
            [*rust, str(PROBE), "-o", str(binary)],
            ROOT,
            environment,
            args.out / "native-baseline-build",
        )
    )
    baseline = process([str(binary)], ROOT, environment, args.out / "native-baseline")
    verifier.require_success(baseline)
    expected = probe_values(baseline.stdout)
    independent = {
        "fee": "Some((1, 0))",
        "fee_full": f"Some(({2**128 - 1}, 0))",
        "swap": "Some(SwapQuote { fee: 0, net: 2, output: 1, input_reserve: 5, output_reserve: 3 })",
        "burn": "Some(1)",
        "minimum": "2",
        "min_bounds": "Range { lower: 1, upper: 3 }",
        "bounds": "Some(Bounds { amount: Range { lower: 0, upper: 3 }, rate: Range { lower: 0, upper: 10000 }, reserve: Range { lower: 1, upper: 5 }, supply: Range { lower: 1, upper: 7 }, fee: Range { lower: 0, upper: 3 }, net: Range { lower: 0, upper: 3 }, denominator: Range { lower: 1, upper: 8 }, swap_output: Range { lower: 0, upper: 4 }, remaining_reserve: Range { lower: 1, upper: 5 }, burn_output: Range { lower: 0, upper: 15 }, mint_side: Range { lower: 0, upper: 21 }, minted: Range { lower: 0, upper: 21 } })",
    }
    if expected != independent:
        raise RuntimeError(
            "native semantic baseline differs from its independent witnesses"
        )
    native_faults = []
    for name, (path, changed, function) in mutations().items():
        specimen = args.work / "specimens" / name
        copy_specimen(specimen, path, changed)
        binary = specimen / "native-probe"
        verifier.require_success(
            process(
                [*rust, str(PROBE), "-o", str(binary)],
                specimen,
                environment,
                args.out / name / "native-build",
            )
        )
        proc = process([str(binary)], specimen, environment, args.out / name / "native")
        verifier.require_success(proc)
        observed = probe_values(proc.stdout)
        if compare_native(observed, expected, args.stub_comparison):
            raise RuntimeError(f"native control {name} survived the comparison")
        differences = {
            key: {"expected": expected[key], "actual": observed[key]}
            for key in expected
            if expected[key] != observed[key]
        }
        # Disable precisely the result comparison, on the same admitted executable
        # and witness. Its negative-control expectation must now fail.
        if not compare_native(observed, expected, True):
            raise RuntimeError(
                "comparison-disable experiment did not bypass the comparison"
            )
        native_faults.append(
            {
                "name": name,
                "function": function,
                "differences": differences,
                "native_exit_code": proc.returncode,
                "comparison_disabled_fault_survives": True,
            }
        )
    if snapshot() != before:
        raise RuntimeError("source changed during qualification")
    domain = {
        "amount": ["0", "3000000000"],
        "fee_rate": ["0", "10000"],
        "reserve": ["1", "3000000000"],
        "supply": ["1", "3000000000"],
        "width": "unsigned128",
        "fee_scale": "10000",
        "business_guards": "separate",
    }
    domain_bytes = json.dumps(domain, sort_keys=True, separators=(",", ":")).encode()
    return {
        "schema": "zeno-fcis/arithmetic-pilot-evidence/1",
        "status": "native-controls-passed"
        if args.native_controls_only
        else "candidate-passed",
        "source_sha256": before,
        "binding": {
            "graph_source_sha256": before[str(GRAPH)],
            "arithmetic_specification_sha256": before[str(SPOT)],
            "bounds_specification_sha256": before[str(BOUNDS)],
            "domain_width_scale": domain,
            "domain_width_scale_sha256": hashlib.sha256(domain_bytes).hexdigest(),
            "scope": "source-bound closed graph descriptions; no compiler correspondence claim",
        },
        "toolchain": pin,
        "verus_report": proof,
        "translated_function_inventory": inventory,
        "proof_controls": proof_faults,
        "native_controls": native_faults,
        "native": oracle,
        "authority": "none",
        "bridge_status": {
            "wide_primitives": "proved in the same harness",
            "closed_graph_admission_and_numeric_driver": "tested",
            "main_rules_and_business_Authority": "out of scope",
        },
        "scope": {
            "amount_limit": "3000000000",
            "fee_gross": "full u128",
            "fee_rate": "0..10000",
            "reserves": "1..3000000000",
            "supply": "1..3000000000",
            "LP_burn": "full u128, supply > 0 and lp <= supply; no greater than pro rata",
            "post_input_reserve_limit": "6000000000",
            "rounding": "fee ceiling, output/burn floor",
        },
        "promotion": "pending independent exact-source inventory review",
        "unproved": [
            "Rust compiler and erasure correspondence",
            "numeric-driver bridge beyond the tested corpus",
            "business admission, raw input decoding, resource sufficiency",
            "main arithmetic rules, original application parity, histories, publication and shell",
            "release readiness",
        ],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--work", type=Path, required=True)
    parser.add_argument(
        "--cache", type=Path, default=verifier.cache_directory()
    )
    parser.add_argument("--native-controls-only", action="store_true")
    parser.add_argument("--stub-comparison", action="store_true")
    args = parser.parse_args()
    args.out = args.out.resolve()
    args.work = args.work.resolve()
    args.cache = args.cache.resolve()
    try:
        receipt = check(args)
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        receipt = {
            "schema": "zeno-fcis/arithmetic-pilot-evidence/1",
            "status": "failed",
            "error": str(error),
            "authority": "none",
        }
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / "receipt.json").write_text(
        json.dumps(receipt, indent=2, sort_keys=True) + "\n"
    )
    print(f"Arithmetic pilot: {receipt['status']}", flush=True)
    if receipt["status"] == "failed":
        print(receipt["error"], flush=True)
        return 1
    return 0


if __name__ == "__main__":
    from telemetry import run_cli
    def _entrypoint():
        raise SystemExit(main())
    run_cli(_entrypoint)
