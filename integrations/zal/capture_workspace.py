"""Example-only capture workspace shell, LLM-generated. No Python judge.

The owner provisions the Rust session outside MCP. Only allowlisted Rust
packets and public feedback enter this workspace's paired transcript.
"""
import argparse
import json
import os
from pathlib import Path
import re
import secrets
import tempfile

from behavior import Refusal
import report_numbers as N
from shared import SharedWorkspace
import loop_doors as D
import loop_language as Z
from capture_language import LEGEND

SCHEMA = "zal/capture-workspace/1"
MAX_BYTES = 2 << 20


def context(abi, tag, revision):
    names = [f"x{i}" for i in range(len(abi["inputs"]))]
    return Z.Context({"tag": tag, "revision": revision, "names": names, "state": [],
                      "kinds": [d["kind"] for d in abi["inputs"]],
                      "outputs": [d["kind"] for d in abi["outputs"]], "variants": {},
                      "reasons": [], "roots": {n: "command" for n in names}})


class CaptureWorkspace:
    def __init__(self, path, cli=None):
        self.path = Path(path).absolute()
        self.cli_given = cli

    def load(self):
        if self.path.is_symlink() or not self.path.is_file() or self.path.stat().st_size > MAX_BYTES:
            raise Refusal("capture-workspace-unreadable")
        saved = json.loads(self.path.read_text(encoding="ascii"))
        if saved.get("schema") != SCHEMA or set(saved) != {"schema", "session", "tag", "revision", "abi", "events"}:
            raise Refusal("capture-workspace-format")
        if len(saved["events"]) > 512:
            raise Refusal("capture-transcript-bound")
        prev = None
        for i, event in enumerate(saved["events"]):
            body = {k: v for k, v in event.items() if k != "hash"}
            if event.get("prev") != prev or event.get("seq") != i or event.get("hash") != Z.digest(body):
                raise Refusal("capture-transcript-chain")
            prev = event["hash"]
        return saved

    def save(self, saved):
        body = json.dumps(saved, sort_keys=True, ensure_ascii=True, separators=(",", ":"))
        if len(body) > MAX_BYTES:
            raise Refusal("capture-workspace-bound")
        fd, temp = tempfile.mkstemp(prefix=".zal-capture-", dir=self.path.parent)
        try:
            with os.fdopen(fd, "w", encoding="ascii") as stream:
                stream.write(body + "\n")
                stream.flush()
                os.fsync(stream.fileno())
            os.replace(temp, self.path)
        finally:
            if os.path.exists(temp):
                os.unlink(temp)

    def cli(self):
        return D.resolve_cli(self.cli_given)

    def packet_data(self, saved):
        code, report = D.run(self.cli(), ["loop", "capture", "packet", "--session", saved["session"]])
        packet = report.get("detail", {}).get("packet")
        if code != 0 or report.get("status") != "packet" or not isinstance(packet, dict) or \
                set(packet) != {"profile", "abi", "round", "examples"} or packet["abi"] != saved["abi"]:
            raise Refusal("capture-packet-refused")
        return {**packet, "examples": [{**e, "input": list(N.integers(e["input"])),
                                        "output": list(N.integers(e["output"]))} for e in packet["examples"]]}

    def init(self, spec, reference, session, reference_format="python-function-v1"):
        # Owner-side entry, never reachable through the MCP tools.
        with SharedWorkspace(self.path).locked():
            if self.path.exists() or self.path.is_symlink():
                raise Refusal("capture-workspace-exists")
            code, report = D.run(self.cli(), ["loop", "capture", "open", "--spec", Path(spec).absolute(),
                "--reference", Path(reference).absolute(), "--session", Path(session).absolute(),
                "--reference-format", reference_format])
            detail = report.get("detail")
            packet = detail.get("packet") if isinstance(detail, dict) else None
            if code != 0 or report.get("status") != "opened" or not isinstance(packet, dict):
                reason = detail.get("reason") if report.get("status") == "refused" and isinstance(detail, dict) else None
                if not isinstance(reason, str) or re.fullmatch(r"[a-z][a-z0-9-]{0,95}", reason) is None:
                    reason = "capture-reference-refused"
                raise Refusal(reason)
            saved = {"schema": SCHEMA, "session": str(Path(session).absolute()), "tag": "capture",
                     "revision": secrets.token_hex(32), "abi": packet["abi"], "events": []}
            self.save(saved)
            return {"status": "initialized", "authority": "none"}

    def ctx(self, saved):
        return context(saved["abi"], saved["tag"], saved["revision"])

    def append(self, saved, doc):
        ctx = self.ctx(saved)
        english, symbolic = Z.render(doc, ctx, True), Z.render(doc, ctx, False)
        if Z.parse(english, ctx) != doc or Z.parse(symbolic, ctx) != doc:
            raise Refusal("capture-transcript-round-trip")
        event = {"seq": len(saved["events"]), "author": doc.author, "symbolic": symbolic,
                 "english": english, "prev": saved["events"][-1]["hash"] if saved["events"] else None}
        event["hash"] = Z.digest(event)
        saved["events"].append(event)
        return event

    def submit(self, source):
        with SharedWorkspace(self.path).locked():
            saved = self.load()
            ctx = self.ctx(saved)
            doc = Z.parse(source, ctx)
            if len(saved["events"]) + 19 > 512:
                raise Refusal("capture-transcript-bound")
            if doc.author != "model" or not doc.acts or any(a.kind != "capture" for a in doc.acts):
                raise Refusal("capture-model-channel-proposals-only")
            ids = {a.id for a in doc.acts}
            outputs = [a.get("output") for a in doc.acts]
            if len(ids) != 1 or len(set(outputs)) != len(outputs) or set(outputs) != set(ctx.outputs):
                raise Refusal("capture-one-complete-proposal-required")
            packet = self.packet_data(saved)
            by_output = {a.get("output"): a.get("expr") for a in doc.acts}
            expressions = {"schema": "zeno-fcis/capture-expressions/1", "outputs": [
                Z.print_symbolic(by_output[out]) for out in ctx.outputs]}
            # This is a public proposal file, never a reference/table path.
            fd, name = tempfile.mkstemp(prefix="zal-capture-proposal-", suffix=".json")
            try:
                with os.fdopen(fd, "w", encoding="ascii") as stream:
                    json.dump(expressions, stream, ensure_ascii=True)
                code, report = D.run(self.cli(), ["loop", "capture", "candidate", "--session", saved["session"],
                                                "--expressions", name])
            finally:
                os.unlink(name)
            detail = report.get("detail", {})
            status = report.get("status")
            i = next(iter(ids))
            replies = []
            if detail.get("precheck") == "examples-equivalent":
                replies.append(Z.act("examples-equivalent", i, examples=len(packet["examples"])))
            if status == "captured-exact":
                if code != 0 or detail.get("check", {}).get("verdict") != "equivalent" or \
                        detail.get("check", {}).get("replay") != "matched":
                    raise Refusal("capture-checker-reply-mismatch")
                replies.append(Z.act("captured-exact", i, tuples=detail["checked"]))
            elif status in {"refuted", "precheck-refuted", "candidate-refused", "inconclusive", "refused"}:
                replies.append(Z.act("capture-result", i, status=status))
            else:
                raise Refusal("capture-cli-reply")
            fresh = self.packet_data(saved)
            known = {tuple(e["input"]) for e in packet["examples"]}
            for e in fresh["examples"]:
                if tuple(e["input"]) not in known:
                    replies.append(Z.act("capture-example", i, input=tuple(e["input"]), output=tuple(e["output"])))
            events = [self.append(saved, doc)]
            for start in range(0, len(replies), Z.MAX_STATEMENTS):
                events.append(self.append(saved, Z.Document(ctx.subject_ref, "judge",
                                                           tuple(replies[start:start + Z.MAX_STATEMENTS]))))
            self.save(saved)
            return events

    def public_packet(self):
        saved = self.load()
        ctx = self.ctx(saved)
        headers = {surface: Z.render(Z.Document(ctx.subject_ref, "model", ()), ctx, surface == "english")
                   for surface in ("symbolic", "english")}
        return N.encode({"packet": self.packet_data(saved), "headers": headers, "integer_encoding": N.ENCODING, "authority": "none"})


