#!/usr/bin/env python3
"""Check that the SQLite shell's misuse examples fail for their stated reason.

The shell's documentation pairs each `compile_fail,EXXXX` example with an
example that compiles. Stable rustdoc runs both, but it only checks that a
`compile_fail` example fails: it ignores the stated error code, so an example
that fails for another reason, such as a typo, still passes. This check closes
that gap on the pinned stable toolchain, without `RUSTC_BOOTSTRAP` or nightly:

1. It reads the examples from the crate's doc comments, the text rustdoc
   compiles, and requires the set of blocks it found to equal the set rustdoc
   lists (`cargo test --doc -- --list`), by file and fence line.
2. It builds the library (`cargo build --message-format=json`) and takes the
   library and dependency paths from cargo's JSON output and metadata.
3. It wraps each example as rustdoc does and runs `rustc --emit=metadata
   --error-format=json` on it, which type-checks and borrow-checks it.
4. From the JSON diagnostics, never the rendered text, it requires every
   misuse example to fail with exactly one error, carrying exactly its stated
   code, and every other example to compile with no error.

It also requires each misuse example to state exactly one code, to follow a
compiling example that it is paired with, and to be introduced by prose that
names the same code, so the documented reason, the fence and the compiler
agree.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = "zeno-fcis-shell-sqlite"
CRATE = "zeno_fcis_shell_sqlite"
TOOLCHAIN = "+1.97.1"
ERROR_CODE = re.compile(r"\bE\d{4}\b")
FENCE = re.compile(r"^(`{3,}|~{3,})(.*)$")
LISTED = re.compile(r"^(?P<file>\S+\.rs) - .+ \(line (?P<line>\d+)\): test$")
# Rustdoc's lang-string attributes. A fence that carries any other word, such
# as `text`, is not a Rust example unless it also says `rust`.
ATTRIBUTES = {"rust", "ignore", "should_panic", "no_run", "compile_fail", "test_harness",
              "standalone_crate", "edition2015", "edition2018", "edition2021", "edition2024"}
# Attributes this check does not model; an example that uses one is refused
# rather than checked on assumptions.
UNSUPPORTED = {"ignore", "test_harness", "edition2015", "edition2018", "edition2021",
               "edition2024"}


def stable() -> dict[str, str]:
    """The environment without `RUSTC_BOOTSTRAP`, so the stable toolchain
    cannot be asked for unstable behaviour while this check runs."""

    return {key: value for key, value in os.environ.items() if key != "RUSTC_BOOTSTRAP"}


class CheckError(Exception):
    """A misuse example, its pairing or its documentation is not as stated."""


@dataclass
class Block:
    """One Rust example from a doc comment, as rustdoc compiles it."""

    file: str
    line: int
    code: str
    expected: str | None  # the stated error code of a misuse example
    prose: str = ""  # the documentation between the previous block and this one
    pair: int | None = None  # the fence line of the compiling example it pairs with
    group: int = 0  # the doc comment it belongs to

    @property
    def name(self) -> str:
        return f"{self.file}:{self.line}"


@dataclass
class Environment:
    """What rustc needs to compile an example against the built library."""

    edition: str
    externs: dict[str, Path] = field(default_factory=dict)

    @property
    def search(self) -> list[Path]:
        """The folders of the crate's dependencies, where rustc also finds the
        indirect ones. Cargo reports the crate's own library as its uplifted
        copy in another folder; `--extern` names it by path, so that folder,
        which may hold unrelated uplifted libraries, is not searched."""

        return sorted({path.parent for name, path in self.externs.items() if name != CRATE})


def doc_comments(text: str) -> list[tuple[int, list[tuple[int, str]]]]:
    """Every run of consecutive `///` or `//!` lines, unindented by their
    common indentation as rustdoc does, with each line's number."""

    groups: list[tuple[int, list[tuple[int, str]]]] = []
    current: list[tuple[int, str]] = []
    marker = None
    for number, raw in enumerate(text.splitlines(), start=1):
        stripped = raw.lstrip()
        kind = "///" if stripped.startswith("///") and not stripped.startswith("////") else (
            "//!" if stripped.startswith("//!") else None)
        if kind is not None and (marker is None or kind == marker):
            current.append((number, stripped[3:]))
            marker = kind
            continue
        if current:
            groups.append((len(groups), current))
        current, marker = [], None
        if kind is not None:
            current.append((number, stripped[3:]))
            marker = kind
    if current:
        groups.append((len(groups), current))
    unindented = []
    for index, lines in groups:
        indents = [len(line) - len(line.lstrip()) for _, line in lines if line.strip()]
        cut = min(indents, default=0)
        unindented.append((index, [(number, line[cut:]) for number, line in lines]))
    return unindented


def parse_lang(info: str) -> tuple[bool, set[str], list[str]]:
    """Whether a fence is a Rust example, its attributes and its error codes."""

    words = [word for word in re.split(r"[\s,]+", info.strip()) if word]
    codes = [word for word in words if re.fullmatch(r"E\d{4}", word)]
    other = [word for word in words if word not in ATTRIBUTES and word not in codes]
    is_rust = "rust" in words or not other
    return is_rust, {word for word in words if word in ATTRIBUTES}, codes


