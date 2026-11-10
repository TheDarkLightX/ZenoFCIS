"""Read-only review desk for the finite ZAL profiles. It shows ZAL's own results and records no agreement.

    python3 integrations/zal/review_desk.py serve SOURCE.zal [--port 8770]
    python3 integrations/zal/review_desk.py snapshot SOURCE.zal [MORE.zal ...] --out PAGE.html
    python3 integrations/zal/review_desk.py data SOURCE.zal

`serve` opens one source file on a loopback page and notices when the file changes on disk. `snapshot`
writes one self-contained page with every result computed beforehand. `data` prints what the page reads.

Every status, sentence, decision and comparison on the page comes from the modules run.py dispatches to:
parse, check, legend, explain, why-not, the counterexample story and the semantic diff. The desk adds three
things, and the page labels each as the desk's own: planted rule edits that ZAL re-checks, the breakdown of
which claim ZAL names for each edit, and a walk of ZAL's decisions from the initial configuration.

The desk never writes a source, a workspace or an agreement, and it has no accept, export or provider route.
This file and review_desk.html are outside the source closure that workflow.checker_identity hashes.
"""
from __future__ import annotations

import argparse
import hashlib
import itertools
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import re
import secrets
import sys
import threading
import time
from urllib.parse import urlsplit

import behavior
import behavior2
import diagnostics
import explain
import explain2
import json_numbers
from behavior import Refusal
from workflow import checker_identity

MAX_SOURCE = behavior2.MAX_SOURCE   # ZAL's own source bound
MAX_TABLE_TUPLES = 2500             # a page holds a decision for every input up to this many input tuples
MAX_NEAREST_TUPLES = 600            # and the nearest opposite decision for every input up to this many
MAX_BREAKDOWN_TUPLES = 4096         # above this the planted edits are not broken down by claim
MAX_WITNESSES = 8                   # distinguishing inputs kept for one compared source
SENTENCE_SAMPLES = 12               # inputs on which the composed decision sentence is compared with why-not
PAGE = Path(__file__).with_name("review_desk.html")
DATA_SLOT, TOKEN_SLOT = "/*__ZAL_REVIEW_DESK_DATA__*/null", "__ZAL_REVIEW_DESK_TOKEN__"
OTHER_PROFILES = frozenset({"zal effects 1;", "ZAL effects version 1.", "zal typed 1;", "ZAL typed version 1."})
STRUCTURAL_CHECKS = frozenset({"initial_nonempty", "typing", "determinism", "update_ranges",
                               "coverage_with_explicit_default"})
CLAIM_CONCEPTS = ("require", "property", "game")
PASSING = ("pass-with-scope", "unsupported")
INVARIANT_OBLIGATIONS = ("invariant", "invariant-preservation")
NAMED = re.compile(r"(?:rule|invariant|require|property|fairness|player|symmetry) (\w+)")
RULE_FIELDS = ("state_matches", "event_matches", "enabled", "guard", "updates")
JSON_TYPE = "application/json"
REQUEST_BOUND = 2 * MAX_SOURCE + 4096   # a candidate source as a JSON string, with room for its escapes
RESPONSE_HEADERS = (("Cache-Control", "no-store"), ("X-Content-Type-Options", "nosniff"), ("Referrer-Policy", "no-referrer"),
                    ("Content-Security-Policy", "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; "
                                                "connect-src 'self'; img-src data:; base-uri 'none'; form-action 'none'; "
                                                "frame-ancestors 'none'"))


# ---- ZAL's own operations, selected the way run.py selects them -----------------------------------------------------

def read_statement(text):
    """Parse one finite-profile source; returns the model and the module of its profile."""
    first = ""
    if isinstance(text, str):
        first = next((line.strip() for line in text.splitlines() if line.strip() and not line.lstrip().startswith("#")), "")
    if first in OTHER_PROFILES:
        raise Refusal("The review desk reads the finite profiles zal/finite-fsm/1, 2 and 3; "
                      "this source is a typed or effects profile")
    syntax = "english" if first.startswith("ZAL ") and first.endswith(".") else "symbolic"
    model = behavior2.parse_any(text, syntax)
    return model, (behavior2 if isinstance(model, behavior2.Machine) else behavior)


