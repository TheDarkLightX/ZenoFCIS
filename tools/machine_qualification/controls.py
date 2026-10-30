"""Registered planted-defect controls for the independent machine-code checkers.

Each defect is one named source edit to `tools/machine_code/verify.py`, `verify2.py`, `verify3.py` or `realize.py`
(whose unsat verdicts carry checker-like weight). The edit is applied to an isolated specimen: a fresh copy of the checker package, the test modules that hold the controls, and the kernel
library. Its checker identity is computed from the specimen's own sources, so it is consistent with the edit. The
defect names the control that should expose it, and optionally a second control that must also expose it; only
those controls are judged. Each run of a control records one outcome:

- `detected`: the intended control failed on a test assertion, raised in the test module or `unittest` itself;
- `survived`: the intended control passed on the defective specimen;
- `inconclusive`: the control crashed, was skipped, timed out, or failed on an identity or stale-hash refusal;
- `invalid-specimen`: the edit does not apply exactly once, the specimen does not compile or import, or the control
  did not run against the specimen's own package.

Crashes, timeouts and identity refusals are never detections. Before any defect is judged, the positive controls
(the verifier suites, every checked-library kernel, and every intended control) must pass on an unmodified
specimen. Every control runs under `python3 -B` and `python3 -O -B`.

    python3 -B -m tools.machine_qualification.controls run [--defect NAME]... [--out REPORT.json]

Exit 0: every positive control passed and every defect was detected in both modes. Exit 1: a positive control
failed or a defect survived. Exit 2: otherwise, some outcome was inconclusive or a specimen invalid. The scope is
the registered ISA models and these controls; a detected defect shows that one control distinguishes one edit, not
that the checker is complete.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import py_compile
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]
SCHEMA = "zeno-fcis/machine-checker-controls/1"
MODES = (("-B",), ("-O", "-B"))
TIMEOUT = 900
TEST_MODULES = ("test_machine_code_verifier.py", "test_machine_code_profile2_verifier.py",
                "test_machine_code_library.py", "test_machine_code_conformance.py",
                "test_machine_code_profile2_conformance.py", "test_machine_code_profile3_verifier.py",
                "test_machine_code_realize.py")
# A failure that mentions any of these is a refusal of the specimen's identity, not of the defect. The session
# refuses a changed checker with "session problem/checker/solver binding drifted".
IDENTITY_MARKERS = ("identity", "stale", "hash refus", "binding drifted")

V1 = "test_machine_code_verifier.VerifierTests."
V2 = "test_machine_code_profile2_verifier.Profile2VerifierTests."
V3 = "test_machine_code_profile3_verifier.Profile3VerifierTests."
REALIZE = "test_machine_code_realize.RealizeCrossCheck."
CONFORMANCE1 = "test_machine_code_conformance.X86ModelConformance."
CONFORMANCE2 = "test_machine_code_profile2_conformance."
POSITIVE = (
    ("profile-1 verifier, including operand aliases, high-bit inputs and quantified scratch",
     "test_machine_code_verifier"),
    ("profile-2 verifier, including flags, aliases, high-bit inputs and quantified scratch and flags",
     "test_machine_code_profile2_verifier"),
    ("every kernel in test-data/machine-code/kernels at its recorded cost", "test_machine_code_library"),
    ("profile-3 verifier, including region, base, scratch and flag quantification and every memory kernel",
     "test_machine_code_profile3_verifier"),
)


@dataclass(frozen=True)
class Defect:
    """One planted edit: `anchor` must occur exactly once in `file` and becomes `replacement`.

    `control` is the intended control. `second`, when given, is a further registered control that must also detect
    the edit; it is run and reported separately, and a survival or inconclusive run there counts like one of the
    intended control.
    """

    name: str
    file: str
    kind: str
    anchor: str
    replacement: str
    control: str
    why: str
    second: str | None = None


DEFECTS = (
    Defect("v1-x86-immediate-low-32-bits", "verify.py", "operand width",
           'value = int.from_bytes(take(8), "little")',
           'value = int.from_bytes(take(8), "little") & 0xFFFFFFFF',
           V1 + "test_x86_ret_byte_inside_immediate_is_not_a_return",
           "MOVABS keeps only the low 32 immediate bits, so the all-ones constant is not the declared form."),
    Defect("v1-rv-addi-not-sign-extended", "verify.py", "immediate sign extension",
           '            if immediate & 0x800:\n                immediate -= 0x1000\n'
           '            op, operands = "addi", (dst, a, immediate)',
           '            op, operands = "addi", (dst, a, immediate)',
           V1 + "test_rv_addi_sign_extension_and_modular_overflow",
           "ADDI -1 decodes as +4095."),
    Defect("v1-scratch-fixed-to-zero", "verify.py", "scratch quantification",
           'scratch = {n: z.BitVec("mc_initial_" + n, 64, ctx=context) for n in _SCRATCH[p["target"]]}',
           'scratch = {n: z.BitVecVal(0, 64, ctx=context) for n in _SCRATCH[p["target"]]}',
           V1 + "test_x86_other_scratch_is_universally_quantified",
           "A body that reads an unwritten scratch register is accepted because scratch is assumed zero."),
    Defect("v1-symbolic-run-drops-last-instruction", "verify.py", "dropped instruction",
           '    for ins in instructions[:-1]:\n        op, a = ins.op, ins.operands\n        if op == "nop": continue',
           '    for ins in instructions[:-2]:\n        op, a = ins.op, ins.operands\n        if op == "nop": continue',
           V1 + "test_x86_add_sub_bitwise_and_full_width_imul",
           "The SMT model omits the last body instruction; correct kernels stop being equivalent."),
    Defect("v1-x86-shift-count-five-bits", "verify.py", "shift count",
           'amount = 1 if opcode == 0xD1 else take(1)[0]',
           'amount = 1 if opcode == 0xD1 else take(1)[0] & 31',
           V1 + "test_x86_shifts_are_64_bit_logical_and_counts_zero_one_63",
           "A 64-bit shift count is masked to five bits, as for 32-bit operands."),
    Defect("v1-x86-shr-arithmetic-in-smt", "verify.py", "high-bit inputs",
           'elif op == "shr": value = z.LShR(r[dst], a[1])',
           'elif op == "shr": value = r[dst] >> a[1]',
           V1 + "test_x86_shifts_are_64_bit_logical_and_counts_zero_one_63",
           "The symbolic SHR copies the sign bit, which differs only for inputs with bit 63 set."),
    Defect("v1-x86-lea-scale-as-shift-in-concrete", "verify.py", "operand scale",
           '            elif op == "lea":\n                value = r[a[1]] + r[a[2]] * a[3]',
           '            elif op == "lea":\n                value = r[a[1]] + (r[a[2]] << a[3])',
           CONFORMANCE1 + "test_hardware_matches_concrete_model_and_leaves_other_registers",
           "The concrete LEA shifts the index by its scale instead of multiplying by it."),
    Defect("v1-rv-zero-register-writable", "verify.py", "zero register",
           '        _require(rd != 0, "ZeroWriteHintRefused")\n', '',
           V1 + "test_rv_unsafe_and_other_extensions_refuse",
           "A write to x0 decodes, so the zero register is treated as writable."),
    Defect("v1-rv-zero-register-declarable", "verify.py", "zero register",
           'writable = registers - ({"zero"} if target == RV64 else set())', 'writable = registers',
           V1 + "test_unused_malformed_or_unsafe_declared_forms_refuse",
           "A grammar may declare x0 as a destination."),
    Defect("v1-solver-unknown-read-as-equivalent", "verify.py", "solver result",
           'if answer == z.unsat:', 'if answer != z.sat:',
           V1 + "test_solver_unknown_and_exception_are_never_equivalent",
           "An unknown solver answer is reported as equivalence."),
    Defect("v1-counterexample-without-replay", "verify.py", "counterexample replay",
           'if e != _ast_concrete(p["expression"], values) or a != run_concrete(p["target"], code, values, initial)'
           ' or e == a:',
           'if False:',
           V1 + "test_intended_decoder_and_semantic_mutants_are_distinguished",
           "A solver model is reported as a counterexample without its concrete replay."),
    Defect("v2-x86-immediate-low-32-bits", "verify2.py", "operand width",
           'op, operands = "imm", (dst, int.from_bytes(take(8), "little"))',
           'op, operands = "imm", (dst, int.from_bytes(take(8), "little") & 0xFFFFFFFF)',
           V2 + "test_x86_saturating_add_absolute_value_and_signed_saturation",
           "MOVABS of INT64_MAX keeps only its low 32 bits."),
    Defect("v2-rv-immediates-not-sign-extended", "verify2.py", "immediate sign extension",
           '            if immediate & 0x800:\n                immediate -= 0x1000\n', '',
           V2 + "test_rv_comparisons_immediates_and_arithmetic_shifts",
           "SLTI, SLTIU, XORI and ORI read a negative immediate as a large positive one."),
    Defect("v2-scratch-fixed-to-zero", "verify2.py", "scratch quantification",
           'initial = {n: z.BitVec("mc_initial_" + n, 64, ctx=context) for n in scratch}',
           'initial = {n: z.BitVecVal(0, 64, ctx=context) for n in scratch}',
           V2 + "test_x86_setcc_writes_only_the_low_byte",
           "SETcc into an unwritten register is accepted because its upper bytes are assumed zero."),
    Defect("v2-entry-flags-fixed-false", "verify2.py", "flag quantification",
           'flags = {n: z.Bool("mc_flag_" + n, ctx=context) for n in FLAGS} if target == X86 else {}',
           'flags = {n: z.BoolVal(False, ctx=context) for n in FLAGS} if target == X86 else {}',
           V2 + "test_x86_entry_flags_are_universally_quantified",
           "ADC that reads the entry carry is accepted as ADD because entry flags are assumed clear."),
    Defect("v2-symbolic-run-drops-last-instruction", "verify2.py", "dropped instruction",
           '    for ins in instructions[:-1]:\n        if target == X86:\n            _x86_smt(z, ins, r, f, context)',
           '    for ins in instructions[:-2]:\n        if target == X86:\n            _x86_smt(z, ins, r, f, context)',
           V2 + "test_x86_128_bit_add_and_sub_carry_through_adc_and_sbb",
           "The SMT model omits the last body instruction."),
    Defect("v2-x86-shift-count-five-bits", "verify2.py", "shift count",
           'amount = 1 if opcode == 0xD1 else take(1)[0]',
           'amount = 1 if opcode == 0xD1 else take(1)[0] & 31',
           V2 + "test_x86_saturating_add_absolute_value_and_signed_saturation",
           "SAR by 63, the sign-mask idiom, becomes SAR by 31."),
    Defect("v2-rv-sra-count-five-bits", "verify2.py", "shift count",
           'elif op == "sra": value = _signed(left) >> (right & 63)',
           'elif op == "sra": value = _signed(left) >> (right & 31)',
           V2 + "test_rv_sra_uses_six_count_bits_on_negative_values",
           "The concrete SRA masks its register count to five bits; the SMT model does not.",
           CONFORMANCE2 + "HostIndependentModelChecks.test_rv64i_concrete_and_smt_models_agree_on_sampled_states"),
    Defect("v2-rv-sra-count-five-bits-in-smt", "verify2.py", "shift count",
           'elif op == "sra": value = left >> (right & word(63))',
           'elif op == "sra": value = left >> (right & word(31))',
           V2 + "test_rv_sra_uses_six_count_bits_on_negative_values",
           "The symbolic SRA masks its register count to five bits; the concrete model does not.",
           CONFORMANCE2 + "HostIndependentModelChecks.test_rv64i_concrete_and_smt_models_agree_on_sampled_states"),
    Defect("v2-x86-shl-carry-off-by-one", "verify2.py", "flag rule CF",
           'result, carry = (value << count) & MASK, (value >> (64 - count)) & 1',
           'result, carry = (value << count) & MASK, (value >> (63 - count)) & 1',
           CONFORMANCE2 + "X86FlagModelConformance.test_hardware_matches_concrete_registers_and_flags",
           "The concrete SHL carry is the bit after the last bit shifted out."),
    Defect("v2-x86-add-overflow-uses-sub-rule", "verify2.py", "flag rule OF",
           'f.update(cf=bit(total, 64), of=bit((left ^ result) & (right ^ result), 63))',
           'f.update(cf=bit(total, 64), of=bit((left ^ right) & (left ^ result), 63))',
           V2 + "test_x86_saturating_add_absolute_value_and_signed_saturation",
           "The symbolic ADD overflow flag uses the subtraction rule."),
    Defect("v2-x86-sub-overflow-wrong-operand-in-concrete", "verify2.py", "flag rule OF",
           'f.update(cf=int(left < right + borrow), of=((left ^ right) & (left ^ result)) >> 63)',
           'f.update(cf=int(left < right + borrow), of=((left ^ right) & (right ^ result)) >> 63)',
           CONFORMANCE2 + "X86FlagModelConformance.test_hardware_matches_concrete_registers_and_flags",
           "The concrete SUB, SBB and CMP overflow flag tests the wrong operand's sign."),
    Defect("v2-x86-zero-flag-inverted-in-smt", "verify2.py", "flag rule ZF",
           'f.update(zf=result == word(0), sf=bit(result, 63))',
           'f.update(zf=result != word(0), sf=bit(result, 63))',
           V2 + "test_x86_setcc_writes_only_the_low_byte",
           "The symbolic ZF is set on a nonzero result."),
    Defect("v2-x86-sign-flag-bit-62", "verify2.py", "flag rule SF",
           'f.update(zf=int(result == 0), sf=result >> 63)',
           'f.update(zf=int(result == 0), sf=(result >> 62) & 1)',
           V2 + "test_x86_sign_flag_is_bit_63_of_the_result",
           "The concrete SF reads bit 62.",
           CONFORMANCE2 + "X86FlagModelConformance.test_hardware_matches_concrete_registers_and_flags"),
    Defect("v2-x86-sign-flag-bit-62-in-smt", "verify2.py", "flag rule SF",
           'f.update(zf=result == word(0), sf=bit(result, 63))',
           'f.update(zf=result == word(0), sf=bit(result, 62))',
           V2 + "test_x86_sign_flag_is_bit_63_of_the_result",
           "The symbolic SF reads bit 62.",
           CONFORMANCE2 + "X86FlagModelConformance.test_hardware_matches_smt_registers_and_flags"),
    Defect("v2-x86-le-ignores-zero-flag-in-smt", "verify2.py", "condition rule",
           '"le": lambda: z.Or(zf, sf != of)', '"le": lambda: sf != of',
           V2 + "test_x86_le_holds_on_equal_values_through_the_zero_flag",
           "The symbolic CMOVLE and SETLE ignore ZF.",
           CONFORMANCE2 + "X86FlagModelConformance.test_hardware_matches_smt_registers_and_flags"),
    Defect("v2-x86-le-ignores-zero-flag-in-concrete", "verify2.py", "condition rule",
           '"le": zf or sf != of', '"le": sf != of',
           V2 + "test_x86_le_holds_on_equal_values_through_the_zero_flag",
           "The concrete CMOVLE and SETLE ignore ZF.",
           CONFORMANCE2 + "X86FlagModelConformance.test_hardware_matches_concrete_registers_and_flags"),
    Defect("v2-rv-zero-register-writable", "verify2.py", "zero register",
           '        _require(rd != 0, "ZeroWriteHintRefused")\n', '',
           V2 + "test_rv_unadmitted_shift_and_word_forms_refuse",
           "A write to x0 decodes, so the zero register is treated as writable."),
    Defect("v2-undefined-flag-read-admitted", "verify2.py", "flag definedness",
           '_require(set(reads) <= defined, "ReadsUndefinedFlag")', 'pass',
           V2 + "test_x86_reading_a_flag_left_undefined_refuses",
           "Reading ZF after IMUL is admitted by the definedness pass."),
    Defect("v2-imul-claims-zero-and-sign-flags", "verify2.py", "flag definedness",
           'return (), ("cf", "of"), ("zf", "sf")', 'return (), FLAGS, ()',
           V2 + "test_x86_reading_a_flag_left_undefined_refuses",
           "IMUL is listed as defining ZF and SF, which it leaves undefined."),
    Defect("v2-solver-unknown-read-as-equivalent", "verify2.py", "solver result",
           'if answer == z.unsat:', 'if answer != z.sat:',
           V2 + "test_solver_absent_unknown_and_replay_mismatch_are_never_equivalent",
           "An unknown solver answer is reported as equivalence."),
    Defect("v2-counterexample-without-replay", "verify2.py", "counterexample replay",
           'if e != [_ast_concrete(output, values) for output in p["outputs"]] or a != replayed or e == a:',
           'if False:',
           V2 + "test_solver_absent_unknown_and_replay_mismatch_are_never_equivalent",
           "A solver model is reported as a counterexample without its concrete replay."),
    Defect("v2-x86-imm8-zero-extended", "verify2.py", "immediate sign extension",
           'value = int.from_bytes(take(1), "little", signed=True)',
           'value = int.from_bytes(take(1), "little")',
           V2 + "test_x86_immediate_alu_forms_sign_extend_both_widths",
           "ADD RAX, -5 decodes as ADD RAX, 251.",
           CONFORMANCE2 + "X86FlagModelConformance.test_decoder_round_trips_every_admitted_form"),
    Defect("v2-x86-imm32-zero-extended", "verify2.py", "immediate sign extension",
           '                    value = int.from_bytes(take(4), "little", signed=True)\n'
           '                    _require(not -128 <= value <= 127, "Imm32WhereImm8Fits")',
           '                    value = int.from_bytes(take(4), "little")\n'
           '                    _require(not -128 <= value <= 127, "Imm32WhereImm8Fits")',
           V2 + "test_x86_immediate_alu_forms_sign_extend_both_widths",
           "SUB R11, -2^31 decodes as an out-of-profile positive 2^31.",
           CONFORMANCE2 + "X86FlagModelConformance.test_decoder_round_trips_every_admitted_form"),
    Defect("v2-x86-accumulator-imm32-zero-extended", "verify2.py", "immediate sign extension",
           '                value = int.from_bytes(take(4), "little", signed=True)\n'
           '                _require(not -128 <= value <= 127, "Imm32WhereImm8Fits")\n'
           '                op, operands = _ACCUMULATOR[opcode], ("rax", value)',
           '                value = int.from_bytes(take(4), "little")\n'
           '                _require(not -128 <= value <= 127, "Imm32WhereImm8Fits")\n'
           '                op, operands = _ACCUMULATOR[opcode], ("rax", value)',
           V2 + "test_x86_immediate_alu_forms_sign_extend_both_widths",
           "AND RAX, -256 in the RAX short form decodes as AND RAX, 2^32-256."),
    Defect("v2-x86-immediate-zero-extended-in-smt", "verify2.py", "immediate sign extension",
           'left, right = r[dst], (r[a[1]] if type(a[1]) is str else word(a[1] & MASK))',
           'left, right = r[dst], (r[a[1]] if type(a[1]) is str else word(a[1] & 0xFFFFFFFF))',
           V2 + "test_x86_immediate_alu_forms_sign_extend_both_widths",
           "The symbolic model reads a negative immediate as a zero-extended 32-bit word."),
    Defect("v2-x86-immediate-cmp-carry-unmasked", "verify2.py", "flag rule CF",
           'left, right = r[dst], (r[a[1]] if type(a[1]) is str else a[1] & MASK)',
           'left, right = r[dst], (r[a[1]] if type(a[1]) is str else a[1])',
           V2 + "test_x86_immediate_compare_flags",
           "The concrete CMP with a negative immediate computes CF against the unwrapped value: 5 < -1 is false.",
           CONFORMANCE2 + "X86FlagModelConformance.test_hardware_matches_concrete_registers_and_flags"),
    Defect("v2-x86-imm32-where-imm8-fits-accepted", "verify2.py", "canonical encoding",
           '                    _require(not -128 <= value <= 127, "Imm32WhereImm8Fits")\n', '',
           V2 + "test_x86_immediate_non_canonical_and_unadmitted_encodings_refuse",
           "81 /n id with an 8-bit value decodes, so one form has two encodings.",
           CONFORMANCE2 + "HostIndependentModelChecks.test_every_accepted_short_x86_body_is_an_enumerated_canonical_form"),
    Defect("v2-x86-accumulator-form-not-required", "verify2.py", "canonical encoding",
           '                    _require(dst != "rax", "AccumulatorFormRequired")\n', '',
           V2 + "test_x86_immediate_non_canonical_and_unadmitted_encodings_refuse",
           "81 /n id on RAX decodes, though the RAX short form is the canonical encoding.",
           CONFORMANCE2 + "HostIndependentModelChecks.test_every_accepted_short_x86_body_is_an_enumerated_canonical_form"),
    Defect("v3-displacement-not-scaled", "verify3.py", "memory displacement scale",
           "    return displacement // WORD", "    return displacement",
           V3 + "test_swap_and_copy_kernels_are_equivalent",
           "A byte displacement is used as a word index, so [base+8] addresses word 8 instead of word 1."),
    Defect("v3-store-to-read-only-admitted", "verify3.py", "memory permission",
           '    _require(op == "load" or region["access"] == "read-write", "StoreToReadOnlyRegion")\n', "",
           V3 + "test_store_to_read_only_region_refuses",
           "A store to a read-only region is admitted."),
    Defect("v3-region-word-unquantified", "verify3.py", "region quantification",
           'memory = {r["name"]: [word(f"mc_region_{r[\'name\']}_{i}") for i in range(r["words"])] for r in p["regions"]}',
           'memory = {r["name"]: [word(f"mc_region_{r[\'name\']}_{i}") if i else z.BitVecVal(0, 64, ctx=context)'
           ' for i in range(r["words"])] for r in p["regions"]}',
           V3 + "test_initial_region_words_are_quantified",
           "The first word of every region is assumed 0 at entry instead of quantified."),
    Defect("v3-misaligned-access-admitted", "verify3.py", "memory alignment",
           '    _require(type(displacement) is int and displacement % WORD == 0, "MisalignedAccess")\n', "",
           V3 + "test_misaligned_access_refuses",
           "A displacement that is not a multiple of 8 is admitted."),
    Defect("v3-store-dropped-in-smt", "verify3.py", "dropped store",
           "        m[regions[base]][_index(displacement)] = r[src]\n    elif target == X86:\n        verify2._x86_smt(",
           "        pass\n    elif target == X86:\n        verify2._x86_smt(",
           V3 + "test_swap_and_copy_kernels_are_equivalent",
           "The symbolic run drops every store, so correct kernels stop being equivalent."),
    Defect("v3-redefined-base-admitted", "verify3.py", "base redefinition",
           '            _require(base not in written, "RedefinedRegionBase")\n', "",
           V3 + "test_a_redefined_base_refuses",
           "An access through a base register that an earlier instruction overwrote is admitted."),
    Defect("v3-out-of-region-admitted", "verify3.py", "region bounds",
           '    _require(0 <= displacement < WORD * region["words"], "DisplacementOutsideRegion")\n', "",
           V3 + "test_out_of_region_displacements_refuse",
           "A displacement past the region's last word, or negative, is admitted."),
    Defect("v3-base-pointer-fixed", "verify3.py", "base quantification",
           'bases = {n: word("mc_base_" + n) for n in base_registers}',
           'bases = {n: z.BitVecVal(0, 64, ctx=context) for n in base_registers}',
           V3 + "test_base_pointer_values_are_quantified",
           "Every region base pointer is assumed 0 instead of quantified."),
    Defect("v3-scratch-fixed-to-zero", "verify3.py", "scratch quantification",
           'initial = {n: word("mc_initial_" + n) for n in scratch}',
           'initial = {n: z.BitVecVal(0, 64, ctx=context) for n in scratch}',
           V3 + "test_scratch_and_flags_are_quantified",
           "Scratch registers are assumed 0 at entry instead of quantified."),
    Defect("v3-solver-unknown-read-as-equivalent", "verify3.py", "solver result",
           "if answer == z.unsat:", "if answer != z.sat:",
           V3 + "test_solver_absent_unknown_and_replay_mismatch_are_never_equivalent",
           "An unknown solver answer is reported as equivalence."),
    Defect("v3-counterexample-without-replay", "verify3.py", "counterexample replay",
           "if e != concrete_expected or a != concrete_actual or (e == a and read_only_kept):", "if False:",
           V3 + "test_solver_absent_unknown_and_replay_mismatch_are_never_equivalent",
           "A solver model is reported as a counterexample without its concrete replay."),
    Defect("v3-rv-store-offset-low-five-bits", "verify3.py", "operand width",
           "        immediate = ((word >> 25) << 5) | ((word >> 7) & 31)", "        immediate = (word >> 7) & 31",
           V3 + "test_rv_store_offsets_use_all_twelve_bits",
           "An RV64I SD offset keeps only imm[4:0], so 40(a0) addresses byte 8."),
    Defect("v3-rv-partial-width-admitted", "verify3.py", "memory permission",
           '    _require(funct3 == 3, "UnsupportedMemoryWidth")\n', "",
           V3 + "test_rv_refusals",
           "An RV64I LW or SW decodes as a 64-bit access."),
    Defect("v3-rv-load-to-zero-admitted", "verify3.py", "zero register",
           '        _require((word >> 7) & 31 != 0, "ZeroWriteHintRefused")\n', "",
           V3 + "test_rv_refusals",
           "An LD into x0 decodes, so the zero register is treated as writable."),
    Defect("realize-slot-range-drops-last-form", "realize.py", "palette coverage",
           "        solver.add(selector >= 0, selector < palette)\n",
           "        solver.add(selector >= 0, selector < palette - 1)\n",
           REALIZE + "test_a_missing_palette_form_is_caught_by_the_cross_check",
           "The query never selects the palette's last form, so a length whose programs all need it is reported "
           "unrealizable.",
           REALIZE + "test_every_small_problem_agrees_with_exhaustive_enumeration"),
    Defect("realize-cost-bound-ignored", "realize.py", "cost bound",
           "        solver.add(total < bound)\n",
           "        pass\n",
           REALIZE + "test_every_small_problem_agrees_with_exhaustive_enumeration",
           "Programs at the bound are admitted, so a length below the least cost is reported realizable."),
)

# Runs one control inside a specimen and prints one JSON line. A failure counts only when its AssertionError was
# raised in the test module or unittest itself, never in the checker package under test.
HARNESS = r'''
import json, sys, traceback, unittest
from pathlib import Path
specimen, name = Path(sys.argv[1]).resolve(), sys.argv[2]
outcome = {"failures": [], "errors": [], "skipped": [], "run": 0}
class Result(unittest.TestResult):
    def addFailure(self, test, err):
        frames = traceback.extract_tb(err[2])
        inner = frames[-1].filename if frames else ""
        bucket = "errors" if str(specimen / "machine_code") in str(Path(inner).resolve()) else "failures"
        outcome[bucket].append({"test": test.id(), "message": str(err[1])[:600]})
    def addError(self, test, err):
        outcome["errors"].append({"test": test.id() if hasattr(test, "id") else str(test),
                                  "message": (err[0].__name__ + ": " + str(err[1]))[:600]})
    def addSubTest(self, test, subtest, err):
        if err is not None:
            (self.addFailure if issubclass(err[0], test.failureException) else self.addError)(subtest, err)
    def addSkip(self, test, reason):
        outcome["skipped"].append({"test": test.id(), "message": reason[:600]})
loader = unittest.TestLoader()
try:
    suite = loader.loadTestsFromName(name)
except Exception as error:
    outcome["load_error"] = type(error).__name__ + ": " + str(error)[:600]
if loader.errors:
    outcome["load_error"] = str(loader.errors[0])[-600:]
if "load_error" not in outcome:
    result = Result()
    suite.run(result)
    outcome["run"] = result.testsRun
import machine_code, machine_code.session as session
outcome["package"] = str(Path(machine_code.__file__).resolve().parent)
try:
    outcome["identity_sources"] = session.identity()["sources"]
except Exception as error:
    outcome["identity_error"] = type(error).__name__ + ": " + str(error)[:600]
print("ZENO_CONTROL_RESULT " + json.dumps(outcome, sort_keys=True))
'''


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def build_specimen(directory: Path, defect: Defect | None) -> dict:
    """Copy the checker package, control modules and kernels; apply `defect`. Returns the edit binding."""
    tools = directory / "tools"
    shutil.copytree(ROOT / "tools/machine_code", tools / "machine_code",
                    ignore=shutil.ignore_patterns("__pycache__", "*.pyc"))
    for module in TEST_MODULES:
        shutil.copy2(ROOT / "tools" / module, tools / module)
    shutil.copytree(ROOT / "test-data/machine-code", directory / "test-data/machine-code")
    if defect is None:
        return {"edited": None}
    target = tools / "machine_code" / defect.file
    source = target.read_text(encoding="utf-8")
    count = source.count(defect.anchor)
    if count != 1:
        return {"invalid": f"anchor occurs {count} times in {defect.file}"}
    edited = source.replace(defect.anchor, defect.replacement)
    target.write_text(edited, encoding="utf-8")
    try:
        py_compile.compile(str(target), cfile=str(directory / "compile-check.pyc"), doraise=True)
    except py_compile.PyCompileError as error:
        return {"invalid": "edited source does not compile: " + str(error)[:300]}
    return {"edited": defect.file, "edited_sha256": digest(edited.encode()),
            "original_sha256": digest(source.encode())}


def run_control(directory: Path, control: str, mode: tuple[str, ...], timeout: int = TIMEOUT) -> dict:
    """Run one control in a specimen under one interpreter mode; never reuses the caller's modules."""
    environment = {key: value for key, value in os.environ.items()
                   if key not in ("PYTHONPATH", "PYTHONSAFEPATH", "PYTHONOPTIMIZE", "PYTHONDONTWRITEBYTECODE")}
    started = time.monotonic()
    try:
        completed = subprocess.run([sys.executable, *mode, "-c", HARNESS, str(directory / "tools"), control],
                                   cwd=directory / "tools", env=environment, capture_output=True, text=True,
                                   timeout=timeout)
    except subprocess.TimeoutExpired:
        return {"timeout": True, "seconds": round(time.monotonic() - started, 1)}
    lines = [line for line in completed.stdout.splitlines() if line.startswith("ZENO_CONTROL_RESULT ")]
    outcome = json.loads(lines[-1].split(" ", 1)[1]) if lines else {"harness_error": completed.stderr[-600:]}
    outcome["seconds"] = round(time.monotonic() - started, 1)
    return outcome


