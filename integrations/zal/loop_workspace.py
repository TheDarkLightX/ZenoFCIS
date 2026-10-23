"""A shared ZAL loop workspace: one file that the owner, a model and the judge work on.

The workspace binds one subject by revision (the SHA-256 of its contract
text, its policy and the exported program's digest) or one family by name.
It holds a hash-chained transcript in which every event carries both ZAL
surfaces, the open questions for the owner, and the session of the bounded
loop whose checked incumbent is the workspace's incumbent. Next to it,
``<workspace>.view.json`` holds what a display needs, and
``<workspace>.d/`` holds the exported program, the loop session, the
submitted door documents and the family's generated instances.

Each operation takes the advisory lock of ``shared.SharedWorkspace``,
reloads the file, verifies the chain, runs the doors through the zeno-fcis
CLI (``loop_doors``), records the judge's replies and replaces the file
atomically. The lock is cooperation between clients of one user, not
authentication.

Channels carry authority by convention: a model proposes and probes through
MCP (``author model``); only the owner's terminal answers questions and
states rules (``author owner``); only the judge speaks for the judge. A tool
permission granted by a harness is not the owner's agreement, and nothing
here adopts a program or approves a requirement: adoption stays
``zeno-fcis contract adopt``.

Stage 1 of the loop language routes program, sketch and family acts to the
doors. Laws, invariants, scenarios, probes, rules and architectures are
parsed and recorded, and answered with "unavailable: needs contract judges".

This module is LLM-generated.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import sys
import tempfile

from behavior import Refusal
import report_numbers as N
import loop_doors as doors

# Composable program loops: shell routing only; no new MCP or owner path.
import loop_composable
# BEGIN unified typed strategy modes and existing owner answer gate.
import loop_strategy_modes
# END unified typed strategy modes.
import loop_judges as J
import loop_language as Z
import loop_versions as versions
import loop_learn as L
from shared import SharedWorkspace

SCHEMA = "zeno-fcis/zal-loop-workspace/1"
VIEW_SCHEMA = "zeno-fcis/zal-loop-view/2"
MAX_BYTES = 32 << 20
MAX_EVENTS = 4096
VIEW_EVENTS = 40
JUDGE_KINDS = ("accepted", "refuted", "region", "site", "completed", "unrealizable", "fact", "decomposed", "blame",
               "expected", "interface", "exact", "holds", "exception", "missing", "same")
ROUTED = ("law", "invariant", "scenario", "probe", "rule", "decompose")
ROOT_KINDS = ("state", "command", "context")

# Learn replies remain judge-only in EVERY workspace, including ordinary ones.
JUDGE_KINDS += L.JUDGE_KINDS


# --- the subject ----------------------------------------------------------------------------------------

def _zeno_lines(text: str):
    for raw in text.splitlines():
        line = raw.split("//")[0].strip()
        if line:
            yield line.rstrip(";").strip()


def fields_of(contract: Path) -> dict:
    """Names, kinds, ranges, variants, reasons and laws from ``project.zeno`` and ``v2/policy.json``.

    Names follow the program's input order: state fields, then command fields, then context fields
    (a root without fields is one input named after the root)."""
    zeno = (contract / "project.zeno").read_text(encoding="ascii")
    policy_text = (contract / "v2" / "policy.json").read_text(encoding="ascii")
    policy = json.loads(policy_text)
    types, fields, variants, reasons, laws = {}, [], {}, {}, []
    for line in _zeno_lines(zeno):
        parts = line.split()
        if parts[0] == "type" and len(parts) >= 4:
            entry = {"kind": parts[2], "name": parts[3]}
            m = re.fullmatch(r"type \d+ int \S+ in (-?\d+)\.\.=(-?\d+)", line)
            if m:
                entry["range"] = (int(m.group(1)), int(m.group(2)))
            types[int(parts[1])] = entry
        elif parts[0] == "field" and len(parts) == 5:
            fields.append((int(parts[1]), int(parts[2]), parts[3], int(parts[4])))
        elif parts[0] == "variant" and len(parts) >= 4:
            variants.setdefault(int(parts[2]), {})[parts[3]] = int(parts[1])
        elif parts[0] == "reason" and len(parts) >= 3:
            reasons[parts[2]] = int(parts[1])
        elif parts[0] == "law":
            m = re.fullmatch(r"law (\d+) (\S+) on (.+?) = (.+)", line)
            if m:
                laws.append({"id": int(m.group(1)), "name": m.group(2), "scopes": m.group(3), "text": m.group(4)})
    roots = {kind: tid for tid, entry in types.items() for kind in ROOT_KINDS if entry["kind"] == kind}
    names, state, kinds, ranges, field_variants, root_of, paths = [], [], [], [], {}, {}, {}

    def typed(name: str, tid: int):
        entry = types.get(tid)
        if entry is None:
            raise Refusal(f"Unknown field type for {name}")
        if entry["kind"] == "bool":
            return "Bool", (0, 1)
        if "range" in entry:
            return "Int", entry["range"]
        binding = policy.get("leaf_bindings", {}).get(str(tid))
        if binding == ["Bool"]:
            return "Bool", (0, 1)
        if type(binding) is list and len(binding) == 3 and binding[0] == "I128":
            return "Int", (N.integer(binding[1]), N.integer(binding[2]))
        table = variants.get(tid)
        if table:
            field_variants[name] = dict(table)
            return "Int", (min(table.values()), max(table.values()))
        raise Refusal(f"Field {name} has a type the loop language cannot describe")

    for root in ROOT_KINDS:
        tid = roots.get(root)
        if tid is None:
            continue
        mine = sorted(f for f in fields if f[1] == tid)
        if not mine:
            kind, span = typed(root, tid)
            names.append(root)
            kinds.append(kind)
            ranges.append(span)
            root_of[root] = root
            paths[f"{root}.{tid}"] = root
            continue
        for fid, _, name, ftype in mine:
            kind, span = typed(name, ftype)
            names.append(name)
            kinds.append(kind)
            ranges.append(span)
            root_of[name] = root
            paths[f"{'pre' if root == 'state' else root}.{tid}.{fid}"] = name
            if root == "state":
                state.append(name)
                paths[f"post.{tid}.{fid}"] = name
    return {"zeno": zeno, "policy": policy, "names": names, "state": state, "kinds": kinds,
            "ranges": [list(r) for r in ranges], "variants": field_variants,
            "reasons": sorted(reasons, key=reasons.get), "reason_ids": reasons,
            "roots": {n: root_of[n] for n in names if root_of[n] != "state"},
            "cases": policy.get("cases", []), "variables": policy.get("variables", {}), "paths": paths,
            "genesis": policy.get("genesis", {}), "laws": laws}


def describe(contract: Path, program: dict, tag: str, original_sha256: str, parameters: dict | None = None,
             family: str | None = None) -> dict:
    """The subject: ``fields_of`` checked against the exported program's input domains, plus its outputs
    and its revision (the SHA-256 of the contract text, the policy and the program's digest). A field
    whose kind or integer range differs from the program's input is refused."""
    fields = fields_of(contract)
    inputs = program["inputs"]
    if len(inputs) != len(fields["names"]):
        raise Refusal("The contract's fields do not match the exported program's inputs")
    for name, kind, span, domain in zip(fields["names"], fields["kinds"], fields["ranges"], inputs):
        if domain["kind"] != kind or (kind == "Int" and [int(domain["min"]), int(domain["max"])] != span):
            raise Refusal(f"Field {name} does not match the exported program's input domain")
    zeno, policy = fields.pop("zeno"), fields.pop("policy")
    revision = Z.digest({"tag": tag, "zeno": zeno, "policy": policy, "original_sha256": original_sha256})
    return {"tag": tag, "revision": revision, "family_subject": family, **fields,
            "outputs": [domain["kind"] for domain in program["outputs"]], "output_domains": program["outputs"],
            "parameters": dict(parameters or {}), "original_sha256": original_sha256}


def vocabulary(subject: dict) -> dict:
    """The vocabulary block of a ``zeno-fcis/loop-expressions/1`` document."""
    return {"inputs": subject["names"], "variants": subject["variants"], "cases": max(1, len(subject["cases"]))}


def from_program(program: dict, names: list, max_size: int = 4096) -> list:
    """Each output of a fixture-vocabulary program as one expression over input names."""
    memo, sizes = [], []
    ops = {"Add": "+", "Sub": "-", "Mul": "*", "Eq": "==", "Lt": "<", "And": "&&"}
    calls = {"DivFloor": "div_floor", "DivCeil": "div_ceil", "MulDivFloor": "mul_div_floor",
             "MulDivCeil": "mul_div_ceil", "ProductsGe": "products_ge"}
    for node in program["nodes"]:
        code, args = node[0], node[1:]
        if code == "Input":
            expr, size = ("var", names[args[0]]), 1
        elif code == "Int":
            expr, size = ("int", int(args[0])), 1
        elif code == "Bool":
            expr, size = ("int", int(bool(args[0]))), 1
        elif code == "Not":
            expr, size = ("not", memo[args[0]]), 1 + sizes[args[0]]
        elif code == "Select":
            expr, size = ("if", *(memo[a] for a in args)), 1 + sum(sizes[a] for a in args)
        elif code in ops:
            expr, size = ("bin", ops[code], memo[args[0]], memo[args[1]]), 1 + sizes[args[0]] + sizes[args[1]]
        elif code in calls:
            if len(args) not in Z.FUNCTIONS[calls[code]]:
                raise Refusal(f"Instruction arity {code}")
            expr, size = ("call", calls[code], tuple(memo[a] for a in args)), 1 + sum(sizes[a] for a in args)
        else:
            raise Refusal(f"Unknown instruction {code}")
        if size > max_size:
            raise Refusal("Program too shared to show as expressions")
        memo.append(expr)
        sizes.append(size)
    return [memo[r] for r in program["roots"]]


# --- the workspace ----------------------------------------------------------------------------------------

SUBJECT_FILES = ("project.zeno", "v2/policy.json")


def _subject_hashes(contract: Path) -> dict:
    out = {}
    for name in SUBJECT_FILES:
        path = contract / name
        if path.is_symlink() or not path.is_file():
            raise Refusal(f"the session's contract copy has no {name}")
        out[name] = hashlib.sha256(path.read_bytes()).hexdigest()
    return out


def _clip(text: str) -> str:
    text = "".join(ch if ch.isascii() and ch.isprintable() else " " for ch in str(text))
    text = text.replace("{", "(").replace("}", ")").replace('"', "'").replace("[", "(").replace("]", ")")
    allowed = set("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_ ,.;:=<>!?()+-/'*|%")
    text = "".join(ch if ch in allowed else " " for ch in text).strip(" .")
    return " ".join(text.split())[:400] or "-"


def _int_value(subject: dict, name: str, value) -> int:
    if isinstance(value, bool):
        return int(value)
    if type(value) is int:
        return N.integer(value)
    if isinstance(value, str):
        table = subject["variants"].get(name, {})
        if value in table:
            return table[value]
        if re.fullmatch(r"-?\d+", value):
            return N.integer(value)
    raise Refusal(f"Unreadable input value for {name}")



def _report_judgements(state: dict, channel: str, events: list) -> None:
    """Tell Zeno Commons, when it is configured, what the judge decided for this subject revision.

    Other agents then see who is looping on which subject and what was already accepted or refuted.
    Reporting never changes the workspace or its verdicts.
    """
    if not os.environ.get("ZENO_FCIS_COMMONS"):
        return
    hook = Path(__file__).resolve().parents[1] / "commons"
    if str(hook) not in sys.path:
        sys.path.insert(0, str(hook))
    try:
        from emit import emit
    except Exception:  # noqa: BLE001 - reporting is advisory
        return
    verdicts = []
    for event in events:
        act = event.get("act") if isinstance(event, dict) else None
        if event.get("author") == "judge" and isinstance(act, dict):
            verdicts.append(str(act.get("kind") or act.get("act") or next(iter(act), "judgement")))
    subject = state.get("subject") or {}
    emit("loop", "judged", str(subject.get("revision") or subject.get("tag") or "unknown-subject"),
         {"channel": channel, "tag": subject.get("tag"), "verdicts": verdicts[:64], "events": len(events)})

class LoopWorkspace:
    def __init__(self, path: str | os.PathLike, cli: str | os.PathLike | None = None):
        self.path = Path(path).absolute()
        self.cli_given = cli

    @property
    def view_path(self) -> Path:
        return self.path.with_name(self.path.name + ".view.json")

    @property
    def data(self) -> Path:
        return self.path.with_name(self.path.name + ".d")

    def locked(self):
        return SharedWorkspace(self.path).locked()

    # -- persistence ---------------------------------------------------------------------------------------
    def load(self) -> dict:
        if not self.path.is_file() or self.path.is_symlink():
            raise Refusal("No loop workspace here; run terminal.py loop init first")
        if self.path.stat().st_size > MAX_BYTES:
            raise Refusal("Loop workspace exceeds its size bound")
        saved = json.loads(self.path.read_text(encoding="ascii"))
        if not isinstance(saved, dict) or set(saved) != {"schema", "loop"} or saved["schema"] != SCHEMA:
            raise Refusal("Unknown loop workspace format")
        state = saved["loop"]
        prev = None
        for event in state["events"]:
            body = {k: v for k, v in event.items() if k != "hash"}
            if event.get("prev") != prev or event.get("hash") != Z.digest(body):
                raise Refusal(f"Loop workspace transcript chain is broken at event {event.get('seq')}")
            prev = event["hash"]
        if len(state["events"]) > MAX_EVENTS:
            raise Refusal("Loop workspace transcript exceeds its bound")
        if state.get("version_space"):
            versions.validate_snapshot(state)
        if state.get("learn"):
            L.validate_snapshot(state)
        return state

    def save(self, state: dict) -> None:
        body = json.dumps({"schema": SCHEMA, "loop": state}, ensure_ascii=True, sort_keys=True,
                          separators=(",", ":")) + "\n"
        if len(body) > MAX_BYTES:
            raise Refusal("Loop workspace exceeds its size bound")
        self._atomic(self.path, body)
        self._atomic(self.view_path, json.dumps(self.view(state), ensure_ascii=True, sort_keys=True, indent=1) + "\n")

    @staticmethod
    def _atomic(path: Path, text: str) -> None:
        descriptor, temporary = tempfile.mkstemp(prefix=".zal-loop-", dir=path.parent)
        try:
            with os.fdopen(descriptor, "w", encoding="ascii") as stream:
                stream.write(text)
                stream.flush()
                os.fsync(stream.fileno())
            os.replace(temporary, path)
        finally:
            if os.path.exists(temporary):
                os.unlink(temporary)

    def cli(self, state: dict | None = None):
        return doors.resolve_cli(self.cli_given or (state or {}).get("cli"))

    # -- lifecycle ---------------------------------------------------------------------------------------------
    def init_learn(self, specification: Path) -> dict:
        """Owner-declared vocabulary/examples, using the ordinary lock and transcript."""
        with self.locked():
            if self.path.exists() or self.data.exists():
                raise Refusal("Loop workspace already exists; existing work is preserved")
            return L.initialize(self, specification)

    def init(self, contract: Path | None = None, family: str | None = None, parameters: list | None = None,
             fit: list | None = None, specification: Path | None = None, version_limits: dict | None = None) -> dict:
        """A workspace for one contract, or for a family of core components (instances generated here)."""
        with self.locked():
            if self.path.exists() or self.data.exists():
                raise Refusal("Loop workspace already exists; existing work is preserved")
            if specification is not None:
                if contract is not None or family is not None:
                    raise Refusal("give exactly one of specification, contract or family")
                return versions.initialize(self, specification, version_limits or {})
            cli = doors.resolve_cli(self.cli_given)
            self.data.mkdir(parents=True)
            family_block = None
            if family:
                params = parameters if parameters is not None else doors.family_instances(family)
                manifest = doors.generate_family(cli, family, self.data / "family",
                                                 lambda contract: vocabulary(fields_of(contract)), params, fit=fit)
                listed = json.loads(manifest.read_text(encoding="ascii"))
                first_entry = listed["instances"][0]
                contract = self.data / "family" / first_entry["name"] / "contract"
                original = self.data / "family" / first_entry["program"]
                tag = first_entry["name"]
                contract = self._bind_subject(contract)
                family_block = {"name": family, "manifest": "family/manifest.json"}
                parameters_of = first_entry["parameters"]
            else:
                if contract is None:
                    raise Refusal("Give a contract directory or a family")
                source = Path(contract).absolute()
                instance = source / "core-instance.json"
                parameters_of = json.loads(instance.read_text())["parameters"] if instance.is_file() else {}
                # Every judge reads the session's own copy, never the source directory, which may change.
                contract = self._bind_subject(source)
                original = self.data / "original.zcve"
                report = doors.export_program(cli, contract, original)
                tag = re.sub(r"[^A-Za-z0-9_-]", "-", str(report.get("application") or source.name))[:64] or "core"
            session = self.data / "session"
            opened = doors.open_session(cli, original, session)
            view = opened["detail"]["view"]
            sha = opened["detail"]["request"]["original"]["sha256"]
            subject = describe(contract, view["original"]["program"], tag, sha, parameters_of, family)
            state = {"profile": Z.PROFILE, "subject": subject, "cli": str(cli), "session": "session",
                     "family": family_block, "contract": str(contract.relative_to(self.data)),
                     "subject_files": _subject_hashes(contract),
                     "incumbent": {"by": "original", "nodes": view["original"]["nodes"],
                                   "bytes": view["original"]["bytes"], "sha256": sha,
                                   "program": view["original"]["program"]},
                     "original": {"nodes": view["original"]["nodes"], "bytes": view["original"]["bytes"]},
                     "original_program": str(original.relative_to(self.data)) if original.is_relative_to(self.data)
                     else str(original),
                     "questions": {}, "refuted": {}, "meanings": {}, "next_question": 1, "next_document": 1,
                     "events": [], "authority": "none"}
            self.save(state)
            return self.view(state)

    # -- transcript -------------------------------------------------------------------------------------------
    def _record(self, state: dict, ctx: Z.Context, author: str, a: Z.Act, detail=None, reply_to=None) -> dict:
        events = state["events"]
        if len(events) >= MAX_EVENTS:
            raise Refusal("Loop workspace transcript is full")
        event = {"seq": len(events) + 1, "author": author, "act": a.data(),
                 "symbolic": Z.render_act(a, ctx, False), "english": Z.render_act(a, ctx, True),
                 "prev": events[-1]["hash"] if events else None}
        if reply_to is not None:
            event["reply_to"] = reply_to
        if detail is not None:
            event["detail"] = detail
        event["hash"] = Z.digest(event)
        events.append(event)
        return event

    def _reply(self, state, ctx, kind: str, id_: str, seq: int, detail=None, **fields) -> dict:
        return self._record(state, ctx, "judge", Z.act(kind, id_, **fields), detail, seq)

    def _fact(self, state, ctx, id_: str, text: str, seq: int, detail=None) -> dict:
        return self._reply(state, ctx, "fact", id_, seq, detail, text=_clip(text))

    # -- operations ---------------------------------------------------------------------------------------------
    def submit(self, source: str, channel: str) -> list:
        """Judges a ZAL document. ``channel`` is ``model`` (MCP) or ``owner`` (terminal)."""
        with self.locked():
            state = self.load()
            ctx = Z.Context(state["subject"])
            doc = Z.parse(source, ctx)
            if doc.author != channel:
                raise Refusal(f"author {doc.author} cannot submit on the {channel} channel")
            events = self._submit(state, ctx, doc)
            self.save(state)
            _report_judgements(state, channel, events)
            return events

    def _submit(self, state: dict, ctx: Z.Context, doc: Z.Document) -> list:
        if doc.author == "judge":
            raise Refusal("only the judge speaks for the judge")
        start = len(state["events"])
        groups, order, part_of = {}, [], {}
        for a in doc.acts:
            # Capture acts belong only to the separate capture workspace,
            # including when a caller supplies an already parsed document.
            if a.kind in {"capture", "captured-exact", "examples-equivalent", "capture-example", "capture-result"}:
                raise Refusal("capture statements require a capture workspace")
            if a.kind in JUDGE_KINDS + versions.JUDGE_KINDS:
                raise Refusal("judge statements cannot be submitted")
            if doc.author == "owner" and a.kind not in ("answer", "rule") + versions.OWNER_KINDS + L.OWNER_KINDS:
                raise Refusal("the owner channel carries answers and rules only")
            if doc.author == "model" and a.kind in ("answer",) + versions.OWNER_KINDS + L.OWNER_KINDS:
                raise Refusal("a model cannot answer for the owner")
            if a.kind in ("component", "case", "decompose"):
                if a.kind == "component":
                    part_of[a.id] = a.get("of")
                if a.kind == "case" and a.id not in part_of:
                    raise Refusal(f"case before its component: {a.id}")
                key = ("decompose", a.id if a.kind == "decompose" else part_of[a.id])
            # Composable program loops (one grouped operation per ID).
            elif a.kind in ("pipeline", "stages", "strategy", "explore", "mine"):
                family = "pipeline" if a.kind == "stages" else "strategy" if a.kind == "explore" else a.kind
                key = (family, a.id)
            elif a.kind in ("program", "family", "scenario", "step"):
                key = ("scenario" if a.kind == "step" else a.kind, a.id)
            else:
                key = (a.kind, a.id, len(order))
            if key not in groups:
                groups[key] = []
                order.append(key)
            groups[key].append(a)
        for key in order:
            acts = groups[key]
            seq = [self._record(state, ctx, doc.author, a)["seq"] for a in acts][-1]
            try:
                self._dispatch(state, ctx, key[0], acts, seq)
            except Refusal as error:
                self._fact(state, ctx, acts[0].id, f"refused: {error}", seq)
        return state["events"][start:]

    def _dispatch(self, state, ctx, kind: str, acts: list, seq: int) -> None:
        if state.get("version_space"):
            versions.handle(self, state, ctx, acts[0], seq)
            return
        if kind in versions.MODEL_KINDS + versions.OWNER_KINDS:
            raise Refusal("version-space-act-needs-requirements-workspace")
        # The learn mode cannot route ordinary program/contract decision acts.
        if state.get("learn") or kind in L.KINDS:
            L.handle(self, state, ctx, acts[0], seq)
            return
        if kind == "program":
            self._program(state, ctx, acts, seq)
        # Composable program loops retain the existing channel/revision gates.
        elif kind in ("pipeline", "strategy", "mine"):
            loop_composable.route(self, state, ctx, kind, acts, seq)
        # BEGIN unified typed strategy mode dispatch.
        elif kind == "strategy-run":
            loop_strategy_modes.route(self, state, ctx, acts, seq)
        # BEGIN public-example knowledge strategy dispatch.
        elif kind == "strategy-kb":
            import loop_strategy_knowledge
            loop_strategy_knowledge.route(self, state, ctx, acts, seq)
        # END knowledge strategy dispatch.
        # END unified typed strategy mode dispatch.
        elif kind == "family":
            self._family(state, ctx, acts, seq)
        elif kind == "ask":
            self._ask(state, ctx, acts[0], seq)
        elif kind == "answer":
            self._answer(state, ctx, acts[0], seq)
        elif kind in ROUTED:
            try:
                getattr(self, f"_{kind}")(state, ctx, acts, seq)
            except J.Unrouted as reason:
                self._fact(state, ctx, acts[0].id, f"unavailable: {reason}", seq)
        else:
            raise Refusal(f"no door for {kind}")

    def _document(self, state: dict, name: str, data: dict) -> Path:
        n = state["next_document"]
        state["next_document"] = n + 1
        return doors.write_new(self.data / "documents" / f"{n:04d}-{name}.json", data)

    @staticmethod
    def _outputs(ctx: Z.Context, acts: list) -> list:
        given = {}
        for a in acts:
            out = a.get("output")
            if out in given:
                raise Refusal(f"output {out} given twice")
            given[out] = a.get("expr")
        return [given.get(out, ("keep",)) for out in ctx.outputs]

    def _program(self, state, ctx, acts: list, seq: int) -> None:
        pid = acts[0].id
        outputs = self._outputs(ctx, acts)
        texts = [Z.print_symbolic(n) for n in outputs]
        key = Z.digest({"program": texts})
        if key in state["refuted"]:
            self._fact(state, ctx, pid, f"already refuted at event {state['refuted'][key]}; nothing new to judge", seq)
            return
        subject = state["subject"]
        sketch = any(Z.holes_of(n) for n in outputs)
        path = self._document(state, f"{'sketch' if sketch else 'program'}-{pid}",
                              doors.document(vocabulary(subject), texts, subject["parameters"]))
        meaning = None if sketch else self._meaning(state, path)
        earlier = state.setdefault("meanings", {}).get(meaning) if meaning else None
        if earlier is not None:
            # It means exactly what an earlier program meant: answered from
            # memory, naming that program and its verdict; no attempt spent.
            self._reply(state, ctx, "same", pid, seq, {"meaning": meaning, "earlier": earlier},
                        **{"as": earlier["id"]}, count=_tuples(subject), over="inputs", event=earlier["seq"],
                        verdict=earlier["verdict"])
            return
        session = self.data / state["session"]
        code, report = (doors.sketch if sketch else doors.candidate)(self.cli(state), session, path)
        detail = report.get("detail", {})
        door = detail.get("door") if isinstance(detail, dict) else None
        if not isinstance(door, dict):
            self._fact(state, ctx, pid, f"{report.get('status', 'refused')}: "
                       f"{detail.get('reason') or detail.get('stop_reason') or 'the session refused the attempt'}",
                       seq, {"report": report})
            return
        outcome = (detail.get("outcome") or {}).get("outcome")
        if sketch:
            self._sketch_reply(state, ctx, pid, key, door, detail, outcome, seq)
        else:
            self._expressions_reply(state, ctx, pid, key, door, detail, outcome, seq)
            if meaning and state["refuted"].get(key) == seq:
                state["meanings"].setdefault(meaning, {"id": pid, "seq": seq, "door": "program",
                                                       "verdict": "refuted"})
        self._refresh_incumbent(state, detail)

    def _meaning(self, state: dict, path: Path) -> str | None:
        """The program's meaning key (its observation on every declared input, by the ZenoFCIS evaluator), or
        None when it has none or means the original: correct programs are never answered from memory, since
        each one is judged for its cost."""
        original = self.data / state.get("original_program", "original.zcve")
        code, report = doors.meaning(self.cli(state), original, path)
        detail = report.get("detail") or {}
        if code != 1 or report.get("status") != "keyed" or detail.get("same_as_original") is not False:
            return None
        key = detail.get("key")
        return key if isinstance(key, str) and re.fullmatch(r"[0-9a-f]{64}", key) else None

    def _refresh_incumbent(self, state: dict, detail: dict) -> None:
        incumbent = detail.get("incumbent") or {}
        program = detail.get("incumbent_program")
        if incumbent.get("kind") == "checked-replacement" and isinstance(program, dict):
            state["incumbent"] = {"by": f"attempt {incumbent.get('attempt')}", "nodes": incumbent["cost"]["nodes"],
                                  "bytes": incumbent["cost"]["bytes"], "sha256": incumbent["sha256"],
                                  "program": program}

    def _accepted(self, state, ctx, pid, door, detail, outcome, seq) -> None:
        check = door.get("check") or {}
        if check.get("verdict") != "equivalent" or check.get("replay") != "matched":
            self._fact(state, ctx, pid, "the session check accepted it but the in-process check did not confirm it",
                       seq, {"door": door})
            return
        cost = (detail.get("outcome") or {}).get("cost") or door.get("lowered") or door.get("completion") or {}
        self._reply(state, ctx, "accepted", pid, seq, {"check": check, "outcome": detail.get("outcome")},
                    tuples=_tuples(state["subject"]), nodes=cost.get("nodes", 0))
        before = state["incumbent"]["nodes"]
        if outcome == "checked-improvement":
            self._fact(state, ctx, pid, f"new incumbent: {cost.get('nodes')} nodes (was {before})", seq)
        else:
            self._fact(state, ctx, pid, f"correct, but not smaller than the incumbent ({before} nodes)", seq)

    def _expressions_reply(self, state, ctx, pid, key, door, detail, outcome, seq) -> None:
        if door.get("lowering") == "refused":
            self._fact(state, ctx, pid, f"refused: {door.get('reason')}", seq, {"door": door})
            return
        if door.get("status") == "completed" and door.get("selection") == "duplicate":
            self._fact(state, ctx, pid, "completed: checked duplicate", seq, {"door": door})
            return
        if outcome == "checked-improvement" or str(outcome).startswith("equivalent"):
            self._accepted(state, ctx, pid, door, detail, outcome, seq)
            return
        witness = (detail.get("outcome") or {}).get("witness")
        if outcome != "different" or not isinstance(witness, dict):
            self._fact(state, ctx, pid, f"{outcome or 'not judged'}", seq, {"outcome": detail.get("outcome")})
            return
        state["refuted"][key] = seq
        subject = state["subject"]
        example = tuple((n, _int_value(subject, n, v)) for n, v in zip(subject["names"], witness["input"]))
        expected = witness["observations"]["original"].get("ok", [])
        got = witness["observations"]["candidate"]
        j = next((k for k, (a, b) in enumerate(zip(expected, got.get("ok", []))) if a != b), 0)
        self._reply(state, ctx, "refuted", pid, seq, {"witness": witness}, example=example, output=f"out{j}",
                    expected=int(expected[j]) if expected else 0,
                    got=int(got["ok"][j]) if "ok" in got else "failure")
        explanation = door.get("explanation") or {}
        region = explanation.get("region") or {}
        if region:
            where = Z.read(region["where"], ctx, False) if region.get("where") else None
            self._reply(state, ctx, "region", pid, seq, {"region": region}, count=region["tuples"], of=region["of"],
                        where=where)
        for output, part in (region.get("output_regions") or {}).items():
            text = f"{output} is wrong on {part['tuples']} of {part['of']} inputs"
            if part.get("where"):
                text += f" exactly where {part['where']}"
            self._fact(state, ctx, pid, text, seq, {"output": output, "output_region": part})
        for site in [s for s in explanation.get("sites", []) if s.get("replacement")][:3]:
            self._reply(state, ctx, "site", pid, seq, {"site": site}, output=site["output"],
                        before=Z.read(site["subexpression"], ctx, False),
                        after=Z.read(site["replacement"], ctx, False))
        repair = explanation.get("repair") or {}
        if repair.get("label") == "checked repair":
            self._fact(state, ctx, pid, f"checked repair: {repair['program']['nodes']} nodes, accepted by transform "
                       "check and its receipt replayed; submit it as a program to adopt it", seq, {"repair": repair})
        elif repair:
            self._fact(state, ctx, pid, f"no checked repair ({repair.get('label') or repair.get('proposed')})", seq,
                       {"repair": repair})

    def _sketch_reply(self, state, ctx, pid, key, door, detail, outcome, seq) -> None:
        status = door.get("status")
        if status == "completed":
            holes = tuple((int(k), Z.read(v["fill"], ctx, False)) for k, v in sorted(door["holes"].items(),
                                                                                   key=lambda kv: int(kv[0])))
            self._reply(state, ctx, "completed", pid, seq, {"door": door}, holes=holes)
            self._accepted(state, ctx, pid, door, detail, outcome, seq)
            return
        evidence = door.get("evidence") or {}
        if status == "unrealizable" and evidence.get("kind") == "witness":
            subject = state["subject"]
            example = tuple((n, _int_value(subject, n, evidence["input"][n])) for n in subject["names"])
            self._reply(state, ctx, "unrealizable", pid, seq, {"door": door}, output=evidence["output"],
                        example=example)
            state["refuted"][key] = seq
            return
        if status == "unrealizable":
            self._fact(state, ctx, pid, "unrealizable: every assignment of a complete stated candidate space was "
                       "exhausted", seq, {"door": door})
            state["refuted"][key] = seq
            return
        self._fact(state, ctx, pid, f"sketch {status}: {door.get('reason') or 'no completion'}", seq, {"door": door})

    def _family(self, state, ctx, acts: list, seq: int) -> None:
        if not state.get("family"):
            raise Refusal("family programs need a family workspace")
        fid = acts[0].id
        outputs = self._outputs(ctx, acts)
        path = self._document(state, f"family-{fid}", doors.document(None, [Z.print_symbolic(n) for n in outputs]))
        manifest = self.data / state["family"]["manifest"]
        code, report = doors.family(self.cli(state), manifest, path, fit=True)
        detail = report.get("detail", {})
        if report.get("door") != "family" or "rows" not in detail:
            self._fact(state, ctx, fid, f"refused: {detail.get('reason')}", seq, {"report": report})
            return
        self._fact(state, ctx, fid, f"{detail['accepted']} of {detail['instances']} instances accepted, "
                   f"{detail['coverage']}; {detail['nodes']} nodes against {detail['optimize_nodes']} for optimize",
                   seq, {"family": detail})
        if detail.get("smallest_failing"):
            self._fact(state, ctx, fid, f"smallest failing instance: {detail['smallest_failing']['instance']}", seq)
        for k, form in sorted((detail.get("fits") or {}).items(), key=lambda kv: int(kv[0])):
            text = form["text"].replace("$", "parameter ") if form else "no linear form"
            self._fact(state, ctx, fid, f"hole {k} fits {text}", seq, {"fit": form})
        lifted = detail.get("lifted_check")
        if isinstance(lifted, dict) and "accepted" in lifted:
            unfitted = lifted.get("unfitted_instances_checked", 0)
            self._fact(state, ctx, fid, f"the fitted program is accepted on {lifted['accepted']} of "
                       f"{lifted['instances']} instances; {unfitted} of the checked instances were not fitted on, "
                       f"and it is accepted on {lifted.get('unfitted_instances_accepted', 0)} of those", seq)

    def _ask(self, state, ctx, a: Z.Act, seq: int) -> None:
        # A question ID is never reused: asking again must not overwrite an
        # owner's recorded answer.
        if a.id in state["questions"]:
            raise Refusal(f"question {a.id} already exists")
        state["questions"][a.id] = {"status": "open", "asked_by": "model",
                                    "options": [o.data() for o in a.get("options")]}
        self._fact(state, ctx, a.id, "question passed to the owner", seq)

    def _answer(self, state, ctx, a: Z.Act, seq: int) -> None:
        q = state["questions"].get(a.id)
        if q is None or q["status"] != "open":
            raise Refusal(f"no open question {a.id}")
        choice = a.get("choice")
        # BEGIN reading confirmations through the existing owner gate.
        if "strategy_reading" in q:
            loop_strategy_modes.confirm(self, state, ctx, a, seq)
            return
        # END reading confirmations through the existing owner gate.
        if isinstance(choice, Z.Option) and choice.data() not in q["options"]:
            raise Refusal("the answer is not an offered option")
        if q["asked_by"] == "judge" and isinstance(choice, Z.Option):
            self._confirm(state, ctx, a.id, q, choice, seq)
            return
        q["status"] = "answered"
        q["answer"] = choice if isinstance(choice, str) else choice.data()
        text = {"any": "any option", "none": "none of the options"}.get(choice) if isinstance(choice, str) else \
            Z.option_symbolic(choice, ctx)
        self._fact(state, ctx, a.id, f"answer recorded: {text}", seq)

    # -- routed to the contract judges -------------------------------------------------------------------------
    def _bind_subject(self, source: Path) -> Path:
        """A read-only copy of the contract's ``project.zeno`` and ``v2/policy.json`` in the workspace: the
        revision, the original program and every routed judge read these bytes."""
        copy = self.data / "subject"
        for name in SUBJECT_FILES:
            data = (source / name).read_bytes()
            if len(data) > MAX_BYTES:
                raise Refusal(f"The contract's {name} exceeds the workspace's size bound")
            target = copy / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
            target.chmod(0o444)
        return copy

    def _contract(self, state: dict) -> Path:
        """The session's bound contract copy, checked against the hashes recorded at initialization: a changed
        copy is refused, so no judge reads a contract other than the one the revision names."""
        contract, recorded = state.get("contract"), state.get("subject_files")
        if not contract or not recorded:
            raise J.Unrouted("the workspace has no bound contract copy; initialize a new workspace")
        path = self.data / contract
        if Path(contract).is_absolute() or not path.resolve().is_relative_to(self.data.resolve()):
            raise Refusal("the workspace's contract copy must lie inside the workspace")
        if _subject_hashes(path) != recorded:
            raise Refusal("the session's contract copy changed since initialization; initialize a new workspace "
                          "for the changed contract")
        return path

    def _subject_key(self, state: dict, meaning: str) -> str:
        """A judge's meaning key bound to the subject: its revision and the bound policy's hash."""
        return Z.digest({"revision": state["subject"]["revision"],
                         "policy": state["subject_files"]["v2/policy.json"], "meaning": meaning})

    def _answers_document(self, state: dict, extra: list | None = None) -> Path | None:
        answers = list(state.get("confirmed", [])) + list(extra or [])
        if not answers:
            return None
        return self._document(state, "answers", {"schema": "zeno-fcis/loop-answers/1", "answers": answers})

    def _remembered(self, state, ctx, id_: str, meaning: str | None, seq: int, count: int, over: str) -> bool:
        earlier = state.setdefault("meanings", {}).get(meaning) if meaning else None
        if earlier is None:
            return False
        self._reply(state, ctx, "same", id_, seq, {"meaning": meaning, "earlier": earlier}, **{"as": earlier["id"]},
                    count=count, over=over, event=earlier["seq"], verdict=earlier["verdict"])
        return True

    def _remember(self, state: dict, meaning: str | None, id_: str, seq: int, door: str, verdict: str) -> None:
        if meaning:
            state.setdefault("meanings", {}).setdefault(meaning, {"id": id_, "seq": seq, "door": door,
                                                                  "verdict": verdict})

    def _law(self, state, ctx, acts: list, seq: int) -> None:
        a = acts[0]
        subject = state["subject"]
        scope = J.law_scope(a.get("scope"))
        formula = J.law_formula(a.get("formula"), subject)
        proposed = state.setdefault("proposed", {})
        law_id = 8000 + len(proposed)
        line = f"law {law_id} proposed_{a.id} on {scope} = {formula};"
        path = self._document(state, f"law-{a.id}", {"law": line})
        laws = path.with_suffix(".zeno")
        laws.write_text(line + "\n", encoding="ascii")
        code, report = doors.gaps(self.cli(state), self._contract(state), laws)
        g = report.get("gaps") or {}
        if g.get("status") == "proposal-false-alarm":
            alarm = g.get("false_alarm") or {}
            proposed[a.id] = {"status": "false-alarm", "law": line}
            point = self._point(subject, alarm.get("input"))
            self._fact(state, ctx, a.id, "false alarm: the core decides against it" +
                       (f" when {point}" if point else ""), seq, {"law": line, "false_alarm": alarm})
            return
        if g.get("status") != "measured":
            raise J.Unrouted(f"the gaps judge did not measure it ({g.get('status') or report.get('status')}: "
                             f"{g.get('reason') or ''})")
        law = ((g.get("proposals") or {}).get("laws") or [{}])[0]
        applies = law.get("applies_on", 0)
        killed = len((g.get("proposals") or {}).get("now_caught") or [])
        status = "holds" if applies else "inactive"
        proposed[a.id] = {"status": status, "law": line, "applies_on": applies, "now_caught": killed}
        self._fact(state, ctx, a.id, f"{status}: applies on {applies} inputs and rules out {killed} surviving "
                   "mutants; pending the owner's review", seq, {"law": line, "proposals": g.get("proposals")})

    @staticmethod
    def _point(subject: dict, values) -> str:
        if not isinstance(values, list) or len(values) != len(subject["names"]):
            return ""
        parts = []
        for name, value in zip(subject["names"], values):
            kind = dict(zip(subject["names"], subject["kinds"]))[name]
            table = {v: n for n, v in subject["variants"].get(name, {}).items()}
            shown = ("true" if value else "false") if kind == "Bool" else table.get(int(value), value)
            parts.append(f"{name} = {shown}")
        return ", ".join(parts)

    def _state_point(self, subject: dict, values) -> str:
        names = subject["state"]
        if not isinstance(values, list) or len(values) != len(names):
            return ""
        kinds = dict(zip(subject["names"], subject["kinds"]))
        return ", ".join(f"{n} = {('true' if v else 'false') if kinds[n] == 'Bool' else v}"
                         for n, v in zip(names, values))

    def _request_point(self, subject: dict, values) -> str:
        names = [n for n in subject["names"] if n not in subject["state"]]
        if not isinstance(values, list) or len(values) != len(names):
            return ""
        kinds = dict(zip(subject["names"], subject["kinds"]))
        out = []
        for n, v in zip(names, values):
            table = {value: name for name, value in subject["variants"].get(n, {}).items()}
            out.append(f"{n} = {('true' if v else 'false') if kinds[n] == 'Bool' else table.get(int(v), v)}")
        return ", ".join(out)

    def _invariant(self, state, ctx, acts: list, seq: int) -> None:
        a = acts[0]
        subject = state["subject"]
        formula = J.state_formula(a.get("formula"), subject)
        code, report = doors.lemma(self.cli(state), self._contract(state), formula)
        verdicts = report.get("lemmas") or []
        if not verdicts:
            reason = (report.get("detail") or {}).get("reason") or report.get("reason") or report.get("status")
            self._fact(state, ctx, a.id, f"refused: {reason}", seq, {"report": report})
            return
        v = verdicts[0]
        # The admitted states alone do not decide induction: the key binds the subject's revision and policy.
        meaning = f"invariant:{self._subject_key(state, v['holds_sha256'])}" if v.get("holds_sha256") else None
        if self._remembered(state, ctx, a.id, meaning, seq, v["states"]["declared"], "states"):
            return
        verdict = v["verdict"]
        feedback = {"verdict": verdict, "formula": formula,
                    "scope": {"declared_states": v["states"]["declared"], "admitted": v["states"]["admitted"],
                              "reachable": v["states"]["reachable"], "exhaustive": True}}
        if verdict == "inductive":
            text = (f"inductive: holds on all {v['states']['reachable']} reachable states; admits "
                    f"{v['states']['admitted']} of {v['states']['declared']} declared"
                    + (", exactly the reachable ones" if v.get("exact") else "")
                    + f"; checked exhaustively over the {v['states']['declared']} declared states")
        elif "counterexample_to_induction" in v:
            cti = v["counterexample_to_induction"]
            decision = self._decision_at(state, cti["state"] + cti["input"])
            feedback["counterexample_to_induction"] = {**cti, "decision": decision}
            step = (f"from {self._state_point(subject, cti['state'])} on {self._request_point(subject, cti['input'])} "
                    f"the core decides {decision or 'a commit'} and reaches {self._state_point(subject, cti['successor'])}")
            if verdict == "false":
                feedback["trace_from_genesis"] = v.get("trace_from_genesis")
                text = f"false: a reachable state steps outside it: {step}"
            else:
                text = f"not inductive: {step}, outside it; that state is unreachable: strengthen the invariant"
        else:
            feedback["genesis"] = v.get("genesis")
            text = f"false: genesis violates it, at {self._state_point(subject, v.get('genesis'))}"
        state.setdefault("invariants", {})[a.id] = feedback
        self._remember(state, meaning, a.id, seq, "invariant", verdict)
        self._fact(state, ctx, a.id, text, seq, {"lemma": v, "feedback": feedback})

    def _decision_at(self, state: dict, values: list) -> str | None:
        code, report = doors.probe(self.cli(state), self._contract(state), "explain",
                                   ["--input", " ".join(str(int(x)) for x in values)])
        authority = ((report.get("result") or {}).get("authority")) or {}
        cls = authority.get("class")
        if not cls:
            return None
        reasons = {v: k for k, v in state["subject"]["reason_ids"].items()}
        reason = reasons.get(authority.get("reason"))
        return {"Accept": "accept", "Reject": f"reject because {reason}",
                "CommittedFailure": f"fail and commit because {reason}"}.get(cls, cls)

    def _scenario(self, state, ctx, acts: list, seq: int) -> None:
        head = next((a for a in acts if a.kind == "scenario"), None)
        steps = sorted((a for a in acts if a.kind == "step"), key=lambda a: a.get("n"))
        if head is None:
            raise Refusal("a step needs its scenario")
        if [s.get("n") for s in steps] != list(range(1, len(steps) + 1)):
            raise Refusal("scenario-steps-must-be-numbered-from-1")
        def evaluate_expression(node, env):
            document = doors.document(vocabulary(state["subject"]), [Z.print_symbolic(node)])
            path = self._document(state, f"scenario-value-{head.id}", document)
            original = self.data / state.get("original_program", "original.zcve")
            return doors.evaluate_expression(self.cli(state), original, path, [env[n] for n in ctx.names])
        text = J.scenario_text(head.id, head.get("start"), steps, state["subject"], evaluate_expression)
        path = self._document(state, f"scenario-{head.id}", {"scenario": text})
        file = path.with_suffix(".txt")
        file.write_text(text, encoding="ascii")
        code, report = doors.scenario(self.cli(state), self._contract(state), file)
        runs = report.get("scenarios") or []
        if not runs:
            self._fact(state, ctx, head.id, f"refused: {(report.get('detail') or {}).get('reason') or report.get('status')}",
                       seq, {"report": report})
            return
        run = runs[0]
        if run["verdict"] == "passed":
            text = f"passed: all {run['steps']} steps match the core"
            mutants = report.get("mutants") or {}
            if "killed" in mutants:
                text += f"; it kills {mutants['killed']} of {mutants.get('count')} mutants"
            everything = report.get("reject_everything") or {}
            if everything.get("status") == "bound" and everything.get("killed"):
                text += "; the reject-everything core fails it"
        else:
            failure = run.get("failure") or {}
            text = f"step {failure.get('step')} differs: {failure.get('reason')}"
            if failure.get("case") is not None:
                text += f" (case {failure['case']})"
        self._fact(state, ctx, head.id, text, seq, {"scenario": run, "mutants": report.get("mutants"),
                                                     "reject_everything": report.get("reject_everything")})

    def _probe(self, state, ctx, acts: list, seq: int) -> None:
        a = acts[0]
        subject = state["subject"]
        query, args, where = a.get("query"), a.get("args"), a.get("where")
        if query == "freedom":
            self._freedom(state, ctx, a.id, seq)
            return
        outputs = {f"out{j}" for j in range(len(subject["outputs"]))}

        def formula(node):
            if Z.names_of(node) & outputs:
                raise J.Unrouted("the contract probes read inputs and decisions, not program outputs")
            return J.plain_formula(node, subject)

        if query == "explain":
            values = [J.number(subject, n, v) for n, v in args]
            arguments = ["--input", " ".join(str(v) for v in values)]
        elif query == "reachable":
            arguments = ["--state", J.state_formula(args[0], subject)]
        elif query == "count":
            arguments = ["--where", formula(args[0])]
        elif query == "implies":
            arguments = ["--if", formula(args[0]), "--then", formula(args[1])]
        elif query == "depends":
            field = args[1] if len(args) > 1 else None
            path = J.plain_paths(subject).get(field)
            if path is None:
                raise J.Unrouted(f"no contract position for {field}")
            arguments = ["--position", path] + (["--where", formula(where)] if where is not None else [])
            query = "depends"
        else:
            raise J.Unrouted(f"no contract probe answers {query}")
        code, report = doors.probe(self.cli(state), self._contract(state), query, arguments)
        result = report.get("result")
        if not isinstance(result, dict):
            self._fact(state, ctx, a.id, f"refused: {(report.get('detail') or {}).get('reason') or report.get('status')}",
                       seq, {"report": report})
            return
        answer = result.get("answer")
        if query == "explain":
            authority = result.get("authority") or {}
            text = f"case {result.get('case')} fires; the core decides {authority.get('class')}"
            if authority.get("reason") is not None:
                reasons = {v: k for k, v in subject["reason_ids"].items()}
                text += f" because {reasons.get(authority['reason'], authority['reason'])}"
        elif query == "depends":
            text = ("the decision depends on it" if answer == "yes" else
                    f"the decision does not depend on it: {result.get('proof', answer)}")
        elif query == "count":
            text = f"{answer} of {result.get('of')} inputs satisfy it"
        else:
            text = f"{answer}" + (f": {result.get('proof')}" if result.get("proof") else "")
        self._fact(state, ctx, a.id, text, seq, {"probe": result})

    def _freedom(self, state, ctx, pid: str, seq: int) -> None:
        code, report = doors.question(self.cli(state), self._contract(state), self._answers_document(state))
        detail = report.get("detail") or {}
        stats = detail.get("stats")
        if not stats:
            self._fact(state, ctx, pid, f"refused: {detail.get('reason') or report.get('status')}", seq,
                       {"report": report})
            return
        decided = stats["tuples"] - stats["reference_refused"]
        by_laws = stats.get("determined_by_laws")
        with_reasons = stats.get("determined_with_reasons", stats["determined"])
        laws_text = f"{by_laws} of {decided} determined by laws; " if by_laws is not None else ""
        self._fact(state, ctx, pid, f"{with_reasons} of {decided} decisions are determined with recorded reasons; "
                   f"{laws_text}{stats['regions']} open regions; {stats['inconsistent']} inputs admit no decision",
                   seq, stats)
        question = detail.get("question")
        if question and not stats["inconsistent"]:
            self._post_question(state, ctx, question, seq)
        else:
            self._supersede_freedom_questions(state, seq)

    def _option(self, ctx: Z.Context, o: dict) -> Z.Option:
        given = o.get("sets") or {}
        sets = tuple((f, Z.read(given[f], ctx, False)) for f in ctx.state if f in given)
        return Z.Option(o["class"], o.get("reason"), sets if o["class"] != "Reject" else ())

    @staticmethod
    def _fresh_question_id(state) -> str:
        """The next judge question ID no question uses: a model may have asked ``q1`` itself, and its
        recorded answer must never be overwritten."""
        while f"q{state['next_question']}" in state["questions"]:
            state["next_question"] += 1
        qid = f"q{state['next_question']}"
        state["next_question"] += 1
        return qid

    @staticmethod
    def _supersede_freedom_questions(state, seq, replacement=None) -> None:
        for q in state["questions"].values():
            legacy = q.get("asked_by") == "judge" and "case" in q and not q.get("kind") \
                and "rule" not in q and "strategy_reading" not in q
            if q["status"] == "open" and (q.get("freedom_key") or legacy):
                q.update(status="superseded", superseded_at=seq, superseded_by=replacement)

    def _post_question(self, state, ctx, question: dict, seq: int) -> str:
        key = Z.digest({"revision": state["subject"]["revision"], "confirmed": state.get("confirmed", []),
                        "question": question})
        for qid, q in state["questions"].items():
            if q["status"] == "open" and q.get("freedom_key") == key:
                self._fact(state, ctx, qid, f"question {qid} remains open", seq, {"question_id": qid})
                return qid
        qid = self._fresh_question_id(state)
        self._supersede_freedom_questions(state, seq, qid)
        options = tuple(self._option(ctx, o) for o in question["options"])
        where = Z.read(question["where"], ctx, False)
        law_where = Z.substitute(where, lambda n: ("var", f"pre.{n[1]}" if n[1] in ctx.state else
                                                   f"{ctx.roots[n[1]]}.{n[1]}")
                                 if n[0] == "var" and n[1] in ctx.names else None)
        example = _ordered(state["subject"], question["example"])
        state["questions"][qid] = {"status": "open", "asked_by": "judge", "case": question["case"],
                                   "freedom_key": key,
                                   "where": question["where"], "tuples": question["tuples"],
                                   "options": [o.data() for o in options],
                                   "current": next((o["class"], o.get("reason")) for o in question["options"]
                                                   if o.get("current")) if any(o.get("current") for o in
                                                                               question["options"]) else None}
        self._reply(state, ctx, "ask", qid, seq, {"region_tuples": question["tuples"]}, where=law_where,
                    example=example, options=options)
        return qid

    def _confirm(self, state, ctx, qid: str, q: dict, choice: Z.Option, seq: int) -> None:
        """An owner's answer to a judge's question: laws over exactly the asked inputs, checked before
        anything is recorded."""
        if choice.cls not in ("Accept", "Reject"):
            raise Refusal("question-choice-unsupported:" + choice.cls)
        rule = q.get("rule")
        item = {"where": rule["where"] if rule else q["where"], "class": choice.cls,
                "reason": choice.reason if choice.cls != "Accept" else None}
        if choice.cls == "Accept":
            if choice.sets:
                item["sets"] = {f: Z.print_symbolic(v) for f, v in choice.sets}
            elif "case" in q:
                item["case"] = q["case"]
        code, report = doors.question(self.cli(state), self._contract(state), self._answers_document(state, [item]))
        detail = report.get("detail") or {}
        if report.get("status") == "conflict":
            if detail.get("example") is not None:
                self._fact(state, ctx, qid, "answer conflicts at " + self._point_of(detail["example"]), seq,
                           {"conflict": detail})
            raise Refusal("answer-conflicts-with-confirmed-requirements")
        records = detail.get("answers") or []
        if not records:
            raise Refusal(f"the question door did not check the answer: {detail.get('reason') or report.get('status')}")
        record = records[-1]
        state.setdefault("confirmed", []).append(item)
        q["status"] = "answered"
        q["answer"] = choice.data()
        self._supersede_freedom_questions(state, seq)
        stats = detail.get("stats") or {}
        rate = stats.get("determined_rate", 0)
        source = f" from rule {rule['id']}" if rule else ""
        text = f"{len(record.get('laws', []))} requirements recorded{source}; {round(rate * 100)}% of decisions now determined with recorded reasons"
        if "determined_by_laws" in stats:
            text += f"; {stats['determined_by_laws']} determined by laws, {stats['determined_with_reasons']} with reasons"
        if record.get("status") == "disagreement":
            text += "; the core decides otherwise on some of these inputs and must change"
        if record.get("reason_requirement"):
            text += "; the reason is recorded apart, since no law can name it"
        self._fact(state, ctx, qid, text, seq, {"record": record, "stats": stats})

    def _rule(self, state, ctx, acts: list, seq: int) -> None:
        a = acts[0]
        o = a.get("option")
        rule = {"id": a.id, "where": Z.print_symbolic(a.get("where")), "class": o.cls,
                "reason": o.reason if o.cls != "Accept" else None}
        if o.sets:
            rule["sets"] = {f: Z.print_symbolic(v) for f, v in o.sets}
        path = self._document(state, f"rule-{a.id}", {"schema": "zeno-fcis/loop-rules/1", "rules": [rule]})
        code, report = doors.teach_back(self.cli(state), self._contract(state), path, self._answers_document(state))
        judged = ((report.get("detail") or {}).get("rules") or [None])[0]
        if not judged:
            self._fact(state, ctx, a.id, f"refused: {(report.get('detail') or {}).get('reason') or report.get('status')}",
                       seq, {"report": report})
            return
        verdict = judged["verdict"]
        meaning = f"rule:{self._subject_key(state, judged['meaning'])}" if judged.get("meaning") else None
        if self._remembered(state, ctx, a.id, meaning, seq, judged.get("domain", 0), "inputs"):
            earlier = state["meanings"][meaning]
            state.setdefault("rules", {})[a.id] = {"verdict": earlier["verdict"], "same_as": earlier["id"]}
            return
        state.setdefault("rules", {})[a.id] = {"verdict": verdict, "applies": judged.get("applies")}
        if verdict == "refused":
            self._fact(state, ctx, a.id, f"refused: {judged.get('reason')}", seq, {"teach_back": judged})
            return
        self._remember(state, meaning, a.id, seq, "rule", verdict)
        if verdict == "vacuous":
            self._fact(state, ctx, a.id, "the rule applies to no input", seq, {"teach_back": judged})
            return
        if verdict in ("exact", "holds"):
            self._reply(state, ctx, verdict, a.id, seq, {"teach_back": judged}, count=judged["applies"])
        for group in judged.get("exception_groups") or []:
            where = Z.read(group["where"], ctx, False) if group.get("where") else None
            actual = group["actual"]
            given = actual.get("sets") or {}
            sets = tuple((f, ("int", _int_value(state["subject"], f, given[f]))) for f in ctx.state if f in given)
            self._reply(state, ctx, "exception", a.id, seq, {"group": group}, count=group["count"],
                        of=judged["applies"], where=where,
                        example=_ordered(state["subject"], group["input"]),
                        actual=Z.Option(actual["class"], actual.get("reason"), sets))
        missing = judged.get("missing_region")
        if missing:
            where = Z.read(missing["where"], ctx, False) if missing.get("where") else None
            self._reply(state, ctx, "missing", a.id, seq, {"missing": missing}, count=missing["count"], where=where,
                        example=_ordered(state["subject"], missing["input"]))
        if verdict in ("wrong", "holds") and judged.get("exact_on_reachable"):
            self._fact(state, ctx, a.id, "every exception and missing input lies on an unreachable state: the rule "
                       "is exact for everything the core can do from genesis", seq)
        required = judged.get("required")
        if required is not None:
            forbidden = required.get("forbidden")
            if forbidden is None:
                raise Refusal("the teach-back report does not say where the requirements forbid the rule")
            if forbidden:
                example = required.get("forbidden_example") or {}
                admitted = " or ".join(f"{d['class']}{' ' + d['reason'] if d.get('reason') else ''}"
                                     for d in example.get("admitted") or []) or "no decision"
                self._fact(state, ctx, a.id, f"forbidden: the confirmed requirements rule this out on {forbidden} of "
                           f"{required['of']} inputs where it holds, for example "
                           f"{self._point_of(example.get('input') or {})}, where they admit only {admitted}", seq,
                           required)
            elif required["open"] == 0:
                self._fact(state, ctx, a.id, f"required: the confirmed requirements already force this on all "
                           f"{required['of']} inputs where it holds", seq, required)
            else:
                self._post_rule_question(state, ctx, a, rule, required, seq)

    def _post_rule_question(self, state, ctx, a: Z.Act, rule: dict, required: dict, seq: int) -> None:
        own = a.get("option")
        alternatives = [Z.Option(c, r) for c, r in required["alternatives"] if (c, r) != (own.cls, own.reason)]
        if not alternatives:
            return
        options = (own,) + tuple(alternatives[:3])
        qid = self._fresh_question_id(state)
        where = a.get("where")
        law_where = Z.substitute(where, lambda n: ("var", f"pre.{n[1]}" if n[1] in ctx.state else
                                                   f"{ctx.roots[n[1]]}.{n[1]}")
                                 if n[0] == "var" and n[1] in ctx.names else None)
        state["questions"][qid] = {"status": "open", "asked_by": "judge", "rule": rule, "tuples": required["open"],
                                   "options": [o.data() for o in options]}
        self._reply(state, ctx, "ask", qid, seq, {"rule": rule, "open": required["open"]}, where=law_where,
                    example=_ordered(state["subject"], required["example"]), options=options)

    def _decompose(self, state, ctx, acts: list, seq: int) -> None:
        heads = [a for a in acts if a.kind == "decompose"]
        if len(heads) != 1:
            raise Refusal("decompose-statement-missing: name the merge order" if not heads else
                          "decompose-given-twice")
        head = heads[0]
        components = {}
        for a in acts:
            if a.kind == "component":
                if a.id in components:
                    raise Refusal(f"component-given-twice:{a.id}")
                components[a.id] = {"name": a.id, "owns": list(a.get("owns")), "reads": list(a.get("reads")),
                                    "decides": list(a.get("decides")), "cases": []}
        for a in sorted((a for a in acts if a.kind == "case"), key=lambda a: a.get("n")):
            if a.id not in components:
                raise Refusal(f"decompose-components:{a.id}")
            if a.get("n") != len(components[a.id]["cases"]):
                raise Refusal(f"decompose-cases-numbered-from-0:{a.id}")
            o = a.get("option")
            case = {"when": Z.print_symbolic(a.get("when")), "class": o.cls}
            if o.cls != "Accept":
                case["reason"] = o.reason
            if o.sets:
                case["sets"] = {f: Z.print_symbolic(v) for f, v in o.sets}
            components[a.id]["cases"].append(case)
        order = list(head.get("order"))
        if set(order) != set(components):
            raise Refusal(f"decompose-components:{sorted(set(order) ^ set(components))[0]}")
        document = {"schema": "zeno-fcis/loop-architecture/1", "components": [components[c] for c in order]}
        path = self._document(state, f"architecture-{head.id}", document)
        code, report = doors.decompose(self.cli(state), self._contract(state), path)
        detail = report.get("detail") or {}
        verdict = detail.get("verdict") or report.get("status")
        state.setdefault("decompositions", {})[head.id] = {"verdict": verdict,
                                                          "local_domain_total": detail.get("local_domain_total")}
        if verdict == "refused" or not detail:
            self._fact(state, ctx, head.id, f"refused: {detail.get('reason') or report.get('status')}", seq,
                       {"report": report})
            return
        subject = state["subject"]

        def point(shown: dict) -> tuple:
            return _ordered(subject, shown)

        for problem in detail.get("structure") or []:
            if problem.get("what") == "reads-later":
                suggestion = problem.get("suggested_order")
                text = f"{problem['component']} reads {problem['field']} owned later by {problem['owner']}"
                text += ("; suggested order: " + ", ".join(suggestion)) if suggestion else "; the reads form a cycle"
            elif problem["kind"] == "unowned-reason":
                text = (f"no component decides {problem['reason']}; the core does on {problem['count']} inputs, "
                        f"for example {self._point_of(problem['input'])}")
            else:
                text = (f"a failure must change {problem['field']}, but its owner {problem['component']} runs after "
                        f"the failing component ({problem['count']} inputs)")
            self._fact(state, ctx, head.id, text, seq, problem)
        for clash in detail.get("interface") or []:
            reads = clash.get("suggested_reads") or clash.get("separating_fields") or []
            self._reply(state, ctx, "interface", head.id, seq, {"interface": clash}, component=clash["component"],
                        pair=(point(clash["inputs"][0]), point(clash["inputs"][1])), reads=tuple(reads))
            if clash.get("suggested_reads_owned_later"):
                self._fact(state, ctx, head.id, ", ".join(clash["suggested_reads_owned_later"]) + " belong to "
                           f"components that come later than {clash['component']}: reorder, or move the check", seq)
        if verdict == "unrealizable":
            order_problem = any(p.get("what") == "reads-later" for p in detail.get("structure") or [])
            self._fact(state, ctx, head.id, "unrealizable in the submitted component order" if order_problem else
                       "unrealizable: no case tables can make this architecture behave as the core", seq)
        elif verdict == "realizable":
            failed = "; ".join(f"{name}: {why.get('reason')}" for name, why in (detail.get("projection") or {}).items()
                               if "reason" in why)
            self._fact(state, ctx, head.id, "realizable: case tables exist" +
                       (f"; the core's own cases do not project ({failed}); write those cases" if failed else ""), seq)
        composed = detail.get("compose") or {}
        if verdict in ("completed", "meets-requirements", "obligations-not-proved", "accepted"):
            for name, table in (detail.get("projection") or {}).items():
                for n, case in enumerate(table.get("table") or []):
                    given = case.get("sets") or {}
                    sets = tuple((f, Z.read(given[f], ctx, False)) for f in ctx.state if f in given)
                    option = Z.Option(case["class"], case.get("reason"), sets if case["class"] != "Reject" else ())
                    self._reply(state, ctx, "case", name, seq, None, n=n, when=Z.read(case["when"], ctx, False),
                                option=option)
        if verdict == "accepted":
            obligations = (composed.get("obligations") or {}).get("result") or {}
            self._reply(state, ctx, "decomposed", head.id, seq, {"compose": composed}, tuples=composed.get("inputs", 0),
                        nodes=composed.get("nodes_total", 0), components=len(composed.get("components") or []))
            self._fact(state, ctx, head.id, f"the composition checker's joint step equals the core on all "
                       f"{composed.get('inputs')} inputs, and its obligation check is "
                       f"{obligations.get('verdict', 'proved')}", seq)
            return
        if verdict == "refuted" and composed.get("differ"):
            first = composed.get("first_difference") or {}
            self._fact(state, ctx, head.id, f"refuted: the composition differs from the core on {composed['differ']} "
                       f"of {composed.get('inputs')} inputs, first when {self._point_of(first.get('input') or {})}",
                       seq, {"compose": composed})
        if verdict in ("completed", "meets-requirements", "obligations-not-proved"):
            # Steps 1 to 3 are the door's own evaluation of the tables, a proposal; only the composed
            # check is a checker's verdict.
            self._fact(state, ctx, head.id, "by the door's own evaluation, " +
                       ("every component" if verdict != "completed" else "completed: every component's case table")
                       + f" meets its required behaviour on all {detail.get('domain')} inputs; local domains "
                       f"total {detail.get('local_domain_total')}" +
                       ("; the composed obligations are not proved" if verdict == "obligations-not-proved" else ""),
                       seq)
        elif verdict == "refuted" and detail.get("blame"):
            self._fact(state, ctx, head.id, "refuted by the door's own evaluation: some components differ from "
                       "their required behaviour", seq)
            for blame in (detail.get("blame") or [])[:3]:
                where = Z.read(blame["where"], ctx, False) if blame.get("where") else None
                self._reply(state, ctx, "blame", head.id, seq, {"blame": blame}, component=blame["component"],
                            count=blame["wrong"], of=blame["decides_on"], where=where)

                def option(o: dict) -> Z.Option:
                    given = o.get("sets") or {}
                    sets = tuple((f, ("int", _int_value(subject, f, given[f]))) for f in ctx.state if f in given)
                    return Z.Option(o["class"], o.get("reason"), sets)

                self._reply(state, ctx, "expected", head.id, seq, None, component=blame["component"],
                            example=point(blame["input"]), required=option(blame["required"]),
                            actual=option(blame["actual"]))

    def _point_of(self, shown: dict) -> str:
        return ", ".join(f"{n} = {str(v).lower() if isinstance(v, bool) else v}" for n, v in shown.items())

    # -- the owner's channel ---------------------------------------------------------------------------------
    def _owner_document(self, state: dict, ctx: Z.Context, statement: str, english: bool) -> str:
        header = Z.HEADER["english" if english else "symbolic"]
        author = "The author is the owner." if english else "author owner;"
        return "\n".join([header, Z.subject_line(ctx.subject_ref, english), author, statement]) + "\n"

    def answer(self, question: str, option: str) -> list:
        """The owner's answer by option number (1-based), ``any`` or ``none``."""
        with self.locked():
            state = self.load()
            ctx = Z.Context(state["subject"])
            if state.get("version_space"):
                events = versions.answer(self, state, ctx, question, option)
                self.save(state)
                return events
            q = state["questions"].get(question)
            if q is None or q["status"] != "open":
                raise Refusal(f"no open question {question}")
            if q.get("kind") == "learn":
                events = L.answer(self, state, ctx, question, option)
                self.save(state)
                return events
            if option in ("any", "none"):
                statement = f"answer {question}: {option};"
            else:
                if not re.fullmatch(r"[1-9][0-9]?", option or ""):
                    raise Refusal("answer with an option number, any or none")
                index = int(option) - 1
                options = _options_of(state, ctx, question)
                if not 0 <= index < len(options):
                    raise Refusal("answer option out of range")
                statement = f"answer {question}: {Z.option_symbolic(options[index], ctx)};"
            doc = Z.parse(self._owner_document(state, ctx, statement, False), ctx)
            events = self._submit(state, ctx, doc)
            self.save(state)
            _report_judgements(state, "owner-answer", events)
            return events

    def say(self, statement: str) -> list:
        """The owner states one teach-back rule, in either surface."""
        statement = statement.strip()
        if not (statement.startswith("Rule ") or statement.startswith("rule ")):
            raise Refusal("the owner states rules here: rule ID: where COND -> DECISION;  or  "
                          "Rule ID: where COND, the core will DECISION.")
        with self.locked():
            state = self.load()
            ctx = Z.Context(state["subject"])
            doc = Z.parse(self._owner_document(state, ctx, statement, statement[:1].isupper()), ctx)
            events = self._submit(state, ctx, doc)
            self.save(state)
            _report_judgements(state, "owner-rule", events)
            return events

    def next_question(self) -> list:
        """Owner terminal starts the contract's question door without granting a decision."""
        with self.locked():
            state = self.load()
            self._contract(state)
            ctx = Z.Context(state["subject"])
            start = len(state["events"])
            act = Z.Act("probe", "next", (("query", "freedom"), ("args", ())))
            seq = self._record(state, ctx, "owner", act)["seq"]
            try:
                self._freedom(state, ctx, "next", seq)
            except Refusal as error:
                self._fact(state, ctx, "next", f"refused: {error}", seq)
            self.save(state)
            return state["events"][start:]

    def stop_versions(self) -> list:
        """Owner terminal only; preserve requirements and close the conversation."""
        with self.locked():
            state = self.load()
            if not state.get("version_space"):
                raise Refusal("loop stop needs a requirements workspace")
            ctx = Z.Context(state["subject"])
            source = self._owner_document(state, ctx, "version-stop owner;", False)
            events = self._submit(state, ctx, Z.parse(source, ctx))
            self.save(state)
            return events

    # -- displays ---------------------------------------------------------------------------------------------
    def view(self, state: dict | None = None) -> dict:
        result = N.encode(self._view(state))
        result["integer_encoding"] = N.ENCODING
        return result

    def _view(self, state: dict | None = None) -> dict:
        state = state or self.load()
        if state.get("version_space"):
            return versions.view(self, state)
        if state.get("learn"):
            return L.view(self, state)
        ctx = Z.Context(state["subject"])
        questions = []
        for qid, q in state["questions"].items():
            if q["status"] != "open":
                continue
            event = next((e for e in reversed(state["events"]) if e["act"]["kind"] == "ask"
                          and e["act"]["id"] == qid), None)
            options = [{"n": k + 1, "english": Z.option_english(o, ctx), "symbolic": Z.option_symbolic(o, ctx)}
                       for k, o in enumerate(_options_of(state, ctx, qid))]
            questions.append({"id": qid, "asked_by": q["asked_by"], "english": event["english"] if event else qid,
                              "symbolic": event["symbolic"] if event else qid, "options": options})
        incumbent = state["incumbent"]
        return {"schema": VIEW_SCHEMA, "workspace": str(self.path), "zal_profile": Z.PROFILE,
                "subject": list(ctx.subject_ref),
                "incumbent": {k: incumbent[k] for k in ("nodes", "bytes", "by", "sha256")},
                "original_nodes": state["original"]["nodes"],
                "events": [{k: e[k] for k in ("seq", "author", "english", "symbolic")}
                           for e in state["events"][-VIEW_EVENTS:]],
                "event_count": len(state["events"]), "open_questions": questions, "authority": "none"}

    def status_line(self) -> str:
        v = self.view()
        q = len(v["open_questions"])
        if v.get("mode") == "version-space":
            return f"ZAL requirements loop: {v['event_count']} events, {q} questions for the owner"
        if v.get("mode") == "learn":
            return f"ZAL learn: {v['event_count']} events, {q} questions for the owner; vocabulary results are proposals"
        return (f"ZAL loop {v['subject'][1]}: {v['incumbent']['nodes']} nodes (original {v['original_nodes']}), "
                f"{v['event_count']} events" + (f", {q} question{'s' if q != 1 else ''} for the owner" if q else ""))

    def packet(self, english: bool = False) -> str:
        """The subject card: vocabulary, case table, laws, incumbent, and the header lines to use."""
        state = self.load()
        if state.get("version_space"):
            return versions.packet(state, english)
        if state.get("learn"):
            return L.packet(state, english)
        subject = state["subject"]
        ctx = Z.Context(subject)
        lines = [Z.HEADER["english" if english else "symbolic"], Z.subject_line(ctx.subject_ref, english),
                 "The author is the model." if english else "author model;", "",
                 "# inputs (program order: state, command, context)"]
        for name, kind, span in zip(subject["names"], subject["kinds"], subject["ranges"]):
            role = "state" if name in subject["state"] else subject["roots"][name]
            values = "true/false" if kind == "Bool" else f"{span[0]}..{span[1]}"
            table = subject["variants"].get(name)
            if table:
                values += " (" + ", ".join(f"{n}={v}" for n, v in sorted(table.items(), key=lambda kv: kv[1])) + ")"
            lines.append(f"#   {name}: {role}, {values}")
        lines.append("# outputs")
        for j, kind in enumerate(subject["outputs"]):
            role = "index of the first matching case" if j == 0 else "successor value"
            lines.append(f"#   out{j}: {kind}, {role}")
        lines.append("# case table (first match decides; out0 is the index of the matching case)")
        reasons = {v: k for k, v in subject["reason_ids"].items()}
        for k, case in enumerate(subject["cases"]):
            when = _policy_expression(subject, case.get("when", "true"))
            when_text = Z.show(when, ctx, english, want="Bool") if when is not None else case.get("when")
            post = []
            for fid, text in sorted(case.get("post", {}).items()):
                name = subject["paths"].get(next((p for p in subject["paths"] if p.startswith("pre.") and
                                                  p.endswith(f".{fid}")), ""), fid)
                expr = _policy_expression(subject, text)
                post.append(f"{name} := {Z.show(expr, ctx, english) if expr is not None else text}")
            reason = reasons.get(case.get("reason"), "")
            lines.append(f"#   {k}: when {when_text} -> {case.get('class')}" + (f" {reason}" if reason else "")
                         + (" " + ", ".join(post) if post else ""))
        lines.append("# declared laws (fields by name)")
        for law in subject["laws"]:
            text = law["text"]
            for path in sorted(subject["paths"], key=len, reverse=True):
                prefix = path.split(".")[0]
                text = text.replace(path, f"{prefix}.{subject['paths'][path]}")
            lines.append(f"#   {law['name']} on {law['scopes']}: {text}")
        if subject["parameters"]:
            lines.append("# family parameters: " + ", ".join(f"${k} = {v}" for k, v in
                                                              sorted(subject["parameters"].items())))
        incumbent = state["incumbent"]
        lines.append(f"# incumbent: {incumbent['nodes']} nodes ({incumbent['by']}); original "
                     f"{state['original']['nodes']}")
        for j, node in enumerate(from_program(incumbent["program"], subject["names"])):
            a = Z.act("program", "incumbent", output=f"out{j}", expr=node)
            lines.append("# " + Z.render_act(a, ctx, english))
        open_q = [q for q, r in state["questions"].items() if r["status"] == "open"]
        if open_q:
            lines.append(f"# open questions for the owner: {', '.join(open_q)}")
        return "\n".join(lines) + "\n"

    def translate(self, source: str, to: str) -> str:
        ctx = Z.Context(self.load()["subject"])
        return Z.render(Z.parse(source, ctx), ctx, english=to == "english")


def _ordered(subject: dict, shown: dict) -> tuple:
    """A door's named input as a loop-language input list, in program order."""
    return tuple((n, _int_value(subject, n, shown[n])) for n in subject["names"] if n in shown)


def _tuples(subject: dict) -> int:
    total = 1
    for low, high in subject["ranges"]:
        total *= high - low + 1
    return total


def _policy_expression(subject: dict, text: str):
    """A policy expression over its variables, rewritten over the input field names (None if unreadable)."""
    variables = subject["variables"]

    def resolve(name: str, depth: int = 0):
        if name in subject["paths"]:
            return ("var", subject["paths"][name])
        if name in variables and depth < 16:
            inner = variables[name]
            if inner in subject["paths"]:
                return ("var", subject["paths"][inner])
            return Z.substitute(Z.parse_expression(inner), lambda n: resolve(n[1], depth + 1) if n[0] == "var"
                                else None)
        return ("var", name)
    try:
        return Z.substitute(Z.parse_expression(text), lambda n: resolve(n[1]) if n[0] == "var" else None)
    except Refusal:
        return None


def _options_of(state: dict, ctx: Z.Context, question: str) -> list:
    out = []
    for o in state["questions"].get(question, {}).get("options", []):
        sets = tuple((f, Z.parse_expression(v)) for f, v in o["sets"])
        out.append(Z.Option(o["class"], o["reason"], sets))
    return out


# --- the owner's terminal command group -----------------------------------------------------------------------

def terminal(argv: list) -> int:
    """``terminal.py loop init|show|next|answer|say``: the owner's commands for a loop workspace."""
    parser = argparse.ArgumentParser(prog="terminal.py loop", description="ZAL loop workspace (owner terminal)")
    sub = parser.add_subparsers(dest="command", required=True)
    init = sub.add_parser("init", help="create a workspace for a contract or a family")
    init.add_argument("--contract", type=Path)
    init.add_argument("--family")
    init.add_argument("--specification", type=Path)
    for flag in ("max-work", "max-domain", "max-meanings", "max-cells", "max-conflict-subsets"):
        init.add_argument("--" + flag, type=int)
    stop = sub.add_parser("stop", help="owner stops a requirements-only conversation")
    init.add_argument("--learn", type=Path, help="fixed vocabulary declaration and owner examples")
    show = sub.add_parser("show", help="the latest events")
    show.add_argument("--symbolic", action="store_true")
    show.add_argument("--last", type=int, default=40)
    show.add_argument("--packet", action="store_true", help="print the subject card instead")
    answer = sub.add_parser("answer", help="answer an open question: N, any or none")
    answer.add_argument("question")
    answer.add_argument("option")
    say = sub.add_parser("say", help="state a teach-back rule")
    say.add_argument("statement")
    next_ = sub.add_parser("next", help="ask the next open contract question")
    for p in (init, show, answer, say, stop, next_):
        p.add_argument("--workspace", type=Path, required=True)
        p.add_argument("--cli", type=Path)
    options = parser.parse_args(argv)
    workspace = LoopWorkspace(options.workspace, options.cli)
    try:
        if options.command == "init":
            if options.learn is not None:
                if options.contract or options.family or options.specification:
                    raise Refusal("--learn cannot be combined with --contract, --family or --specification")
                if any(getattr(options, key) is not None for key in ("max_work", "max_domain", "max_meanings", "max_cells", "max_conflict_subsets")):
                    raise Refusal("version-space budgets need --specification")
                view = workspace.init_learn(options.learn)
                print(json.dumps({"status": "initialized", "subject": view["subject"], "incumbent": None,
                                  "authority": "none"}, indent=1))
                return 0
            if sum(bool(x) for x in (options.contract, options.family, options.specification)) != 1:
                raise Refusal("give exactly one of --contract, --family and --specification")
            limits = {k: getattr(options, k) for k in ("max_work", "max_domain", "max_meanings", "max_cells", "max_conflict_subsets")
                      if getattr(options, k) is not None}
            if limits and options.specification is None:
                raise Refusal("version-space budgets need --specification")
            view = workspace.init(contract=options.contract, family=options.family,
                                  specification=options.specification, version_limits=limits)
            print(json.dumps({"status": "initialized", "subject": view["subject"], "incumbent": view["incumbent"],
                              "authority": "none"}, indent=1))
        elif options.command == "show":
            if options.packet:
                print(workspace.packet(english=not options.symbolic), end="")
            else:
                print(workspace.status_line())
                for e in workspace.load()["events"][-max(1, options.last):]:
                    print(f"{e['seq']:>4} {e['author']:<5} {e['symbolic' if options.symbolic else 'english']}")
        elif options.command == "answer":
            _print(workspace.answer(options.question, options.option))
        elif options.command == "stop":
            _print(workspace.stop_versions())
        elif options.command == "next":
            _print(workspace.next_question())
        else:
            _print(workspace.say(options.statement))
        return 0
    except (ValueError, OSError, KeyError) as error:
        print(json.dumps({"status": "refused", "diagnostic": str(error)[:2000], "authority": "none"}), file=sys.stderr)
        return 2


def _print(events: list) -> None:
    for e in events:
        print(f"{e['seq']:>4} {e['author']:<5} {e['english']}")


# --- the MCP tools ---------------------------------------------------------------------------------------------

def _events_text(events: list) -> str:
    english = "\n".join(f"{e['seq']:>4} {e['author']:<5} {e['english']}" for e in events)
    symbolic = "\n".join(f"{e['seq']:>4} {e['author']:<5} {e['symbolic']}" for e in events)
    return f"{english}\n\nsymbolic:\n{symbolic}"


def mcp_call(name: str, arguments: dict) -> dict:
    """One ``zal_loop_*`` MCP tool on the workspace named by ``ZAL_LOOP_WORKSPACE``. No answer or say tool."""
    if name == "zal_loop_legend":
        return {"legend": Z.legend(), "profile": Z.PROFILE, "authority": "none"}
    path = os.environ.get("ZAL_LOOP_WORKSPACE")
    if not path:
        raise Refusal("Set ZAL_LOOP_WORKSPACE to the loop workspace in the MCP server's environment")
    workspace = LoopWorkspace(path)
    if name == "zal_loop_read":
        state = workspace.load()
        last = int(arguments.get("last", 20))
        view = workspace.view(state)
        return {"status": workspace.status_line(), "open_questions": view["open_questions"],
                "events": _events_text(state["events"][-last:]) if state["events"] else "(no events yet)",
                "workflow": "Only the owner answers questions, in the terminal. A tool permission is not the "
                            "owner's agreement.", "authority": "none"}
    if name == "zal_loop_packet":
        return {"packet": workspace.packet(english=arguments.get("surface") == "english"), "authority": "none"}
    if name == "zal_loop_submit":
        events = workspace.submit(arguments["source"], "model")
        return {"replies": _events_text(events), "authority": "none"}
    if name == "zal_loop_translate":
        return {"text": workspace.translate(arguments["source"], arguments["to"]), "authority": "none"}
    raise Refusal("Unknown tool")
