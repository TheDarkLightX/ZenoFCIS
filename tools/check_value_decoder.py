#!/usr/bin/env python3
"""Check the production value decoder, its refinement proof and six defects."""
from __future__ import annotations

import argparse
from dataclasses import dataclass
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import tempfile
import time
import zipfile

import check_verus as verifier

ROOT = Path(__file__).resolve().parents[1]
PROFILE = Path("verification/verus/value-decoder.json")
HARNESS = Path("verification/verus/value_decoder.rs")
SUBJECT = Path("crates/zeno-fcis-codec/src/lib.rs")
STD_CONTRACTS = Path("verification/verus/value_decoder_std.rs")
STD_TESTS = Path("verification/verus/value_decoder_std_tests.rs")
UNIT_SOURCES = (
    HARNESS, STD_CONTRACTS, STD_TESTS,
    Path("verification/verus/value_decoder_vectors.rs"),
    SUBJECT,
    Path("crates/zeno-fcis-codec/src/decoder_bytes.rs"),
    Path("crates/zeno-fcis-codec/src/decoder_spec.rs"),
    Path("crates/zeno-fcis-codec/src/decoder_induction.rs"),
    Path("crates/zeno-fcis-codec/src/decoder_refinement.rs"),
    Path("crates/zeno-fcis-value/src/lib.rs"),
    Path("crates/zeno-fcis-value/src/decoder_bytes.rs"),
    Path("crates/zeno-fcis-value/src/decoder_model.rs"),
    Path("crates/zeno-fcis-value/src/decoder_refinement.rs"),
)
SOURCES = (*UNIT_SOURCES, PROFILE, verifier.PIN, Path("tools/check_verus.py"),
           Path("tools/check_value_decoder.py"), Path("tools/test_check_value_decoder.py"),
           Path("verification/verus/value_decoder_differential.rs"),
           Path("tools/capture_value_decoder_differential.py"),
           Path("Cargo.toml"), Path("Cargo.lock"), Path("rust-toolchain.toml"))
FORBIDDEN = re.compile(
    r"\b(?:assume|admit)\s*\(|\bassume_specification\b|\bexternal_body\b|"
    r"verifier\s*::\s*external\b|\bexternal_type_specification\b|"
    r"\b(?:exec_allows_no_decreases_clause|decreases_by|assume_termination)\b")
SEMANTIC_TARGETS = {
    "accept_trailing_bytes": "decode_complete_value",
    "skip_map_key_order": "decode_entries",
    "skip_depth_check": "enter",
    "little_endian_u32": "take_blob",
}
NATIVE_TARGETS = {
    "native_early_return": "value_decoder::codec::decode_complete_value",
    "native_replacement_reader": "value_decoder::codec::Cursor::take_u8",
}


@dataclass(frozen=True)
class RustToken:
    text: str
    start: int
    end: int


@dataclass(frozen=True)
class SourceItem:
    name: str
    kind: str
    start: int
    end: int
    attributes: tuple


RUST_TOKEN = re.compile(
    r'\s+|//[^\n]*|/\*|(?:br|cr|r)(?P<hashes>\#*)"[\s\S]*?"(?P=hashes)'
    r'|(?:b|c)?"(?:\\[\s\S]|[^"\\])*"'
    r"|b?'(?:\\(?:u\{[0-9a-fA-F_]+\}|x[0-9a-fA-F]{2}|.)|[^'\\\n])'"
    r'|(?:r\#)?[^\W\d]\w*|[0-9][\w]*|::|[^\s]', re.UNICODE)


def rust_tokens(source: str) -> list[RustToken]:
    """Keep byte-independent character offsets; literals never act as syntax."""
    tokens, offset = [], 0
    while offset < len(source):
        match = RUST_TOKEN.match(source, offset)
        if match is None:
            raise ValueError(f"unrecognized Rust source at character {offset}")
        value, end = match[0], match.end()
        if value == "/*":
            depth = 1
            while depth:
                nested = re.search(r'/\*|\*/', source[end:])
                if nested is None:
                    raise ValueError("unterminated Rust block comment")
                depth += 1 if nested[0] == "/*" else -1
                end += nested.end()
        elif not value.isspace() and not value.startswith("//"):
            tokens.append(RustToken(value.removeprefix("r#") if not '"' in value else value,
                                    offset, end))
        offset = end
    return tokens


