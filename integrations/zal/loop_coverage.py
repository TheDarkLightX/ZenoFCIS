"""Advisory witness coverage expansion; only the native gap judge gives verdicts."""

from __future__ import annotations

from dataclasses import asdict, dataclass
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys

SCHEMA = "zeno-fcis/coverage-loop/1"
DIMENSIONS = ("value", "authority", "state", "input", "order", "privacy", "liveness",
              "failure", "time and resources", "story", "unclassified")
DEFAULT_BIN = "/media/trevormoc/4986929f-65e6-4021-94a2-943a5e427718/tmp/zf26-rev-zalgames/bin/zeno-fcis"
MAX_FILE = 1024 * 1024
MAX_PROPOSALS = 128
DIMENSION_RULES = (
    ("value", r"balance|value|amount|supply|fee|reserv|payment|credit|debit|count"),
    ("authority", r"authoriz|permission|role|owner|officer|operator|grant|approv"),
    ("state", r"state|status|control|terminal|pending|closed|empty|full"),
    ("input", r"command|context|input|duplicate|alias|malformed|zero|maximum"),
    ("order", r"order|precedence|sequence|commut"),
    ("privacy", r"private|privacy|secret|noninterference|confidential"),
    ("liveness", r"liveness|progress|eventually|stuck|live|withdraw|cancel"),
    ("failure", r"reject|refus|failure|reason|error"),
    ("time and resources", r"deadline|expir|quota|budget|time|resource|capacity|limit"),
    ("story", r"story|document|guarantee|requirement"),
)


def canonical(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=True, separators=(",", ":"))


def read_text(path):
    with Path(path).open(encoding="utf-8") as stream:
        text = stream.read(MAX_FILE + 1)
    if len(text.encode()) > MAX_FILE:
        raise ValueError(f"input exceeds {MAX_FILE} bytes: {path}")
    return text


@dataclass(frozen=True)
class Candidate:
    id: str
    source: str
    laws: tuple[str, ...]
    targets: tuple[str, ...]
    plain: str
    zal: str | None = None
    template: str | None = None


class JudgeStop(Exception):
    def __init__(self, code, status):
        self.code, self.status = code, status


def classify(text):
    """Explain heuristic tags; these tags confer no semantic coverage claim."""
    rules = [{"dimension": dimension, "rule": f"name-or-rule matches /{pattern}/"}
             for dimension, pattern in DIMENSION_RULES if re.search(pattern, text, re.I)]
    return rules or [{"dimension": "unclassified", "rule": "no name-or-rule pattern matched"}]


def measurement(packet):
    gaps = packet["gaps"]
    if gaps.get("status") != "measured":
        raise ValueError("a measured gap packet is required")
    return gaps.get("proposals", {}).get("after", gaps) if gaps.get("proposals") else gaps


def witnesses(view, packet, project, policy):
    """Collect every emitted witness, retaining the judge's sampling boundary."""
    fields = {number: name for number, name in re.findall(r"field\s+(\d+)\s+\d+\s+(\w+)", project)}
    positions = packet.get("inputs", {}).get("positions", [])
    context = " ".join(p["name"] + " " + fields.get(str(p.get("field")), "") for p in positions)
    rows = []
    for gap in view["admissible"]["full"]["examples"]:
        key = hashlib.sha256(canonical(gap["input"]).encode()).hexdigest()[:16]
        rows.append({"id": "gap-" + key, "kind": "gap", "witness": gap})
    for mutant in view["mutation"]["list"]:
        if mutant["classification"] == "admitted":
            rows.append({"id": mutant["id"], "kind": "mutant", "witness": mutant})
    result = []
    for row in rows:
        witness = row["witness"]
        site = re.search(r"cases\[(\d+)\]", witness.get("site", ""))
        rule = policy["cases"][int(site[1])] if site else {}
        text = context + " " + canonical(rule) + " " + canonical(witness)
        rules = classify(text)
        result.append({**row, "dimensions": sorted({r["dimension"] for r in rules}), "classification_rules": rules})
    return result


