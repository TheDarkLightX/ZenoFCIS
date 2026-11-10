"""Replay-checkable, non-authorizing decision evidence for ZAL finite-FSM/2.

An evidence envelope is an untrusted *claim*. Verification binds the installed
model revision and exact observed prestate, event, context and poststate, then
replays the existing ZAL transition evaluator and local safety obligations.
It is NOT a SNARK, a global proof of policy correctness, an authentication
check, or permission to execute an effect. The effect field is always empty
because this authoring profile has no effect implementation.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import hashlib
import json
from pathlib import Path

import behavior2
from behavior import Refusal, canonical


MAGIC = b"ZFCIS-PCD1\0"
FORMAT = "zeno-fcis/decision-evidence/1"
RELATION = b"zal/finite-step/1\0"
MAX_FIELD_BYTES = 16_384
MAX_ENVELOPE_BYTES = 65_536
_FIELDS = ("prestate", "command", "context", "poststate", "effect")


@dataclass(frozen=True)
class Evidence:
    """Owned binary fields. Anyone may construct a claim; only replay checks it."""

    specification: bytes
    policy: bytes
    prestate: bytes
    command: bytes
    context: bytes
    poststate: bytes
    effect: bytes

    def __post_init__(self):
        for label in ("specification", "policy"):
            digest = getattr(self, label)
            if type(digest) is not bytes or len(digest) != 32 or digest == bytes(32):
                raise Refusal(f"{label}: nonzero 32-byte identity required")
        for label in _FIELDS:
            value = getattr(self, label)
            if type(value) is not bytes or len(value) > MAX_FIELD_BYTES:
                raise Refusal(f"{label}: bytes exceed the profile limit")
        if not self.prestate or not self.command or not self.poststate:
            raise Refusal("prestate, command and poststate must be nonempty")
        if len(self.encode()) > MAX_ENVELOPE_BYTES:
            raise Refusal("decision evidence exceeds envelope limit")

    def encode(self) -> bytes:
        """Versioned, unambiguous transport: magic, identities, five length-prefixed fields."""
        result = bytearray(MAGIC + self.specification + self.policy)
        for label in _FIELDS:
            value = getattr(self, label)
            result.extend(len(value).to_bytes(4, "big"))
            result.extend(value)
        return bytes(result)

    @classmethod
    def decode(cls, raw: bytes) -> Evidence:
        """Fail closed on truncated, oversized, noncanonical or trailing input."""
        if type(raw) is not bytes or not len(MAGIC) + 64 + 5 * 4 <= len(raw) <= MAX_ENVELOPE_BYTES:
            raise Refusal("invalid evidence envelope length")
        if not raw.startswith(MAGIC):
            raise Refusal("unsupported decision evidence version")
        offset = len(MAGIC)
        specification, policy = raw[offset:offset + 32], raw[offset + 32:offset + 64]
        offset += 64
        fields = []
        for _ in _FIELDS:
            if len(raw) - offset < 4:
                raise Refusal("truncated evidence field")
            length = int.from_bytes(raw[offset:offset + 4], "big")
            offset += 4
            if length > MAX_FIELD_BYTES or length > len(raw) - offset:
                raise Refusal("invalid evidence field length")
            fields.append(raw[offset:offset + length])
            offset += length
        if offset != len(raw):
            raise Refusal("trailing evidence bytes")
        return cls(specification, policy, *fields)

    @property
    def digest(self) -> str:
        """Transport fingerprint, not a proof or an authorization token."""
        return hashlib.sha256(self.encode()).hexdigest()


@dataclass(frozen=True)
class CheckedDecision:
    """Immutable explanatory result; never an execution/publication capability."""

    evidence_digest: str
    state: str
    variables: tuple[tuple[str, int], ...]
    authority: str = "none"


def _model_id(model: behavior2.Machine) -> tuple[bytes, bytes]:
    if type(model) is not behavior2.Machine or model.profile != behavior2.PROFILE:
        raise Refusal("decision evidence currently supports only zal/finite-fsm/2")
    if model.obligations:
        raise Refusal("unresolved model obligations forbid decision evidence")
    specification = bytes.fromhex(model.revision)
    policy = hashlib.sha256(RELATION + specification).digest()
    return specification, policy


def _bytes(data: object) -> bytes:
    return canonical(data).encode("utf-8")


def _snapshot(state: str, variables: dict) -> bytes:
    return _bytes({"state": state, "variables": variables})


def _command(event: str) -> bytes:
    return _bytes({"event": event})


def _local_decision(model: behavior2.Machine, state: str, variables: dict,
                    event: str, context: dict) -> dict:
    """Check a single accepted transition; do not claim full-domain qualification."""
    result = model.decide(state, variables, event, context)
    if result["class"] != "Accept":
        raise Refusal(f"decision rejected by ZAL rule {result['rule']}")
    before = behavior2._Step(state, dict(variables), event, dict(context))
    after = behavior2._Step(result["state"], dict(result["variables"]), None, {})
    for name, formula in model.invariants:
        if not behavior2.holds(formula, before) or not behavior2.holds(formula, after):
            raise Refusal(f"invariant {name} does not hold across this transition")
    before.accepted = True
    before.post_state = result["state"]
    before.post_values = dict(result["variables"])
    for name, formula in model.requirements:
        if not behavior2.holds(formula, before):
            raise Refusal(f"transition requirement {name} fails")
    return result


def issue(model: behavior2.Machine, state: str, variables: dict,
          event: str, context: dict) -> Evidence:
    """Generate a portable claim from a finite ZAL transition, without I/O."""
    specification, policy = _model_id(model)
    result = _local_decision(model, state, variables, event, context)
    return Evidence(specification, policy, _snapshot(state, variables),
                    _command(event), _bytes(context),
                    _snapshot(result["state"], result["variables"]), b"")


def verify(model: behavior2.Machine, evidence: Evidence, state: str,
           variables: dict, event: str, context: dict) -> CheckedDecision:
    """Replay against an independently selected model and current caller inputs."""
    specification, policy = _model_id(model)
    if not isinstance(evidence, Evidence):
        raise Refusal("expected a decoded decision evidence envelope")
    if (evidence.specification, evidence.policy) != (specification, policy):
        raise Refusal("model or policy identity mismatch")
    if (evidence.prestate, evidence.command, evidence.context) != (
            _snapshot(state, variables), _command(event), _bytes(context)):
        raise Refusal("stale, replayed, or substituted state/command/context")
    if evidence.effect:
        raise Refusal("ZAL finite-FSM/2 has no executable effect to authorize")
    result = _local_decision(model, state, variables, event, context)
    if evidence.poststate != _snapshot(result["state"], result["variables"]):
        raise Refusal("the claimed transition differs from the checked ZAL result")
    return CheckedDecision(evidence.digest, result["state"],
                           tuple(sorted(result["variables"].items())))


def _object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise Refusal(f"duplicate JSON field {key}")
        result[key] = value
    return result


def main(argv=None) -> int:
    """Minimal file/JSON shell; the issue and replay functions above remain pure."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("operation", choices=("issue", "verify"))
    parser.add_argument("source", type=Path, help="trusted ZAL 2 model source")
    parser.add_argument("--state", required=True)
    parser.add_argument("--variables", required=True, help="JSON map of numeric variables")
    parser.add_argument("--event", required=True)
    parser.add_argument("--context", required=True, help="JSON map of observed facts and inputs")
    parser.add_argument("--evidence-hex", help="exact PCD1 bytes, required for verify")
    args = parser.parse_args(argv)
    try:
        model = behavior2.parse(args.source.read_text(encoding="utf-8"))
        variables = json.loads(args.variables, object_pairs_hook=_object)
        context = json.loads(args.context, object_pairs_hook=_object)
        if args.operation == "issue":
            result = issue(model, args.state, variables, args.event, context)
            output = {"status": "issued", "format": FORMAT, "digest": result.digest,
                      "evidence_hex": result.encode().hex(), "authority": "none"}
        else:
            if not args.evidence_hex:
                raise Refusal("--evidence-hex is required for verify")
            result = verify(model, Evidence.decode(bytes.fromhex(args.evidence_hex)),
                            args.state, variables, args.event, context)
            output = {"status": "verified", "format": FORMAT,
                      "digest": result.evidence_digest,
                      "poststate": {"state": result.state,
                                    "variables": dict(result.variables)}, "authority": "none"}
        print(json.dumps(output, sort_keys=True))
        return 0
    except (OSError, ValueError, TypeError) as error:
        print(json.dumps({"status": "refused", "reason": str(error), "authority": "none"},
                         sort_keys=True))
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