def classify(outcome: dict, directory: Path, binding: dict) -> tuple[str, str]:
    """The outcome of one control run against one specimen, with its evidence."""
    if "invalid" in binding:
        return "invalid-specimen", binding["invalid"]
    if outcome.get("timeout"):
        return "inconclusive", f"timeout after {outcome['seconds']} s"
    if "harness_error" in outcome or "load_error" in outcome:
        return "invalid-specimen", (outcome.get("load_error") or outcome["harness_error"]).strip()[-300:]
    if Path(outcome.get("package", "")) != (directory / "tools/machine_code").resolve():
        return "invalid-specimen", "the control imported another checker package: " + outcome.get("package", "")
    if "identity_error" in outcome:
        return "invalid-specimen", outcome["identity_error"]
    if binding.get("edited") and outcome["identity_sources"].get(binding["edited"]) != binding["edited_sha256"]:
        return "invalid-specimen", "the specimen identity does not bind the edited source"
    if outcome["run"] == 0:
        return "invalid-specimen", "the control ran no test"
    if outcome["errors"]:
        return "inconclusive", "crash: " + outcome["errors"][0]["message"]
    if outcome["skipped"]:
        return "inconclusive", "skipped: " + outcome["skipped"][0]["message"]
    if outcome["failures"]:
        message = outcome["failures"][0]["message"]
        if any(marker in message.lower() for marker in IDENTITY_MARKERS):
            return "inconclusive", "identity refusal: " + message
        return "detected", message.splitlines()[0][:300] if message else "assertion failed"
    return "survived", f"{outcome['run']} test(s) passed"


