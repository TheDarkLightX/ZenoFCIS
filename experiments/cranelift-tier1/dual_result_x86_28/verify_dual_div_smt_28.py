#!/usr/bin/env python3
"""QF_BV audit of the *specified* two handwritten x86-64 DIV kernels.

The exact source instruction listing is parsed and symbolically interpreted.
This proves their equivalence under the declared instruction model, NOT
actual x86 machine-code correctness or a Cranelift compiler transformation.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import z3

SCHEMA = 'zeno-fcis/tier1-x86-dual-div-symbolic-qfbv/1'
BODIES = {
 'tier1_two_div': (
  'movq %rdi, %rax', 'xorl %edx, %edx', 'divq %rsi',
  'movq %rax, %rcx', 'movq %rdi, %rax', 'xorl %edx, %edx',
  'divq %rsi', 'rolq $17, %rdx', 'xorq %rcx, %rdx',
  'movq %rdx, %rax', 'retq'),
 'tier1_one_div': (
  'movq %rdi, %rax', 'xorl %edx, %edx', 'divq %rsi',
  'rolq $17, %rdx', 'xorq %rdx, %rax', 'retq'),
}

def extract(source):
    # Fail closed on new instructions, labels, directives or altered bodies.
    bodies = {}
    current = None
    for raw in source.splitlines():
        line = raw.strip()
        if not line or line.startswith('#'):
            continue
        if line.endswith(':'):
            label = line[:-1]
            if current is not None or label not in BODIES or label in bodies:
                raise ValueError('unexpected assembly label')
            current = label
            bodies[current] = []
        elif line.startswith('.size '):
            if current is None or line != f'.size {current}, .-{current}':
                raise ValueError('unexpected symbol size directive')
            current = None
        elif line.startswith('.'):
            if current is not None or not (
                line in ('.text', '.p2align 4',
                         '.section .note.GNU-stack,"",@progbits')
                or re.fullmatch(
                    r'\.(?:globl|type) tier1_(?:one|two)_div(?:,@function)?',
                    line)):
                raise ValueError('unrecognized assembler directive')
        else:
            if current is None:
                raise ValueError('instruction outside checked functions')
            bodies[current].append(line)
    if current is not None:
        raise ValueError('unterminated symbol')
    normalized = {k: tuple(v) for k, v in bodies.items()}
    if normalized != BODIES:
        raise ValueError('source instructions differ from checked exact bodies')
    return normalized

def execute(body, x, y):
    # Exact two bodies overwrite incoming scratch regs before reading them.
    regs = {'rdi': x, 'rsi': y}
    trap_predicates = []
    div_count = 0
    for inst in body:
        if inst == 'movq %rdi, %rax':
            regs['rax'] = regs['rdi']
        elif inst == 'movq %rax, %rcx':
            regs['rcx'] = regs['rax']
        elif inst == 'movq %rdx, %rax':
            regs['rax'] = regs['rdx']
        elif inst == 'xorl %edx, %edx':
            regs['rdx'] = z3.BitVecVal(0, 64)
        elif inst == 'divq %rsi':
            # x86 DIV consumes RDX:RAX, but the exact bodies clear RDX.
            # For y!=0, unsigned quotient fits: no quotient-overflow fault.
            high = z3.simplify(regs['rdx'])
            if not z3.is_bv_value(high) or high.as_long() != 0:
                raise ValueError('unmodeled 128-bit dividend: RDX not zero')
            old_low = regs['rax']
            regs['rax'] = z3.UDiv(old_low, y)
            regs['rdx'] = z3.URem(old_low, y)
            trap_predicates.append(y == 0)
            div_count += 1
        elif inst == 'rolq $17, %rdx':
            regs['rdx'] = z3.RotateLeft(regs['rdx'], 17)
        elif inst == 'xorq %rcx, %rdx':
            regs['rdx'] = regs['rdx'] ^ regs['rcx']
        elif inst == 'xorq %rdx, %rax':
            regs['rax'] = regs['rax'] ^ regs['rdx']
        elif inst == 'retq':
            pass
        else:
            raise ValueError('unmodeled instruction')
    return regs['rax'], trap_predicates, div_count

def unsat(name, assumptions, bad, timeout_ms):
    s = z3.SolverFor('QF_BV')
    s.set(timeout=timeout_ms)
    s.add(*assumptions, bad)
    result = s.check()
    if result != z3.unsat:
        reason = s.reason_unknown() if result == z3.unknown else str(s.model())
        raise RuntimeError(f'{name}: {result}: {reason}')
    return {'name': name, 'result': 'unsat',
            'logic': 'QF_BV', 'timeout_ms': timeout_ms}

def self_test(source):
    if extract(source) != BODIES:
        raise AssertionError('source mismatch')
    corruptions = (
        source.replace('divq %rsi', 'idivq %rsi', 1),
        source.replace('rolq $17', 'rolq $16', 1),
        source + '\nnop\n',
        source.replace('retq', 'retq\n    pushq %rax', 1))
    for altered in corruptions:
        try:
            extract(altered)
        except ValueError:
            continue
        raise AssertionError('altered source accepted')
    return True

def qualify(src, out, timeout_ms):
    raw = src.read_bytes()
    source = raw.decode('utf-8')
    self_test(source)
    x, y = z3.BitVecs('x y', 64)
    two, traps_two, n2 = execute(BODIES['tier1_two_div'], x, y)
    one, traps_one, n1 = execute(BODIES['tier1_one_div'], x, y)
    if (n2, n1) != (2, 1):
        raise AssertionError('division count')
    reference = z3.UDiv(x, y) ^ z3.RotateLeft(z3.URem(x, y), 17)
    admitted = [y != z3.BitVecVal(0, 64)]
    checks = [
        unsat('two-div-versus-independent-reference',
              admitted, two != reference, timeout_ms),
        unsat('one-div-versus-independent-reference',
              admitted, one != reference, timeout_ms),
        unsat('one-div-versus-two-div',
              admitted, one != two, timeout_ms)]
    if len(traps_two) != 2 or len(traps_one) != 1:
        raise AssertionError('divisor-trap count')
    if not all(z3.eq(t, y == 0) for t in [*traps_two, *traps_one]):
        raise AssertionError('unexpected division-trap predicate')
    report = {
        'schema': SCHEMA,
        'status': 'proved-in-declared-QF_BV-instruction-model',
        'source_file': str(src),
        'source_sha256': hashlib.sha256(raw).hexdigest(),
        'source_symbol_names': list(BODIES),
        'semantics': {
            'inputs': 'uint64 x in RDI; uint64 y in RSI; result in RAX',
            'admitted_domain': 'all unsigned 64-bit x,y with y != 0',
            'result': '(x/y) XOR rotl64(x%y,17)',
            'zero_divisor': 'both execute a faulting DIV on y=0; explicit trap predicate, not arbitrary Z3 BV UDiv(0) semantics',
            'quotient_overflow': 'RDX cleared before each DIV; y!=0 guarantees quotient fits 64 bits',
            'signedness': 'unsigned DIV only; signed IDIV/overflow excluded',
            'flags': 'DIV flags undefined; XOR/ROL affect flags; neither body reads flags; SysV ABI does not preserve flags',
            'undef_poison': 'not a CLIF/LLVM proof; no poison in this modeled machine-language contract',
            'target': 'x86-64 SysV, exact written assembly; no regalloc or compiler-pass claim'},
        'verification': {
            'z3_version': z3.get_version_string(),
            'proof_obligations': checks,
            'source_parser_negative_controls': 4,
            'assurance_scope': 'symbolic interpretation of fixed whitelisted source instructions; does not decode executable machine bytes',
            'unknown_policy': 'any SAT, UNKNOWN or timeout raises and fails CI'},
        'cost_scope': 'no new code-size/throughput claim; see separately archived native receipt',
        'upstream_claim': 'none',
    }
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(report, sort_keys=True, indent=2) + '\n')
    return report

if __name__ == '__main__':
    p = argparse.ArgumentParser()
    p.add_argument('--source', type=Path,
                   default=Path(__file__).with_name('dual_div.S'))
    p.add_argument('--out', type=Path,
                   default=Path('tier1-dual-result-smt/receipt.json'))
    p.add_argument('--timeout-ms', type=int, default=20000)
    p.add_argument('--self-test', action='store_true')
    a = p.parse_args()
    if not 1000 <= a.timeout_ms <= 120000:
        raise ValueError('invalid timeout')
    if a.self_test:
        self_test(a.source.read_text())
        print('parser and four negative controls passed')
    else:
        report = qualify(a.source, a.out, a.timeout_ms)
        print(json.dumps({'status': report['status'],
                          'checks': report['verification']['proof_obligations']},
                         sort_keys=True))
