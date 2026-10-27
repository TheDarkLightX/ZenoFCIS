#!/usr/bin/env python3
"""Run an explicit replay command in a fresh output directory, then compare JSON artifacts.

The selected files and exact ignored JSON pointers define the comparison scope.
No shell is used, and equality is diagnostic evidence, never checker authority.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import subprocess
import sys


def read_json(path):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError("duplicate JSON key")
            result[key] = value
        return result
    if path.is_symlink() or not path.is_file() or path.stat().st_size > 16 * 1024 * 1024:
        raise ValueError("expected regular JSON artifact of at most 16 MiB")
    return json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=unique,
                      parse_constant=lambda value: (_ for _ in ()).throw(ValueError("nonfinite JSON")))


def pointer(path, key):
    return path + "/" + str(key).replace("~", "~0").replace("/", "~1")


def validate_ignores(value, ignores):
    for location in ignores:
        selected = value
        for token in location.split("/")[1:]:
            # Reject malformed escapes, rather than silently ignoring a typo.
            tail = token.replace("~0", "").replace("~1", "")
            if "~" in tail:
                raise ValueError("invalid JSON pointer escape")
            token = token.replace("~1", "/").replace("~0", "~")
            if isinstance(selected, dict) and token in selected:
                selected = selected[token]
            elif isinstance(selected, list) and token.isascii() and token.isdecimal() and str(int(token)) == token and int(token) < len(selected):
                selected = selected[int(token)]
            else:
                raise ValueError("ignored pointer is absent from recorded artifact")
        if isinstance(selected, (dict, list)):
            raise ValueError("ignore must identify a scalar observation")


def first_difference(left, right, ignores, roots, path=""):
    # Ignored observations must still exist on both sides with the same kind.
    # Keys and list lengths are always compared, even below an ignored pointer.
    if type(left) is not type(right):
        return {"pointer": path, "recorded": left, "replayed": right}
    if isinstance(left, dict):
        for key in sorted(left.keys() | right.keys()):
            here = pointer(path, key)
            if key not in left or key not in right:
                return {"pointer": here, "recorded_present": key in left, "replayed_present": key in right}
            difference = first_difference(left[key], right[key], ignores, roots, here)
            if difference:
                return difference
    elif isinstance(left, list):
        for index, (a, b) in enumerate(zip(left, right)):
            difference = first_difference(a, b, ignores, roots, pointer(path, index))
            if difference:
                return difference
        if len(left) != len(right):
            return {"pointer": path, "recorded_length": len(left), "replayed_length": len(right)}
    elif path not in ignores:
        if isinstance(left, str):
            # Normalize only the explicit run roots, at a path boundary.
            def relative(value, root):
                if value == root:
                    return "<run>"
                if value.startswith(root + "/"):
                    return "<run>" + value[len(root):]
                return value
            left, right = relative(left, roots[0]), relative(right, roots[1])
        if left != right:
            return {"pointer": path, "recorded": left, "replayed": right}
    return None


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("recorded", type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--file", action="append", required=True, dest="files",
                        help="Relative JSON artifact to compare; repeat to select more")
    parser.add_argument("--ignore", action="append", default=[], metavar="FILE:POINTER",
                        help="Exact JSON scalar pointer whose value is nondeterministic")
    parser.add_argument("--timeout", type=int, default=300)
    # The explicit separator prevents replay arguments becoming tool options.
    arguments = list(sys.argv[1:] if argv is None else argv)
    if "--" not in arguments:
        parser.error("an explicit replay command after -- is required")
    split = arguments.index("--")
    args = parser.parse_args(arguments[:split])
    command = arguments[split + 1:]
    try:
        recorded = args.recorded.resolve(strict=True)
        if args.out.is_symlink():
            raise ValueError("output must not be a symlink")
        out = args.out.resolve()
        if not recorded.is_dir() or out.exists() or out.is_symlink() or args.timeout <= 0:
            raise ValueError("recorded directory must exist, output must be fresh, timeout must be positive")
        if not command or not any("{out}" in token for token in command):
            raise ValueError("replay command must explicitly use {out}")
        if out.is_relative_to(recorded):
            raise ValueError("output must be outside recorded run")
        selected = sorted(set(args.files))
        for name in selected:
            path = Path(name)
            if path.is_absolute() or ".." in path.parts or not path.parts:
                raise ValueError("artifact must be a relative file inside the run")
            if not (recorded / path).resolve().is_relative_to(recorded):
                raise ValueError("artifact escapes recorded run")
        ignores = {name: set() for name in selected}
        for entry in args.ignore:
            name, sep, location = entry.partition(":")
            if not sep or name not in ignores or not location.startswith("/"):
                raise ValueError("ignore must name a selected file and exact JSON pointer")
            ignores[name].add(location)
        # Validate recorded artifacts before running any command.
        before = {name: read_json(recorded / name) for name in selected}
        for name in selected:
            validate_ignores(before[name], ignores[name])
        out.parent.mkdir(parents=True, exist_ok=True)
        stdout_path = out.with_name(out.name + ".stdout.log")
        stderr_path = out.with_name(out.name + ".stderr.log")
        if stdout_path.exists() or stderr_path.exists():
            raise ValueError("replay log paths must also be fresh")
        command = [token.replace("{out}", str(out)).replace("{recorded}", str(recorded)) for token in command]
        with stdout_path.open("xb") as stdout, stderr_path.open("xb") as stderr:
            completed = subprocess.run(command, stdout=stdout, stderr=stderr, timeout=args.timeout, check=False)
        report = {"schema":"zeno-fcis/run-comparison/1", "authority":"none", "files":selected,
                  "ignored":sorted(args.ignore), "normalized_roots":[str(recorded), str(out)],
                  "replay_exit":completed.returncode, "stdout_log":str(stdout_path), "stderr_log":str(stderr_path), "status":"matched"}
        if completed.returncode != 0:
            report["status"] = "replay-failed"
        else:
            for name in selected:
                if not (out / name).resolve().is_relative_to(out):
                    raise ValueError("artifact escapes replayed run")
                difference = first_difference(before[name], read_json(out / name), ignores[name], (str(recorded), str(out)))
                if difference:
                    report.update(status="differs", first_difference={"file":name, **difference})
                    break
        print(json.dumps(report, sort_keys=True))
        return 0 if report["status"] == "matched" else 1
    except (OSError, ValueError, RecursionError, subprocess.SubprocessError) as error:
        print(json.dumps({"schema":"zeno-fcis/run-comparison/1", "authority":"none", "status":"refused", "reason":str(error)}))
        return 2


if __name__ == "__main__":
    sys.exit(main())
