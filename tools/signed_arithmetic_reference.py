"""Independent D7 integer/rational oracle. This module does not execute Rust."""
from fractions import Fraction
from math import floor
from random import Random

MIN, MAX = -(1 << 127), (1 << 127) - 1

def result(a: int, b: int, d: int) -> int | None:
    if any(type(x) is not int or not MIN <= x <= MAX for x in (a,b,d)) or d <= 0:
        return None
    q = floor(Fraction(a*b,d))
    return q if MIN <= q <= MAX else None

def cases() -> tuple[tuple[int,int,int], ...]:
    edge=(MIN,MIN+1,-(1<<64),-3,-2,-1,0,1,2,3,1<<64,MAX-1,MAX)
    rows=[(a,b,d) for a in edge for b in edge for d in edge]
    rows += [(1001,-25,10000),(1001,25,10000),(MIN,-1,2),(MAX,MAX,MAX),
             (MIN,MIN,1),(MIN,0,-1),(-3,2,4),(-1,1,2),(1,1,2)]
    rng=Random(20261016)
    for _ in range(256):
        rows.append((rng.randint(MIN,MAX),rng.randint(MIN,MAX),rng.randint(1,MAX)))
    return tuple(dict.fromkeys(rows))

def vectors() -> str:
    return ''.join(f'{a}\t{b}\t{d}\t'+('arithmetic' if result(a,b,d) is None else str(result(a,b,d)))+'\n'
                   for a,b,d in cases())
