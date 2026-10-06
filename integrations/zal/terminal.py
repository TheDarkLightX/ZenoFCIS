"""Human keyboard review over the same shared workspace used by coding harnesses."""

import argparse
import json
from pathlib import Path
import shlex
import unicodedata

from behavior import Refusal, canonical, check, explain, help_topic, legend, replay
from shared import SharedWorkspace
from workflow import checker_identity


HELP = """show | select TYPED_ID | legend [TOPIC] | check | candidate
? [TOPIC] | man [TOPIC] | help [TOPIC] | translate english|symbolic
why (selected object's built-in meaning)
why STATE EVENT fact=yes (exact local guard/outcome explanation)
propose FILE [symbolic|english]
trace submit authorized=no then approve authorized=yes
accept | reject (the exact candidate last displayed here)
cancel | quit
Ask/propose in your usual Codex/Claude coding session through the MCP bridge.
This human loop owns acceptance; tool/command approval does not accept meaning.
Full-hash accept/reject and JSON-array traces remain available for low-level use.
"""


def trace_inputs(text):
    """Read a short sequence of assumed events, retaining the JSON expert form."""
    if text.lstrip().startswith("["):
        return json.loads(text)
    inputs = []
    for step in text.split(" then "):
        words = shlex.split(step)
        if not words:
            raise Refusal("Trace needs an event; separate steps with 'then'")
        context = {}
        for observation in words[1:]:
            name, separator, value = observation.partition("=")
            if not separator or name in context or value not in {"yes", "no", "true", "false"}:
                raise Refusal("Use each fresh observation once, for example authorized=yes")
            context[name] = value in {"yes", "true"}
        inputs.append({"event": words[0], "context": context})
    return inputs


def command(session, line):
    words = shlex.split(line)
    if not words:
        return None
    name, args = words[0], words[1:]
    if name == "show" and not args:
        return {"revision": session.model.revision, "selected": session.selected,
                "agreement": session.agreement, "realization": session.realization,
                "english": session.model.render(True), "symbolic": session.model.render(),
                "evidence": session.evidence, "object_ids": sorted(session.objects())}
    if name == "select" and len(args) == 1:
        return session.select(args[0], session.model.revision)
    if name == "legend" and not args:
        return legend(session.model)
    if name in {"?", "man", "help", "legend"} and len(args) <= 1:
        return help_topic(session.model, args[0] if args else "", checker_identity())
    if name == "translate" and len(args) == 1 and args[0] in {"english", "symbolic"}:
        return {"kind": "translation", "revision": session.model.revision,
                "syntax": args[0], "surface": session.model.render(args[0] == "english"),
                "method": "deterministic renderer; no language model"}
    if name == "why" and not args:
        return help_topic(session.model, session.selected, checker_identity())
    if name == "why" and len(args) >= 2:
        inputs = trace_inputs(" ".join(args[1:]))
        if len(inputs) != 1:
            raise Refusal("why explains one explicit state/event input; use trace for sequences")
        return explain(session.model, args[0], inputs[0]["event"], inputs[0]["context"], checker_identity())
    if name == "check" and not args:
        return check(session.model, checker_identity())
    if name == "candidate" and not args:
        return session.view()["proposal"] or {"status": "no candidate"}
    if name == "propose" and 1 <= len(args) <= 2:
        with Path(args[0]).open(encoding="ascii") as stream:
            source = stream.read(65537)
        session.propose_human(source, args[1] if len(args) == 2 else "symbolic", session.model.revision)
        return session.view()["proposal"]
    if name in {"accept", "reject"} and len(args) == 2:
        if name == "accept":
            session.accept_human(*args)
        else:
            session.reject_human(*args)
        return {"agreement": session.agreement, "revision": session.model.revision,
                "evidence": session.evidence, "realization": session.realization}
    if name == "trace":
        return replay(session.model, trace_inputs(line.partition(" ")[2]))
    if name == "cancel" and not args:
        proposal = session.proposal
        if proposal:
            session.reject_human(proposal["base_revision"], proposal["revision"])
        return {"status": "Pending candidate cancelled; current meaning preserved"}
    raise Refusal("Unknown command or argument count; type help")


