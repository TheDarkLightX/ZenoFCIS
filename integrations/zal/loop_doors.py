"""Shell for the bounded loop's doors: writes door documents, runs the zeno-fcis CLI, judges nothing.

Every verdict comes from the Rust CLI (``zeno-fcis loop candidate
--expressions``, ``loop sketch``, ``loop family``, ``loop transfer``), which
judges with ``transform check`` and its receipt replay. This module only:

* resolves the CLI;
* writes ``zeno-fcis/loop-expressions/1`` documents and
  ``zeno-fcis/loop-family-manifest/1`` manifests;
* generates family instances with ZenoFCIS's own tools
  (``tools/instantiate_core.py``, ``zeno-fcis generate contract`` and
  ``zeno-fcis contract export-program``, and ``zeno-fcis optimize`` for the
  size comparison);
* runs the commands with bounded output and returns their JSON.

This module is LLM-generated.
"""
from __future__ import annotations

import json
import os
import re
from contextlib import contextmanager
from contextvars import ContextVar
from pathlib import Path
import subprocess
import sys

from behavior import Refusal
import report_numbers as N

ROOT = Path(__file__).resolve().parents[2]
EXPRESSIONS_SCHEMA = "zeno-fcis/loop-expressions/1"
MANIFEST_SCHEMA = "zeno-fcis/loop-family-manifest/1"
OUTPUT_LIMIT = 8 << 20
TIMEOUT = 120


def resolve_cli(given: str | os.PathLike | None = None) -> Path:
    """The zeno-fcis CLI: the given path, ``ZENO_FCIS_CLI``, or the debug build in ``CARGO_TARGET_DIR``."""
    if given:
        cli = Path(given)
    elif os.environ.get("ZENO_FCIS_CLI"):
        cli = Path(os.environ["ZENO_FCIS_CLI"])
    else:
        target = Path(os.environ.get("CARGO_TARGET_DIR", str(ROOT / "target")))
        if not target.is_absolute():
            target = ROOT / target
        cli = target / "debug" / "zeno-fcis"
    cli = cli.resolve()
    if not cli.is_file() or not os.access(cli, os.X_OK):
        raise Refusal("Build the zeno-fcis CLI first, or name it with --cli or ZENO_FCIS_CLI")
    return cli


_automatic_kb_disabled = ContextVar('automatic_kb_disabled', default=False)


@contextmanager
def disable_automatic_knowledge():
    """A single MCP call cannot inherit a KB proposal destination."""
    token=_automatic_kb_disabled.set(True)
    try:
        yield
    finally:
        _automatic_kb_disabled.reset(token)


def run(cli: Path, arguments: list, cwd: Path | None = None) -> tuple[int, dict]:
    """Runs one CLI command and returns its exit code and JSON report."""
    if _automatic_kb_disabled.get():
        arguments=["--no-kb",*arguments]
    environment=None
    if any(str(a).split('=',1)[0]=='--kb-op' for a in arguments):
        environment={k:v for k,v in os.environ.items() if k!='ZENO_FCIS_KB'}
    try:
        done = subprocess.run([str(cli), *[str(a) for a in arguments]], capture_output=True, timeout=TIMEOUT,
                              cwd=cwd, env=environment)
    except subprocess.TimeoutExpired:
        raise Refusal("zeno-fcis did not finish within its time limit") from None
    if len(done.stdout) > OUTPUT_LIMIT:
        raise Refusal("zeno-fcis output exceeds its bound")
    try:
        report = json.loads(done.stdout or b"{}")
    except ValueError:
        raise Refusal(f"zeno-fcis printed no JSON report (exit {done.returncode})") from None
    if not isinstance(report, dict):
        raise Refusal("zeno-fcis report is not an object")
    return done.returncode, report


def write_new(path: Path, data: dict) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    text = json.dumps(data, ensure_ascii=True, sort_keys=True, indent=1) + "\n"
    with open(path, "x", encoding="ascii") as stream:
        stream.write(text)
    return path


def document(vocabulary: dict | None, outputs: list, parameters: dict | None = None) -> dict:
    data = {"schema": EXPRESSIONS_SCHEMA, "outputs": list(outputs)}
    if vocabulary is not None:
        data["vocabulary"] = vocabulary
    if parameters:
        data["parameters"] = dict(parameters)
    return data


def open_session(cli: Path, original: Path, session: Path) -> dict:
    code, report = run(cli, ["loop", "open", "--original", original, "--session", session,
                             "--profile", "checked-i128-v1"])
    if code != 0 or report.get("status") != "opened":
        raise Refusal(f"loop open refused the program: {report.get('status')}")
    return report


def candidate(cli: Path, session: Path, path: Path) -> tuple[int, dict]:
    return run(cli, ["loop", "candidate", "--session", session, "--expressions", path])


def sketch(cli: Path, session: Path, path: Path) -> tuple[int, dict]:
    return run(cli, ["loop", "sketch", "--session", session, "--sketch", path])


def meaning(cli: Path, original: Path, path: Path) -> tuple[int, dict]:
    """The meaning key of a complete expressions document: its observation on every declared input."""
    return run(cli, ["loop", "meaning", "--original", original, "--expressions", path])


def question(cli: Path, contract: Path, answers: Path | None) -> tuple[int, dict]:
    """The largest open region as a question, after the recorded answers (``loop question``)."""
    return run(cli, ["loop", "question", "--contract", contract] + (["--answers", answers] if answers else []))


