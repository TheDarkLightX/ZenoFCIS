#!/usr/bin/env python3
"""Cheap admission controls only; native qualification has a separate runner."""
from __future__ import annotations
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import check_runtime_refinement as check
import make_runtime_refinement as producer



def write_candidate(root, files=None):
    files = producer.candidate_files() if files is None else files
    root.mkdir()
    for name,data in files.items():
        path=root/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(data)


def repin(files):
    mapping=json.loads(files['abstraction.json'])
    mapping['concrete_project_sha256']=check.sha(files['concrete/project.zeno'])
    mapping['concrete_policy_sha256']=check.sha(files['concrete/v2/policy.json'])
    files['abstraction.json']=producer.dump(mapping).encode()
    manifest=json.loads(files['candidate.json'])
    manifest['files']={name:check.sha(files[name]) for name in check.CANDIDATE_FILES}
    files['candidate.json']=producer.dump(manifest).encode()


class Admission(unittest.TestCase):
    def test_producer_is_not_checker_dependency(self):
        self.assertNotIn('import make_runtime_refinement', Path(check.__file__).read_text())

    def test_valid_candidate_and_exact_domain(self):
        with tempfile.TemporaryDirectory() as d:
            path=Path(d)/'candidate';write_candidate(path)
            _,content=check.candidate(path)
            mapping=json.loads(content['abstraction.json'])['abstraction']
            self.assertEqual(sorted(mapping),list(range(96)))
            self.assertEqual(sum(producer.valid(producer.decode(s)) for s in range(96)),40)

    def test_unknown_duplicate_malformed_and_nonfinite(self):
        for data in (b'{"a":1,"a":2}',b'{',b'{"a":NaN}'):
            with self.subTest(data=data), self.assertRaises(check.Refused): check.decode(data)
        with self.assertRaises(check.Refused):check.exact_keys({'known':1,'extra':2},['known'])

    def test_missing_extra_hash_profile_claim_and_symlink(self):
        for fault in ('missing','extra','hash','profile','claim','symlink'):
            with self.subTest(fault=fault),tempfile.TemporaryDirectory() as d:
                path=Path(d)/'candidate';write_candidate(path)
                if fault=='missing':(path/'abstraction.json').unlink()
                elif fault=='extra':(path/'forged-receipt.json').write_text('{"status":"CHECKED"}')
                elif fault=='hash':(path/'concrete/project.zeno').write_text('changed')
                elif fault=='symlink':
                    source=path/'concrete/project.zeno'; saved=Path(d)/'source';source.rename(saved);source.symlink_to(saved)
                else:
                    doc=json.loads((path/'candidate.json').read_text());doc[fault]='EXECUTABLE_ASSEMBLY_REFINEMENT';(path/'candidate.json').write_text(json.dumps(doc))
                with self.assertRaises((check.Refused,OSError)):check.candidate(path)

    def test_executable_claim_cannot_be_upgraded_by_any_binding(self):
        with tempfile.TemporaryDirectory() as d:
            path=Path(d)
            with self.assertRaisesRegex(check.Refused,'unsupported-executable'):
                check.check(path,path,path,path/'out',claim='EXECUTABLE_ASSEMBLY_REFINEMENT')
            self.assertFalse((path/'out').exists())

    def test_checks_survive_python_optimization(self):
        run=subprocess.run(['python3','-O','-c',"import sys;sys.path.insert(0,'tools');from check_runtime_refinement import require;require(False,'control')"],cwd=check.ROOT,capture_output=True)
        self.assertNotEqual(run.returncode,0)

    def test_missing_binary_and_output_inside_inputs_refuse(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d)
            doc={'format':'zeno-fcis/assembled-app/1','authority':'none','assembly_sha256':'0'*64,
                 'files':{'assembly.json':'0'*64},'limits':[]}
            (root/'package.json').write_text(json.dumps(doc))
            with self.assertRaisesRegex(check.Refused,'package-files'):check.package_binding(root)
            with self.assertRaisesRegex(check.Refused,'output-inside-input'):
                check.check(root,root,root,root/'out')

    def test_timeout_never_returns_evidence(self):
        with tempfile.TemporaryDirectory() as d,patch.object(check.subprocess,'run',side_effect=subprocess.TimeoutExpired('verifier',1)):
            with self.assertRaisesRegex(check.Refused,'timeout'):check.run(Path('/trusted/cli'),[],Path(d)/'report',timeout=1)


if __name__ == '__main__':
    unittest.main()
