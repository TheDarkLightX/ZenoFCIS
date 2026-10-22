"""Exercise a real replay process, nondeterministic fields and a semantic mismatch."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


class ReplayRun(unittest.TestCase):
    def test_replay_reports_first_semantic_difference_and_fails_closed(self):
        script = Path(__file__).with_name("replay_run.py")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            recorded = root / "recorded"
            recorded.mkdir()
            (recorded / "run.json").write_text(json.dumps({"elapsed_ms":1,"output":str(recorded / "result"),"decision":[1,2]}))
            program = ("import json,pathlib,sys; p=pathlib.Path(sys.argv[1]); p.mkdir(); "
                       "(p/'run.json').write_text(json.dumps({'elapsed_ms':99,'output':str(p/'result'),'decision':[1,int(sys.argv[2])]}))")
            def run(name, number, ignore=True):
                args = [sys.executable, str(script), str(recorded), "--out", str(root/name), "--file", "run.json"]
                if ignore: args += ["--ignore", "run.json:/elapsed_ms"]
                result = subprocess.run([*args,"--",sys.executable,"-c",program,"{out}",str(number)],capture_output=True,text=True)
                return result.returncode, json.loads(result.stdout)
            code, report = run("same",2)
            self.assertEqual((code,report["status"]),(0,"matched"),report)
            code, report = run("different",3)
            self.assertEqual((code,report["status"]),(1,"differs"),report)
            self.assertEqual(report["first_difference"],{"file":"run.json","pointer":"/decision/1","recorded":2,"replayed":3})
            print("replay witness: " + json.dumps(report, sort_keys=True))
            code, report = run("time-not-ignored",2,False)
            self.assertEqual(report["first_difference"]["pointer"],"/elapsed_ms")
            self.assertEqual(code,1)
            code, report = run("same",2)
            self.assertEqual((code,report["status"]),(2,"refused"),report)
            # Declared nondeterminism cannot hide a missing field or type change.
            import replay_run
            for replayed in [{}, {"elapsed_ms":"changed-kind"}]:
                self.assertIsNotNone(replay_run.first_difference({"elapsed_ms":1},replayed,{"/elapsed_ms"},("/a","/b")))


if __name__ == "__main__":
    unittest.main()
