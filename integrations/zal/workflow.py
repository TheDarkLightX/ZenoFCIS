"""Client-owned discussion revisions; model output is never an acceptance route."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path

from behavior import Model, PROFILE, Refusal, canonical, check, parse, semantic_diff


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
    digest = hashlib.sha256(b"zal/checker/1\0")
    for name in ["behavior.py", "workflow.py", "factory.py"]:
        path = here / name
        digest.update(name.encode() + b"\0")
        digest.update(path.read_bytes() if path.exists() else b"absent")
    return digest.hexdigest()


_PROCESS_CHECKER_IDENTITY = _disk_checker_identity()


def checker_identity():
    """Refuse disk drift rather than stamp old imported code with new bytes."""
    if _disk_checker_identity() != _PROCESS_CHECKER_IDENTITY:
        raise Refusal("Checker source changed after process start; restart before checking or accepting")
    return _PROCESS_CHECKER_IDENTITY


class Session:
    def __init__(self, model: Model):
        self.model = model
        self.proposal: dict | None = None
        self.events = []
        self.agreement = "seed-example"
        self.realization = "not-run"
        self.selected = "rule:" + model.rules[0].id if model.rules else "default"
        self.evidence = check(model, checker_identity())

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
                             "diff": semantic_diff(self.model, candidate), "agreement": "pending-human"}
        elif value["surface"] is not None:
            raise Refusal("Only a proposal may supply a normative surface")
        return self.record(value["act"], "agent", value)

    def propose_human(self, surface, syntax, revision):
        self.agent_act({"act": "propose", "base_revision": revision, "object_ids": [self.selected],
                        "message": "Human-authored candidate; review its canonical paraphrase and behavior diff.",
                        "syntax": syntax, "surface": surface})
        self.events[-1]["actor"] = "human"

    def accept_human(self, base_revision, candidate_revision):
        proposal = self.proposal
        if not proposal or base_revision != self.model.revision or base_revision != proposal["base_revision"] or candidate_revision != proposal["revision"]:
            raise Refusal("Agreement is stale or already consumed; review the current candidate")
        evidence = check(proposal["model"], checker_identity())
        if evidence != proposal["evidence"] or evidence["status"] == "counterexample":
            raise Refusal("Current checks are stale or failed; acceptance is blocked")
        self.record("agree", "human", {"base_revision": base_revision, "candidate_revision": candidate_revision,
                                        "canonical_paraphrase": proposal["model"].render(True)})
        self.model = proposal["model"]
        self.evidence = evidence
        self.proposal = None
        self.agreement = "human-accepted"
        self.realization = "not-run"
        if self.selected not in self.objects():
            self.selected = "default"

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
        return json.loads(canonical({"model": projection(self.model), "selected": self.selected, "agreement": self.agreement,
                "evidence": self.evidence, "realization": self.realization,
                "events": self.events, "proposal": None if not proposal else {
                    **{key: value for key, value in proposal.items() if key != "model"},
                    "model": projection(proposal["model"]),
                }}))