def explain_input(model, profile, given, checker):
    if profile is behavior2:
        return profile.explain(model, given["state"], given["variables"], given["event"], given["context"], checker)
    return profile.explain(model, given["state"], given["event"], given["context"], checker)


def why_not(model, profile, given, want, scope, checker):
    if profile is behavior2:
        return explain2.why_not(model, given["state"], given["variables"], given["event"], given["context"],
                                want, scope, checker)
    return explain.why_not(model, given["state"], given["event"], given["context"], want, scope, checker)


def input_tuples(model, profile):
    """Every declared input tuple, in ZAL's own enumeration order."""
    if profile is behavior2:
        for state, values in model.configurations():
            for event in model.events:
                for context in model.contexts():
                    yield {"state": state, "variables": dict(values), "event": event, "context": dict(context)}
    else:
        for state, event, context in model.inputs():
            yield {"state": state, "variables": {}, "event": event, "context": dict(context)}


def decision_sentence(model, profile, given):
    """ZAL's one-line account of a decision: the first line of its why-not answer.

    why_not scans the whole declared domain on every call, and a page needs this line for every input. So the
    line is composed here from the phrase functions why_not itself calls. check_sentences compares the two.
    """
    state, variables, event, context = given["state"], given["variables"], given["event"], given["context"]
    if profile is behavior2:
        current, overlap = explain2.outcome_of(model, state, variables, event, context)
        if overlap:
            raise Refusal("Overlapping transitions; no precedence is implied")
        return (f"From {explain2.configuration(state, variables)}, event {event} with "
                f"{explain2.observations(context)}: {explain2.decider_text(current)} {explain2.outcome_text(current)}.")
    current = model.decide(state, event, context)
    return (f"From {state}, event {event} with {explain.observations(context)}: "
            f"{explain.decider_text(current)} {explain.outcome_text(current)}.")


