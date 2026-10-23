#!/usr/bin/env python3
"""Check documented commands, CLI refusals and independently replay witnesses."""
import argparse
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import tempfile
import unittest
from two_run_reference import Contract

ROOT=Path(__file__).resolve().parents[1]
BIN=ROOT.parent/'target/debug/zeno-fcis'
EVIDENCE=None


class TwoRunCli(unittest.TestCase):
    def run_cli(self, args):
        return subprocess.run([str(BIN),'contract','two-run',*args],cwd=ROOT,capture_output=True,text=True)

    def test_documented_commands_and_witnesses(self):
        text=(ROOT/'docs/TWO_RUN_PROPERTIES.md').read_text()
        blocks=re.findall(r'<!-- two-run-example: exit=(\d+) -->\s*```sh\n([^`]+)```\s*```text\n([^`]+)```',text)
        self.assertEqual(len(blocks),5)
        for expected_exit,command,output in blocks:
            args=shlex.split(command.strip())[3:]
            filename=args[args.index('--out')+1]
            args[args.index('--out')+1]=str(EVIDENCE/filename)
            p=self.run_cli(args)
            self.assertEqual(p.returncode,int(expected_exit),p.stderr)
            self.assertEqual(p.stdout,output)
            self.assertEqual(p.stderr,'')
            packet=json.loads((EVIDENCE/filename).read_text())
            self.assertEqual(packet['authority'],'none')
            if p.returncode==1:
                for run in packet['result']['runs']:
                    self.assertTrue(all(path.startswith('post.') for path in run['observed']))
                difference=packet['result']['first_difference']
                if 'path' in difference:self.assertTrue(difference['path'].startswith('post.'))
            self.assertEqual(packet['scope'],'two runs from every admissible state over the declared finite domain; one step per side (noninterference) or two (order independence); longer interleavings are not claimed')
            if p.returncode==1:
                folder=ROOT/args[0]
                c=Contract((folder/'project.zeno').read_text(),json.loads((folder/'v2/policy.json').read_text()))
                self.replay(c,packet['result'],args,packet)

    def replay(self,c,witness,args,packet):
        def state(obj):return tuple(int(obj[n]) for n in c.names[:c.width])
        def inp(obj):return tuple(int(obj[n]) for n in c.names[c.width:])
        a=state(witness['state']);i,j=map(inp,witness['inputs'])
        self.assertTrue(c.valid(a))
        if '--commute' in args:
            for n, sequence in enumerate(((i,j),(j,i))):
                current=a
                for step,command in enumerate(sequence):
                    outcome=c.outcome(current,command)
                    actual=witness['runs'][n]['decisions'][step]
                    self.assertEqual(outcome['class'],actual['class'])
                    if outcome['class']!='Refused': self.assertEqual(outcome,actual)
                    current=c.after(current,outcome)
                self.assertEqual(tuple(map(int,witness['runs'][n]['final_state'])),current)
            first=args[args.index('--first')+1];second=args[args.index('--second')+1]
            from two_run_reference import Expression
            self.assertEqual(Expression(first).evaluate(dict(zip(c.names[c.width:],i)),c.variables),1)
            self.assertEqual(Expression(second).evaluate(dict(zip(c.names[c.width:],j)),c.variables),1)
            left=witness['runs'][0];right=witness['runs'][1]
            obs=args[args.index('--observe')+1].split(',')
            k=int(args[args.index('--within')+1]) if '--within' in args else 0
            differs=any(left['decisions'][n]['class']!=right['decisions'][1-n]['class'] for n in (0,1)) or any(abs(int(left['observed'][p.replace('pre.','post.')])-int(right['observed'][p.replace('pre.','post.')]))>k for p in obs)
            self.assertTrue(differs)
        else:
            b=state(witness['other_state']);self.assertTrue(c.valid(b));self.assertEqual(i,j)
            secrets=set(args[args.index('--secret')+1].replace('post.','pre.').split(','))
            self.assertTrue(all(x==y for n,x,y in zip(c.names,a,b) if n not in secrets))
            for n,s in enumerate((a,b)):
                actual=witness['runs'][n]['decisions'][0]
                expected=c.outcome(s,i)
                self.assertEqual(expected['class'],actual['class'])
                if expected['class']!='Refused':self.assertEqual(expected,actual)
            self.assertNotEqual(witness['runs'][0]['observed'],witness['runs'][1]['observed'])

    def test_reference_regressions(self):
        for name,verdict,exit_code in [('refusal-reason-only','refuted',1),('no-admissible','UNKNOWN',2),('invalid-payload','checked',0)]:
            with self.subTest(witness=name):
                folder=ROOT/'test-data/two-run'/name
                c=Contract((folder/'project.zeno').read_text(),json.loads((folder/'v2/policy.json').read_text()))
                self.assertEqual(c.noninterference(['pre.100.111'],['post.100.110'])[0],verdict)
                p=self.run_cli([str(folder),'--noninterference','--secret','pre.100.111','--public','post.100.110'])
                self.assertEqual(p.returncode,exit_code,p.stderr)
                packet=json.loads(p.stdout);self.assertEqual(packet['status'],verdict)
                (EVIDENCE/(name+'.json')).write_text(p.stdout)
                if name=='refusal-reason-only':
                    self.assertEqual(packet['result']['first_difference']['kind'],'reason')
                    self.assertEqual([r['decisions'][0]['law'] for r in packet['result']['runs']],[502,501])

    def test_refuses_invalid_options_and_paths(self):
        ni=['examples/two-run/ledger-private','--noninterference','--secret','pre.100.112','--public','post.100.110']
        commute=['examples/two-run/uniform-price','--commute','--first','true','--second','true','--observe','post.100.110']
        for args,expected in [(ni+['--public','post.100.112'],65),(commute+['--within=-1'],64),(['examples/two-run/ledger-private'],64),(['examples/two-run/ledger-private','--commute'],64),
            ([*commute[:commute.index('--first')],'--first','pre.100.110 == 0',*commute[commute.index('--second'):]],65),
            (['examples/two-run/ledger-private','--noninterference','--secret','pre.100.112','--public','post.100.112'],65),
            (['examples/two-run/ledger-private','--noninterference','--secret','pre.100.999','--public','post.100.110'],65)]:
            with self.subTest(args=args):self.assertEqual(self.run_cli(args).returncode,expected)

    def test_capacity_refusal_survives_any_numeric_tolerance(self):
        args=['examples/two-run/constant-product','--commute','--first','command.101.120 == 0','--second','command.101.120 == 1','--observe','post.100.110,post.100.111,post.100.112,post.100.113','--within',str(2**128-1)]
        p=self.run_cli(args);self.assertEqual(p.returncode,1)
        packet=json.loads(p.stdout)
        self.assertEqual(packet['result']['first_difference']['kind'],'decision_class')
        (EVIDENCE/'swap-capacity.json').write_text(p.stdout)

    def test_documented_dependency_probes(self):
        for name,answer in [('constant-product','yes'),('uniform-price','yes'),('ledger-private','yes'),('ledger-leak','no')]:
            cmd=[str(BIN),'contract','probe','depends','examples/two-run/'+name,'--position','command.101.120','--no-kb']
            p=subprocess.run(cmd,cwd=ROOT,capture_output=True,text=True)
            self.assertEqual(p.returncode,0,p.stderr);packet=json.loads(p.stdout)
            self.assertEqual(packet['status'],'answered');self.assertEqual(packet['result']['answer'],answer)
            (EVIDENCE/('depends-'+name+'.json')).write_text(p.stdout)

    def test_public_subset_does_not_compose(self):
        folder=ROOT/'test-data/two-run/two-step-leak'
        c=Contract((folder/'project.zeno').read_text(),json.loads((folder/'v2/policy.json').read_text()))
        for public,verdict,exit_code in [('post.100.110','checked',0),('post.100.110,post.100.112','refuted',1)]:
            self.assertEqual(c.noninterference(['pre.100.111'],public.split(','))[0],verdict)
            p=self.run_cli([str(folder),'--noninterference','--secret','pre.100.111','--public',public])
            self.assertEqual(p.returncode,exit_code,p.stderr)
            (EVIDENCE/('two-step-ni-'+verdict+'.json')).write_text(p.stdout)
        histories=[]
        for secret in (0,1):
            state=(0,secret,0);history=[state]
            for inp in ((0,0),(1,0)):
                state=c.after(state,c.outcome(state,inp));history.append(state)
            histories.append(history)
        self.assertEqual([h[-1][0] for h in histories],[0,1])
        (EVIDENCE/'two-step-histories.json').write_text(json.dumps(histories)+'\n')
        p=subprocess.run([str(BIN),'contract','runs',str(folder),'--no-kb'],cwd=ROOT,capture_output=True,text=True)
        self.assertEqual(p.returncode,1,p.stderr)
        packet=json.loads(p.stdout)
        (EVIDENCE/'two-step-runs.json').write_text(p.stdout)
        self.assertEqual(packet['properties'][0]['verdict'],'false')
        self.assertEqual(packet['properties'][0]['trace_from_genesis'],[[0,0],[1,0]])

    def test_order_invocation_limit_is_unknown_before_execution(self):
        with tempfile.TemporaryDirectory(dir=BIN.parents[1]) as tmp:
            folder=Path(tmp);(folder/'v2').mkdir()
            original=ROOT/'examples/two-run/ledger-private'
            project=(original/'project.zeno').read_text().replace('field 120 101 amount 107;', 'type 108 int Input in 0..=1023;\nfield 120 101 amount 108;')
            policy=json.loads((original/'v2/policy.json').read_text())
            (folder/'project.zeno').write_text(project);(folder/'v2/policy.json').write_text(json.dumps(policy))
            c=Contract(project,policy)
            self.assertEqual(c.order('true','true',['post.100.110'],0),('UNKNOWN',0,0))
            p=self.run_cli([str(folder),'--commute','--first','true','--second','true','--observe','post.100.110'])
            self.assertEqual(p.returncode,2,p.stderr);packet=json.loads(p.stdout)
            self.assertEqual(packet['planned_order_invocations'],50331648)
            self.assertEqual(packet['max_order_invocations'],1048576)
            self.assertEqual(packet['measured']['authority_invocations'],0)
            self.assertEqual(packet['measured']['comparisons'],0)
            self.assertIn('exceeding limit',packet['result']['reason']);self.assertIsNone(packet['claim'])
            (EVIDENCE/'order-work-limit.json').write_text(p.stdout)

    def test_effective_cap_is_reported(self):
        args=['examples/two-run/ledger-private','--noninterference','--secret','pre.100.112','--public','post.100.110','--max-tuples','2000000']
        p=self.run_cli(args);self.assertEqual(p.returncode,0,p.stderr)
        packet=json.loads(p.stdout);self.assertEqual(packet['max_tuples'],1048576)
        (EVIDENCE/'effective-cap.json').write_text(p.stdout)
        with tempfile.TemporaryDirectory(dir=BIN.parents[1]) as tmp:
            folder=Path(tmp);(folder/'v2').mkdir()
            original=ROOT/'examples/two-run/ledger-private'
            (folder/'project.zeno').write_text((original/'project.zeno').read_text().replace('Balance in 0..=2','Balance in 0..=131072'))
            (folder/'v2/policy.json').write_text((original/'v2/policy.json').read_text())
            p=self.run_cli([str(folder),*args[1:]]);self.assertEqual(p.returncode,2,p.stderr)
            packet=json.loads(p.stdout);self.assertEqual(packet['max_tuples'],1048576)
            self.assertIn('tuple limit',packet['result']['reason']);self.assertIsNone(packet['claim'])
            (EVIDENCE/'above-hard-cap.json').write_text(p.stdout)

    def test_unknown_has_reason_and_no_claim(self):
        args=['examples/two-run/ledger-private','--noninterference','--secret','pre.100.112','--public','post.100.110','--max-tuples','1']
        p=self.run_cli(args);self.assertEqual(p.returncode,2)
        packet=json.loads(p.stdout);self.assertEqual(packet['status'],'UNKNOWN');self.assertIsNone(packet['claim']);self.assertIn('tuple limit',packet['result']['reason'])


if __name__=='__main__':
    ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--binary',type=Path,default=BIN);ap.add_argument('--evidence',type=Path)
    args=ap.parse_args();BIN=args.binary.resolve()
    if args.evidence:
        EVIDENCE=args.evidence.resolve();EVIDENCE.mkdir(parents=True,exist_ok=True)
        unittest.main(argv=[__file__],verbosity=2)
    else:
        target=Path(os.environ.get('CARGO_TARGET_DIR',ROOT.parent/'target'));target.mkdir(parents=True,exist_ok=True)
        with tempfile.TemporaryDirectory(prefix='two-run-docs-',dir=target) as tmp:
            EVIDENCE=Path(tmp);unittest.main(argv=[__file__],verbosity=2)