def snapshot(packet, project, policy):
    view = measurement(packet)
    full = view["admissible"]["full"]
    rows = witnesses(view, packet, project, policy)
    return {"gaps": full["undetermined"], "determined": full["determined"], "tuples": full["tuples"],
            "admitted_mutants": view["mutation"]["full"]["admitted"],
            "per_dimension": {d: {k: sum(d in w["dimensions"] and w["kind"] == k for w in rows)
                                  for k in ("gap", "mutant")} for d in DIMENSIONS},
            "dimension_scope": "emitted witnesses only; gaps are sampled, tags may overlap",
            "witnesses": rows}


def disagreement(packet):
    """Read native negative verdicts before looking at any apparent improvement."""
    gaps = packet.get("gaps", {})
    return (gaps.get("false_alarm") or
            (packet if gaps.get("status") in {"proposal-false-alarm", "disagreement"} else None) or
            (packet if packet.get("status") == "disagreement" else None) or
            packet.get("examples", {}).get("disagreements") or
            (packet if packet.get("summary", {}).get("findings", 0) else None))


def assess(packet, code, before, project, policy):
    finding = disagreement(packet)
    if finding:
        return "disagreeing", {"witness": finding}
    if code != 0 or packet.get("gaps", {}).get("status") != "measured":
        status = packet.get("gaps", {}).get("status", "invalid-packet")
        raise JudgeStop(code if code else 65, status)
    after = snapshot(packet, project, policy)
    gain = before["gaps"] - after["gaps"]
    kills = before["admitted_mutants"] - after["admitted_mutants"]
    if gain <= 0 and kills <= 0:
        return "rejected", {"reason": "closes no gap and rejects no admitted mutant", "after": after}
    remaining = {w["id"] for w in after["witnesses"]}
    closed = [w for w in before["witnesses"] if w["id"] not in remaining]
    return "kept", {"gaps_closed": gain, "mutants_rejected": kills, "closed_witnesses": closed,
                    "after": after}


class Judge:
    """I/O shell. Retains exact requests, law bytes, packets, stderr and exit codes."""
    def __init__(self, binary, contract, out, max_work=None):
        self.binary, self.contract, self.out = binary, contract, out
        self.max_work, self.calls = max_work, 0
        self.binary_hash = hashlib.sha256(binary.read_bytes()).hexdigest()

    def review(self, laws=()):
        if hashlib.sha256(self.binary.read_bytes()).hexdigest() != self.binary_hash:
            raise JudgeStop(3, "binary-changed")
        self.calls += 1
        stem = self.out / "packets" / f"{self.calls:04d}"
        stem.parent.mkdir(parents=True, exist_ok=True)
        args = [str(self.binary), "contract", "review", str(self.contract), "--gaps", "--format", "json"]
        if laws:
            law_file = stem.with_suffix(".laws")
            law_file.write_text("\n".join(laws) + "\n")
            args += ["--gaps-laws", str(law_file)]
        if self.max_work is not None:
            args += ["--max-work", str(self.max_work)]
        run = subprocess.run(args, capture_output=True, text=True)
        stem.with_suffix(".stdout").write_text(run.stdout)
        stem.with_suffix(".stderr").write_text(run.stderr)
        stem.with_suffix(".request.json").write_text(json.dumps({"argv": args, "exit": run.returncode}, indent=2) + "\n")
        if hashlib.sha256(self.binary.read_bytes()).hexdigest() != self.binary_hash:
            raise JudgeStop(3, "binary-changed")
        try:
            packet = json.loads(run.stdout)
            if not isinstance(packet, dict):
                raise ValueError("not an object")
        except ValueError:
            raise JudgeStop(run.returncode if run.returncode else 65, "missing-review-packet") from None
        return run.returncode, packet, f"packets/{stem.name}.stdout"