def check_sentences(model, profile, count, checker):
    """Compare decision_sentence with why-not on evenly spaced inputs; a difference refuses the statement."""
    stride, compared = max(1, count // SENTENCE_SAMPLES), 0
    for given in itertools.islice(input_tuples(model, profile), 0, None, stride):
        if compared == SENTENCE_SAMPLES:
            break
        try:
            expected = why_not(model, profile, given, {"class": "Accept"}, "context", checker)["summary"][0]
        except ValueError:                                             # overlapping rules: why-not gives no line
            continue
        compared += 1
        if decision_sentence(model, profile, given) != expected:
            raise Refusal("The review desk's decision sentence no longer matches ZAL's why-not answer; "
                          "update review_desk.decision_sentence")
    return compared


def nearest_opposite(model, profile, given, outcome, checker):
    """ZAL's nearest declared input that is decided the other way."""
    want = {"class": "Reject" if outcome["class"] == "Accept" else "Accept"}
    answer = why_not(model, profile, given, want, "input", checker)
    first = answer["minimal_changes"][0] if answer.get("minimal_changes") else None
    return {"want": want["class"], "status": answer["status"], "distance": answer.get("distance"),
            "count": answer.get("minimal_count"),
            "first": {key: first[key] for key in ("changes", "input", "outcome")} if first else None}


def counterexample_story(model, report, checker):
    try:
        told = explain.narrate_check(model, report, checker)
    except (ValueError, KeyError, TypeError) as error:
        return {"status": "not-narrated", "reason": str(error)[:300]}
    return {key: told.get(key) for key in ("status", "obligation", "english", "symbolic", "method")}


# ---- what the page reads about one statement --------------------------------------------------------------------------

class DistinctValues:
    """Keeps each distinct JSON value once; a decision table repeats the same evaluations many times."""

    def __init__(self):
        self.items, self.index = [], {}

    def add(self, value):
        key = json.dumps(value, sort_keys=True, separators=(",", ":"))
        if key not in self.index:
            self.index[key] = len(self.items)
            self.items.append(value)
        return self.index[key]


def declarations(model, profile):
    """ZAL's legend entries, one per rendered line, each with the name its line declares."""
    out = []
    for entry in profile.legend(model)["entries"]:
        found = NAMED.match(entry["symbolic"]) if entry["concept"] not in ("header", "name") else None
        out.append({"concept": entry["concept"], "symbolic": entry["symbolic"], "english": entry["english"],
                    "id": found.group(1) if found else None})
    return out


def input_axes(data):
    return {"states": data["states"], "events": data["events"], "facts": data.get("context", []),
            "inputs": [{"name": name, "low": low, "high": high} for name, low, high in data.get("inputs", [])],
            "variables": [{"name": name, "low": low, "high": high, "initial": initial}
                          for name, low, high, initial in data.get("variables", [])],
            "initial": data["initial"]}


def input_key(axes, given):
    """The page's name for one input tuple: state, variables, event, facts and integer inputs in declared order."""
    parts = [given["state"]] + [given["variables"][v["name"]] for v in axes["variables"]] + [given["event"]]
    parts += [int(given["context"][fact]) for fact in axes["facts"]] + [given["context"][i["name"]] for i in axes["inputs"]]
    return "|".join(str(part) for part in parts)


def decision_table(model, profile, axes, count, checker):
    """ZAL's explanation of every declared input, or the reason a page does not hold one."""
    if count > MAX_TABLE_TUPLES:
        return {"status": "not-precomputed",
                "reason": f"it has {count:,} input tuples, and a page holds a table for at most {MAX_TABLE_TUPLES:,}"}
    kept = {name: DistinctValues() for name in ("outcome", "rule", "claim", "sentence", "nearest")}
    static, index, rows, with_nearest = None, {}, [], count <= MAX_NEAREST_TUPLES
    for given in input_tuples(model, profile):
        told = explain_input(model, profile, given, checker)
        if static is None:
            static = {"rules": [{key: rule[key] for key in ("id", "source", "event", "guard_english")} for rule in told["rules"]],
                      "requirements": [{"id": r["id"], "english": r["english"]} for r in told.get("requirements", [])],
                      "invariants": [{"id": r["id"], "english": r.get("english")} for r in told.get("invariants", [])]}
        # A rule written for another state or event is only counted by the page, so its guard evaluation is left out.
        rules = [kept["rule"].add({key: rule[key] for key in (RULE_FIELDS if rule["state_matches"] and rule["event_matches"]
                                                             else RULE_FIELDS[:3]) if key in rule}) for rule in told["rules"]]
        nearest = nearest_opposite(model, profile, given, told["outcome"], checker) if with_nearest else None
        index[input_key(axes, given)] = len(rows)
        rows.append([kept["outcome"].add(told["outcome"]), int(bool(told.get("admissible", True))), rules,
                     [kept["claim"].add({"value": r["value"], "tree": r.get("tree")}) for r in told.get("requirements", [])],
                     [kept["claim"].add({key: value for key, value in r.items() if key not in ("id", "english")})
                      for r in told.get("invariants", [])],
                     kept["sentence"].add(decision_sentence(model, profile, given)), kept["nearest"].add(nearest)])
    return {"status": "complete", "count": len(rows), "static": static, "index": index, "rows": rows,
            "kept": {name: values.items for name, values in kept.items()}}


def reachable_configurations(model, profile, axes):
    """The desk's own walk of ZAL's decisions from the initial configuration; compared with ZAL's count."""
    start = (axes["initial"], tuple(v["initial"] for v in axes["variables"]))
    contexts = list(model.contexts()) if profile is behavior2 else None
    seen, frontier = {start}, [start]
    while frontier:
        state, values = frontier.pop()
        named = {v["name"]: values[i] for i, v in enumerate(axes["variables"])}
        for event in axes["events"]:
            if profile is behavior2:
                outcomes = [model.decide(state, named, event, context) for context in contexts]
            else:
                outcomes = [model.decide(state, event, context) for s, e, context in model.inputs() if (s, e) == (state, event)]
            for outcome in outcomes:
                after = (outcome["state"], tuple(outcome.get("variables", {}).get(v["name"], named[v["name"]])
                                                 for v in axes["variables"]))
                if after not in seen:
                    seen.add(after)
                    frontier.append(after)
    return sorted("|".join([state, *map(str, values)]) for state, values in seen)


def compare_candidate(base, base_profile, text, checker, with_story=True):
    """Parse, check and semantically compare one candidate source with the base model.

    Returns what ZAL said and the candidate model; the model is None when ZAL refuses the candidate.
    """
    try:
        model, profile = read_statement(text)
    except ValueError as error:
        located = None
        try:
            located = diagnostics.describe(error, text, Path("candidate.zal"))
        except Exception:                                              # the located form is a courtesy
            pass
        return {"status": "refused", "diagnostic": str(error), "source_diagnostic": located}, None
    try:
        report = profile.check(model, checker)
    except ValueError as error:                                        # for example a declared symmetry that no longer holds
        return {"status": "refused", "diagnostic": str(error), "source_diagnostic": None}, None
    shown = report.get("witness") or {}
    told = {"status": report["status"], "revision": model.revision, "obligation": report.get("obligation"),
            "advisories": report.get("advisories", []), "checks": report.get("checks"),
            # The check ZAL reports as failed. Checks after it are "not-run", so "not passing" would name the wrong one.
            "failed_check": next((name for name, value in (report.get("checks") or {}).items() if value == "counterexample"), None),
            "failed_claim": shown.get("english") or report.get("english"),
            "failed_name": report.get("requirement") or report.get("invariant") or report.get("property")}
    if report["status"] == "counterexample" and with_story:
        told["story"] = counterexample_story(model, report, checker)
    if profile is base_profile:
        diff = profile.semantic_diff(base, model)
        told["diff"] = {"comparison": diff["comparison"], "changed_objects": diff["changed_objects"],
                        "domain_same": diff["domain_same"], "witness_count": len(diff["witnesses"]),
                        "witnesses": [{**witness, "sentences": witness_sentences(base, model, profile, witness)}
                                      for witness in diff["witnesses"][:MAX_WITNESSES]]}
    else:
        told["diff"] = {"comparison": "different-profile", "changed_objects": [], "witness_count": 0, "witnesses": []}
    return told, model


def witness_sentences(old, new, profile, witness):
    """ZAL's sentence for one distinguishing input, before and after; None for a changed initial configuration."""
    if "event" not in witness:
        return None
    given = {"state": witness["state"], "variables": witness.get("variables", {}), "event": witness["event"],
             "context": witness.get("context", {})}
    try:
        return {"before": decision_sentence(old, profile, given), "after": decision_sentence(new, profile, given)}
    except (ValueError, KeyError, TypeError):
        return None


# ---- planted rule edits: the desk's own instrument ------------------------------------------------------------------

def rule_edits(lines, declared):
    """Delete each rule, replace its guard with true, and remove its updates: one small edit at a time."""
    for at, entry in enumerate(declared):
        if entry["concept"] != "rule":
            continue
        line, name = entry["symbolic"], entry["id"]
        head, opened, rest = line.partition(" [")
        guard, arrow, outcome = rest.rpartition("] -> ")
        if not (name and opened and arrow and outcome.endswith(";")):
            raise Refusal(f"Rule line is not in the rendered form the review desk edits: {line[:120]}")
        edits = [("delete", f"Delete rule {name}", None)]
        if guard != "true":
            edits.append(("open", f"Replace the guard of rule {name} with true", f"{head} [true] -> {outcome}"))
        if outcome.startswith("accept") and "," in outcome:
            edits.append(("strip", f"Remove the updates of rule {name}", f"{head} [{guard}] -> {outcome.split(',')[0]};"))
        for kind, label, after in edits:
            yield ({"kind": kind, "rule": name, "label": label, "line": at, "after": after},
                   lines[:at] + ([after] if after else []) + lines[at + 1:])


def report_of(lines, checker):
    """ZAL's check report for a statement given as its rendered lines; None when ZAL refuses it."""
    try:
        model, profile = read_statement("\n".join(lines) + "\n")
        return profile.check(model, checker)
    except ValueError:
        return None


def failing_claim(report):
    """The claim a report names as failed: a requirement or property name, "invariants", or None."""
    if report is None or report["status"] != "counterexample":
        return None
    if report.get("obligation") in INVARIANT_OBLIGATIONS:
        return "invariants"
    return report.get("requirement") or report.get("property")


def report_under_one_claim(lines, declared_concepts, keep, checker):
    """ZAL's report for the same lines with every other requirement and property removed.

    `keep` is the line of one requirement, property or game property, or None for the invariants alone.
    Invariants always stay, because they decide which configurations the other checks cover.
    """
    kept_concept = next((concept for line, concept in zip(lines, declared_concepts) if line == keep), None)

    def chosen(narrow):
        out = []
        for line, concept in zip(lines, declared_concepts):
            if concept in CLAIM_CONCEPTS and line != keep:
                continue
            if narrow and concept == "fairness" and kept_concept not in ("property", "game"):
                continue
            if narrow and concept == "player" and kept_concept != "game":
                continue
            out.append(line)
        return out

    report = report_of(chosen(True), checker)
    return report_of(chosen(False), checker) if report is None else report


def assumption_probes(lines, declared_concepts, data, checker):
    """For a two-player game with fairness: can the other player always bring each assumption about?

    The probe restates a fairness formula as that player's own game objective. ZAL admits only post-state
    fairness beside a game, so removing the primes gives the same formula over configurations. Nothing is said
    about an assumption unless ZAL decides its probe.
    """
    players = [player[0] for player in data.get("players", [])]
    claimants = sorted({game[1] for game in data.get("games", [])})
    fair = [(line, concept) for line, concept in zip(lines, declared_concepts) if concept == "fairness"]
    if len(players) != 2 or not claimants or not fair:
        return []
    bare = [line for line, concept in zip(lines, declared_concepts) if concept not in ("fairness", "property", "game")]
    taken = set(NAMED.findall("\n".join(lines)))
    probe = next(name for name in (f"desk_probe{n}" for n in itertools.count()) if name not in taken)
    out = []
    for claimant in claimants:
        other = next(player for player in players if player != claimant)
        for line, _ in fair:
            name = NAMED.match(line).group(1)
            prefix = f"fairness {name}: infinitely often ("
            if not (line.startswith(prefix) and line.endswith(");")):
                continue
            formula = line[len(prefix):-2].replace("'", "")
            report = report_of(bare + [f"property {probe}: {other} can force infinitely often ({formula});"], checker)
            if report is not None and report["status"] in ("pass-with-scope", "counterexample"):
                out.append({"assumption": name, "claimant": claimant, "other": other, "status": report["status"]})
    return out


def planted_edits(model, profile, count, checker):
    """Every rule edit, ZAL's re-check of it, what ZAL's semantic diff reports, and which claims ZAL names as failing."""
    declared = declarations(model, profile)
    lines, concepts = [entry["symbolic"] for entry in declared], [entry["concept"] for entry in declared]
    if lines != model.render().splitlines():
        raise Refusal("The legend no longer lists the rendered source line by line; the review desk cannot edit it")
    columns = [{"id": "invariants", "label": "the invariants together", "line": None}] if "invariant" in concepts else []
    columns += [{"id": entry["id"], "label": entry["id"], "line": entry["symbolic"]}
                for entry in declared if entry["concept"] in CLAIM_CONCEPTS]
    breakdown = count <= MAX_BREAKDOWN_TUPLES and bool(columns)
    if breakdown:                                                      # a column is usable only if the base passes under it
        for column in columns:
            alone = report_under_one_claim(lines, concepts, column["line"], checker)
            column["usable"] = alone is not None and alone["status"] in PASSING
    data = model.data()
    probes = assumption_probes(lines, concepts, data, checker)
    rows = []
    for edit, edited in rule_edits(lines, declared):
        edited_concepts = concepts[:edit["line"]] + (["rule"] if edit["after"] else []) + concepts[edit["line"] + 1:]
        told, candidate = compare_candidate(model, profile, "\n".join(edited) + "\n", checker, with_story=False)
        row = {**edit, "before": lines[edit["line"]], "status": told["status"], "noticed_by": [], "lost_assumptions": []}
        if candidate is None:
            rows.append({**row, "class": "refused", "diagnostic": told["diagnostic"]})
            continue
        row.update({key: told[key] for key in ("obligation", "failed_check", "failed_claim", "failed_name", "diff")})
        if told["status"] == "counterexample":
            row["class"] = "structure" if row["failed_check"] in STRUCTURAL_CHECKS else "claim"
        else:
            row["class"] = "unnoticed" if told["diff"]["comparison"] == "different" else "no-change"
        if breakdown and row["class"] == "claim":                     # an edit that passes whole fails under no single claim
            row["noticed_by"] = [column["id"] for column in columns if column["usable"] and failing_claim(
                report_under_one_claim(edited, edited_concepts, column["line"], checker)) == column["id"]]
        if row["class"] == "unnoticed" and probes:
            after = {(p["assumption"], p["claimant"]): p["status"]
                     for p in assumption_probes(edited, edited_concepts, candidate.data(), checker)}
            row["lost_assumptions"] = [p for p in probes if p["status"] == "pass-with-scope"
                                       and after.get((p["assumption"], p["claimant"])) == "counterexample"]
            if row["lost_assumptions"]:
                row["class"] = "vacuous"
        rows.append(row)
    note = None
    if not breakdown:
        note = ("the statement makes no claim to break the edits down by" if not columns else
                f"the statement has {count:,} input tuples, and the breakdown by claim stops at {MAX_BREAKDOWN_TUPLES:,}")
    return {"rows": rows, "columns": [{key: value for key, value in column.items() if key != "line"} for column in columns],
            "breakdown": breakdown, "breakdown_note": note, "probes": probes,
            "method": "Each rule is deleted, has its guard replaced with true, and has its updates removed. ZAL re-checks "
                      "every edited statement. A claim's column re-checks the edit with the other requirements and "
                      "properties removed and the invariants kept, and is marked when ZAL names that claim as the one "
                      "that fails. When an invariant fails ZAL stops there, so the other claims are not asked about that "
                      "edit. A game's assumption is probed by restating it as the other player's own objective."}


def statement_bundle(text, name, with_edits=True):
    """Everything the page shows about one source; returns the bundle, the model and its profile module."""
    checker = checker_identity()
    model, profile = read_statement(text)
    report = profile.check(model, checker)
    data = model.data()
    axes = input_axes(data)
    count = report.get("input_tuples") or sum(1 for _ in input_tuples(model, profile))
    legend = profile.legend(model)
    bundle = {"name": data["name"], "file": name, "profile": data["profile"], "revision": model.revision,
              "comment": [line.lstrip("# ").rstrip() for line in text.splitlines() if line.lstrip().startswith("#")][:6],
              "symbolic": model.render(), "model": data, "axes": axes, "check": report,
              "declared": declarations(model, profile), "readings": legend.get("readings", []),
              "checker": checker, "python": sys.version.split()[0]}
    if report["status"] == "counterexample":
        bundle["story"] = counterexample_story(model, report, checker)
    passing = report["status"] in PASSING
    compared = check_sentences(model, profile, count, checker)
    try:
        bundle["decisions"] = decision_table(model, profile, axes, count, checker)
    except (ValueError, KeyError, TypeError) as error:                 # a statement with overlapping rules has no table
        bundle["decisions"] = {"status": "not-precomputed", "reason": f"ZAL could not explain every input ({str(error)[:200]})"}
    walked = reachable_configurations(model, profile, axes) if passing and count <= MAX_TABLE_TUPLES else None
    expected = report.get("reachable_configurations", len(report["reachable_states"]) if "reachable_states" in report else None)
    bundle["reachable"] = walked
    bundle["cross_checks"] = {"tuples": bundle["decisions"].get("count") in (None, count), "sentences_compared": compared,
                              "reachable": None if walked is None or expected is None else len(walked) == expected}
    bundle["edits"] = planted_edits(model, profile, count, checker) if with_edits and passing else None
    return bundle, model, profile


# ---- the page ---------------------------------------------------------------------------------------------------------

def page(payload, token=""):
    """review_desk.html with its data and page token filled in."""
    template = PAGE.read_text(encoding="utf-8")
    if template.count(DATA_SLOT) != 1 or template.count(TOKEN_SLOT) != 1:
        raise Refusal("review_desk.html does not have exactly one data slot and one token slot")
    # Source text reaches the page inside this object. No "<" survives, so no source can close the script element.
    data = json.dumps(payload, separators=(",", ":"), ensure_ascii=True).replace("<", "\\u003c")
    return template.replace(TOKEN_SLOT, token).replace(DATA_SLOT, data)


def read_bounded(path):
    """At most one byte more than ZAL's source bound, so an oversized file is noticed without being read in full."""
    with open(path, "rb") as handle:
        return handle.read(MAX_SOURCE + 1)


def source_text(data):
    if len(data) > MAX_SOURCE or not data.isascii():
        raise Refusal("Source must be ASCII text at most 64 KiB")
    return data.decode("ascii")


def write_snapshot(sources, out):
    bundles = []
    for path in sources:
        started = time.perf_counter()
        try:
            bundle = statement_bundle(source_text(read_bounded(path)), path.name)[0]
        except (OSError, ValueError, KeyError, TypeError) as error:    # unreadable, refused by ZAL or in error: say so and go on
            print(f"{path.name}: left out, {error}", file=sys.stderr)
            continue
        bundles.append(bundle)
        print(f"{path.name}: {bundle['check']['status']}, {len((bundle['edits'] or {}).get('rows', []))} planted edits, "
              f"{time.perf_counter() - started:.1f}s", file=sys.stderr)
    if not bundles:
        raise SystemExit("no source could be read; nothing written")
    out.write_text(page({"mode": "snapshot", "bundles": bundles}), encoding="utf-8")
    print(f"wrote {out} ({out.stat().st_size} bytes)", file=sys.stderr)


# ---- the loopback page for one file ---------------------------------------------------------------------------------

class OpenStatement:
    """One source file as last read from disk, with ZAL's results for it. The file is never written."""

    def __init__(self, path):
        self.path, self.lock, self.digest = path, threading.Lock(), ""
        self.load()

    def load(self):
        self.bundle, self.model, self.profile, self.error = None, None, None, None
        text = ""
        try:
            data = read_bounded(self.path)
            self.digest = hashlib.sha256(data).hexdigest()             # recorded first, so a refused file is not re-read forever
            text = source_text(data)
            self.bundle, self.model, self.profile = statement_bundle(text, self.path.name)
        except OSError as error:
            self.digest = ""
            self.error = {"reason": f"The file could not be read: {error.strerror or error}"}
        except ValueError as error:                                    # ZAL refuses the source
            self.error = {"reason": str(error)}
            try:
                self.error = diagnostics.describe(error, text, Path(self.path.name))
            except Exception:                                          # the located form is a courtesy
                pass
        except (KeyError, TypeError) as error:                         # not a refusal: shown as an error, not hidden
            self.error = {"reason": f"{type(error).__name__}: {error}", "unexpected": True}

    def changed(self):
        try:
            return hashlib.sha256(read_bounded(self.path)).hexdigest() != self.digest
        except OSError:
            return bool(self.digest)                                   # a file that was readable and no longer is

    def act(self, route, data):
        if self.model is None:
            raise Refusal("ZAL refuses the file on disk; correct it and reload")
        if data.get("revision") != self.model.revision:
            raise Refusal("This page shows an older revision; reload")
        checker = checker_identity()
        if route == "explain":
            told = explain_input(self.model, self.profile, data["input"], checker)
            told["sentence"] = decision_sentence(self.model, self.profile, data["input"])
            told["nearest"] = nearest_opposite(self.model, self.profile, data["input"], told["outcome"], checker)
            return told
        if route == "candidate":
            return compare_candidate(self.model, self.profile, data["source"], checker)[0]
        raise Refusal("Unknown review desk action")

    def view(self):
        base = {"mode": "live", "file": self.path.name}
        return {**base, "bundles": [], "error": self.error} if self.bundle is None else {**base, "bundles": [self.bundle]}


def answer(statement, token, port, method, target, headers, read):
    """Decide one exchange with the loopback page: (status, body, content type). No socket is touched here.

    `headers` maps names to values and `read(n)` returns the request body; at most REQUEST_BOUND bytes are read.
    A request is the page's own only if it names this loopback server as its host, carries the page's token
    and, when it states an origin, states this server. A page on another origin can arrange none of the three.
    """
    here = f"127.0.0.1:{port}"
    local = headers.get("Host") == here
    own = (local and headers.get("Origin", f"http://{here}") == f"http://{here}"
           and secrets.compare_digest(headers.get("X-ZAL-Review-Desk", "").encode("utf-8", "replace"), token.encode()))
    if method == "GET":
        if not local:
            return 403, {"error": "Open the review desk at its 127.0.0.1 address"}, JSON_TYPE
        if target == "/":
            with statement.lock:
                if statement.changed():                                # a browser refresh shows the file as it is now
                    statement.load()
                return 200, page(statement.view(), token), "text/html"
        if target == "/favicon.ico":
            return 204, "", "image/x-icon"
        if target == "/api/changed" and own:
            return 200, {"changed": statement.changed()}, JSON_TYPE
        return 404, {"error": "Unknown or unauthorized route"}, JSON_TYPE
    if method != "POST" or not own:
        return 403, {"error": "The review desk page's own token is required"}, JSON_TYPE
    try:
        length = int(headers.get("Content-Length", "0"))
        if not 0 < length <= REQUEST_BOUND:
            raise Refusal("Request is empty or exceeds the review desk bound")
        data = json.loads(read(length), **json_numbers.hooks(json_numbers.raising(Refusal, "request: ")))
        if not isinstance(data, dict):
            raise Refusal("Expected a review desk request object")
        with statement.lock:
            return 200, statement.act(urlsplit(target).path.removeprefix("/api/"), data), JSON_TYPE
    except (ValueError, KeyError, TypeError, OSError) as error:
        return 400, {"error": str(error)[:2000]}, JSON_TYPE


def make_server(path, port):
    """The loopback server for one file: a thin shell that reads a request, asks answer() and writes the reply."""
    statement, token = OpenStatement(path), secrets.token_urlsafe(32)

    class Handler(BaseHTTPRequestHandler):
        timeout = 30

        def log_message(self, *args):
            pass

        def exchange(self):
            status, body, mime = answer(statement, token, self.server.server_port, self.command, self.path,
                                        self.headers, self.rfile.read)
            data = body.encode() if isinstance(body, str) else json.dumps(body, separators=(",", ":")).encode()
            self.send_response(status)
            self.send_header("Content-Type", mime + "; charset=utf-8")
            self.send_header("Content-Length", str(len(data)))
            for name, value in RESPONSE_HEADERS:
                self.send_header(name, value)
            self.end_headers()
            self.wfile.write(data)

        do_GET = do_POST = exchange

    return ThreadingHTTPServer(("127.0.0.1", port), Handler), token


def serve(path, port=8770):
    try:
        httpd, _ = make_server(path, port)
    except OSError as error:
        raise SystemExit(f"cannot listen on 127.0.0.1:{port} ({error.strerror}); pass --port") from None
    print(f"ZAL review desk: http://127.0.0.1:{httpd.server_port} ({path.name}; loopback only; records no agreement)", flush=True)
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        httpd.server_close()


def main(argv=None):
    parser = argparse.ArgumentParser(description="Read-only ZAL review desk for the finite profiles")
    parser.add_argument("command", choices=["serve", "snapshot", "data"])
    parser.add_argument("sources", type=Path, nargs="+")
    parser.add_argument("--port", type=int, default=8770)
    parser.add_argument("--out", type=Path, help="snapshot: the page to write")
    options = parser.parse_args(argv)
    if options.command == "snapshot":
        if options.out is None:
            raise SystemExit("snapshot needs --out PAGE.html")
        write_snapshot(options.sources, options.out)
        return 0
    if len(options.sources) != 1 or not options.sources[0].is_file():
        raise SystemExit(f"{options.command} takes one existing source file")
    if options.command == "serve":
        serve(options.sources[0].resolve(), options.port)
    else:
        try:
            bundle = statement_bundle(source_text(read_bounded(options.sources[0])), options.sources[0].name)[0]
        except ValueError as error:
            print(json.dumps({"status": "refused", "diagnostic": str(error), "authority": "none"}, indent=2), file=sys.stderr)
            return 2
        print(json.dumps(bundle, indent=1))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
