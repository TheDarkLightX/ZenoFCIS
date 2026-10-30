#!/usr/bin/env python3
"""Seeded differential fuzzer for the machine-code decoders in tools/machine_code/verify*.py.

Three oracles, each over every target of the three profiles:

1. Round trip. A program drawn from the proposal grammar is encoded by
   `targets.encode_program`, decoded by the profile's verifier, and the decoded
   forms are encoded again. The decoder must accept the bytes, the re-encoding
   must be byte-identical, and decoding it again must give the same
   instructions. The encoder is the independent oracle: the verifier never
   imports it.
2. Mutation. Random edits of valid code (bit flips, byte replacement, insertion,
   deletion, truncation, swaps and fully random bytes) must either be refused
   with a `ValueError` naming a reason, or decode to instructions whose
   re-encoding is exactly the mutated bytes. Any other exception is a crash;
   a decode above the wall budget is a hang.
3. Cross-profile. Bytes the profile-1 verifier accepts must decode to the same
   instructions in the profile-2 and profile-3 verifiers of the same
   architecture, and profile-2 bytes in the profile-3 verifier, because each
   profile admits every form of the one below it.

Exit 0 when no finding was recorded, 1 otherwise. The seed reproduces a run
exactly; `--json` writes the summary with every finding and the refusal
histogram. This is a bounded search, not a proof: it covers the drawn programs
and edits only.
"""
from __future__ import annotations

import argparse
import json
import random
import sys
import time
import traceback
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from machine_code import targets, verify, verify2, verify3  # noqa: E402
from machine_code import model, profile2, profile3  # noqa: E402

DECODERS = {
    model.TARGETS[0]: verify, model.TARGETS[1]: verify,
    profile2.TARGETS[0]: verify2, profile2.TARGETS[1]: verify2,
    profile3.TARGETS[0]: verify3, profile3.TARGETS[1]: verify3,
}
# Each architecture's profiles, lowest first; a higher profile admits every lower form.
LADDERS = (
    (model.TARGETS[0], profile2.TARGETS[0], profile3.TARGETS[0]),
    (model.TARGETS[1], profile2.TARGETS[1], profile3.TARGETS[1]),
)
X86_REGS = tuple(targets.X86)
RV1_REGS = tuple(targets.RV)
RV2_REGS = tuple(targets.RV2)
CONDITIONS = tuple(targets.CONDITIONS)
MAX_BODY = 16
WALL_BUDGET_S = 0.5


