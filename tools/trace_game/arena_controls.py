#!/usr/bin/env python3
"""Compile solver/checker mutations and require named assertion failures, never errors."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import unittest
from unittest.mock import patch
import arena_check
import omega
import test_knowledge
import test_omega


def exercise(case):
    log=io.StringIO()
    result=unittest.TextTestRunner(stream=log,verbosity=2).run(unittest.TestSuite([case]))
    return dict(passed=result.wasSuccessful(),failures=len(result.failures),errors=len(result.errors),log=log.getvalue())


def witness(label):
    if label=='hidden-state':
        arena=test_knowledge.game([0,0,1,1],[[('a',2),('b',3)],[('a',3),('b',2)],
            [('stay',2)],[('stay',3)]],[3],['hidden','hidden','safe','bad'],[0,1])
        fake=arena.wire()
        for i,node in enumerate(fake['nodes']): node['observation']=str(i)
        packet=test_knowledge.solve(test_knowledge.parse(fake)); packet['game']=arena.wire()
    elif label=='parity-off-by-one':
        arena=test_omega.objective(test_knowledge.game([0],[[('stay',0)]],partial=False),'parity',[0])
        packet=test_omega.solve(test_omega.objective(arena,'parity',[1])); packet['game']=arena.wire()
    else:
        arena=test_knowledge.game([1,1],[[('stay',0)],[('stay',1)]],[1],roots=[0,1],partial=False)
        packet=test_knowledge.solve(arena); packet['certificate']['ranks']=[]
    return arena,packet


def verdict(arena,packet):
    try: return arena_check.check(packet,arena)
    except ValueError as error: return dict(evidence='refused',reason=str(error))


def deadline_control(out):
    path=Path(omega.__file__); original=path.read_text()
    old='budget.charge(0)'
    if original.count(old)!=2: raise ValueError('deadline checkpoint mutation anchors')
    mutated=original.replace(old,'pass'); file=out/'zero-time.py'; file.write_text(mutated)
    code=compile(mutated,str(file),'exec')
    arena=test_omega.objective(test_knowledge.game([0],[[('stay',0)]],roots=[0],partial=False),'reachability',[0])
    (out/'zero-time-witness.json').write_text(json.dumps(arena.wire(),indent=2)+'\n')
    name='test_zero_seconds_all_target_reachability'
    normal=exercise(test_omega.Omega(name))
    normal['verdict']=omega.solve(arena,test_omega.Budget(seconds=0))
    try:
        exec(code,omega.__dict__)
        with patch.object(test_omega,'solve',omega.solve):
            mutant=exercise(test_omega.Omega(name))
            mutant['verdict']=verdict(arena,omega.solve(arena,test_omega.Budget(seconds=0)))
    finally: exec(compile(original,str(path),'exec'),omega.__dict__)
    return dict(control='zero-time',test='Omega.'+name,build='compiled',
                source_sha256=hashlib.sha256(mutated.encode()).hexdigest(),normal=normal,mutated=mutant)


def main():
    parser=argparse.ArgumentParser(description=__doc__); parser.add_argument('--out',type=Path,required=True)
    args=parser.parse_args(); args.out.mkdir(parents=True,exist_ok=False)
    path=Path(arena_check.__file__); original=path.read_text(); records=[]
    controls=(
        ('hidden-state',test_knowledge.Knowledge,'test_hidden_state_policy_rejected',
         'parts.setdefault(arena.observations[s], set()).add(s)',
         'parts.setdefault(str(s), set()).add(s)'),
        ('parity-off-by-one',test_omega.Omega,'test_parity_priority_off_by_one_rejected',
         'if d % 2 == player: continue','if (d + 1) % 2 == player: continue'),
        ('missing-state',test_knowledge.Knowledge,'test_missing_certificate_state_rejected',
         "ranks = mapping(cert['ranks'], L, 'certificate state coverage')",
         "ranks = mapping(cert['ranks'] + [[i, 0] for i in L if i not in {row[0] for row in cert['ranks']}], L, 'certificate state coverage')"))
    for label,cls,name,old,new in controls:
        normal=exercise(cls(name)); arena,packet=witness(label)
        normal['verdict']=verdict(arena,packet)
        (args.out/(label+'-witness.json')).write_text(json.dumps(packet,indent=2)+'\n')
        if original.count(old)!=1: raise ValueError('unique mutation anchor: '+label)
        mutated=original.replace(old,new); file=args.out/(label+'.py'); file.write_text(mutated)
        code=compile(mutated,str(file),'exec')
        try:
            exec(code,arena_check.__dict__)
            mutant=exercise(cls(name)); mutant['verdict']=verdict(arena,packet)
        finally: exec(compile(original,str(path),'exec'),arena_check.__dict__)
        records.append(dict(control=label,test=cls.__name__+'.'+name,build='compiled',
            source_sha256=hashlib.sha256(mutated.encode()).hexdigest(),normal=normal,mutated=mutant))
    records.append(deadline_control(args.out))
    (args.out/'controls.json').write_text(json.dumps(records,indent=2)+'\n')
    valid=all(r['normal']['passed'] and r['mutated']['failures']==1 and r['mutated']['errors']==0 for r in records)
    print(json.dumps(dict(controls=len(records),all_built_and_failed_named_assertions=valid)))
    return 0 if valid else 1


if __name__=='__main__': raise SystemExit(main())