def delimiter_pairs(tokens: list[RustToken]) -> dict[int, int]:
    pairs, stack = {}, []
    for index, token in enumerate(tokens):
        if token.text in ("(", "[", "{"):
            stack.append(index)
        elif token.text in (")", "]", "}"):
            if not stack or tokens[stack[-1]].text != {")": "(", "]": "[", "}": "{"}[token.text]:
                raise ValueError("unbalanced Rust delimiters")
            start = stack.pop()
            pairs[start] = index
    if stack:
        raise ValueError("unclosed Rust delimiter")
    return pairs


def source_items(source: str, namespace: str) -> list[SourceItem]:
    """Map this harness's declarations, including includes and verus! groups.

    This is a lexical source guard, not Rust macro expansion or a compiler
    equivalence proof. Unmapped reported declarations fail closed below.
    """
    tokens = rust_tokens(source)
    pairs = delimiter_pairs(tokens)
    items = []

    def walk(first, last, scope, inherited=()):
        index, attributes = first, []
        while index < last:
            if tokens[index].text == "#":
                bracket = index + 1
                inner = tokens[bracket].text == "!"
                bracket += int(inner)
                if tokens[bracket].text != "[":
                    raise ValueError("unsupported Rust attribute")
                end = pairs[bracket]
                attribute = tuple(tokens[bracket + 1:end])
                if inner:
                    # Only conditional erasure on a container is inherited;
                    # crate no_std and feature annotations do not erase items.
                    if erasing_attribute(attribute):
                        inherited = (*inherited, attribute)
                else:
                    attributes.append(attribute)
                index = end + 1
                continue
            start, cursor = index, index
            while cursor < last and tokens[cursor].text not in ("{", ";"):
                if tokens[cursor].text in ("(", "["):
                    cursor = pairs[cursor]
                cursor += 1
            if cursor == last:
                break
            header = [token.text for token in tokens[start:cursor]]
            end = pairs[cursor] if tokens[cursor].text == "{" else cursor
            if "fn" in header and tokens[cursor].text == "{":
                # Verus contract expressions may themselves have braces (if,
                # match, forall). Continue through those to the actual body.
                item_starts = {"#", "}", "pub", "fn", "proof", "spec", "exec", "open", "closed",
                               "impl", "mod", "struct", "enum", "type", "const", "static", "use",
                               "extern", "unsafe", "async", "trait", "verus"}
                while end + 1 < last and tokens[end + 1].text not in item_starts:
                    following = end + 1
                    if following + 1 < last and tokens[following + 1].text in ("!", "::"):
                        break  # A following macro item.
                    cursor = end + 1
                    while cursor < last and tokens[cursor].text not in ("{", ";"):
                        if tokens[cursor].text in ("(", "["):
                            cursor = pairs[cursor]
                        cursor += 1
                    if cursor == last or tokens[cursor].text != "{":
                        raise ValueError(f"cannot locate complete function body in {scope}")
                    end = pairs[cursor]
            attrs = (*inherited, *attributes)
            kind = next((key for key in ("fn", "impl", "mod", "struct", "enum", "type", "const", "static")
                         if key in header), None)
            if kind in ("fn", "struct", "enum", "type", "const", "static"):
                name = header[header.index(kind) + 1]
                items.append(SourceItem(f"{scope}::{name}", kind, tokens[start].start,
                                        tokens[end].end, attrs))
            elif kind == "impl":
                words = header[header.index("impl") + 1:]
                if words and words[0] == "<":
                    depth, cut = 1, 1
                    while cut < len(words) and depth:
                        depth += (words[cut] == "<") - (words[cut] == ">")
                        cut += 1
                    words = words[cut:]
                if "for" in words:
                    words = words[words.index("for") + 1:]
                if not words or not re.fullmatch(r'[^\W\d]\w*', words[0]):
                    raise ValueError(f"unsupported impl in {scope}")
                walk(cursor + 1, end, f"{scope}::{words[0]}", attrs)
            elif kind == "mod":
                name = f"{scope}::{header[header.index('mod') + 1]}"
                items.append(SourceItem(name, kind, tokens[start].start,
                                        tokens[end].end, attrs))
                if tokens[cursor].text == "{":
                    walk(cursor + 1, end, name, attrs)
            elif header == ["verus", "!"]:
                walk(cursor + 1, end, scope, attrs)
            attributes = []
            index = end + 1

    walk(0, len(tokens), namespace)
    return items