class Draw:
    """Random instruction forms inside each target's proposal grammar."""

    def __init__(self, rng: random.Random):
        self.rng = rng

    def x86_profile1(self):
        r = self.rng
        kind = r.randrange(5)
        if kind == 0:
            return [r.choice(("mov", "add", "sub", "and", "or", "xor", "imul")), r.choice(X86_REGS), r.choice(X86_REGS)]
        if kind == 1:
            value = r.choice((0, 1, 127, 128, 255, 256, (1 << 31) - 1, 1 << 31, (1 << 63) - 1, 1 << 63, (1 << 64) - 1, r.getrandbits(64)))
            return ["imm", r.choice(X86_REGS), str(value)]
        if kind == 2:
            return [r.choice(("shl", "shr")), r.choice(X86_REGS), r.choice((0, 1, 2, 31, 32, 63, r.randrange(64)))]
        if kind == 3:
            return ["lea", r.choice(X86_REGS), r.choice(X86_REGS), r.choice(X86_REGS), r.choice((1, 2, 4, 8))]
        return ["nop"]

    def x86_profile2(self):
        r = self.rng
        kind = r.randrange(8)
        if kind == 0:
            return self.x86_profile1()
        if kind == 1:
            value = r.choice((0, 1, -1, 127, -128, 128, -129, 255, (1 << 31) - 1, -(1 << 31), r.randrange(-(1 << 31), 1 << 31)))
            return [r.choice(tuple(targets.IMMEDIATE_GROUP)), r.choice(X86_REGS), value]
        if kind == 2:
            return [r.choice(("adc", "sbb", "cmp", "test")), r.choice(X86_REGS), r.choice(X86_REGS)]
        if kind == 3:
            return [r.choice(("movzx", "cmov" + r.choice(CONDITIONS))), r.choice(X86_REGS), r.choice(X86_REGS)]
        if kind == 4:
            return ["set" + r.choice(CONDITIONS), r.choice(X86_REGS)]
        if kind == 5:
            return ["neg", r.choice(X86_REGS)]
        if kind == 6:
            return ["sar", r.choice(X86_REGS), r.choice((0, 1, 2, 63, r.randrange(64)))]
        return ["nop"]

    def x86_profile3(self):
        r = self.rng
        kind = r.randrange(4)
        if kind == 0:
            disp = r.choice((0, 8, -8, 16, 120, -128, 127, r.randrange(-128, 128)))
            return ["load", r.choice(X86_REGS), r.choice(X86_REGS), disp]
        if kind == 1:
            disp = r.choice((0, 8, -8, 16, 120, -128, 127, r.randrange(-128, 128)))
            return ["store", r.choice(X86_REGS), disp, r.choice(X86_REGS)]
        return self.x86_profile2()

    def rv_profile1(self):
        r = self.rng
        dst = r.choice([name for name in RV1_REGS if name != "zero"])
        kind = r.randrange(4)
        if kind == 0:
            return [r.choice(("add", "sub", "xor", "and", "or", "sltu")), dst, r.choice(RV1_REGS), r.choice(RV1_REGS)]
        if kind == 1:
            return ["addi", dst, r.choice(RV1_REGS), r.choice((0, 1, -1, 2047, -2048, r.randrange(-2048, 2048)))]
        if kind == 2:
            return [r.choice(("slli", "srli")), dst, r.choice(RV1_REGS), r.choice((0, 1, 63, r.randrange(64)))]
        return ["nop"]

    def rv_profile2(self):
        r = self.rng
        dst = r.choice([name for name in RV2_REGS if name != "zero"])
        kind = r.randrange(5)
        if kind == 0:
            return self.rv_profile1()
        if kind == 1:
            return [r.choice(("add", "sub", "xor", "and", "or", "slt", "sltu", "sra")), dst, r.choice(RV2_REGS), r.choice(RV2_REGS)]
        if kind == 2:
            return [r.choice(("addi", "slti", "sltiu", "xori", "ori", "andi")), dst, r.choice(RV2_REGS),
                    r.choice((0, 1, -1, 2047, -2048, r.randrange(-2048, 2048)))]
        if kind == 3:
            return [r.choice(("slli", "srli", "srai")), dst, r.choice(RV2_REGS), r.choice((0, 1, 63, r.randrange(64)))]
        return ["nop"]

    def rv_profile3(self):
        r = self.rng
        kind = r.randrange(4)
        disp = r.choice((0, 8, -8, 2040, -2048, 2047, r.randrange(-2048, 2048)))
        if kind == 0:
            return ["load", r.choice([n for n in RV2_REGS if n != "zero"]), r.choice(RV2_REGS), disp]
        if kind == 1:
            return ["store", r.choice(RV2_REGS), disp, r.choice(RV2_REGS)]
        return self.rv_profile2()

    def program(self, target):
        draw = {
            model.TARGETS[0]: self.x86_profile1, profile2.TARGETS[0]: self.x86_profile2, profile3.TARGETS[0]: self.x86_profile3,
            model.TARGETS[1]: self.rv_profile1, profile2.TARGETS[1]: self.rv_profile2, profile3.TARGETS[1]: self.rv_profile3,
        }[target]
        return [draw() for _ in range(self.rng.randrange(0, MAX_BODY + 1))]

    def mutate(self, code: bytes, aligned: bool) -> bytes:
        r = self.rng
        data = bytearray(code)
        kind = r.randrange(8)
        if kind == 0 and data:
            data[r.randrange(len(data))] ^= 1 << r.randrange(8)
        elif kind == 1 and data:
            data[r.randrange(len(data))] = r.randrange(256)
        elif kind == 2:
            at = r.randrange(len(data) + 1)
            data[at:at] = bytes(r.randrange(256) for _ in range(4 if aligned else r.randrange(1, 5)))
        elif kind == 3 and len(data) > (4 if aligned else 1):
            at = r.randrange(len(data) - (3 if aligned else 0))
            del data[at:at + (4 if aligned else 1)]
        elif kind == 4 and data:
            keep = r.randrange(len(data))
            del data[keep - keep % 4 if aligned else keep:]
        elif kind == 5 and len(data) > 1:
            i, j = r.randrange(len(data)), r.randrange(len(data))
            data[i], data[j] = data[j], data[i]
        elif kind == 6:
            data = bytearray(r.randrange(256) for _ in range(r.randrange(0, 41)))
        else:
            # Several edits at once.
            for _ in range(r.randrange(2, 5)):
                data = bytearray(self.mutate(bytes(data), aligned))
        return bytes(data)


def forms(instructions):
    return [ins.form() for ins in instructions if ins.op != "ret"]


def shape(instructions):
    return [(ins.op, tuple(ins.operands)) for ins in instructions]


def decode_timed(target, code, findings, origin):
    """Decode; return (instructions, reason). Crashes and hangs are findings."""
    start = time.monotonic()
    try:
        decoded = DECODERS[target].decode(target, code)
        reason = None
    except ValueError as error:
        decoded, reason = None, str(error) or "ValueError"
    except Exception as error:  # noqa: BLE001 - any other exception is the finding
        findings.append({"oracle": "crash", "origin": origin, "target": target, "code": code.hex(),
                         "exception": type(error).__name__, "message": str(error),
                         "trace": traceback.format_exc().splitlines()[-3:]})
        return None, "crash"
    elapsed = time.monotonic() - start
    if elapsed > WALL_BUDGET_S:
        findings.append({"oracle": "hang", "origin": origin, "target": target, "code": code.hex(), "seconds": round(elapsed, 3)})
    return decoded, reason


