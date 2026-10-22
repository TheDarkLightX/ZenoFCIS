#!/usr/bin/env python3
"""Produce source-bound checks, CLI packets, solver queries and six-input counts."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
LOW = ["nice", "-n", "19", "ionice", "-c3"]
CARGO = ["cargo", "+1.97.1"]
TEST = [*CARGO, "test", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "--locked", "--offline"]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--git-dir", type=Path)
    args = parser.parse_args()
    fixture = ROOT / "tests/fixtures/repair-pre-lemma"
    # Full swap laws run in pinned qualification. This sparse amount fixture
    # replays the earlier wide-domain CLI integration without a large catalog.
    wide_fixture = ROOT / "tests/fixtures/numeric-wide"
    for contract in (fixture, wide_fixture):
        for name in ("project.zeno", "v2/policy.json"):
            assert (contract / name).is_file(), f"missing evidence fixture: {contract / name}"
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    env = os.environ.copy()
    env.pop("ZENO_FCIS_SYMBOLIC_QUERIES", None)
    env.update(CARGO_PROFILE_DEV_DEBUG="0", CARGO_PROFILE_TEST_DEBUG="0")
    assert env["CARGO_BUILD_JOBS"] == "3"
    git = ["git"] + ([f"--git-dir={args.git_dir.resolve()}"] if args.git_dir else [])
    git += [f"--work-tree={ROOT}"]
    records = []

    def run(label, command, expected=0, extra_env=None, timeout=1800):
        current = dict(env, **(extra_env or {}))
        scratch = Path(env["TMPDIR"]) / f"{out.name}-{label}"
        scratch.mkdir(exist_ok=False)
        current["TMPDIR"] = str(scratch)
        started = time.monotonic()
        command = [*LOW, *map(str, command)]
        with (out / f"{label}.log").open("w") as log:
            completed = subprocess.run(command, cwd=ROOT, env=current, stdout=log,
                                       stderr=subprocess.STDOUT, timeout=timeout)
        record = dict(label=label, command=command, exit=completed.returncode,
                      expected_exit=expected, seconds=round(time.monotonic() - started, 3),
                      log_sha256=digest(out / f"{label}.log"),
                      env={k: current[k] for k in ("CARGO_TARGET_DIR", "CARGO_BUILD_JOBS", "TMPDIR",
                           "ZENO_FCIS_Z3", "ZENO_FCIS_CVC5")})
        if extra_env:
            record["env"].update(extra_env)
        records.append(record)
        (out / "COMMANDS.json").write_text(json.dumps(records, indent=2) + "\n")
        print(json.dumps({k: record[k] for k in ("label", "exit", "seconds")}), flush=True)
        assert completed.returncode == expected, f"unexpected exit: {label}"
        return (out / f"{label}.log").read_text()

    def git_text(*arguments):
        return subprocess.check_output([*LOW, *git, *arguments], cwd=ROOT, text=True).strip()

    assert not git_text("status", "--porcelain"), "commit the candidate before qualification"
    paths = subprocess.check_output([*LOW, *git, "ls-files", "-z"], cwd=ROOT).decode().split("\0")
    files = {path: digest(ROOT / path) for path in paths if path and (ROOT / path).is_file()}
    source = dict(commit=git_text("rev-parse", "HEAD"), tree=git_text("rev-parse", "HEAD^{tree}"),
                  files=files, solvers={key: dict(path=env[key], sha256=digest(Path(env[key])))
                                      for key in ("ZENO_FCIS_Z3", "ZENO_FCIS_CVC5")})
    (out / "SOURCE.json").write_text(json.dumps(source, indent=2) + "\n")
    for key in ("ZENO_FCIS_Z3", "ZENO_FCIS_CVC5"):
        run(key.lower(), [env[key], "--version"])
    run("numeric-qualification", [*TEST, "contract::review::numeric_symbolic::tests::", "--",
                                 "--include-ignored", "--nocapture", "--test-threads=1"])
    run("finite-review-tests", [*TEST, "contract::review::", "--", "--test-threads=1"])
    run("original-controls", ["python3", "scale-26/run_controls.py", "--out", out / "original-controls"])
    run("premise-controls", ["python3", "scale-26/run_review_fix_control.py", "--out", out / "premise-controls"])
    run("restored-lemma-tests", [*TEST, "contract::review::numeric_symbolic::tests::lemma_tests::", "--",
                                 "--include-ignored", "--nocapture", "--test-threads=1"])
    # The control builds have now been restored. Rebuild before CLI evidence.
    run("final-build", [*CARGO, "build", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "--locked", "--offline"])
    binary = Path(env["CARGO_TARGET_DIR"]) / "debug/zeno-fcis"
    source["binary_sha256"] = digest(binary)
    (out / "SOURCE.json").write_text(json.dumps(source, indent=2) + "\n")

    def packet(label, contract, cap, lemma=None, transition=False, expected=0, **options):
        target = out / f"{label}.json"
        query_dir = out / f"{label}-queries"
        command = [binary, "contract", "lemma" if lemma else "review", contract,
                   "--max-tuples", str(cap), "--out", target]
        if lemma:
            command += ["--lemma", lemma]
            if transition:
                command.append("--transition")
        for option in options:
            command.append("--" + option.replace("_", "-"))
        extra = {"ZENO_FCIS_SYMBOLIC_QUERIES": str(query_dir)} if cap == 1 else None
        run(label, command, expected, extra)
        return json.loads(target.read_text())

    native = packet("repair-native", fixture, 6, "pre.100.110 >= 1", True, 1)
    symbolic = packet("repair-symbolic", fixture, 1, "pre.100.110 >= 1", True, 1)
    assert native["status"] == symbolic["status"] == "refuted"
    assert symbolic["lemmas"][0]["obligations"][0]["counterexample"]["authority"]["class"] == "Accept"
    assert packet("repair-review", fixture, 1)["status"] == "solver-checked"
    assert packet("wide-review", wide_fixture, 1)["status"] == "solver-checked"
    assert packet("symbolic-gaps-refusal", fixture, 1, expected=2, gaps=True)["status"] == "UNKNOWN"
    assert packet("state-induction", fixture, 1, "post.100.110 >= 1")["status"] == "solver-checked"

    repair_only = out / "repair-only"
    (repair_only / "v2").mkdir(parents=True)
    (repair_only / "project.zeno").write_bytes((fixture / "project.zeno").read_bytes())
    rules = json.loads((fixture / "v2/policy.json").read_text())
    rules["cases"][0]["when"] = "pre.100.110 >= 1"
    (repair_only / "v2/policy.json").write_text(json.dumps(rules, indent=2) + "\n")
    assert packet("repair-only-nonvacuity", repair_only, 1, "true", True)["status"] == "solver-checked"

    counts = {}
    for label, wanted in [("lemma-0-nonvacuity", 6), ("lemma-0-case-1", 2)]:
        original = (out / "repair-symbolic-queries" / f"{label}.smt2").read_text()
        table = []
        for state in range(3):
            for context in range(2):
                fixed = original.replace("(check-sat)", f"(assert (= x0 {state}))\n(assert (= x1 0))\n(assert (= x2 {context}))\n(check-sat)")
                query = out / f"{label}-pre{state}-ctx{context}.smt2"
                query.write_text(fixed)
                answers = {}
                for key, flags in [("ZENO_FCIS_Z3", ["-smt2"]), ("ZENO_FCIS_CVC5", ["--lang=smt2"])]:
                    name = f"{label}-pre{state}-ctx{context}-{key.lower()}"
                    output = run(name, [env[key], *flags, query], timeout=15)
                    answer = output.strip()
                    assert answer in ("sat", "unsat"), "timeout/unknown is not a count"
                    answers[key] = answer
                assert len(set(answers.values())) == 1, "count requires solver agreement"
                table.append(dict(state=state, command=0, context=bool(context), answers=answers))
        count = sum(row["answers"]["ZENO_FCIS_Z3"] == "sat" for row in table)
        assert count == wanted
        counts[label] = dict(count=count, inputs=table)
    admitted = counts["lemma-0-nonvacuity"]["count"]
    failed = counts["lemma-0-case-1"]["count"]
    assert native["lemmas"][0]["admitted"] == admitted == 6
    assert native["lemmas"][0]["failed"] == failed == 2
    assert native["lemmas"][0]["checked"] == admitted - failed == 4
    counts["comparison"] = dict(tuples=6, admitted=admitted, failed=failed, checked=admitted-failed,
                                 scope="six-input regression fixture; no wide-domain counting claim")
    (out / "COUNTS.json").write_text(json.dumps(counts, indent=2) + "\n")
    assert files == {path: digest(ROOT / path) for path in files}, "source changed during qualification"
    assert not git_text("status", "--porcelain")
    run("final-diff-check", [*git, "diff", "--check"])
    manifest = {str(path.relative_to(out)): digest(path) for path in sorted(out.rglob("*"))
                if path.is_file()}
    (out / "ARTIFACTS.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(json.dumps(dict(status="passed", source=source["commit"], commands=len(records),
                          counts=counts["comparison"])), flush=True)


if __name__ == "__main__":
    main()