def doc_line(line: str) -> str:
    """One example line as rustdoc compiles it: `# ` marks a hidden line."""

    trimmed = line.lstrip()
    if trimmed == "#":
        return ""
    if trimmed.startswith("##"):
        return line.replace("##", "#", 1)
    if trimmed.startswith("# "):
        return trimmed[2:]
    return line


def extract(text: str, file: str) -> list[Block]:
    """The Rust examples of one source file, in order, with their prose."""

    if re.search(r"/\*\*|/\*!|#!?\[doc\b", text):
        raise CheckError(f"{file}: only `///` and `//!` doc comments are supported")
    blocks: list[Block] = []
    for group, lines in doc_comments(text):
        prose: list[str] = []
        index = 0
        while index < len(lines):
            number, line = lines[index]
            fence = FENCE.match(line.strip())
            if fence is None:
                prose.append(line)
                index += 1
                continue
            ticks, info = fence.group(1), fence.group(2)
            body: list[str] = []
            index += 1
            while index < len(lines) and not lines[index][1].strip().startswith(ticks):
                body.append(lines[index][1])
                index += 1
            if index == len(lines):
                raise CheckError(f"{file}:{number}: unterminated code block")
            index += 1
            is_rust, attributes, codes = parse_lang(info)
            if not is_rust:
                prose = []
                continue
            if attributes & UNSUPPORTED:
                raise CheckError(f"{file}:{number}: unsupported example attributes "
                                 f"{sorted(attributes & UNSUPPORTED)}")
            failing = "compile_fail" in attributes
            if failing and len(codes) != 1:
                raise CheckError(f"{file}:{number}: a compile_fail example must state exactly "
                                 f"one error code, found {codes}")
            if not failing and codes:
                raise CheckError(f"{file}:{number}: error codes {codes} without compile_fail")
            blocks.append(Block(file, number, "\n".join(doc_line(b) for b in body) + "\n",
                                codes[0] if failing else None, "\n".join(prose), group=group))
            prose = []
    return blocks


def pair(blocks: list[Block]) -> None:
    """Each misuse example must follow its own compiling example, in the same
    doc comment, and its prose must name the same code and no other."""

    used: set[int] = set()
    for previous, block in zip([None, *blocks], blocks):
        if block.expected is None:
            continue
        named = sorted(set(ERROR_CODE.findall(block.prose)))
        if named != [block.expected]:
            raise CheckError(f"{block.name}: the prose names {named}, the fence states "
                             f"{block.expected}")
        if (previous is None or previous.expected is not None or previous.file != block.file
                or previous.group != block.group or previous.line in used):
            raise CheckError(f"{block.name}: a compile_fail example must directly follow its "
                             "own compiling example in the same doc comment")
        used.add(previous.line)
        block.pair = previous.line


def crate_blocks(root: Path, source: Path | None = None) -> list[Block]:
    """Every Rust example of the crate, or of one replacement file."""

    package = root / "crates" / PACKAGE
    if source is not None:
        files = [source]
    else:
        files = sorted((package / "src").rglob("*.rs"))
    blocks: list[Block] = []
    for path in files:
        name = path.relative_to(root).as_posix() if path.is_relative_to(root) else str(path)
        blocks.extend(extract(path.read_text(encoding="utf-8"), name))
    pair(blocks)
    return blocks


def cargo(*arguments: str, root: Path) -> str:
    completed = subprocess.run(("cargo", TOOLCHAIN, *arguments), cwd=root, check=True,
                               stdout=subprocess.PIPE, text=True, env=stable())
    return completed.stdout


def listed(root: Path) -> set[tuple[str, int]]:
    """The examples rustdoc runs, by file and fence line."""

    output = cargo("test", "-p", PACKAGE, "--doc", "--locked", "--", "--list", root=root)
    found = set()
    for line in output.splitlines():
        match = LISTED.match(line.strip())
        if match:
            found.add((match["file"], int(match["line"])))
    return found


def environment(root: Path) -> Environment:
    """Build the library and find it and its direct dependencies."""

    metadata = json.loads(cargo("metadata", "--format-version", "1", "--locked", root=root))
    [package] = [p for p in metadata["packages"] if p["name"] == PACKAGE
                 and Path(p["manifest_path"]).parent == root / "crates" / PACKAGE]
    [node] = [n for n in metadata["resolve"]["nodes"] if n["id"] == package["id"]]
    direct = {dep["pkg"]: dep["name"] for dep in node["deps"]
              if any(kind["kind"] is None for kind in dep["dep_kinds"])}
    direct[package["id"]] = CRATE
    output = cargo("build", "-p", PACKAGE, "--lib", "--locked", "--message-format=json",
                   root=root)
    externs: dict[str, Path] = {}
    for line in output.splitlines():
        message = json.loads(line)
        if message.get("reason") != "compiler-artifact" or message["package_id"] not in direct:
            continue
        kinds = set(message["target"]["kind"])
        if not kinds & {"lib", "rlib", "proc-macro"}:
            continue
        paths = [Path(f) for f in message["filenames"] if f.endswith((".rlib", ".so", ".dylib"))]
        if len(paths) != 1:
            raise CheckError(f"{message['package_id']}: expected one library, found {paths}")
        externs[direct[message["package_id"]]] = paths[0]
    missing = sorted(set(direct.values()) - set(externs))
    if missing:
        raise CheckError(f"cargo reported no library for {missing}")
    return Environment(package["edition"], externs)


