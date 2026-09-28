"""Check tau/rule-base.tau against the complete synthesized decision table.

Usage, from the application's directory:

    python3 tau/check.py --check-vectors
    python3 tau/check.py --tau PATH/TO/tau

The Tau Language binary comes from IDNI (https://github.com/IDNI/tau-lang)
under IDNI's license. This check is optional and local: it is never run by the
library's gates, and it needs the binary to be installed already.

The specification is run in the Tau REPL as a stream program: each input of
the table is one execution step, the five feature streams, action, and
reviewer flag are fed on stdin, and the three output streams are read back
and compared with the table on every one of its inputs. A disagreement names
the input. Tau assigns 0 to an
output the specification leaves unconstrained, so the specification pins all
three outputs on every branch; agreement means the ladder in rule-base.tau
and the selected program decide every input alike.
"""
import argparse
import itertools
import json
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
INPUTS, OUTPUTS = 7, 3
ASSIGNMENT = re.compile(r"o([1-9])\[(\d+)\] := (\d+)")
DOMAINS = (range(4), range(4), range(3), range(5), range(3), range(2), range(2))


def specification() -> str:
    lines = [line.split("#", 1)[0].strip() for line in (HERE / "rule-base.tau").read_text().splitlines()]
    return " ".join(line for line in lines if line)


def checked_cases() -> list[dict]:
    """Refuse missing, duplicate, malformed, or out-of-domain vectors."""
    cases = json.loads((HERE.parent / "synthesized" / "vectors.json").read_text())["cases"]
    expected = set(itertools.product(*DOMAINS))
    seen = set()
    for case in cases:
        inputs, outputs = case["input"], case["output"]
        if (len(inputs) != INPUTS or len(outputs) != OUTPUTS
                or any(type(value) is not int for value in (*inputs, *outputs))):
            raise ValueError("vectors.json must have seven integer inputs and three integer outputs per case")
        key = tuple(inputs)
        if key not in expected or key in seen:
            raise ValueError(f"vectors.json has an out-of-domain or duplicate input: {inputs}")
        if not (0 <= outputs[0] <= 5 and 0 <= outputs[1] <= 11 and 0 <= outputs[2] <= 3):
            raise ValueError(f"vectors.json has an out-of-domain output: {outputs}")
        seen.add(key)
    if seen != expected:
        raise ValueError(f"vectors.json covers {len(seen)} of {len(expected)} admitted inputs")
    return cases


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--tau", type=Path, help="the Tau Language binary")
    source.add_argument("--check-vectors", action="store_true",
                        help="check complete vector coverage without running Tau")
    parser.add_argument("--smoke", action="store_true",
                        help="with --tau, run one input per decision code instead of all inputs")
    parser.add_argument("--jobs", type=int, default=1,
                        help="REPL processes to run at once; each takes a share of the inputs")
    parser.add_argument("--timeout", type=int, default=14400, help="seconds for the whole run")
    args = parser.parse_args()
    try:
        cases = checked_cases()
    except (KeyError, TypeError, ValueError) as error:
        print(f"invalid vectors.json: {error}", file=sys.stderr)
        return 2
    if args.check_vectors:
        if args.smoke:
            parser.error("--smoke requires --tau")
        print(f"tau: {len(cases)} seven-input vectors cover the admitted domain")
        return 0
    if args.smoke:
        selected = {case["output"][0]: case for case in reversed(cases)}
        if set(selected) != set(range(6)):
            print("vectors.json lacks a decision code for the Tau smoke test", file=sys.stderr)
            return 2
        cases = [selected[code] for code in range(6)]
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
        deadline = time.monotonic() + args.timeout
        try:
            outputs = [process.communicate(timeout=max(0, deadline - time.monotonic()))[0]
                       for process in processes]
        finally:
            for process in processes:
                if process.poll() is None:
                    process.kill()
                    process.wait()
    disagreements = 0
    for shard, output in zip(shards, outputs):
        text = re.sub(r"\x1b\[[0-9;]*m", "", output)
        if any(word in text.lower() for word in ("syntax error", "unsat")):
            print("tau reported an error; first lines of its output:")
            print("\n".join(text.splitlines()[:12]))
            return 1
        observed: dict[int, list] = {}
        for stream, step, value in ASSIGNMENT.findall(text):
            if 1 <= int(stream) <= OUTPUTS:
                observed.setdefault(int(step), [None] * OUTPUTS)[int(stream) - 1] = int(value)
        for step, case in enumerate(shard):
            if observed.get(step) != case["output"]:
                disagreements += 1
                if disagreements <= 10:
                    print(f"input {case['input']}: table {case['output']}, tau {observed.get(step)}")
    scope = "sampled inputs" if args.smoke else "inputs of the decision table"
    print(f"tau: {len(cases)} {scope}, {disagreements} disagreements")
    return 1 if disagreements else 0


if __name__ == "__main__":
    sys.exit(main())
