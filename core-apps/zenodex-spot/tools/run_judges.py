#!/usr/bin/env python3
"""Run only small declared-domain contract judges; never invoke heavy tools."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import time
from pathlib import Path

from build_contracts import source_hash

PINNED_CLI = Path(os.environ.get("ZENO_FCIS_CLI", "zeno-fcis"))
LEMMAS = {"create": "post.100.113 >= 0 && post.100.113 <= 1",
          "lp": "post.100.110 >= 1000 && post.100.110 <= 1002",
          "swap-in": "post.100.110 > 0 && post.100.111 > 0 && post.100.112 == 1000",
          "swap-out": "post.100.110 > 0 && post.100.111 > 0 && post.100.112 == 1000",
          "ingress-edge": "post.100.111 == 4294967295 && post.100.112 == 4294967295 && post.100.113 == 4294967295"}


def invoke(cli, arguments, out, label, timeout=40):
    argv = [str(cli), "--no-kb", *map(str, arguments), "--out", str(out / f"{label}.json")]
    started = time.monotonic()
    try:
        run = subprocess.run(argv, capture_output=True, text=True, timeout=timeout, check=False)
        code, stdout, stderr = run.returncode, run.stdout, run.stderr
    except subprocess.TimeoutExpired as error:
        code, stdout, stderr = 124, error.stdout or "", error.stderr or ""
        if type(stdout) is bytes: stdout = stdout.decode(errors="replace")
        if type(stderr) is bytes: stderr = stderr.decode(errors="replace")
    record = {"argv": argv, "exit_code": code, "seconds": round(time.monotonic() - started, 3),
              "stdout": stdout, "stderr": stderr}
    (out / f"{label}.command.json").write_text(json.dumps(record, indent=2) + "\n")
    packet = out / f"{label}.json"
    return record, json.loads(packet.read_text()) if packet.is_file() else None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--project", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--cli", type=Path, default=PINNED_CLI)
    parser.add_argument("--only", choices=("review", "lemma", "scenario", "all"), default="all")
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    name = args.project.name
    results = []
    for kind in ("review", "lemma", "scenario"):
        if args.only not in (kind, "all"): continue
        extra = (["--gaps", "--max-work", "2000000"] if kind == "review" else
                 ["--lemma", LEMMAS[name]] if kind == "lemma" else [])
        record, packet = invoke(args.cli, ["contract", kind, args.project, "--max-tuples", "512", *extra], args.out, kind)
        summary = {"judge": kind, "exit_code": record["exit_code"], "seconds": record["seconds"],
                   "status": None if packet is None else packet.get("status"), "stderr": record["stderr"]}
        results.append(summary)
        print(json.dumps(summary), flush=True)
    (args.out / "identity.json").write_text(json.dumps({"cli_sha256": hashlib.sha256(args.cli.read_bytes()).hexdigest(),
        "sources": source_hash(args.project), "results": results, "authority": "none"}, indent=2) + "\n")
    raise SystemExit(0 if all(r["exit_code"] == 0 for r in results) else 1)


if __name__ == "__main__": main()
