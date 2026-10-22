#!/usr/bin/env python3
"""Light D7 controls. No compiler, verifier, ATDD or native process is launched."""
import argparse,json
from pathlib import Path
from signed_arithmetic_reference import cases,result,vectors
from signed_arithmetic_controls import grade,specimens,WITNESS,magnitude_model

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--disable-comparison',action='store_true');parser.add_argument('--out',type=Path)
    args=parser.parse_args();root=Path(__file__).resolve().parents[1]
    rows=cases();report=grade(rows,result,comparison=not args.disable_comparison)
    report.update(rows=len(rows),witness=list(WITNESS),expected=result(*WITNESS),
                  mutant=magnitude_model(*WITNESS,fault=True),comparison_enabled=not args.disable_comparison,
                  source_specimens=list(specimens(root)),native='not-run',verus='not-run')
    report['fixture_matches']=vectors()==(root/'tools/test-data/signed-arithmetic/vectors.tsv').read_text()
    report['passed'] &= report['fixture_matches']
    raw=json.dumps(report,indent=2)+'\n'
    if args.out:args.out.write_text(raw)
    print(raw,end='');return 0 if report['passed'] else 1
if __name__=='__main__':raise SystemExit(main())