def native_possible(predicate: tuple[RustToken, ...]) -> bool:
    """Conservative Boolean cfg evaluation: other cfg atoms may be true/false.

    Native has verus_keep_ghost=false. Unknown platform/features are never
    assumed false; only predicates forced false are safe proof-only guards.
    """
    return True in cfg_values(predicate)


def cfg_values(predicate) -> set[bool]:
    words = [token.text for token in predicate]
    if words in (["verus_keep_ghost"], ["false"]):
        return {False}
    if words == ["true"]:
        return {True}
    if len(words) >= 3 and words[0] in ("all", "any", "not") and words[1] == "(" and words[-1] == ")":
        parts, depth, start = [], 0, 2
        for index in range(2, len(words) - 1):
            depth += (words[index] == "(") - (words[index] == ")")
            if words[index] == "," and depth == 0:
                parts.append(predicate[start:index])
                start = index + 1
        if start < len(words) - 1:
            parts.append(predicate[start:-1])
        values = [cfg_values(part) for part in parts]
        if words[0] == "not" and len(values) == 1:
            return {not value for value in values[0]}
        if words[0] in ("all", "any"):
            result = {words[0] == "all"}
            for choices in values:
                result = {(a and b) if words[0] == "all" else (a or b)
                          for a in result for b in choices}
            return result
    return {False, True}


def risky_attribute(attribute) -> bool:
    if not attribute or attribute[0].text not in ("cfg", "cfg_attr"):
        return False
    if attribute[0].text == "cfg":
        return native_possible(attribute[2:-1])
    depth, split = 0, None
    for index in range(2, len(attribute) - 1):
        depth += (attribute[index].text == "(") - (attribute[index].text == ")")
        if attribute[index].text == "," and depth == 0:
            split = index
            break
    if split is None:
        return True
    return (native_possible(attribute[2:split])
            or any(token.text in ("cfg", "cfg_attr", "path") for token in attribute[split + 1:-1]))


def erasing_attribute(attribute) -> bool:
    """Type derives are legitimate; nested cfg can replace the type itself."""
    return bool(attribute and (attribute[0].text == "cfg"
                or attribute[0].text == "cfg_attr"
                and any(token.text in ("cfg", "cfg_attr") for token in attribute[2:])))


class NativeBindingRefused(RuntimeError):
    def __init__(self, path, line, function):
        self.path, self.line, self.function = path, line, function
        super().__init__(f"native configuration in verified source: {path}:{line}: {function}")


