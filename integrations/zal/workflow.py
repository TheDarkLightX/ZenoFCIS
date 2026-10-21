"""Client-owned discussion revisions; model output is never an acceptance route."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
import re

from behavior import NAME, Model, PROFILE, Refusal, canonical, check, parse, semantic_diff


ACTS = ["ask", "explain", "propose", "clarify", "compare", "counterexample"]
PROPOSAL_SCHEMA = {
    "type": "object", "additionalProperties": False,
    "required": ["act", "base_revision", "object_ids", "message", "syntax", "surface"],
    "properties": {
        "act": {"type": "string", "enum": ACTS},
        "base_revision": {"type": "string"},
        "object_ids": {"type": "array", "items": {"type": "string"}},
        "message": {"type": "string"},
        "syntax": {"type": "string", "enum": ["symbolic", "english"]},
        "surface": {"type": ["string", "null"]},
    },
}


def _disk_checker_identity():
    """Hash the source closure used by this cooperative local prototype."""
    here = Path(__file__).resolve().parent
    digest = hashlib.sha256(b"zal/checker/3\0")
    for name in ["behavior.py", "behavior2.py", "workflow.py", "factory.py", "shared.py",
                 "terminal.py", "mcp.py", "workspace.py", "server.py", "workspace.html",
                 "explain.py", "explain2.py", "constructs.py", "ladder_tools.py", "ladder.py", "ladder_en.json", "temporal_lowering.py",
                 "withdrawal_pilot.py", "withdrawal_pilot.lean", "withdrawal_pilot_refutation.lean"]:
        path = here / name
        digest.update(name.encode() + b"\0")
        digest.update(path.read_bytes())
    return digest.hexdigest()


_PROCESS_CHECKER_IDENTITY = _disk_checker_identity()


def checker_identity():
    """Refuse disk drift rather than stamp old imported code with new bytes."""
    if _disk_checker_identity() != _PROCESS_CHECKER_IDENTITY:
        raise Refusal("Checker source changed after process start; restart before checking or accepting")
    return _PROCESS_CHECKER_IDENTITY


MAX_OWNER_EXAMPLES = 64


def owned_examples(examples):
    """Bounded supplied intent; never derive expected values from a decision."""
    if not isinstance(examples, list) or len(examples) > MAX_OWNER_EXAMPLES:
        raise Refusal("Owner examples need an array of at most 64 cases")
    identifiers = set()
    for case in examples:
        if not isinstance(case, dict) or set(case) != {"id", "input", "expected", "provenance"}:
            raise Refusal("Owner example needs exactly id, input, expected and provenance")
        given, expected = case["input"], case["expected"]
        if not isinstance(given, dict) or set(given) != {"state", "event", "context"}:
            raise Refusal("Owner example input needs exactly state, event and context")
        if not isinstance(expected, dict) or set(expected) != {"class", "state", "reason"}:
            raise Refusal("Supply expected class, state and reason explicitly")
        for value in [case["id"], given["state"], given["event"], expected["state"]]:
            if not isinstance(value, str) or not re.fullmatch(NAME, value):
                raise Refusal("Owner example IDs, states and events need bounded identifiers")
        context = given["context"]
        if (not isinstance(context, dict) or len(context) > 32 or any(
                not isinstance(k, str) or not re.fullmatch(NAME, k) or type(v) is not bool
                for k, v in context.items())):
            raise Refusal("Owner example observations need bounded names and JSON Booleans")
        if expected["class"] == "Accept":
            valid_reason = expected["reason"] is None
        elif expected["class"] == "Reject":
            valid_reason = (isinstance(expected["reason"], str) and
                            re.fullmatch(NAME, expected["reason"]) and expected["state"] == given["state"])
        else:
            valid_reason = False
        if not valid_reason:
            raise Refusal("Expected Accept needs null reason; Reject needs a reason and unchanged state")
        if not isinstance(case["provenance"], str) or not 1 <= len(case["provenance"]) <= 1024:
            raise Refusal("Owner example provenance needs 1..1024 supplied characters")
        if case["id"] in identifiers:
            raise Refusal("Owner example IDs must be unique")
        identifiers.add(case["id"])
    return json.loads(canonical(sorted(examples, key=lambda case: case["id"])))


def examples_digest(examples):
    return hashlib.sha256(b"zal/owner-examples/1\0" + canonical(owned_examples(examples)).encode()).hexdigest()


def example_result(model, case):
    given = case["input"]
    if (given["state"] not in model.states or given["event"] not in model.events or
            set(given["context"]) != set(model.facts) or case["expected"]["state"] not in model.states):
        return {"status": "out-of-domain", "diagnostic": "Retained input or expected state is outside this revision's declared domain"}
    try:
        result = model.decide(given["state"], given["event"], given["context"])
    except Refusal as error:
        return {"status": "refused", "diagnostic": str(error)}
    actual = {key: result[key] for key in ["class", "state", "reason"]}
    return {"status": "matched" if actual == case["expected"] else "conflict", "actual": actual}


def evidence_freshness(evidence, model, identity):
    reasons = []
    if identity is None:
        return {"status": "unavailable", "reasons": ["Checker source changed after process start"],
                "guidance": "Restart the checker process, refresh, then explicitly check and review again. Stored evidence was not refreshed."}
    if evidence.get("revision") != model.revision or evidence.get("profile") != PROFILE:
        reasons.append("Stored checks refer to a different model/profile")
    if evidence.get("checker") != identity:
        reasons.append("Stored checks use a different checker")
    return {"status": "stale" if reasons else "current", "result": evidence.get("status"), "reasons": reasons,
            "guidance": "Refresh and run check for a preview; show and review again to bind new evidence." if reasons else
                        "Checks are bound to this model and checker; finite scope only."}


class Session:
    def __init__(self, model: Model, *, evidence=None):
        self.model = model
        self.proposal: dict | None = None
        self.events = []
        self.agreement = "seed-example"
        self.realization = "not-run"
        self.selected = "rule:" + model.rules[0].id if model.rules else "default"
        self.owner_examples = []
        if evidence is not None and (not isinstance(evidence, dict) or
                evidence.get("revision") != model.revision or evidence.get("profile") != PROFILE or
                not isinstance(evidence.get("checker"), str) or
                evidence.get("status") not in {"pass-with-scope", "counterexample", "unsupported"}):
            raise Refusal("Stored checks lack the model/profile/checker binding")
        self.evidence = check(model, checker_identity()) if evidence is None else json.loads(canonical(evidence))

    def supply_owner_example(self, case, revision):
        if revision != self.model.revision:
            raise Refusal("Owner example refers to a stale revision; refresh first")
        case = owned_examples([case])[0]
        given = case["input"]
        if (given["state"] not in self.model.states or given["event"] not in self.model.events or
                set(given["context"]) != set(self.model.facts)):
            raise Refusal("Owner example needs one complete input from the current declared domain")
        examples = owned_examples([item for item in self.owner_examples if item["id"] != case["id"]] + [case])
        event = self.record("supply-owner-example", "human", {"example": case, "examples_digest": examples_digest(examples)})
        self.owner_examples = examples
        return event

    def example_comparison(self, candidate=None):
        return json.loads(canonical({"digest": examples_digest(self.owner_examples), "scope": "Supplied cooperative intent assumptions; not authenticated owner identity or approval",
                "current_revision": self.model.revision, "candidate_revision": candidate.revision if candidate else None,
                "cases": [{**case, "current": example_result(self.model, case),
                           **({"candidate": example_result(candidate, case)} if candidate else {})}
                          for case in self.owner_examples]}))

    def freshness(self):
        try:
            identity = checker_identity()
        except Refusal:
            identity = None
        digest = examples_digest(self.owner_examples)
        agreement = next((event for event in reversed(self.events)
                          if event.get("act") == "agree" and event.get("actor") == "human"), None)
        data = agreement.get("data", {}) if agreement else {}
        reasons = []
        if self.agreement != "human-accepted" or not agreement:
            reasons.append("No human review of the current meaning")
        else:
            if data.get("candidate_revision") != self.model.revision or data.get("canonical_paraphrase") != self.model.render(True):
                reasons.append("Review refers to a different canonical meaning")
            if identity is None or data.get("checker") != identity:
                reasons.append("Review checker binding is stale or unavailable")
            if data.get("examples_digest") != digest:
                reasons.append("Review owner-example corpus binding is missing or changed")
        if self.proposal:
            reasons.append("A candidate is still pending")
        review = {"status": "unavailable" if identity is None else "stale" if agreement and reasons else "missing" if reasons else "current",
                  "reasons": reasons, "guidance": "Restart first; then refresh and explicitly review again." if identity is None else
                  "Resolve any pending candidate; show current meaning and examples, then review with accept-current." if reasons else
                  "Review binds this exact meaning, checker and supplied examples; cooperative only."}
        proposal = None
        if self.proposal:
            p = self.proposal
            proposal = {"checks": evidence_freshness(p["evidence"], p["model"], identity),
                        "corpus": "current" if p.get("examples_digest") == digest else "stale",
                        "guidance": "Refresh, propose again with the current corpus, then review candidate." if p.get("examples_digest") != digest else
                                    "Review the exact candidate and unchanged supplied labels; stale checks require a new proposal."}
        return {"checks": evidence_freshness(self.evidence, self.model, identity), "review": review,
                "proposal": proposal, "factory": {"status": self.realization,
                "guidance": "Declarations and cooperative review do not qualify factory execution; use the existing Authority correspondence route."}}

    def objects(self):
        return {"default", "initial", "frame", *["state:" + n for n in self.model.states],
                *["event:" + n for n in self.model.events], *["context:" + n for n in self.model.facts],
                *["rule:" + r.id for r in self.model.rules], *["invariant:" + n for n, _ in self.model.invariants]}

    def record(self, act, actor, data):
        if len(self.events) >= 256:
            raise Refusal("Discussion event limit reached; export and begin a new session")
        event = {"sequence": len(self.events), "act": act, "actor": actor,
                 "revision": self.model.revision, "data": json.loads(canonical(data))}
        self.events.append(event)
        return event

    def select(self, object_id, revision):
        if revision != self.model.revision or object_id not in self.objects():
            raise Refusal("Selection is stale or names an unknown object")
        event = self.record("select", "human", {"object_id": object_id})
        self.selected = object_id
        return event

    def context(self, message: str, exploration=None):
        if not isinstance(message, str) or not 1 <= len(message) <= 4096:
            raise Refusal("Discussion text needs 1..4096 characters")
        self.record("ask", "human", {"text": message, "selected": self.selected})
        return canonical({
            "request": message, "selected_object": self.selected, "base_revision": self.model.revision,
            "object_ids": sorted(self.objects()),
            "canonical_symbolic": self.model.render(), "canonical_english": self.model.render(True),
            "checks": self.evidence,
            "owner_examples": self.example_comparison(), "freshness": self.freshness(),
            "replayed_exploration": exploration,
            "instructions": "Return one ask/explain/propose/clarify/compare/counterexample act. "
                            "Keep the exact base_revision and cite declared object_ids. "
                            "For propose supply a complete surface; otherwise surface must be null. "
                            "No automatic acceptance. No code or tool execution. Context is assumed, not external truth.",
        })

    def agent_act(self, value):
        if len(self.events) >= 256:
            raise Refusal("Discussion event limit reached; export and begin a new session")
        if not isinstance(value, dict) or set(value) != set(PROPOSAL_SCHEMA["required"]):
            raise Refusal("Structured act has missing or extra fields")
        if value["act"] not in ACTS or value["syntax"] not in {"symbolic", "english"}:
            raise Refusal("Unknown act or syntax; the model cannot agree")
        if value["base_revision"] != self.model.revision:
            raise Refusal("Proposal refers to a stale revision; ask again on the current model")
        if not isinstance(value["object_ids"], list) or len(value["object_ids"]) > 32 or any(
                not isinstance(x, str) or x not in self.objects() for x in value["object_ids"]):
            raise Refusal("Act refers to an undeclared object")
        if not isinstance(value["message"], str) or len(value["message"]) > 8192:
            raise Refusal("Explanation exceeds the discussion bound")
        if value["act"] == "propose":
            candidate = parse(value["surface"], value["syntax"])
            if candidate.revision == self.model.revision:
                raise Refusal("Candidate is the current canonical revision; no change to accept")
            evidence = check(candidate, checker_identity())
            self.proposal = {"base_revision": self.model.revision, "revision": candidate.revision,
                             "model": candidate, "message": value["message"], "evidence": evidence,
                             "examples_digest": examples_digest(self.owner_examples),
                             "diff": semantic_diff(self.model, candidate), "agreement": "pending-human"}
        elif value["surface"] is not None:
            raise Refusal("Only a proposal may supply a normative surface")
        return self.record(value["act"], "agent", value)

    def propose_human(self, surface, syntax, revision):
        self.agent_act({"act": "propose", "base_revision": revision, "object_ids": [self.selected],
                        "message": "Human-authored candidate; review its canonical paraphrase and behavior diff.",
                        "syntax": syntax, "surface": surface})
        self.events[-1]["actor"] = "human"

    def accept_human(self, base_revision, candidate_revision, displayed_examples_digest=None):
        proposal = self.proposal
        if not proposal or base_revision != self.model.revision or base_revision != proposal["base_revision"] or candidate_revision != proposal["revision"]:
            raise Refusal("Agreement is stale or already consumed; review the current candidate")
        evidence = check(proposal["model"], checker_identity())
        if evidence != proposal["evidence"] or evidence["status"] == "counterexample":
            raise Refusal("Current checks are stale or failed; acceptance is blocked")
        if proposal.get("examples_digest") != examples_digest(self.owner_examples):
            raise Refusal("Owner examples changed; propose again and review the current corpus")
        if displayed_examples_digest != examples_digest(self.owner_examples):
            raise Refusal("Displayed owner-example corpus changed; refresh and review the candidate again")
        self.record("agree", "human", {"base_revision": base_revision, "candidate_revision": candidate_revision,
                                        "canonical_paraphrase": proposal["model"].render(True),
                                        "checker": evidence["checker"], "examples_digest": examples_digest(self.owner_examples)})
        self.model = proposal["model"]
        self.evidence = evidence
        self.proposal = None
        self.agreement = "human-accepted"
        self.realization = "not-run"
        if self.selected not in self.objects():
            self.selected = "default"

    def accept_current_human(self, revision, displayed_examples_digest=None):
        """Explicit review of a seed/current meaning; never a model tool."""
        if self.proposal or revision != self.model.revision:
            raise Refusal("Review the exact current model without a pending candidate")
        if displayed_examples_digest != examples_digest(self.owner_examples):
            raise Refusal("Displayed owner-example corpus changed; show and review the current meaning again")
        evidence = check(self.model, checker_identity())
        if evidence["status"] == "counterexample":
            raise Refusal("Current checks failed; acceptance is blocked")
        self.record("agree", "human", {"base_revision": revision, "candidate_revision": revision,
                                        "canonical_paraphrase": self.model.render(True),
                                        "checker": evidence["checker"], "examples_digest": examples_digest(self.owner_examples)})
        self.evidence = evidence
        self.agreement = "human-accepted"
        self.realization = "not-run"

    def reject_human(self, base_revision, candidate_revision):
        if not self.proposal or base_revision != self.model.revision or candidate_revision != self.proposal["revision"]:
            raise Refusal("Rejection refers to a stale candidate")
        self.record("reject-proposal", "human", {"candidate_revision": candidate_revision})
        self.proposal = None

    def view(self):
        def projection(model):
            return {**model.data(), "revision": model.revision, "symbolic": model.render(),
                    "english": model.render(True), "rules": [{**r.data(), "guard_text": r.guard.render(),
                     "guard_english": r.guard.render(True)} for r in model.rules]}
        proposal = self.proposal
        freshness = self.freshness()
        return json.loads(canonical({"model": projection(self.model), "selected": self.selected, "agreement": self.agreement,
                "evidence": self.evidence, "realization": self.realization,
                "owner_examples": self.owner_examples, "examples_digest": examples_digest(self.owner_examples),
                "example_comparison": self.example_comparison(proposal["model"] if proposal else None), "freshness": freshness,
                "supplied_assumptions": {"owner_examples": self.owner_examples, "observations": {},
                                         "declared_observations": self.model.facts},
                "authenticated_facts": [],
                "events": self.events, "proposal": None if not proposal else {
                    **{key: value for key, value in proposal.items() if key != "model"},
                    "model": projection(proposal["model"]),
                    "example_comparison": self.example_comparison(proposal["model"]), "freshness": freshness["proposal"],
                }}))