def run(seed: int, iterations: int, selected: tuple[str, ...]):
    rng = random.Random(seed)
    draw = Draw(rng)
    findings: list[dict] = []
    refusals: dict[str, dict[str, int]] = {target: {} for target in selected}
    counts = {"round_trip": 0, "mutation_refused": 0, "mutation_accepted": 0, "cross_profile": 0, "unencodable_draws": 0}
    for target in selected:
        aligned = not targets.is_x86(target)
        for _ in range(iterations):
            program = draw.program(target)
            try:
                code = targets.encode_program(target, program)
            except ValueError:
                counts["unencodable_draws"] += 1
                continue
            decoded, reason = decode_timed(target, code, findings, "round-trip")
            if decoded is None:
                if reason != "crash":
                    findings.append({"oracle": "round-trip", "target": target, "program": program, "code": code.hex(),
                                     "problem": "encoder output refused", "reason": reason})
                continue
            try:
                again = targets.encode_program(target, forms(decoded))
            except ValueError as error:
                findings.append({"oracle": "round-trip", "target": target, "program": program, "code": code.hex(),
                                 "problem": "decoded forms do not encode", "reason": str(error)})
                continue
            if again != code:
                findings.append({"oracle": "round-trip", "target": target, "program": program, "code": code.hex(),
                                 "problem": "re-encoding differs", "again": again.hex(), "decoded": forms(decoded)})
                continue
            decoded_again, _ = decode_timed(target, again, findings, "round-trip-again")
            if decoded_again is None or shape(decoded_again) != shape(decoded):
                findings.append({"oracle": "round-trip", "target": target, "code": code.hex(), "problem": "second decode differs"})
                continue
            counts["round_trip"] += 1

            mutated = draw.mutate(code, aligned)
            decoded, reason = decode_timed(target, mutated, findings, "mutation")
            if decoded is None:
                if reason != "crash":
                    counts["mutation_refused"] += 1
                    refusals[target][reason] = refusals[target].get(reason, 0) + 1
                continue
            counts["mutation_accepted"] += 1
            try:
                again = targets.encode_program(target, forms(decoded))
            except ValueError as error:
                findings.append({"oracle": "mutation", "target": target, "code": mutated.hex(),
                                 "problem": "accepted bytes whose forms do not encode", "reason": str(error), "decoded": forms(decoded)})
                continue
            if again != mutated:
                findings.append({"oracle": "mutation", "target": target, "code": mutated.hex(), "again": again.hex(),
                                 "problem": "accepted a non-canonical encoding", "decoded": forms(decoded)})

    for ladder in LADDERS:
        for low_index, low in enumerate(ladder[:-1]):
            if low not in selected:
                continue
            for _ in range(iterations):
                program = draw.program(low)
                try:
                    code = targets.encode_program(low, program)
                except ValueError:
                    continue
                base, reason = decode_timed(low, code, findings, "cross-profile")
                if base is None:
                    continue
                for high in ladder[low_index + 1:]:
                    other, reason = decode_timed(high, code, findings, "cross-profile")
                    if other is None:
                        if reason != "crash":
                            findings.append({"oracle": "cross-profile", "low": low, "high": high, "code": code.hex(),
                                             "problem": "higher profile refuses lower-profile bytes", "reason": reason})
                        continue
                    if shape(other) != shape(base):
                        findings.append({"oracle": "cross-profile", "low": low, "high": high, "code": code.hex(),
                                         "problem": "profiles decode the same bytes differently",
                                         "low_forms": forms(base), "high_forms": forms(other)})
                        continue
                    counts["cross_profile"] += 1
    return {"schema": "zeno-fcis/machine-code-decoder-fuzz/1", "seed": seed, "iterations": iterations,
            "targets": list(selected), "counts": counts, "refusals": refusals, "findings": findings,
            "authority": "none", "scope": "drawn programs and edits only; not a proof of the decoders"}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--seed", type=int, default=20261030)
    parser.add_argument("--iterations", type=int, default=2000, help="programs per target and per oracle")
    parser.add_argument("--target", action="append", choices=sorted(DECODERS), help="repeatable; default all six")
    parser.add_argument("--json", type=Path, help="write the summary with every finding here")
    args = parser.parse_args(argv)
    if args.iterations < 1:
        parser.error("--iterations must be at least 1")
    selected = tuple(args.target or list(DECODERS))
    summary = run(args.seed, args.iterations, selected)
    if args.json:
        args.json.write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps({key: summary[key] for key in ("seed", "iterations", "counts")}))
    for finding in summary["findings"][:20]:
        print(json.dumps(finding))
    print(f"machine-code decoder fuzz: {'PASS' if not summary['findings'] else 'FINDINGS ' + str(len(summary['findings']))}")
    return 0 if not summary["findings"] else 1


if __name__ == "__main__":
    sys.exit(main())