def audit_native_binding(root: Path, report: dict) -> list[dict]:
    """Bind every local func-details name to a production/harness declaration."""
    declarations, sources = [], {}
    for path in UNIT_SOURCES:
        if path == STD_TESTS:
            continue  # Native std samples are not in the verified harness.
        source = sources[path] = (root / path).read_text()
        if path.parts[0] == "crates":
            namespace = "value_decoder::" + ("codec" if path.parts[1] == "zeno-fcis-codec" else "native_value")
            if path.stem not in ("lib", "decoder_refinement"):
                namespace += "::" + path.stem
        else:
            namespace = {HARNESS: "value_decoder", STD_CONTRACTS: "value_decoder::std_contracts",
                         Path("verification/verus/value_decoder_vectors.rs"): "value_decoder::vectors"}[path]
        declarations.extend((path, item) for item in source_items(source, namespace))
    inventory = sorted(name for name in report["func-details"] if name.startswith("value_decoder::"))
    if not inventory:
        raise RuntimeError("no local verified function inventory")
    bindings = []
    for name in inventory:
        # This pinned Verus names associated constants by impl ordinal and
        # generates enum projections and Clone bodies at their type span.
        normalized = re.sub(r'::impl&%[0-9]+::', '::', name)
        owner, _, leaf = normalized.rpartition("::")
        candidates = [(path, item) for path, item in declarations if item.name == normalized
                      or ("::impl&%" in name and item.name.rpartition("::")[2] == leaf
                          and item.name.startswith(owner + "::"))
                      or (leaf == "clone" or leaf.startswith("arrow_")) and item.name == owner
                          and item.kind in ("struct", "enum")]
        if not candidates:
            raise RuntimeError(f"unmapped verified function: {name}")
        for path, item in candidates:
            source = sources[path]
            attributes = item.attributes if item.kind == "fn" else tuple(
                attribute for attribute in item.attributes if erasing_attribute(attribute))
            for parent_path, parent in declarations:
                if (parent.kind == "mod" and item.name.startswith(parent.name + "::")
                        and parent_path != path):
                    for attribute in parent.attributes:
                        if risky_attribute(attribute):
                            raise NativeBindingRefused(parent_path,
                                sources[parent_path].count("\n", 0, attribute[0].start) + 1, name)
            for attribute in attributes:
                if risky_attribute(attribute):
                    raise NativeBindingRefused(path, source.count("\n", 0, attribute[0].start) + 1, name)
            tokens = rust_tokens(source[item.start:item.end])
            pairs = delimiter_pairs(tokens)
            for index, token in enumerate(tokens):
                if token.text not in ("cfg", "cfg_attr"):
                    continue
                if index + 1 < len(tokens) and tokens[index + 1].text == "!":
                    raise NativeBindingRefused(path, source.count("\n", 0, item.start + token.start) + 1, name)
                if (index >= 2 and [t.text for t in tokens[index - 2:index]] == ["#", "["]
                        or index >= 3 and [t.text for t in tokens[index - 3:index]] == ["#", "!", "["]):
                    attribute = tokens[index:pairs[index - 1]]
                    if risky_attribute(attribute):
                        raise NativeBindingRefused(path, source.count("\n", 0, item.start + token.start) + 1, name)
            bindings.append({"function": name, "file": str(path), "kind": item.kind,
                             "start_line": source.count("\n", 0, item.start) + 1,
                             "end_line": source.count("\n", 0, item.end) + 1})
    # Types are not a separate func-details inventory. Their declarations and
    # method owners identify names which a native-only alternative may replace.
    verified_types = {item.name for _, item in declarations if item.kind in ("struct", "enum", "type")
                      and not any(risky_attribute(a) for a in item.attributes if erasing_attribute(a))}
    for path, item in declarations:
        if item.kind in ("struct", "enum", "type") and item.name in verified_types:
            for attribute in item.attributes:
                if erasing_attribute(attribute) and risky_attribute(attribute):
                    raise NativeBindingRefused(path, sources[path].count("\n", 0, attribute[0].start) + 1, item.name)
    return bindings


def snapshot(root: Path = ROOT) -> dict[str, str]:
    for path in SOURCES:
        if (root / path).is_symlink():
            raise RuntimeError(f"decoder evidence source is a symbolic link: {path}")
    return {str(path): verifier.digest(root / path) for path in SOURCES}