def wrap(code: str) -> str:
    """The crate rustdoc makes of an example: crate attributes first, the
    crate under test linked, and the body inside `fn main` unless it has one."""

    lines = code.splitlines()
    attributes = [line for line in lines if line.startswith("#![")]
    body = "\n".join(line for line in lines if not line.startswith("#!["))
    prelude = ["#![allow(unused)]", *attributes]
    if CRATE in body and f"extern crate {CRATE}" not in body:
        prelude.append(f"#[allow(unused_extern_crates)] extern crate {CRATE};")
    if re.search(r"\bfn\s+main\s*\(", body):
        return "\n".join(prelude) + "\n" + body + "\n"
    return "\n".join(prelude) + "\nfn main() {\n" + body + "\n}\n"


@dataclass
class Compiled:
    """rustc's exit status and its JSON diagnostics for one example."""

    status: int
    diagnostics: list[dict]

    @property
    def errors(self) -> list[dict]:
        return [d for d in self.diagnostics if str(d.get("level", "")).startswith("error")]


def compile_block(block: Block, env: Environment, scratch: Path) -> Compiled:
    source = scratch / f"example_{block.line}.rs"
    source.write_text(wrap(block.code), encoding="utf-8")
    command = ["rustc", TOOLCHAIN, "--edition", env.edition, "--crate-type", "bin",
               "--crate-name", f"example_{block.line}", "--emit=metadata",
               "--error-format=json", "--out-dir", str(scratch)]
    for folder in env.search:
        command += ["-L", f"dependency={folder}"]
    for name, path in sorted(env.externs.items()):
        command += ["--extern", f"{name}={path}"]
    completed = subprocess.run([*command, str(source)], stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, text=True, env=stable())
    diagnostics = []
    for line in completed.stderr.splitlines():
        try:
            message = json.loads(line)
        except json.JSONDecodeError as error:
            raise CheckError(f"{block.name}: rustc wrote a line that is not JSON: "
                             f"{line[:200]!r}") from error
        diagnostics.append(message)
    return Compiled(completed.returncode, diagnostics)


def judge(block: Block, compiled: Compiled) -> str | None:
    """None when the example behaves as documented, else what is wrong."""

    errors = compiled.errors
    located = [e for e in errors if e.get("spans")]
    summary = [e for e in errors if not e.get("spans")]
    codes = [(e.get("code") or {}).get("code") for e in located]
    if block.expected is None:
        if compiled.status != 0 or errors:
            return f"does not compile: status {compiled.status}, error codes {codes}"
        return None
    if compiled.status == 0 and not errors:
        return f"compiles, but must fail with {block.expected}"
    if compiled.status != 1:
        return f"rustc exited with status {compiled.status}, not a compile error"
    if codes != [block.expected]:
        return f"must fail with exactly [{block.expected}], failed with {codes}"
    if len(summary) > 1 or any(e.get("code") for e in summary):
        return f"unexpected errors without a location: {len(summary)}"
    return None


def check(blocks: list[Block], env: Environment) -> list[tuple[Block, str | None]]:
    with tempfile.TemporaryDirectory(prefix="zeno-fcis-compile-fail-") as directory:
        return [(block, judge(block, compile_block(block, env, Path(directory))))
                for block in blocks]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.parse_args()
    try:
        blocks = crate_blocks(ROOT)
        rustdoc = listed(ROOT)
        ours = {(block.file, block.line) for block in blocks}
        if ours != rustdoc:
            raise CheckError(f"examples found here and by rustdoc differ: only here "
                             f"{sorted(ours - rustdoc)}, only rustdoc {sorted(rustdoc - ours)}")
        failing = [block for block in blocks if block.expected is not None]
        if not failing:
            raise CheckError("no compile_fail example found")
        results = check(blocks, environment(ROOT))
    except (CheckError, subprocess.CalledProcessError) as error:
        print(f"compile-fail: FAIL: {error}", file=sys.stderr)
        return 1
    problems = 0
    for block, problem in results:
        kind = f"fails with {block.expected}" if block.expected else "compiles"
        if block.pair is not None:
            kind += f", paired with line {block.pair}"
        print(f"compile-fail: {block.name}: {'FAIL: ' + problem if problem else kind}")
        problems += problem is not None
    if problems:
        print(f"compile-fail: FAIL ({problems} of {len(results)} examples)", file=sys.stderr)
        return 1
    print(f"compile-fail: PASS ({len(failing)} misuse examples, each with exactly its stated "
          f"error; {len(results) - len(failing)} compiling examples)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
