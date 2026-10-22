#!/usr/bin/env python3
"""Independent integer/rational D6/D10 oracle and deterministic test-data generator."""
import argparse
from fractions import Fraction
from itertools import product
from pathlib import Path

LOW, HIGH = -(1 << 127), (1 << 127) - 1
FIXTURE = Path('crates/zeno-fcis-contract/tests/test-data/w25/oracle.tsv')


def fused(a, b, d, *, signed=False, ceil=False, p=0, t=0, j=0, q=0):
    if d <= 0 or (not signed and (a < 0 or b < 0)):
        return None
    value = Fraction(a * b * 10 ** (q + j), d * 10 ** (p + t))
    result = -((-value.numerator) // value.denominator) if ceil else value.numerator // value.denominator
    return result if LOW <= result <= HIGH else None


def recurrence(x, a, d, count, *, signed=False):
    values = [x]
    for _ in range(count):
        value = fused(values[-1], a, d, signed=signed)
        if value is None:
            return values, None
        values.append(value)
    return values, values[-1]


def rows():
    output = []
    factors = [LOW, -2, -1, 0, 1, 2, HIGH]
    for signed in [False, True]:
        for a, b in product(factors, repeat=2):
            for d in [-1, 0, 1, 2, HIGH]:
                for ceil in ([False] if signed else [False, True]):
                    answer = fused(a, b, d, signed=signed, ceil=ceil)
                    output.append(['signed' if signed else 'ceil' if ceil else 'floor', a, b, d, 1, 'none' if answer is None else answer])
    for signed in [False, True]:
        for p in range(17):
            for x, a, d in [(1001, 225000, 300000), (1, 2, 1), (1, -2, 1), (HIGH, HIGH, HIGH), (HIGH, 2, 1), (LOW, 1, 1), (1, 1, 0), (0, 1, -1)]:
                _, answer = recurrence(x, a, d, p, signed=signed)
                output.append(['repeat_signed' if signed else 'repeat', x, a, d, p, 'none' if answer is None else answer])
    return output


def render():
    return '# kind\ta\tb\td\tcount\texpected\n' + ''.join('\t'.join(map(str, row)) + '\n' for row in rows())


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--generate', action='store_true')
    args = parser.parse_args()
    path = Path(__file__).resolve().parents[1] / FIXTURE
    scaled = []
    for p in range(39):
        for t, j, q in [(0,0,p),(0,0,0),(0,0,38),(4,0,p),(0,p,0)]:
            k = q+j-p-t
            if abs(k)>38: continue
            for signed, ceil, a, b, d in [(False,False,123456789,25,1),(False,True,123456789,25,1),(True,False,-1001,25,10000),(True,False,LOW,1,HIGH),(False,False,HIGH,HIGH,HIGH),(True,False,-1,-1,2),(True,False,0,1,0)]:
                factor=10**abs(k)
                fits = (LOW <= d*factor <= HIGH) if k<0 else (LOW <= a*factor <= HIGH or LOW <= b*factor <= HIGH)
                if k and not fits: continue
                expected=fused(a,b,d,signed=signed,ceil=ceil,p=p,t=t,j=j,q=q)
                scaled.append([int(signed),int(ceil),a,b,d,p,t,j,q,'none' if expected is None else expected])
    scale_data = ''.join('\t'.join(map(str,row))+'\n' for row in scaled)
    if args.generate: path.with_name('scales.tsv').write_text(scale_data)
    assert path.with_name('scales.tsv').read_text() == scale_data
    aggregate = fused(HIGH, 1, 2 * (HIGH // 10), p=1, q=0)
    assert aggregate == 0 and 2 * (HIGH // 10) <= HIGH < 20 * (HIGH // 10)
    examples = f"aggregate_denominator_floor\t{aggregate}\n" + f"bps_floor\t{fused(123456789,25,1,p=8,t=4,q=8)}\nbps_ceil\t{fused(123456789,25,1,p=8,t=4,q=8,ceil=True)}\n" + ''.join(f"chain_{p}\t{recurrence(1001,225000,300000,p)[1]}\n" for p in range(17))
    for x, a, d in [(2,1,2),(1001,3,4)]:
        for first, second in [('floor','ceil'),('ceil','floor')]:
            prefix=fused(x,a,d,ceil=first=='ceil')
            final=fused(prefix,a,d,ceil=second=='ceil')
            key=f'mixed_{x}_{a}_{d}_{first}_{second}'
            examples += f'{key}_prefix\t{prefix}\n{key}_final\t{final}\n'
    if args.generate:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(render())
        path.with_name('examples.tsv').write_text(examples)
    assert path.with_name('examples.tsv').read_text() == examples
    assert path.read_text() == render(), 'stale quantity/chain oracle fixture'
    assert fused(123456789, 25, 1, p=8, t=4, q=8) == 308641
    assert fused(123456789, 25, 1, p=8, t=4, q=8, ceil=True) == 308642
    assert recurrence(1001, 225000, 300000, 2)[0] == [1001, 750, 562]
    print(f'{len(rows())} exact rational/prefix vectors and {len(scaled)} scale vectors PASS')


if __name__ == '__main__':
    main()
