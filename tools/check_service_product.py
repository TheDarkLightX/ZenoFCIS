#!/usr/bin/env python3
"""Complete admissible product and read-outcome checks against an independent Python oracle.

The Python policy/formula interpreter is independent of Rust's checked evaluator.
This tool is evidence only and grants no service or publication authority.
"""
import argparse
import hashlib
import itertools
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import composition_domain_reference as reference

CAP = 65536

def sha(data):
    return hashlib.sha256(data).hexdigest()

def repin(binary, root):
    result = subprocess.run([binary, 'pins', str(root)], capture_output=True, text=True, check=True)
    manifest = json.loads((root / 'composition.json').read_text())
    for entry, pins in zip(manifest['components'], json.loads(result.stdout), strict=True):
        entry.update(pins)
    (root / 'composition.json').write_text(json.dumps(manifest, indent=2) + '\n')

def admissible(comp):
    spaces = [[s for s in itertools.product(*(core.domains[n] for n in core.state_names))
               if core.invariant_holds(s)] for core in comp.owned]
    for states in itertools.product(*spaces):
        named = dict(zip([c.name for c in comp.owned], states, strict=True))
        env = comp.environment({}, comp.named_states(named))
        if all(reference.holds(formula, env) for _, _, formula in comp.couplings):
            yield named

def read_law(state, request, outcome, post):
    if request[0] in (5, 6) and outcome['kind'] == 'Accept':
        device = 0 if request[0] == 5 else 1
        slot = 6 + (request[1] - 1) * 2
        assert state[device] == 144, f'read_law: wrong device/route accepted: {state} {request}'
        assert state[slot] != 140 and state[slot+1] <= state[3+device], 'read_law: missing key'
        assert post[slot] == 142, 'read_law: read route did not run mailbox'

def compare(binary, root, cap=CAP, control=None):
    comp = reference.Composition(root / 'composition.json')
    states = list(admissible(comp))
    requests = list(comp.requests())
    planned = len(states) * len(requests)
    if planned > cap:
        raise ValueError(f'over-budget: {planned} > {cap}; no rows evaluated')
    rows = []
    expected = []
    if control:
        states = [{'keydir': (145,144,2,1,2,0), 'mailbox': (141,2,140,0)}]
        requests = [{'op':5, 'target':1}]
    for state in states:
        for request in requests:
            flat = [v for core in comp.owned for v in state[core.name]]
            req = [request[n] for n, _ in comp.request]
            rows.append({'state':flat, 'request':req})
            kind, post = comp.joint_step(state, request)
            observed = {'kind':kind[0]}
            if len(kind) > 1:
                observed['component'] = kind[1]
                observed['class' if kind[0] == 'Refused' else 'reason'] = kind[2]
            expected.append((observed, [v for core in comp.owned for v in post[core.name]]))
    data = ''.join(json.dumps(row, separators=(',',':'))+'\n' for row in rows)
    result = subprocess.run([binary, 'product', str(root)], input=data, capture_output=True, text=True)
    assert result.returncode == 0, f'joint build/run failed: {result.stderr}'
    actual = [json.loads(line) for line in result.stdout.splitlines()]
    assert len(actual) == len(rows), 'complete product was not evaluated'
    counts = {}
    for row, output, (kind, post) in zip(rows, actual, expected, strict=True):
        read_law(row['state'], row['request'], output['outcome'], output['successor'])
        assert (output['outcome'], output['successor']) == (kind,post), f'reference mismatch: {row} {output} {(kind,post)}'
        if kind['kind'] in ('Reject','Refused'):
            assert post == row['state'], 'refusal changed state'
        counts[kind['kind']] = counts.get(kind['kind'],0)+1
    return {'planned_rows':planned, 'compared_rows':len(rows), 'admissible_states':len(states), 'requests':len(requests),
            'outcomes':counts, 'domain':'all declared states satisfying component invariants and global couplings, all requests',
            'input_sha256':sha(data.encode()), 'output_sha256':sha(result.stdout.encode()), 'cap':cap}

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True)
    parser.add_argument('--fixture',type=Path,default=Path('examples/composition-service'))
    parser.add_argument('--repin',action='store_true')
    parser.add_argument('--control',choices=['wrong-device','wrong-route'])
    parser.add_argument('--scratch',type=Path)
    parser.add_argument('--cap',type=int,default=CAP)
    args = parser.parse_args()
    if args.repin:
        repin(args.binary,args.fixture)
        return
    if args.control:
        if args.scratch is None:
            parser.error('--control requires --scratch')
        args.scratch.mkdir(parents=True,exist_ok=True)
        with tempfile.TemporaryDirectory(dir=args.scratch,prefix=args.control+'-') as temporary:
            root = Path(temporary)
            shutil.copytree(args.fixture,root,dirs_exist_ok=True)
            manifest = json.loads((root/'composition.json').read_text())
            mailbox = manifest['components'][1]
            if args.control == 'wrong-device':
                for name, value in mailbox['bind'].items():
                    if name in ('reader_status','reader_generation'):
                        mailbox['bind'][name] = value.replace('op == 6','op == 5')
            else:
                mailbox['participates'] = 'op >= 4 && op != 5'
            (root/'composition.json').write_text(json.dumps(manifest))
            compare(args.binary,root,control=args.control)
    else:
        print(json.dumps(compare(args.binary,args.fixture,args.cap),indent=2))

if __name__ == '__main__':
    main()
