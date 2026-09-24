"""Check tau/rule-base.tau against the decision table in synthesized/vectors.json.

Usage, from the application's directory:

    python3 tau/check.py --tau PATH/TO/tau

The Tau Language binary comes from IDNI (https://github.com/IDNI/tau-lang)
under IDNI's license. This check is optional and local: it is never run by the
library's gates, and it needs the binary to be installed already.

The specification is run in the Tau REPL as a stream program: each input of
the table is one execution step, the five feature streams are fed on stdin,
and the three output streams are read back and compared with the table on
every one of its inputs. A disagreement names the input. Tau assigns 0 to an
output the specification leaves unconstrained, so the specification pins all
three outputs on every branch; agreement means the ladder in rule-base.tau
and the selected program decide every input alike.
"""
import argparse
import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
INPUTS, OUTPUTS = 5, 3
ASSIGNMENT = re.compile(r"o([1-9])\[(\d+)\] := (\d+)")


def specification() -> str:
    lines = [line.split("#", 1)[0].strip() for line in (HERE / "rule-base.tau").read_text().splitlines()]
    return " ".join(line for line in lines if line)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--tau", required=True, type=Path, help="the Tau Language binary")
    parser.add_argument("--jobs", type=int, default=1,
                        help="REPL processes to run at once; each takes a share of the inputs")
    parser.add_argument("--timeout", type=int, default=14400, help="seconds for the whole run")
    args = parser.parse_args()
    cases = json.loads((HERE.parent / "synthesized" / "vectors.json").read_text())["cases"]
    if any(len(case["input"]) != INPUTS or len(case["output"]) != OUTPUTS for case in cases):
        print("vectors.json does not have five inputs and three outputs per case", file=sys.stderr)
        return 2
    jobs = max(1, min(args.jobs, len(cases)))
    shards = [cases[index::jobs] for index in range(jobs)]
    # Each input is one execution step of the same specification, so the
    # shards are independent and each numbers its steps from 0. The REPL
    # writes its history into the working directory, so every process runs
    # in its own temporary one and leaves the application untouched.
    with tempfile.TemporaryDirectory(prefix="tau-check-") as scratch:
        processes = []
        for index, shard in enumerate(shards):
            script = [f"i{n} : bv[8] = in console" for n in range(1, INPUTS + 1)]
            script += [f"o{n} : bv[8] = out console" for n in range(1, OUTPUTS + 1)]
            script.append("r " + specification())
            for case in shard:
                script.extend(str(value) for value in case["input"])
            cwd = Path(scratch) / f"shard-{index}"
            cwd.mkdir()
            (cwd / "input.txt").write_text("\n".join(script) + "\n")
            with (cwd / "input.txt").open() as stdin:
                processes.append(subprocess.Popen([str(args.tau.resolve())], stdin=stdin,
                                                  stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                                  text=True, cwd=cwd))
        try:
            outputs = [process.communicate(timeout=args.timeout)[0] for process in processes]
        finally:
            for process in processes:
                if process.poll() is None:
                    process.kill()
    disagreements = 0
    for shard, output in zip(shards, outputs):
        text = re.sub(r"\x1b\[[0-9;]*m", "", output)
        if any(word in text.lower() for word in ("syntax error", "unsat")):
            print("tau reported an error; first lines of its output:")
            print("\n".join(text.splitlines()[:12]))
            return 1
        observed: dict[int, list] = {}
        for stream, step, value in ASSIGNMENT.findall(text):
            observed.setdefault(int(step), [None] * OUTPUTS)[int(stream) - 1] = int(value)
        for step, case in enumerate(shard):
            if observed.get(step) != case["output"]:
                disagreements += 1
                if disagreements <= 10:
                    print(f"input {case['input']}: table {case['output']}, tau {observed.get(step)}")
    print(f"tau: {len(cases)} inputs of the decision table, {disagreements} disagreements")
    return 1 if disagreements else 0


if __name__ == "__main__":
    sys.exit(main())
