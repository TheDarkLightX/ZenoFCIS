#!/usr/bin/env python3
"""Reproduce the small reservation contracts and their source-pinned abstraction.

This constructs rules, not evidence: contract refine-check must execute them.
The independent semantic oracle is in refinement_reference.py.
"""
# SPDX-License-Identifier: MIT OR Apache-2.0
# LLM-generated implementation; checked by the repository's qualification gates.
import argparse
import hashlib
import json
from itertools import product
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EXAMPLE = ROOT / "examples/contract-refinement/reservation"


def project(label, maximum, valid=None):
    invalid_reason = "reason 204 invalid_state precedence 4;\n" if valid is not None else ""
    text = f"""// SPDX-License-Identifier: MIT OR Apache-2.0
// LLM-generated example; qualification runs through the contract Authority.
zeno 1;
project 1 reservation_{label};
namespace 2 reservation;
type 100 state State;
type 101 command Request;
type 102 context Context;
type 103 destination Destination;
type 104 payload Event;
type 105 int Representation in 0..={maximum};
type 106 int Action in 0..=5;
type 107 int Unit in 0..=0;
field 110 100 representation 105;
field 120 101 action 106;
field 130 102 unit 107;
field 140 104 event 106;
reason 200 already_active precedence 0;
reason 201 retired precedence 1;
reason 202 full precedence 2;
reason 203 not_active precedence 3;
{invalid_reason}channel 300 events destination 103 payload 104;
component 400 reservation {{
  owns 100;
  reads pre.100;
  writes post.100;
  contexts context.102;
  outbox outbox.300;
  budget steps 65536;
}}
merge [400];
"""
    if valid is not None:
        expression = " || ".join(f"pre.100.110 == {state}" for state in valid)
        text += f"law 500 valid_representation on any = {expression};\n"
    text += f"law 501 state_bounds on commit, genesis = post.100.110 >= 0 && post.100.110 <= {maximum};\n"
    return text


def case(state, command, successor, reason):
    accepting = reason is None
    return {"when": f"state == {state} && action == {command}",
            "class": "Accept" if accepting else "Reject", "reason": reason,
            "post": {"110": str(successor)} if accepting else {},
            "outbox": [{"ordinal": 0, "channel": 300, "destination": "reservation",
                        "payload": {"140": str(command)}, "idempotency_ordinal": 0}] if accepting else []}


def policy(label, cases, concrete=False):
    if concrete:
        cases = cases + [{"when": "true", "class": "Reject", "reason": 204, "post": {}, "outbox": []}]
    else:
        # Abstract rows enumerate the whole declared domain. The final real row
        # supplies the compiler's required catch-all without an unreachable rule.
        cases = cases[:-1] + [dict(cases[-1], when="true")]
    return {"schema": "zeno-fcis/template-declarative-policy/2", "template": f"reservation-{label}",
            "roots": {"state": 100, "command": 101, "context": 102},
            "leaf_bindings": {"103": ["Text", 1, 32]},
            "variables": {"state": "pre.100.110", "action": "command.101.120"},
            "cases": cases,
            "genesis": {"110": 0}, "law_kinds": ({"500": "AuthoritySubjectRecipient", "501": "StateInvariant"}
                                                if concrete else {"501": "StateInvariant"})}


def files():
    trits = [t for t in product(range(3), repeat=3) if sum(x == 1 for x in t) <= 2]
    abstract = []
    for number, state in enumerate(trits):
        for command in range(6):
            identity = command % 3
            post = list(state)
            if command < 3:
                reason = {1: 200, 2: 201}.get(state[identity])
                if reason is None and sum(x == 1 for x in state) == 2:
                    reason = 202
                if reason is None:
                    post[identity] = 1
            else:
                reason = None if state[identity] == 1 else 203
                if reason is None:
                    post[identity] = 2
            abstract.append(case(number, command, trits.index(tuple(post)), reason))
    concrete, mapping = [], []
    for raw in range(64):
        active, seen = raw & 7, raw >> 3
        valid = active & ~seen == 0 and active.bit_count() <= 2
        state = tuple(1 if active & (1 << i) else 2 if seen & (1 << i) else 0 for i in range(3))
        mapping.append(trits.index(state) if valid else None)
        if not valid:
            continue
        for command in range(6):
            bit = 1 << (command % 3)
            after = raw
            if command < 3:
                reason = 200 if active & bit else 201 if seen & bit else 202 if active.bit_count() == 2 else None
                if reason is None:
                    after = (active | bit) | ((seen | bit) << 3)
            else:
                reason = None if active & bit else 203
                if reason is None:
                    after = (active ^ bit) | (seen << 3)
            concrete.append(case(raw, command, after, reason))
    # Every raw state is in the declared domain; law 500 rejects the 38 invalid ones.
    valid = [state for state, abstract_state in enumerate(mapping) if abstract_state is not None]
    result = {"abstract/project.zeno": project("abstract", 25),
              "concrete/project.zeno": project("concrete", 63, valid),
              "abstract/v2/policy.json": dump(policy("abstract", abstract)),
              "concrete/v2/policy.json": dump(policy("concrete", concrete, True))}
    binding = {"schema": "zeno-fcis/contract-refinement/1", "abstraction": mapping, "invalid_state_law": 500}
    for label in ("abstract", "concrete"):
        for kind, name in (("project", "project.zeno"), ("policy", "v2/policy.json")):
            binding[f"{label}_{kind}_sha256"] = hashlib.sha256(result[f"{label}/{name}"].encode()).hexdigest()
    result["abstraction.json"] = dump(binding)
    return result


def dump(value):
    return json.dumps(value, indent=2) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="verify committed files without rewriting")
    args = parser.parse_args()
    for name, text in files().items():
        path = EXAMPLE / name
        if args.check:
            if path.read_text() != text:
                raise SystemExit(f"fixture differs: {name}")
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
    print("reservation fixture: PASS")


if __name__ == "__main__":
    main()