def mcp_call(name, arguments):
    if name == "zal_capture_legend":
        return {"legend": LEGEND, "profile": Z.PROFILE, "authority": "none"}
    path = os.environ.get("ZAL_CAPTURE_WORKSPACE")
    if not path:
        raise Refusal("Set ZAL_CAPTURE_WORKSPACE in the MCP server environment")
    workspace = CaptureWorkspace(path)
    if name == "zal_capture_packet":
        return workspace.public_packet()
    if name == "zal_capture_read":
        saved = workspace.load()
        return {"events": saved["events"][-20:], "authority": "none",
                "workflow": "Only public examples are shown. Tool permission is not owner agreement."}
    if name == "zal_capture_submit":
        return {"replies": workspace.submit(arguments["source"]), "authority": "none"}
    if name == "zal_capture_translate":
        saved = workspace.load()
        ctx = workspace.ctx(saved)
        doc = Z.parse(arguments["source"], ctx)
        if doc.author != "model" or any(a.kind != "capture" for a in doc.acts):
            raise Refusal("capture-model-channel-proposals-only")
        return {"text": Z.render(doc, ctx, arguments["to"] == "english"), "authority": "none"}
    raise Refusal("Unknown capture tool")


def terminal(argv):
    parser = argparse.ArgumentParser(prog="terminal.py capture")
    sub = parser.add_subparsers(dest="command", required=True)
    init = sub.add_parser("init", help="owner-side executable reference provisioning")
    init.add_argument("--spec", type=Path, required=True)
    init.add_argument("--reference", type=Path, required=True)
    init.add_argument("--session", type=Path, required=True)
    init.add_argument("--reference-format", default="python-function-v1")
    show = sub.add_parser("show", help="public ABI and selected examples")
    for command in (init, show):
        command.add_argument("--workspace", type=Path, required=True)
        command.add_argument("--cli", type=Path)
    options = parser.parse_args(argv)
    try:
        workspace = CaptureWorkspace(options.workspace, options.cli)
        result = workspace.init(options.spec, options.reference, options.session, options.reference_format) \
            if options.command == "init" else workspace.public_packet()
        print(json.dumps(result, indent=1))
        return 0
    except (Refusal, OSError, ValueError) as error:
        detail_reason = str(error) if isinstance(error, Refusal) else "capture-setup-input-error"
        if re.fullmatch(r"[a-z][a-z0-9-]{0,95}", detail_reason) is None:
            detail_reason = "capture-setup-input-error"
        if detail_reason == "capture-containment-unavailable":
            message = "Capture cannot start because reference containment is unavailable. Use the systemd backend on a supported Linux host with a running systemd user manager, then retry."
        elif detail_reason == "capture-workspace-exists":
            message = "This capture workspace already exists. Choose another --workspace path before starting a new capture."
        elif detail_reason == "capture-reference-refused":
            message = "The CLI refused the capture specification or reference. Check those files, then run the command again."
        else:
            message = f"Capture setup was refused ({detail_reason}). Check the specification and reference files, then retry."
        print(json.dumps({"status": "refused", "reason": "capture-workspace-operation",
                          "detail_reason": detail_reason, "message": message, "authority": "none"}))
        return 1