def teach_back(cli: Path, contract: Path, rules: Path, answers: Path | None) -> tuple[int, dict]:
    """Restated rules judged on the whole declared domain (``loop teach-back``)."""
    return run(cli, ["loop", "teach-back", "--contract", contract, "--rule", rules]
               + (["--answers", answers] if answers else []))


def decompose(cli: Path, contract: Path, architecture: Path, compose: bool = True) -> tuple[int, dict]:
    """An architecture held against the core (``loop decompose``): steps 1 to 3 and, with ``compose``, the
    composed check through the composition checker's joint step."""
    return run(cli, ["loop", "decompose", "--contract", contract, "--architecture", architecture]
               + (["--compose"] if compose else []))


def gaps(cli: Path, contract: Path, laws: Path) -> tuple[int, dict]:
    """The specification-gap judge with proposed laws (``contract review --gaps --gaps-laws``)."""
    return run(cli, ["contract", "review", contract, "--gaps", "--gaps-laws", laws, "--format", "json"])


def evaluate_expression(cli: Path, original: Path, expressions: Path, values: list[int]) -> int:
    """Read one scalar through Rust's existing library evaluator, with no verdict or receipt."""
    code, report = run(cli, ["loop", "evaluate", "--original", original, "--expressions", expressions,
                             "--input=" + ",".join(str(v) for v in values)])
    result = (report.get("detail") or {}).get("values")
    if code != 0 or report.get("status") != "evaluated" or not isinstance(result, list) or len(result) != 1:
        raise Refusal("scenario-expression-refused-by-library")
    return N.integer(result[0])


def lemma(cli: Path, contract: Path, formula: str) -> tuple[int, dict]:
    """One state formula judged as a lemma (``contract lemma``)."""
    return run(cli, ["contract", "lemma", contract, "--lemma", formula, "--format", "json"])


def scenario(cli: Path, contract: Path, path: Path) -> tuple[int, dict]:
    """A scenario file replayed through the Authority (``contract scenario``)."""
    return run(cli, ["contract", "scenario", contract, "--file", path, "--format", "json"])


def probe(cli: Path, contract: Path, question: str, arguments: list) -> tuple[int, dict]:
    """One exact question about the contract (``contract probe``)."""
    return run(cli, ["contract", "probe", question, contract, *arguments, "--format", "json"])


def family(cli: Path, manifest: Path, program: Path, fit: bool = True) -> tuple[int, dict]:
    return run(cli, ["loop", "family", "--instances", manifest, "--program", program] + (["--fit"] if fit else []))


def transfer(cli: Path, source: Path, manifest: Path) -> tuple[int, dict]:
    return run(cli, ["loop", "transfer", "--from", source, "--to-instances", manifest])


def export_program(cli: Path, contract: Path, out: Path) -> dict:
    code, report = run(cli, ["contract", "export-program", contract, "--out", out, "--format", "json"])
    if code != 0 or not out.is_file():
        raise Refusal(f"contract export-program failed for {contract.name}")
    return report


def instantiate(family_name: str, parameters: dict, contract: Path) -> dict:
    """One core-component instance, by ZenoFCIS's own instantiation tool."""
    sys.path.insert(0, str(ROOT / "tools"))
    try:
        import instantiate_core
    finally:
        sys.path.pop(0)
    return instantiate_core.instantiate(family_name, parameters, contract, include_proof=False)


def family_instances(family_name: str) -> list:
    """The catalogue's declared parameter sets for one family."""
    path = ROOT / "core-components" / family_name / "family.json"
    if not path.is_file():
        raise Refusal(f"unknown family: {family_name}")
    return json.loads(path.read_text(encoding="ascii"))["instances"]


def instance_name(family_name: str, parameters: dict) -> str:
    return family_name + "-" + "-".join(f"{k.lower()}{v}" for k, v in sorted(parameters.items()))


def generate_family(cli: Path, family_name: str, directory: Path, vocabulary,
                    parameters: list | None = None, optimize: bool = True, fit: list | None = None) -> Path:
    """Instantiates and exports every listed instance into ``directory`` and writes its manifest.

    ``vocabulary`` maps the first instance's contract directory to the manifest's vocabulary block."""
    instances = []
    for params in parameters if parameters is not None else family_instances(family_name):
        name = instance_name(family_name, params)
        folder = directory / name
        contract = folder / "contract"
        folder.mkdir(parents=True, exist_ok=False)
        instantiate(family_name, params, contract)
        code, report = run(cli, ["generate", "contract", contract, "--format", "json"])
        if code != 0:
            raise Refusal(f"generate contract failed for {name}: {report.get('status')}")
        export_program(cli, contract, folder / "original.zcve")
        entry = {"name": name, "parameters": params, "program": f"{name}/original.zcve"}
        if optimize:
            code, report = run(cli, ["optimize", "--program", folder / "original.zcve", "--profile",
                                     "checked-i128-v1", "--candidate-out", folder / "optimized.zcve"])
            if code == 0 and (folder / "optimized.zcve").is_file():
                entry["optimize"] = f"{name}/optimized.zcve"
        if fit is not None:
            entry["fit"] = params in fit
        instances.append(entry)
    if not instances:
        raise Refusal("A family needs at least one instance")
    block = vocabulary(directory / instances[0]["name"] / "contract")
    return write_new(directory / "manifest.json", {"schema": MANIFEST_SCHEMA, "family": family_name,
                                                   "vocabulary": block, "instances": instances})
