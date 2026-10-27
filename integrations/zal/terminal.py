"""Human keyboard review over the same shared workspace used by coding harnesses."""

import argparse
import json
from pathlib import Path
import shlex
import unicodedata

from behavior import Refusal, canonical, check, explain, help_topic, legend, replay
import constructs
import explain as explanations
import ladder_tools
from shared import SharedWorkspace
from workflow import checker_identity


HELP = """show | select TYPED_ID | legend [TOPIC] | check | candidate
? [TOPIC] | man [TOPIC] | help [TOPIC] | translate english|symbolic
why (selected object's built-in meaning)
why STATE EVENT fact=yes (exact local guard/outcome explanation)
propose FILE [symbolic|english]
trace submit authorized=no then approve authorized=yes
whynot STATE EVENT fact=no want Accept [TARGET] | want Reject [REASON] | want rule ID [scope input]
whatif STATE EVENT fact=no change fact=yes [state:NAME] [event:NAME]
story | story candidate | story trace submit authorized=no then approve authorized=yes
guide [TOPIC] (construct guide generated from the registry)
ladder list | ladder render ID [RUNG] | ladder parse ID RUNG TEXT | ladder lower ID
ladder same ID OTHER_ID | ladder same ID RUNG TEXT | ladder explain ID [[RUNG] TEXT]
labels (retained supplied intent examples, not authenticated owner identity)
label CASE STATE EVENT fact=yes expect Accept TARGET - "supplied provenance"
label CASE STATE EVENT fact=no expect Reject STATE REASON "supplied provenance"
accept | reject (the exact candidate last displayed here)
show | accept-current (review the current/initial model, with no pending candidate)
cancel | quit
Ask/propose in your usual Codex/Claude coding session through the MCP bridge.
This human loop owns acceptance; tool/command approval does not accept meaning.
Full-hash accept also needs the displayed corpus digest; JSON traces remain available.
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


def is_ladder(line):
    return line.split(None, 1)[:1] == ["ladder"]


def ladder_command(line):
    """Ladder statements bind the revision read at command time; text keeps its exact spelling."""
    words = line.split(None, 2)
    action, rest = (words[1], words[2] if len(words) > 2 else "") if len(words) > 1 else ("", "")
    if action == "list" and not rest:
        return ladder_tools.read_catalogue(checker_identity())
    statement, _, rest = rest.partition(" ")
    rest = rest.strip()
    current = {item["id"]: item.get("revision") for item in ladder_tools.read_catalogue()["statements"]}
    if statement not in current:
        raise Refusal("Unknown ladder statement; type ladder list")
    revision, identity = current[statement], checker_identity()
    first, _, text = rest.partition(" ")
    rung = first if first in ladder_tools.l.RUNGS else None
    if action == "render" and (not rest or rung and not text):
        return ladder_tools.render(statement, revision, rung, identity)
    if action == "lower" and not rest:
        return ladder_tools.lower(statement, revision, identity)
    if action == "parse" and rung and text.strip():
        return ladder_tools.parse(statement, revision, rung, text.strip(), identity)
    if action == "same" and first in current and not text:
        return ladder_tools.same(statement, revision, first, current[first], renderer=identity)
    if action == "same" and rung and text.strip():
        return ladder_tools.same(statement, revision, rung=rung, text=text.strip(), renderer=identity)
    if action == "explain":
        if not rest:
            return ladder_tools.explain(statement, revision, renderer=identity)
        return ladder_tools.explain(statement, revision, rung, text.strip() if rung else rest, identity)
    raise Refusal("Unknown ladder command; type help")


def input_changes(words):
    """Parse what-if changes: fact=yes|no, state:NAME and event:NAME."""
    change, context = {}, {}
    for word in words:
        kind, colon, name = word.partition(":")
        if colon and kind in {"state", "event"} and "=" not in word:
            if kind in change:
                raise Refusal("Change the state or event at most once")
            change[kind] = name
            continue
        name, separator, value = word.partition("=")
        if not separator or name in context or value not in {"yes", "no", "true", "false"}:
            raise Refusal("Use fact=yes, fact=no, state:NAME or event:NAME, each once")
        context[name] = value in {"yes", "true"}
    return {**change, **({"context": context} if context else {})}


def explanation_command(session, name, args, line):
    identity = checker_identity()
    if name == "story":
        if not args:
            return explanations.narrate_check(session.model, check(session.model, identity), identity)
        if args == ["candidate"]:
            if not session.proposal:
                raise Refusal("No pending candidate to narrate")
            candidate = session.proposal["model"]
            return explanations.narrate_check(candidate, check(candidate, identity), identity)
        if args[0] == "trace":
            return explanations.narrate_trace(session.model, trace_inputs(line.split(None, 2)[2] if len(args) > 1 else "[]"), identity)
        raise Refusal("Use story, story candidate or story trace STEPS")
    marker = "want" if name == "whynot" else "change"
    if marker not in args[2:]:
        raise Refusal(f"Supply {name} STATE EVENT observations {marker} ...")
    split = args.index(marker, 2)
    given = trace_inputs(" ".join(args[1:split]))
    if len(given) != 1:
        raise Refusal(f"{name} explains one explicit state/event input")
    state, event, context, tail = args[0], given[0]["event"], given[0]["context"], args[split + 1:]
    if name == "whatif":
        return explanations.what_if(session.model, state, event, context, input_changes(tail), identity)
    scope = "context"
    if tail[-2:-1] == ["scope"]:
        scope, tail = tail[-1], tail[:-2]
    if len(tail) == 2 and tail[0] == "rule":
        want = {"rule": tail[1]}
    elif 1 <= len(tail) <= 2 and tail[0] in {"Accept", "Reject"}:
        want = {"class": tail[0], **({("state" if tail[0] == "Accept" else "reason"): tail[1]} if len(tail) == 2 else {})}
    else:
        raise Refusal("Use want Accept [TARGET], want Reject [REASON] or want rule ID")
    return explanations.why_not(session.model, state, event, context, want, scope, identity)


def command(session, line):
    if is_ladder(line):
        return ladder_command(line)
    words = shlex.split(line)
    if not words:
        return None
    name, args = words[0], words[1:]
    if name == "show" and not args:
        view = session.view()
        return {"revision": session.model.revision, "selected": session.selected,
                "agreement": session.agreement, "realization": session.realization,
                "english": session.model.render(True), "symbolic": session.model.render(),
                "evidence": session.evidence, "object_ids": sorted(session.objects()),
                **{key: view[key] for key in ["owner_examples", "examples_digest", "example_comparison", "freshness",
                                             "supplied_assumptions", "authenticated_facts"]}}
    if name == "labels" and not args:
        return {"kind": "owner-examples", "example_comparison": session.example_comparison(session.proposal["model"] if session.proposal else None),
                "freshness": session.freshness()}
    if name == "label":
        try:
            marker = args.index("expect", 3)
        except ValueError:
            raise Refusal("Supply label CASE STATE EVENT observations expect CLASS STATE REASON PROVENANCE") from None
        if len(args) != marker + 5:
            raise Refusal("Supply expected class, state, reason (- for Accept) and one quoted provenance explicitly")
        given = trace_inputs(" ".join(args[2:marker]))
        if len(given) != 1:
            raise Refusal("Owner example needs one exact input")
        session.supply_owner_example({"id": args[0], "input": {"state": args[1], **given[0]},
                "expected": {"class": args[marker + 1], "state": args[marker + 2],
                             "reason": None if args[marker + 3] == "-" else args[marker + 3]},
                "provenance": args[marker + 4]}, session.model.revision)
        return command(session, "labels")
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
    if name in {"whynot", "whatif"} and len(args) >= 4 or name == "story":
        return explanation_command(session, name, args, line)
    if name == "guide" and len(args) <= 1:
        return constructs.guide(session.model, args[0] if args else "", checker_identity())
    if name == "candidate" and not args:
        return session.view()["proposal"] or {"status": "no candidate"}
    if name == "propose" and 1 <= len(args) <= 2:
        with Path(args[0]).open(encoding="ascii") as stream:
            source = stream.read(65537)
        session.propose_human(source, args[1] if len(args) == 2 else "symbolic", session.model.revision)
        return session.view()["proposal"]
    if name == "reject" and len(args) == 2 or name == "accept" and len(args) == 3:
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
        self.current_displayed = None
        self.awaiting_current = None

    def execute(self, line):
        words = line.split() if is_ladder(line) else shlex.split(line)
        self.awaiting_display = None
        self.awaiting_current = None
        if words == ["show"]:
            self.current_displayed = None
        if words and words[0] in {"candidate", "propose"}:
            self.displayed = None
        def action(session):
            if words == ["accept-current"]:
                current = (session.model.revision, checker_identity(), session.view()["examples_digest"], len(session.events))
                if self.current_displayed != current or session.proposal:
                    raise Refusal("Current meaning changed or has not been displayed here; run show and review it first")
                session.accept_current_human(current[0], current[2])
                return {"agreement": session.agreement, "revision": session.model.revision,
                        "evidence": session.evidence, "realization": session.realization}, None, None
            if words in [["accept"], ["reject"]]:
                proposal = session.proposal
                current = None if not proposal else (
                    proposal["base_revision"], proposal["revision"], len(session.events), session.view()["examples_digest"])
                if self.displayed is None or self.displayed != current:
                    raise Refusal("Candidate changed or has not been displayed here; run candidate and review it first")
                arguments = [words[0], *self.displayed[:2], *([self.displayed[3]] if words == ["accept"] else [])]
                return command(session, " ".join(arguments)), None, None
            result = command(session, line)
            displayed = None
            if words and words[0] in {"candidate", "propose"} and session.proposal:
                displayed = (session.proposal["base_revision"], session.proposal["revision"], len(session.events), session.view()["examples_digest"])
            current = ((session.model.revision, checker_identity(), session.view()["examples_digest"], len(session.events))
                       if words == ["show"] and result["freshness"]["checks"]["status"] != "unavailable" else None)
            return result, displayed, current
        result, displayed, current = self.workspace.use(action)
        if current:
            self.awaiting_current = (canonical(result), current)
        if words and words[0] in {"candidate", "propose", "accept", "reject", "cancel"}:
            self.displayed = None
            self.awaiting_display = (canonical(result), displayed) if displayed else None
        if words and words[0] in {"accept", "reject", "accept-current", "cancel", "label"}:
            self.current_displayed = None
            self.displayed = None
        return result

    def present(self, result, writer=print, json_output=False):
        """Bind review only after the exact candidate was successfully rendered."""
        pending = self.awaiting_display
        current = self.awaiting_current
        try:
            if pending and canonical(result) != pending[0]:
                raise Refusal("Displayed candidate changed before rendering; review it again")
            if current and canonical(result) != current[0]:
                raise Refusal("Displayed current meaning changed before rendering; review it again")
            text = json.dumps(result, indent=2) if json_output else human_text(result)
            writer(text)
            if pending:
                self.displayed = pending[1]
            if current:
                self.current_displayed = current[1]
        finally:
            self.awaiting_display = None
            self.awaiting_current = None


def observations(context):
    return ", ".join(f"{name}={'yes' if value else 'no'}" for name, value in context.items()) or "no context facts"


def outcome(value):
    reason = f"; reason {value['reason']}" if value.get("reason") else ""
    return f"{value['class']} -> {value['state']}{reason}"


def freshness_lines(freshness):
    lines = []
    for name in ["checks", "review", "factory"]:
        item = freshness[name]
        lines.append(f"{name.capitalize()} freshness: {item['status']}")
        if item.get("reasons"):
            lines.append("  " + "; ".join(item["reasons"]))
        lines.append("  " + item["guidance"])
    if freshness.get("proposal"):
        item = freshness["proposal"]
        lines.append(f"Candidate checks freshness: {item['checks']['status']}; owner corpus: {item['corpus']}")
        lines.append("  " + item["checks"]["guidance"] + " " + item["guidance"])
    return lines


def example_lines(comparison):
    lines = [f"Supplied owner examples: {len(comparison['cases'])} | corpus {comparison['digest'][:12]}", comparison["scope"]]
    for case in comparison["cases"]:
        given = case["input"]
        lines.append(f"{case['id']}: {given['state']} / {given['event']} ({observations(given['context'])}); expected {outcome(case['expected'])}")
        lines.append("  Supplied provenance: " + case["provenance"])
        for revision in ["current", "candidate"]:
            if revision in case:
                result = case[revision]
                lines.append("  " + revision + ": " + result["status"] +
                             ("; " + outcome(result["actual"]) if "actual" in result else "; " + result["diagnostic"]))
    return lines


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
    if isinstance(result, dict) and str(result.get("kind", "")).startswith("ladder-"):
        return ladder_tools.human_text(result)
    if isinstance(result, dict) and result.get("kind") in {"why-not", "what-if", "narrative"}:
        return explanations.human_text(result)
    if isinstance(result, dict) and result.get("schema") == constructs.GUIDE_SCHEMA:
        return constructs.human_text(result)
    if isinstance(result, list):
        lines = ["Trace from the declared initial state (assumed inputs):"]
        lines.extend(f"{i}. {row['pre']} -- {row['event']} ({observations(row['context'])}) --> {outcome(row['outcome'])}"
                     for i, row in enumerate(result, 1))
        lines = lines if result else ["Empty trace; no step taken."]
        return "\n".join([*lines, "Supplied assumptions: each row's Boolean observations.",
                          "Authenticated facts: none supplied by this profile."])
    if result.get("kind") == "owner-examples":
        return "\n".join([*example_lines(result["example_comparison"]), *freshness_lines(result["freshness"]),
                          "Authenticated facts: none supplied by this profile."])
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
                 "Supplied assumptions: " + observations(result["supplied_assumptions"]["observations"]),
                 "Authenticated facts: none supplied by this profile.",
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
        lines.extend(example_lines(result["example_comparison"]))
        item = result["freshness"]
        lines.extend([f"Candidate checks freshness: {item['checks']['status']}; owner corpus: {item['corpus']}",
                      item["checks"]["guidance"], item["guidance"], "Authenticated facts: none supplied by this profile."])
        lines.append("Review the meaning and evidence, then type accept or reject. Any intervening change requires review again.")
        return "\n".join(lines)
    if "english" in result:
        selected = result["selected"]
        selected_lines = [line for line in result["english"].splitlines()
                          if selected.startswith("rule:") and line.startswith("Rule " + selected[5:] + ":")]
        return "\n".join([f"Current meaning {result['revision'][:12]} | agreement: {result['agreement']}",
                          "Selected: " + selected, *selected_lines, "", result["english"].rstrip(), "",
                          *evidence_lines(result["evidence"]), *example_lines(result["example_comparison"]),
                          *freshness_lines(result["freshness"]), "Supplied assumptions: owner labels and hypothetical Boolean observations.",
                          "Authenticated facts: none supplied by this profile.", "Use candidate to review a proposed change."])
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
    import sys
    # Owner-only capture provisioning; no equivalent MCP initialization tool.
    if sys.argv[1:2] == ["capture"]:
        from capture_workspace import terminal as capture_terminal
        raise SystemExit(capture_terminal(sys.argv[2:]))
    if sys.argv[1:2] == ["loop"]:
        # The owner's loop workspace commands: init, show, next, answer, say (loop_workspace.py).
        from loop_workspace import terminal as loop_terminal
        raise SystemExit(loop_terminal(sys.argv[2:]))
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
        client.present(client.execute("show"))
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