class ReviewClient:
    """Remember only the exact candidate this review client returned for display."""
    def __init__(self, workspace):
        self.workspace = workspace
        self.displayed = None
        self.awaiting_display = None

    def execute(self, line):
        words = shlex.split(line)
        self.awaiting_display = None
        if words and words[0] in {"candidate", "propose"}:
            self.displayed = None
        def action(session):
            if words in [["accept"], ["reject"]]:
                proposal = session.proposal
                current = None if not proposal else (
                    proposal["base_revision"], proposal["revision"], len(session.events))
                if self.displayed is None or self.displayed != current:
                    raise Refusal("Candidate changed or has not been displayed here; run candidate and review it first")
                return command(session, " ".join([words[0], *self.displayed[:2]])), None
            result = command(session, line)
            displayed = None
            if words and words[0] in {"candidate", "propose"} and session.proposal:
                displayed = (session.proposal["base_revision"], session.proposal["revision"], len(session.events))
            return result, displayed
        result, displayed = self.workspace.use(action)
        if words and words[0] in {"candidate", "propose", "accept", "reject", "cancel"}:
            self.displayed = None
            self.awaiting_display = (canonical(result), displayed) if displayed else None
        return result

    def present(self, result, writer=print, json_output=False):
        """Bind review only after the exact candidate was successfully rendered."""
        pending = self.awaiting_display
        try:
            if pending and canonical(result) != pending[0]:
                raise Refusal("Displayed candidate changed before rendering; review it again")
            text = json.dumps(result, indent=2) if json_output else human_text(result)
            writer(text)
            if pending:
                self.displayed = pending[1]
        finally:
            self.awaiting_display = None


def observations(context):
    return ", ".join(f"{name}={'yes' if value else 'no'}" for name, value in context.items()) or "no context facts"


def outcome(value):
    reason = f"; reason {value['reason']}" if value.get("reason") else ""
    return f"{value['class']} -> {value['state']}{reason}"


def evidence_lines(evidence):
    lines = [f"Check: {evidence['status']} (finite control-state profile)"]
    lines.append(f"Revision: {evidence['revision'][:12]}; checker: {evidence['checker'][:12]}")
    if "input_tuples" in evidence:
        lines.append(f"Scope: {evidence['input_tuples']} raw input tuples; fresh Boolean context on each step.")
    if evidence.get("unresolved"):
        lines.append("Unresolved requirements: " + ", ".join(item[0] for item in evidence["unresolved"]))
    if evidence.get("obligation"):
        lines.append("Finding: " + evidence["obligation"])
    witness = evidence.get("witness")
    if witness:
        lines.append("Witness state: " + witness["state"])
        if "event" in witness:
            lines.append(f"Event: {witness['event']} ({observations(witness['context'])})")
        if evidence.get("rules"):
            lines.append("Overlapping rules: " + ", ".join(evidence["rules"]))
        for i, step in enumerate(witness.get("trace", []), 1):
            lines.append(f"  {i}. {step['state']} -- {step['event']} ({observations(step['context'])}) --> {outcome(step['outcome'])}")
    lines.extend("Advisory: " + advisory["message"] for advisory in evidence.get("advisories", []))
    lines.append("Production authority: none. Factory realization is a separate operation.")
    return lines


def safe_text(text):
    """Make control, format and surrogate characters visible in terminal output."""
    return "".join(f"\\u{ord(char):04x}" if char not in {"\n", "\t"} and
                   unicodedata.category(char) in {"Cc", "Cf", "Cs"} else char for char in text)


def human_text(result):
    return safe_text(_human_text(result))


def guard_lines(node, indent="    "):
    label = node["operator"].upper()
    if node["operator"] == "fact":
        label = node["name"] + " observed " + ("yes" if node["observed"] else "no")
    elif node["operator"] == "state":
        label = f"state is {node['expected']} (observed {node['observed']})"
    lines = [indent + label + " = " + str(node["value"]).lower()]
    for child in node.get("operands", []):
        lines.extend(guard_lines(child, indent + "  "))
    return lines