def audit_trust(root: Path, profile: dict) -> None:
    for path in UNIT_SOURCES:
        if path == STD_CONTRACTS:
            continue
        # Comments do not grant trust. Keeping them in this check also makes
        # an unexpected new trust annotation visible during review.
        source = (root / path).read_text()
        source = re.sub(r"//[^\n]*", "", source)
        if FORBIDDEN.search(source):
            raise RuntimeError(f"application proof contains a forbidden trust escape: {path}")
    source = (root / STD_CONTRACTS).read_text()
    if (verifier.digest(root / STD_CONTRACTS) != profile["std_contracts_sha256"]
            or len(re.findall(r"\bpub assume_specification\b", source)) != 5
            or source.count("#[verifier::external_body]") != 1
            or source.count("#[verifier::external_type_specification]") != 1
            or "pub struct ExUtf8Error(core::str::Utf8Error);" not in source
            or re.search(r"\b(?:assume|admit)\s*\(", source)):
        raise RuntimeError("additional standard-library contracts differ from the reviewed profile")
    harness = (root / HARNESS).read_text()
    for path in ("../../crates/zeno-fcis-value/src/lib.rs", "../../crates/zeno-fcis-codec/src/lib.rs"):
        if harness.count(f'#[path = "{path}"]') != 1:
            raise RuntimeError("decoder harness does not include the actual production files")


def replace_once(source: str, before: str, after: str) -> str:
    if source.count(before) != 1:
        raise ValueError(f"mutation anchor is not unique: {before}")
    return source.replace(before, after, 1)


def mutation_sources(source: str) -> dict[str, str]:
    trailing = replace_once(source,
        "let value = decode_value_inner(&mut cursor, state, depth)?;\n    if cursor.remaining() != 0 {",
        "let value = decode_value_inner(&mut cursor, state, depth)?;\n    if false {")
    # Both checks must change for this defect to accept an otherwise canonical
    # value followed by extra bytes. Removing only the first guard still refuses.
    trailing = replace_once(trailing, "if canonical.as_slice() != bytes {",
        "if canonical.as_slice() != &bytes[..cursor.offset] {")
    return {
        "accept_trailing_bytes": trailing,
        "skip_map_key_order": replace_once(source,
            "if match previous {\n            Some(value) => !bytes_before(value, encoded_key),\n"
            "            None => false,\n        } {",
            "if false {"),
        "skip_depth_check": replace_once(source,
            "if depth > self.limits.effective_max_depth() {", "if false {"),
        "little_endian_u32": replace_once(source, "let length = self.take_u32()?;",
            "let mut bytes = [0_u8; 4];\n        bytes.copy_from_slice(self.take(4)?);\n"
            "        let length = u32::from(bytes[0]) | (u32::from(bytes[1]) << 8)"
            " | (u32::from(bytes[2]) << 16) | (u32::from(bytes[3]) << 24);"),
        "native_early_return": replace_once(source,
            "    let mut cursor = Cursor::new(bytes);\n    let value = decode_value_inner",
            "    #[cfg(not(verus_keep_ghost))]\n    { return Err(DecodeError::NonCanonical); }\n"
            "    let mut cursor = Cursor::new(bytes);\n    let value = decode_value_inner"),
        "native_replacement_reader": replace_once(source,
            "    fn take_u8(&mut self) -> Result<u8, DecodeError> {",
            "    #[cfg(verus_keep_ghost)]\n"
            "    fn take_u8(&mut self) -> Result<u8, DecodeError> {"
        ) + "\n#[cfg(not(verus_keep_ghost))]\nimpl Cursor<'_> {\n"
            "    fn take_u8(&mut self) -> Result<u8, DecodeError> {\n"
            "        self.take(1)?;\n        Ok(0)\n    }\n}\n",
    }


def accepted(proc: subprocess.CompletedProcess, report: dict, pin: dict, profile: dict) -> bool:
    report_pin = {**pin, "expected_verified": profile["expected_verified"],
                  "target_functions": profile["target_functions"]}
    return (type(profile["expected_verified"]) is int and profile["expected_verified"] > 0
            and proc.returncode == 0 and verifier.accepted(report, report_pin)
            and verifier.proof_succeeded(report, pin)
            and not verifier.ERROR_HEADER.search(proc.stderr)
            and not verifier.NON_SEMANTIC.search(proc.stderr))