def policy_expression(text, policy, positions, value=False):
    """Reuse the loop language's parser/writer, including aliases and choose."""
    from loop_language import parse_expression
    from loop_judges import _Writer
    paths = {f"coverage_input_{i}": p["name"] for i, p in enumerate(positions)}
    subject = {"names": list(paths), "kinds": ["Bool" if p["domain"]["kind"] == "bool" else "I128" for p in positions],
               "variants": {}}
    def parse(raw):
        for alias, path in sorted(paths.items(), key=lambda item: -len(item[1])):
            raw = re.sub(re.escape(path) + r"(?![\d.])", alias, str(raw))
        return parse_expression(str(raw), allow_holes=False, allow_params=False)
    def expand(node, seen=()):
        if node[0] == "var" and node[1] in policy.get("variables", {}):
            if node[1] in seen:
                raise ValueError("cyclic policy variable")
            return expand(parse(policy["variables"][node[1]]), (*seen, node[1]))
        if node[0] == "call":
            return ("call", node[1], tuple(expand(a, seen) for a in node[2]))
        return tuple(expand(a, seen) if isinstance(a, tuple) else a for a in node)
    tree = expand(parse(text))
    writer = _Writer(subject, paths)
    return writer.value(tree) if value else writer.formula(tree)


def synthesize(packet, policy, before, allocate):
    """Fixed templates over ordered rule guards; never execute a rule in Python."""
    positions = packet["inputs"]["positions"]
    state = [p for p in positions if p["source"] == "state"]
    roots = policy["roots"]
    rows, earlier = [], []
    for case in policy["cases"]:
        guard = policy_expression(case["when"], policy, positions)
        effective = " && ".join([*(f"!({g})" for g in earlier), f"({guard})"])
        earlier.append(guard)
        rows.append((case, effective))
    targets = tuple(w["id"] for w in before["witnesses"])
    candidates = []
    def propose(name, scope, body, template):
        law = f"law {allocate()} {name} on {scope} = {body};"
        candidates.append(Candidate(name, "synthesized", (law,), targets, law, template=template))
    rejects = [(c, g) for c, g in rows if c["class"] == "Reject"]
    # This aggregate template includes the documented guide-counter acceptance law.
    propose("coverage_rejects_only_when_needed", "reject",
            " || ".join(f"({policy_expression(c['when'], policy, positions)})" for c, _ in rejects) or "false",
            "a rejection must satisfy a declared rejection guard")
    for reason in sorted({c["reason"] for c, _ in rejects}):
        propose(f"coverage_reason_{reason}", f"reject:{reason}",
                " || ".join(f"({g})" for c, g in rejects if c["reason"] == reason),
                "rejection reasons preserve ordered rule precedence")
    for cls, scope in [("Accept", "accept"), ("CommittedFailure", "failure")]:
        cases = [(c, g) for c, g in rows if c["class"] == cls]
        propose(f"coverage_{scope}_relation", scope, " || ".join(f"({g})" for _, g in cases) or "false",
                "a decision outside its declared input relation is refused")
        for field in state:
            number = str(field["field"])
            terms = []
            for case, guard in cases:
                expression = policy_expression(case.get("post", {}).get(number, field["name"]), policy, positions, value=True)
                terms.append(f"(({guard}) -> post.{roots['state']}.{number} == {expression})")
            if terms:
                propose(f"coverage_{scope}_field_{number}", scope, " && ".join(terms),
                        "a committed field follows its declared update, including the unchanged frame")
    seen = set()
    for witness in before["witnesses"]:
        data = witness["witness"]
        if witness["kind"] == "mutant":
            data = data["admitted_witness"]
        own = data.get("contract", {})
        if own.get("class") not in {"Accept", "Reject"}:
            continue
        guard = " && ".join(f"{p['name']} == {n}" for p, n in zip(positions, data["input"]))
        body = f"!({guard})" if own["class"] == "Accept" else f"({guard}) -> decision_reason == {own['reason']}"
        if body in seen:
            continue
        seen.add(body)
        name = f"coverage_witness_{hashlib.sha256(body.encode()).hexdigest()[:12]}"
        law = f"law {allocate()} {name} on reject = {body};"
        candidates.append(Candidate(name, "synthesized", (law,), (witness["id"],), law,
                                    template="the witnessed input either must not reject or must give its actual rejection reason"))
    return candidates