def mode_name(mode: tuple[str, ...]) -> str:
    return " ".join(mode)


def campaign(defects: tuple[Defect, ...], modes=MODES, timeout: int = TIMEOUT, positive=POSITIVE) -> dict:
    """Positive controls on an unmodified specimen, then each defect's intended control on its own specimen."""
    report = {"schema": SCHEMA, "authority": "none", "python": sys.version.split()[0],
              "sources": {name: digest((ROOT / "tools/machine_code" / name).read_bytes())
                          for name in ("verify.py", "verify2.py")},
              "positive": [], "defects": []}
    with tempfile.TemporaryDirectory(prefix="zeno-machine-controls-") as temporary:
        base = Path(temporary) / "base"
        build_specimen(base, None)
        controls = [(label, name) for label, name in positive]
        controls += [(f"control of {d.name} on the unmodified checker", name)
                     for d in defects for name in (d.control, d.second) if name is not None
                     and name not in {module for _, module in positive}
                     and not any(name.startswith(module + ".") for _, module in positive)]
        seen = set()
        for label, name in controls:
            if name in seen:
                continue
            seen.add(name)
            for mode in modes:
                outcome, evidence = classify(run_control(base, name, mode, timeout), base, {"edited": None})
                passed = outcome == "survived"
                report["positive"].append({"control": name, "label": label, "mode": mode_name(mode),
                                           "status": "pass" if passed else "fail", "evidence": evidence})
        for index, defect in enumerate(defects):
            specimen = Path(temporary) / f"defect-{index:02d}"
            binding = build_specimen(specimen, defect)
            row = {"defect": defect.name, "file": defect.file, "kind": defect.kind, "control": defect.control,
                   "second": defect.second, "why": defect.why, "binding": binding, "outcomes": {},
                   "second_outcomes": {}}
            for key, control in (("outcomes", defect.control), ("second_outcomes", defect.second)):
                for mode in modes if control is not None else ():
                    outcome = {} if "invalid" in binding else run_control(specimen, control, mode, timeout)
                    status, evidence = classify(outcome, specimen, binding)
                    row[key][mode_name(mode)] = {"outcome": status, "evidence": evidence,
                                                 "seconds": outcome.get("seconds")}
            report["defects"].append(row)
            shutil.rmtree(specimen)
    outcomes = [o["outcome"] for row in report["defects"]
                for o in (*row["outcomes"].values(), *row["second_outcomes"].values())]
    if any(p["status"] != "pass" for p in report["positive"]) or "survived" in outcomes:
        report["status"] = "failed"
    elif any(o != "detected" for o in outcomes):
        report["status"] = "inconclusive"
    else:
        report["status"] = "all-detected"
    return report