def classify_mutation(proc: subprocess.CompletedProcess, report: dict, name: str,
                      source: str, specimen: Path, pin: dict, profile: dict) -> bool:
    if name not in SEMANTIC_TARGETS:
        return False
    region = verifier.function_lines(source, SEMANTIC_TARGETS[name])
    return verifier.semantic_refusal(proc, report, pin, specimen / SUBJECT, ROOT,
        region=region, target_functions=profile["target_functions"])


def retained_run(command: list[str], directory: Path, environment: dict,
                 timeout: int = 900) -> subprocess.CompletedProcess:
    directory.mkdir(parents=True, exist_ok=True)
    started = time.monotonic()
    record = {"command": command, "cwd": str(ROOT), "timeout_seconds": timeout}
    try:
        result = verifier.run(command, ROOT, environment, timeout=timeout)
    except subprocess.TimeoutExpired as error:
        stdout, stderr = error.stdout or "", error.stderr or ""
        if isinstance(stdout, bytes):
            stdout = stdout.decode(errors="replace")
        if isinstance(stderr, bytes):
            stderr = stderr.decode(errors="replace")
        result = subprocess.CompletedProcess(command, None, stdout, stderr)
        record["timed_out"] = True
    record.update({"exit_code": result.returncode,
                   "elapsed_seconds": time.monotonic() - started})
    (directory / "command.json").write_text(json.dumps(record, indent=2) + "\n")
    (directory / "stdout.log").write_text(result.stdout)
    (directory / "stderr.log").write_text(result.stderr)
    return result


def parse_report(proc: subprocess.CompletedProcess) -> dict:
    try:
        result = json.loads(proc.stdout)
    except ValueError:
        return {}
    return result if isinstance(result, dict) else {}