def proposals(path, zal_path, project, allocate):
    """One law or profile-2 statement per line, with optional # targets: TAG,..."""
    if path is None:
        return []
    model = None
    if zal_path:
        import behavior2
        model = behavior2.parse(read_text(zal_path))
        # Bind the numeric lowering vocabulary before any proposal is judged.
        expected = [(110, 100, "control"), *[(170 + i, 100, "var_" + v.name) for i, v in enumerate(model.variables)],
                    *[(160 + i, 102, "observed_" + n) for i, n in enumerate(model.facts)],
                    *[(164 + i, 102, "input_" + v.name) for i, v in enumerate(model.inputs)]]
        fields = {(int(i), int(root), name) for i, root, name in
                  re.findall(r"field\s+(\d+)\s+(\d+)\s+(\w+)", project)}
        if not set(expected) <= fields:
            raise ValueError("ZAL model and contract do not share the profile-2 lowering fields")
        variants = {(int(i), int(t), name) for i, t, name in
                    re.findall(r"variant\s+(\d+)\s+(\d+)\s+(\w+)", project)}
        expected_variants = {(150 + i, 105, "state_" + n) for i, n in enumerate(model.states)}
        expected_variants |= {(120 + i, 101, "event_" + n) for i, n in enumerate(model.events)}
        if not expected_variants <= variants:
            raise ValueError("ZAL model and contract do not share the profile-2 lowering variants")
    result = []
    for number, line in enumerate(read_text(path).splitlines(), 1):
        text, _, comment = line.partition("#")
        text = text.strip()
        if not text:
            continue
        target = re.fullmatch(r"\s*targets:\s*(.+?)\s*", comment)
        tags = tuple(t.strip() for t in target[1].split(",")) if target else ("unclassified",)
        if any(not t for t in tags):
            raise ValueError(f"empty proposal target at line {number}")
        zal = None
        if text.startswith("law "):
            match = re.fullmatch(r"law\s+\d+\s+([A-Za-z_]\w*)\s+(on\s+.+;)", text)
            if not match:
                raise ValueError(f"one complete law required at line {number}")
            laws = (f"law {allocate()} {match[1]} {match[2]}",)
            plain = text
        else:
            if model is None:
                raise ValueError("ZAL proposals need --zal MODEL.zal")
            import behavior2
            from factory import profile2_proposal
            match = re.fullmatch(r"(invariant|require)\s+([a-z][a-z0-9_]*):\s*(.+);", text)
            if not match:
                raise ValueError(f"invariant or require statement expected at line {number}")
            kind, name, body = match.groups()
            declarations = {"facts": set(model.facts), "inputs": {v.name for v in model.inputs},
                            "variables": {v.name for v in model.variables}, "states": set(model.states),
                            "events": set(model.events)}
            formula = behavior2.parse_formula(body, declarations, "invariant" if kind == "invariant" else "step")
            first = allocate()
            laws = profile2_proposal(model, kind, name, formula, first)
            for _ in laws[1:]:
                allocate()
            plain = f"{'Invariant' if kind == 'invariant' else 'Requirement'} {name}: {formula.render(True)}."
            zal = text
        result.append(Candidate(f"proposed-line-{number}", "proposed", laws, tags, plain, zal))
        if len(result) > MAX_PROPOSALS:
            raise ValueError(f"at most {MAX_PROPOSALS} proposals")
    return result