def table(report: dict) -> str:
    modes = list(report["defects"][0]["outcomes"]) if report["defects"] else []
    lines = ["defect | kind | intended control | " + " | ".join(modes) + " | second control | "
             + " | ".join(modes)]
    for row in report["defects"]:
        second = row["second_outcomes"]
        lines.append(" | ".join([row["defect"], row["kind"], row["control"].rsplit(".", 1)[-1],
                                 *(row["outcomes"][m]["outcome"] for m in modes),
                                 row["second"].rsplit(".", 1)[-1] if row["second"] else "-",
                                 *(second[m]["outcome"] if m in second else "-" for m in modes)]))
    failed = [p for p in report["positive"] if p["status"] != "pass"]
    lines.append(f"positive controls: {len(report['positive']) - len(failed)} of {len(report['positive'])} passed"
                 + "".join(f"\n  FAIL {p['control']} ({p['mode']}): {p['evidence']}" for p in failed))
    lines.append("status: " + report["status"])
    return "\n".join(lines)


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    run = commands.add_parser("run")
    run.add_argument("--defect", action="append", choices=[d.name for d in DEFECTS])
    run.add_argument("--out", type=Path)
    commands.add_parser("list")
    args = parser.parse_args(argv)
    if args.command == "list":
        for d in DEFECTS:
            print(f"{d.name}\t{d.file}\t{d.kind}\t{d.control}" + (f"\t{d.second}" if d.second else ""))
        return 0
    selected = tuple(d for d in DEFECTS if not args.defect or d.name in args.defect)
    report = campaign(selected)
    if args.out:
        args.out.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(table(report))
    return {"all-detected": 0, "failed": 1}.get(report["status"], 2)


if __name__ == "__main__":
    raise SystemExit(main())
