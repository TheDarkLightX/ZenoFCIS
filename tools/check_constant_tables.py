#!/usr/bin/env python3
"""Compare constant-hierarchy and stored-hierarchy Authority decisions exactly."""
from __future__ import annotations
import argparse
import itertools
import json
from pathlib import Path
import re
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--driver',type=Path,required=True)
    parser.add_argument('--cli',type=Path,required=True)
    parser.add_argument('--out',type=Path,required=True)
    args=parser.parse_args()
    base=Path(__file__).resolve().parents[1]/'examples'/'contract-tables'
    state_source=(base/'role-inheritance/project.zeno').read_text()
    state_rules=json.loads((base/'role-inheritance/v2/policy.json').read_text())
    template=(base/'constant-role-hierarchy/project.zeno').read_text()
    constant_rules=json.loads((base/'constant-role-hierarchy/v2/policy.json').read_text())
    parents=list(itertools.product(range(4),repeat=3))
    queries=[(grants,role) for grants in itertools.product(range(2),repeat=3) for role in range(3)]
    rows=[(*parent,*grants,role,130) for parent in parents for grants,role in queries]
    def run(folder,source,rules,rows):
        project,policy=folder/'project.zeno',folder/'rules.json'
        project.write_text(source);policy.write_text(json.dumps(rules))
        result=subprocess.run([str(args.driver),str(project),str(policy),'--decisions'],input=''.join(json.dumps(row)+'\n' for row in rows),capture_output=True,text=True)
        assert result.returncode==0,(result.returncode,result.stderr)
        values=[json.loads(line) for line in result.stdout.splitlines()]
        assert len(values)==len(rows)
        for value in values:value['post']=[field for field in value['post'] if field[0]>=210]
        return values
    with tempfile.TemporaryDirectory(prefix='constant-hierarchy-') as directory:
        folder=Path(directory)
        expected=run(folder,state_source,state_rules,rows)
        checked=0
        for ordinal,parent in enumerate(parents):
            source=template.replace('size 3 [0,1,2]','size 3 ['+','.join(map(str,parent))+']')
            project=folder/'project.zeno';project.write_text(source)
            result=subprocess.run([str(args.cli),'check',str(project),'--expand','--format','json'],capture_output=True,text=True)
            assert result.returncode==0,(result.returncode,result.stderr,result.stdout[:500])
            packet=json.loads(result.stdout);assert packet['canonical_match']
            expression=re.search(r'law 502 .*? = (.*?);\s*law 503',packet['expansion'],re.S).group(1)
            rules={**constant_rules,'variables':{'permitted':expression}}
            actual=run(folder,source,rules,[(*grants,role,130) for grants,role in queries])
            wanted=expected[ordinal*len(queries):(ordinal+1)*len(queries)]
            assert actual==wanted,(parent,actual,wanted)
            # Independent naive fixed point retains permission identities and handles cycles.
            for found,(grants,role) in zip(actual,queries):
                permission=[False]*3
                while True:
                    following=[bool(grants[i]) or (parent[i]>0 and permission[parent[i]-1]) for i in range(3)]
                    if following==permission:break
                    permission=following
                assert found['class']==('Accept' if permission[role] else 'Reject')
                assert found['reason']==(None if permission[role] else 200)
            checked+=len(actual)
    report={'parent_tables':len(parents),'grant_vectors':8,'commands':3,'decision_comparisons':checked,'projection':'class, reason, grant successor, empty outbox; fixed parent fields removed','stored_state_count':512,'constant_state_count':8,'per_policy_domain':24,'exit':0}
    args.out.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report))


if __name__=='__main__':main()