def markdown(report):
    lines = ["# Coverage proposals", "", "Advisory only. Confirm each law before adoption.", "",
             "Coverage means witnessed by the gap judge's mutants and gaps, not every possible behaviour.",
             "Dimension counts cover emitted witnesses; gap samples are not the complete open-input set.", "",
             f"Status: {report['status']}; exit {report['exit_code']}.", ""]
    for rnd in report["rounds"]:
        a, b = rnd["before"], rnd["after"]
        lines += [f"## Round {rnd['round']}", "", f"Gaps: {a['gaps']} -> {b['gaps']}; admitted mutants: {a['admitted_mutants']} -> {b['admitted_mutants']}.", "",
                  "| Dimension | Gap witnesses before / after | Mutant witnesses before / after |",
                  "| --- | --- | --- |"]
        for d in DIMENSIONS:
            x, y = a["per_dimension"][d], b["per_dimension"][d]
            lines.append(f"| {d} | {x['gap']} / {y['gap']} | {x['mutant']} / {y['mutant']} |")
        for source in ("synthesized", "proposed"):
            lines += ["", f"### {source.capitalize()}", ""]
            for verdict in ("kept", "rejected", "disagreeing"):
                for c in rnd[verdict]:
                    if c["source"] != source:
                        continue
                    lines += [f"- {verdict}: {c['id']}; targets {', '.join(c['targets'])}.",
                              "  Reading: " + c["plain"], "  Laws: " + " ".join(c["laws"]),
                              "  Evidence: " + c["packet"]]
                    if c.get("closed_witnesses"):
                        lines.append("  Closes: " + ", ".join(w["id"] for w in c["closed_witnesses"]))
                    if verdict == "disagreeing":
                        lines.append("  Disagreement witness: " + canonical(c["witness"]))
                    if c.get("reason"):
                        lines.append("  Reason: " + c["reason"])
        for c in rnd.get("unchecked", []):
            lines += ["", f"Unchecked: {c['id']} ({c['status']}); evidence {c['packet']}."]
    lines += ["", "## Residual emitted witnesses", ""]
    for w in (report.get("residual") or {}).get("witnesses", []):
        lines.append(f"- {w['id']}: {', '.join(w['dimensions'])}; {canonical(w['witness'])}")
    return "\n".join(lines) + "\n"


