#!/usr/bin/env python3
"""Independent exact-integer oracle vectors; no candidate implementation import."""
from __future__ import annotations
import itertools
from pathlib import Path

MAX = 2**128 - 1
DESTINATION = Path(__file__).resolve().parents[1] / "crates/zeno-fcis-synthesis/tests/test-data/oracle/arithmetic.tsv"

def vectors() -> list[str]:
    rows = ["# oracle/1 test-only exact-integer vectors; M=median, C=capacity"]
    for values in itertools.product(range(1, 6), repeat=3):
        m = sorted(values)[1]
        radius = max(abs(x-m) for x in values)
        deviation = (10_000 * radius + m - 1) // m
        rows.append("M\t" + "\t".join(map(str, (*values, m, radius, deviation))))
    amounts = (0, 1, 2, 10**8-1, 10**8, 2**64, 2**127, MAX-1, MAX)
    fractions = ((1,2), (2,3), (MAX-1,MAX), (1,MAX))
    for s, p, (a,b) in itertools.product(amounts, amounts, fractions):
        value = s*p*a // (10**8*b)
        result = str(value) if value <= MAX else "overflow"
        rows.append("C\t" + "\t".join(map(str,(s,p,a,b))) + "\t" + result)
    for values in ((1,1,MAX),(1,MAX,MAX),(MAX,MAX,MAX),(2**127,MAX-1,MAX)):
        m=sorted(values)[1]
        c=max(abs(x-m) for x in values)
        d=(10_000*c+m-1)//m
        rows.append("M\t"+"\t".join(map(str,values))+"\t"+(f"{m}\t{c}\t{d}" if d<=MAX else "overflow"))
    for bonds in itertools.product(range(5),repeat=3):
        coalition=min(sum(pair) for pair in itertools.combinations(bonds,2))
        for exposure in range(5):
            capacity=coalition//2
            rows.append("B\t"+"\t".join(map(str,(*bonds,exposure,capacity,int(exposure<=capacity)))))
    return rows

PAIRS = DESTINATION.with_name("capacity-pairs.tsv")

def pair_vectors() -> list[str]:
    """Full-width coalition capacity: the two smallest bonds may sum past MAX."""
    rows = ["# oracle/1 test-only coalition capacity; P=b0,b1,b2,stress,a,b,floor(min pair*stress*a/(10^8*b))"]
    bonds = (0, 1, 2**127-1, 2**127, MAX)
    stresses = (1, 10**8, MAX)
    fractions = ((1,2), (MAX-1,MAX))
    for triple in itertools.product(bonds, repeat=3):
        stake = min(sum(pair) for pair in itertools.combinations(triple, 2))
        for p, (a,b) in itertools.product(stresses, fractions):
            value = stake*p*a // (10**8*b)
            result = str(value) if value <= MAX else "overflow"
            rows.append("P\t" + "\t".join(map(str,(*triple,p,a,b))) + "\t" + result)
    return rows

if __name__ == "__main__":
    DESTINATION.parent.mkdir(parents=True,exist_ok=True)
    payload="\n".join(vectors())+"\n"
    DESTINATION.write_text(payload)
    counts={kind:sum(line.startswith(kind+"\t") for line in payload.splitlines()) for kind in "MCB"}
    pairs="\n".join(pair_vectors())+"\n"
    PAIRS.write_text(pairs)
    counts["P"]=sum(line.startswith("P\t") for line in pairs.splitlines())
    print(counts)
