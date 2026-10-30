"""Arithmetic contracts that name their meaning before any machine problem exists.

Machine profile 2 computes modular 64-bit words. A contract states the intended
arithmetic: the operation, the signedness, the width and the meaning (modular,
checked with a status word, saturating, or trapping). Routing is a finite,
pure decision: a contract lowers to an explicit profile-2 problem, or it is
refused with the name of the missing bridge, or as unsupported. Nothing narrows
silently: a checked contract always lowers to two results, the wrapped value and
a 0/1 status, and a saturating contract to its exact clamped expression.

The contract-bound identity hashes the contract with the lowered problem, so two
contracts that share a problem (modular signed and unsigned addition) stay
distinct. The lowering record has no authority; only the checker decides whether
bytes meet the lowered problem.
"""

from __future__ import annotations

from pathlib import Path

from . import profile2
from .model import MASK, canonical, digest

SCHEMA = "zeno-fcis/machine-contract/1"
LOWERING = "zeno-fcis/machine-contract-lowering/1"
FIELDS = {"schema", "target", "operation", "signedness", "bits", "meaning", "inputs",
          "grammar", "max_instructions", "objective"}
OPERATIONS = ("add", "sub", "mul")
SIGNEDNESS = ("unsigned", "signed")
MEANINGS = ("modular", "checked", "saturating", "trapping")
WIDTHS = (64, 128, 256)
MIN, MAX = str(1 << 63), str((1 << 63) - 1)
BRIDGES = {
    "effectful-overflow": "A trapping contract raises an effect on overflow; profile-2 kernels are pure "
                          "register functions with no trap, exception or effect.",
    "multiword": "Profile-2 words are 64 bits and a kernel returns at most two words; a 128- or 256-bit "
                 "contract needs a multiword lowering, and a checked one needs a status word as well.",
    "high-product": "Profile 2 has only the low 64 bits of a product (mul); overflow of a 64-bit product "
                    "needs its high word, which no profile-2 expression computes.",
}
STATUS = {"values": {"0": "exact: result 1 is the mathematical result",
                     "1": "overflow: the mathematical result is outside the declared range; result 1 holds "
                          "it modulo 2^64"},
          "precedence": "Read result 2 first. Result 1 is the mathematical result only when result 2 is 0; "
                        "it is always the wrapped value, never a default."}


def route(meaning, operation, signedness, bits, target):
    """The finite routing decision, in precedence order; exhaustively checked by its test."""
    if meaning not in MEANINGS:
        return "unsupported", "meaning must be modular, checked, saturating or trapping"
    if operation not in OPERATIONS:
        return "unsupported", "operation must be add, sub or mul"
    if signedness not in SIGNEDNESS:
        return "unsupported", "signedness must be unsigned or signed"
    if bits not in WIDTHS:
        return "unsupported", "bits must be 64, 128 or 256"
    if target not in profile2.TARGETS:
        return "unsupported", "contracts lower only to the profile-2 targets"
    if meaning == "trapping":
        return "missing-bridge", "effectful-overflow"
    if bits != 64:
        return "missing-bridge", "multiword"
    if operation == "mul" and meaning != "modular":
        return "missing-bridge", "high-product"
    return "profile-2", None


def _validate(contract) -> dict:
    if not isinstance(contract, dict) or set(contract) != FIELDS or contract["schema"] != SCHEMA:
        raise ValueError("machine contract fields differ from the closed version-1 schema")
    inputs = contract["inputs"]
    if not isinstance(inputs, list) or len(inputs) != 2:
        raise ValueError("a binary arithmetic contract names exactly two inputs")
    for name in ("meaning", "operation", "signedness", "target"):
        if type(contract[name]) is not str:
            raise ValueError(name + " must be a string")
    if type(contract["bits"]) is not int:
        raise ValueError("bits must be an integer")
    return contract


def _expressions(operation, signedness, meaning, x, y):
    """Value, overflow status and saturating clamp for one routed add/sub/mul contract."""
    value = [operation, x, y]
    if meaning == "modular":
        return [value]
    if signedness == "unsigned":
        overflow = ["ult", value, x] if operation == "add" else ["ult", x, y]
        clamp = ["const", str(MASK)] if operation == "add" else ["const", "0"]
    else:
        # Two's-complement overflow: the result's sign differs from the operands it must follow.
        mixed = ["and", ["xor", x, value], ["xor", y, value]] if operation == "add" else \
            ["and", ["xor", x, y], ["xor", x, value]]
        overflow = ["slt", mixed, ["const", "0"]]
        clamp = ["ite", ["slt", x, ["const", "0"]], ["const", MIN], ["const", MAX]]
    if meaning == "checked":
        return [value, overflow]
    return [["ite", overflow, clamp, value]]


def _results(contract):
    x, y = contract["inputs"]
    symbol = {"add": "+", "sub": "-", "mul": "*"}[contract["operation"]]
    exact = f"{x} {symbol} {y} over {contract['signedness']} 64-bit words"
    if contract["meaning"] == "modular":
        return [f"{exact}, modulo 2^64"]
    if contract["meaning"] == "saturating":
        bounds = "0 and 2^64-1" if contract["signedness"] == "unsigned" else "-2^63 and 2^63-1"
        return [f"{exact}, clamped to {bounds}"]
    return [f"{exact}, modulo 2^64", "status: 1 when the mathematical result is outside the declared range, else 0"]


def lower(contract) -> dict:
    """The lowering record: a validated profile-2 problem, or a named refusal; never a default."""
    contract = _validate(contract)
    kind, detail = route(contract["meaning"], contract["operation"], contract["signedness"],
                         contract["bits"], contract["target"])
    record = {"schema": LOWERING, "contract": contract, "contract_sha256": digest(canonical(contract)),
              "route": kind, "authority": "none"}
    if kind != "profile-2":
        reason = BRIDGES[detail] if kind == "missing-bridge" else detail
        return {**record, "status": "refused", "bridge": detail if kind == "missing-bridge" else None,
                "reason": reason}
    x, y = (["input", name] for name in contract["inputs"])
    problem = profile2.validate_problem({
        "schema": profile2.SCHEMA, "target": contract["target"], "inputs": contract["inputs"],
        "outputs": _expressions(contract["operation"], contract["signedness"], contract["meaning"], x, y),
        "grammar": contract["grammar"], "max_instructions": contract["max_instructions"],
        "objective": contract["objective"]})
    from . import session, verify2  # the palette checks a session would apply on open
    verify2._palette(problem["target"], problem["grammar"])
    session._grammar(problem)
    problem_sha256 = digest(canonical(problem))
    return {**record, "status": "lowered", "problem": problem, "problem_sha256": problem_sha256,
            "contract_problem_sha256": digest(canonical({"contract_sha256": record["contract_sha256"],
                                                          "problem_sha256": problem_sha256})),
            "results": _results(contract), "status_word": STATUS if contract["meaning"] == "checked" else None,
            "implementation_sha256": digest(Path(__file__).read_bytes())}


def check(contract, code: bytes, timeout_ms: int = 1000) -> dict:
    """Check bytes against the lowered problem; a refused contract checks nothing."""
    record = lower(contract)
    if record["status"] != "lowered":
        return record
    from . import verify2
    result = verify2.check(record["problem"], code, timeout_ms)
    return {**result, "contract_sha256": record["contract_sha256"], "problem_sha256": record["problem_sha256"],
            "contract_problem_sha256": record["contract_problem_sha256"], "authority": "none"}