def check(cache: Path, install: bool, directory: Path) -> dict:
    if (platform.system(), platform.machine()) != ("Linux", "x86_64"):
        raise RuntimeError("this decoder profile qualifies Linux x86-64")
    pin = json.loads((ROOT / verifier.PIN).read_text())
    profile = json.loads((ROOT / PROFILE).read_text())
    audit_trust(ROOT, profile)
    before = snapshot()
    (directory / "source_sha256.json").write_text(json.dumps(before, indent=2, sort_keys=True) + "\n")
    qualified = verifier.prepare_tools(cache, pin, install)
    environment = {key: value for key, value in os.environ.items()
                   if not key.startswith(("VERUS_", "VARGO_"))
                   and key not in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS")}
    environment["RUSTUP_TOOLCHAIN"] = pin["rust_toolchain"]
    environment["VERUS_Z3_PATH"] = str(qualified / "z3")
    # --no-cheating rejects the five explicitly permitted std contracts too.
    # The source audit admits only the exact reviewed std module; all native
    # bodies, termination, lifetime checking and erasure remain enabled.
    command = [str(qualified / "verus"), "--crate-type=lib", "--edition=2024",
               "--no-external-by-default", "--num-threads", "2", "--rlimit", str(profile["rlimit"]),
               "-V", "spinoff-all", "--output-json"]
    positive = retained_run([*command, str(HARNESS)], directory / "normal", environment)
    report = parse_report(positive)
    if not accepted(positive, report, pin, profile):
        raise RuntimeError("decoder proof lacks exact pinned tools, complete coverage or the expected obligation count")
    bindings = audit_native_binding(ROOT, report)
    (directory / "verified_source_spans.json").write_text(json.dumps(bindings, indent=2) + "\n")
    print("Value decoder: normal proof passed", flush=True)

    native_binary = directory / "std-contract-tests"
    native_build = retained_run(["rustc", f"+{pin['runtime_rust']}", "--edition=2024", "--test",
        str(STD_TESTS), "-o", str(native_binary)], directory / "std-build", environment)
    verifier.require_success(native_build)
    native = retained_run([str(native_binary)], directory / "std-samples", environment)
    verifier.require_success(native)

    controls = []
    for name, changed in mutation_sources((ROOT / SUBJECT).read_text()).items():
        specimen = directory / name / "source"
        for path in UNIT_SOURCES:
            target = specimen / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(changed.encode() if path == SUBJECT else (ROOT / path).read_bytes())
        mutant = retained_run([*command, str(specimen / HARNESS)], directory / name / "run", environment)
        parsed = parse_report(mutant)
        diagnostic = None
        if name in NATIVE_TARGETS:
            rejected = False
            if accepted(mutant, parsed, pin, profile):
                try:
                    audit_native_binding(specimen, parsed)
                except NativeBindingRefused as error:
                    diagnostic = {"file": str(error.path), "line": error.line,
                                  "function": error.function, "message": str(error)}
                    rejected = error.path == SUBJECT and error.function == NATIVE_TARGETS[name]
        else:
            rejected = classify_mutation(mutant, parsed, name, changed, specimen, pin, profile)
        control = {"name": name, "rejected_semantically": rejected,
                   "obligation_function": SEMANTIC_TARGETS.get(name, NATIVE_TARGETS.get(name)),
                   "exit_code": mutant.returncode,
                   "normal_sha256": before[str(SUBJECT)], "mutant_sha256": verifier.digest(specimen / SUBJECT),
                   "original_after_sha256": verifier.digest(ROOT / SUBJECT),
                   "verification_results": parsed.get("verification-results", {})}
        if name in NATIVE_TARGETS:
            del control["rejected_semantically"]
            control.update({"rejected_native_binding": rejected, "diagnostic": diagnostic})
        controls.append(control)
        (directory / "controls.json").write_text(json.dumps(controls, indent=2) + "\n")
        if not rejected:
            raise RuntimeError(f"decoder control {name} survived or failed for an unrelated reason")
        print(f"Value decoder: rejected {name} at "
              f"{diagnostic['message'] if diagnostic else SEMANTIC_TARGETS[name]}", flush=True)

    if before != snapshot():
        raise RuntimeError("decoder evidence sources changed during verification")
    restored = retained_run([*command, str(HARNESS)], directory / "restored", environment)
    restored_report = parse_report(restored)
    if not accepted(restored, restored_report, pin, profile):
        raise RuntimeError("unchanged production source failed the restored proof run")
    restored_bindings = audit_native_binding(ROOT, restored_report)
    if restored_bindings != bindings:
        raise RuntimeError("verified source spans changed during the restored run")
    verifier.verify_tool_files(qualified, pin)
    if before != snapshot():
        raise RuntimeError("decoder evidence sources changed during the restored run")
    return {"schema": "zeno-fcis/value-decoder-evidence/1", "status": "passed",
        "revision": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "working_tree_dirty": bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True).strip()),
        "source_sha256": before, "toolchain": pin, "verus_report": report,
        "verified_source_spans": bindings,
        "restored_verus_report": restored_report, "controls": controls,
        "std_native_exit_code": native.returncode,
        "trusted_base": profile["trusted_base"], "scope": profile["scope"],
        "unproved": profile["unproved"]}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--install", action="store_true")
    parser.add_argument("--cache", type=Path, default=verifier.cache_directory())
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    directory = Path(tempfile.mkdtemp(prefix="value-decoder-run-", dir=args.out.parent)).resolve()
    try:
        receipt = check(args.cache.resolve(), args.install, directory)
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError, zipfile.BadZipFile) as error:
        receipt = {"schema": "zeno-fcis/value-decoder-evidence/1", "status": "failed", "error": str(error)}
    receipt["logs"] = str(directory)
    args.out.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    print(f"Value decoder: {receipt['status']}; receipt: {args.out}")
    if receipt["status"] != "passed":
        print(receipt["error"])
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