def _human_text(result):
    """Readable abstraction-level presentation; JSON is an explicit option."""
    if isinstance(result, list):
        lines = ["Trace from the declared initial state (assumed inputs):"]
        lines.extend(f"{i}. {row['pre']} -- {row['event']} ({observations(row['context'])}) --> {outcome(row['outcome'])}"
                     for i, row in enumerate(result, 1))
        return "\n".join(lines if result else ["Empty trace; no step taken."])
    if result.get("kind") == "translation":
        return f"Current meaning {result['revision'][:12]} in {result['syntax']} (deterministic; no LLM):\n{result['surface']}"
    if result.get("kind") == "language-help":
        lines = [f"{result['title']} | {result['profile']} | revision {result['revision'][:12]}", result["meaning"]]
        for entry in result["entries"]:
            lines.extend([entry["english"], "  Symbolic: " + entry["symbolic"]])
        lines.extend(result["notes"])
        if result["topic"] == "overview":
            lines.append("Topics: " + ", ".join(result["available_topics"]))
        lines.append("Built-in language help; no LLM, agreement or production authority.")
        return "\n".join(lines)
    if result.get("kind") == "decision-explanation":
        given = result["input"]
        lines = [f"Local explanation at {result['revision'][:12]} (typed logic; no LLM)",
                 f"Input: {given['state']} -- {given['event']} ({observations(given['context'])})",
                 "Outcome: " + outcome(result["outcome"]), "Frame: " + result["frame"]]
        for rule in result["rules"]:
            causes = []
            if not rule["state_matches"]:
                causes.append("source state differs")
            if not rule["event_matches"]:
                causes.append("event differs")
            if not rule["guard"]["value"]:
                causes.append("guard is false")
            lines.append(f"Rule {rule['id']}: " + ("enabled" if rule["enabled"] else "; ".join(causes)))
            lines.append(f"  Guard: {rule['guard_english']} = {str(rule['guard']['value']).lower()}")
            if rule["state_matches"] and rule["event_matches"]:
                lines.extend(guard_lines(rule["guard"]))
        if result["used_default"]:
            lines.append("No rule is enabled; the visible default rejection applies.")
        lines.extend(["Effects: " + result["effects"], result["scope"]])
        return "\n".join(lines)
    if "model" in result and "base_revision" in result:
        lines = [f"Candidate {result['revision'][:12]} from {result['base_revision'][:12]}",
                 result["message"], "", "Proposed controlled-English meaning:", result["model"]["english"].rstrip(),
                 "", "Changed objects: " + (", ".join(result["diff"]["changed_objects"]) or "none in this listing"),
                 "Comparison: " + result["diff"]["comparison"],
                 "Compared observations: " + ", ".join(result["diff"]["observations"]),
                 "Excluded observations: " + ", ".join(result["diff"]["excluded_observations"])]
        if not result["diff"]["domain_same"]:
            lines.append("Domains changed; no cross-domain decision comparison was performed.")
        count = len(result["diff"]["witnesses"])
        lines.append(f"Showing {min(3, count)} of {count} concrete witnesses.")
        for witness in result["diff"]["witnesses"][:3]:
            if witness.get("kind") == "initial-state":
                lines.append(f"Initial state: {witness['before']} -> {witness['after']}")
            else:
                lines.append(f"Witness: from {witness['state']}, {witness['event']} with {observations(witness['context'])}")
                lines.append(f"  Before: {outcome(witness['before'])}; after: {outcome(witness['after'])}")
        lines.extend(evidence_lines(result["evidence"]))
        lines.append("Review the meaning and evidence, then type accept or reject. Any intervening change requires review again.")
        return "\n".join(lines)
    if "english" in result:
        selected = result["selected"]
        selected_lines = [line for line in result["english"].splitlines()
                          if selected.startswith("rule:") and line.startswith("Rule " + selected[5:] + ":")]
        return "\n".join([f"Current meaning {result['revision'][:12]} | agreement: {result['agreement']}",
                          "Selected: " + selected, *selected_lines, "", result["english"].rstrip(), "",
                          *evidence_lines(result["evidence"]), "Use candidate to review a proposed change."])
    if "entries" in result:
        lines = ["Legend: " + result["profile"]]
        for entry in result["entries"]:
            lines.extend([entry["english"], "  Symbolic: " + entry["symbolic"]])
        lines.append("Operators: " + "; ".join(x["symbol"] + " means " + x["english"] for x in result["operators"]))
        lines.append("Unsupported: " + ", ".join(result["unsupported"]))
        return "\n".join(lines)
    if "agreement" in result:
        return f"Meaning: {result['agreement']} at {result['revision'][:12]}. Factory realization: {result['realization']}."
    if "checks" in result:
        return "\n".join(evidence_lines(result))
    if result.get("act") == "select":
        return "Selected: " + result["data"]["object_id"]
    return str(result.get("status", "Done."))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["init", "review"])
    parser.add_argument("--workspace", type=Path, required=True)
    parser.add_argument("--source", type=Path, default=Path(__file__).parent / "examples/order.zal")
    parser.add_argument("--json", action="store_true", help="Print structured results instead of the human review view")
    options = parser.parse_args()
    workspace = SharedWorkspace(options.workspace)
    if options.mode == "init":
        with options.source.open(encoding="ascii") as stream:
            workspace.initialize(stream.read(65537))
        print(json.dumps({"status": "initialized", "agreement": "seed-example"}) if options.json else
              "Created a seed workspace; human agreement and realization remain separate.")
        return
    client = ReviewClient(workspace)
    if not options.json:
        print("ZAL human review · finite-fsm/1 · no production authority\n" + HELP)
        print(human_text(client.execute("show")))
    while True:
        try:
            line = input("" if options.json else "zal> ")
            if len(line) > 65536:
                raise Refusal("Terminal command exceeds64KiB")
            if line.strip() == "quit":
                return
            if line.strip() == "help":
                print(json.dumps({"help": HELP}) if options.json else HELP)
            result = client.execute(line)
            if result is not None:
                client.present(result, json_output=options.json)
        except KeyboardInterrupt:
            print(json.dumps({"status": "input-cancelled"}) if options.json else "\nInput cancelled; no acceptance performed.")
        except EOFError:
            return
        except (ValueError, TypeError, KeyError, OSError) as error:
            print(json.dumps({"status": "refused", "diagnostic": str(error)[:2000]}) if options.json else safe_text("Refused: " + str(error)[:2000]))


if __name__ == "__main__":
    main()