def run(options):
    out, contract = options.out.resolve(), options.directory.resolve()
    if out == contract or contract in out.parents:
        raise ValueError("--out must be outside the contract directory")
    out.mkdir(parents=True, exist_ok=True)
    if any(out.iterdir()):
        raise ValueError("--out must be an empty directory")
    # Each invocation reads the same owned bytes, even if the caller edits DIR.
    subject = out / "subject"
    for name in ("project.zeno", "v2/policy.json", "tests/decision-examples.txt"):
        original = contract / name
        if original.exists():
            destination = subject / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(original.read_bytes())
    project = read_text(subject / "project.zeno")
    policy = json.loads(read_text(subject / "v2/policy.json"))
    used = {int(x) for x in re.findall(r"\b(?:law|claim)\s+(\d+)", project)}
    next_id = 9000
    def allocate():
        nonlocal next_id
        while next_id in used:
            next_id += 1
        if next_id > 65535:
            raise ValueError("proposal law identifiers exhausted")
        used.add(next_id)
        result = next_id
        next_id += 1
        return result
    supplied = proposals(options.proposals, options.zal, project, allocate)
    binary = Path(os.environ.get("ZENO_FCIS_BIN", DEFAULT_BIN)).resolve()
    report = {"schema": SCHEMA, "authority": "none", "status": "ran", "exit_code": 0, "rounds": [],
              "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
              "sources": {"project_sha256": hashlib.sha256(project.encode()).hexdigest(),
                          "policy_sha256": hashlib.sha256((subject / "v2/policy.json").read_bytes()).hexdigest()},
              "scope": "witnessed by the gap judge's mutants and gaps; proposals require human confirmation"}
    here = Path(__file__).resolve().parent
    report["adapter_sources"] = {name: hashlib.sha256((here / name).read_bytes()).hexdigest()
                                 for name in ("loop_coverage.py", "factory.py", "behavior.py", "behavior2.py",
                                              "loop_language.py", "loop_judges.py", "run.py")}
    judge = Judge(binary, subject, out, options.max_work)
    kept_laws, tried, pending = [], set(), list(supplied)
    current = None
    try:
        code, packet, packet_path = judge.review()
        if disagreement(packet):
            report["baseline_disagreement"] = disagreement(packet)
            raise JudgeStop(1, "disagreement")
        if code or packet.get("gaps", {}).get("status") != "measured":
            raise JudgeStop(code if code else 65, packet.get("gaps", {}).get("status", "invalid-packet"))
        current = snapshot(packet, project, policy)
        report["initial"] = current
        for round_number in range(1, options.rounds + 1):
            rnd = {"round": round_number, "before": current, "after": current,
                   "kept": [], "rejected": [], "disagreeing": []}
            report["rounds"].append(rnd)
            candidates = pending + (synthesize(packet, policy, current, allocate) if current["gaps"] or current["admitted_mutants"] else [])
            pending = []
            for candidate in candidates:
                if candidate.source == "synthesized" and not current["gaps"] and not current["admitted_mutants"]:
                    break
                # IDs vary across rounds; compare the law bodies and source instead.
                signature = canonical([candidate.source, [re.sub(r"^law \d+", "law", l) for l in candidate.laws]])
                if signature in tried:
                    continue
                tried.add(signature)
                code, checked, reference = judge.review(tuple(kept_laws) + candidate.laws)
                try:
                    verdict, detail = assess(checked, code, current, project, policy)
                except JudgeStop as stop:
                    rnd.setdefault("unchecked", []).append({**asdict(candidate), "status": stop.status,
                                                             "packet": reference, "binary_exit": code})
                    raise
                rnd[verdict].append({**asdict(candidate), **detail, "packet": reference})
                if verdict == "disagreeing":
                    report["exit_code"] = 1
                elif verdict == "kept":
                    kept_laws.extend(candidate.laws)
                    current, packet = detail["after"], checked
                    rnd["after"] = current
            if not current["gaps"] and not current["admitted_mutants"]:
                report["status"] = "no-residual-witness"
                break
            if not rnd["kept"]:
                report["status"] = "no-progress"
                break
    except JudgeStop as stop:
        report["status"] = stop.status
        report["exit_code"] = stop.code
    if report["exit_code"] == 1:
        report["status"] = "disagreement"
    report["residual"] = current
    report["kept_laws"] = kept_laws
    report["binary_calls"] = judge.calls
    (out / "coverage.json").write_text(json.dumps(report, indent=2) + "\n")
    (out / "COVERAGE.md").write_text(markdown(report))
    print(json.dumps({"schema": SCHEMA, "status": report["status"], "exit": report["exit_code"],
                      "before": {k: report.get("initial", {}).get(k) for k in ("gaps", "admitted_mutants")},
                      "after": {k: (current or {}).get(k) for k in ("gaps", "admitted_mutants")}}))
    return report["exit_code"]


def add_command(subparsers):
    parser = subparsers.add_parser("coverage", help=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--zal", type=Path)
    parser.add_argument("--proposals", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--rounds", type=int, default=3, choices=range(1, 33), metavar="N")
    parser.add_argument("--max-work", type=int, help="forward the gap judge's law-check budget")


def main(options):
    try:
        return run(options)
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(json.dumps({"status": "refused", "reason": str(error), "authority": "none"}), file=sys.stderr)
        return 3 if isinstance(error, OSError) else 65
