"""Exact production leaf policies and real isolated worker regressions."""
import ast
import json
from pathlib import Path
import resource
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[4]
WORKER = ROOT / "crates/zeno-fcis-cli/src/capture_worker.py"
TARGETS = {resource.RLIMIT_CPU: 2, resource.RLIMIT_AS: 48 << 20,
           resource.RLIMIT_FSIZE: 65536, resource.RLIMIT_CORE: 0,
           resource.RLIMIT_NOFILE: 32}


def policy_nodes(path):
    tree = ast.parse(path.read_text())
    if path == WORKER:
        return [next(n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name == "tighten_limit")]
    if path.name == "runner.py":
        child = next(ast.literal_eval(n.value) for n in tree.body if isinstance(n, ast.Assign)
                     and any(isinstance(t, ast.Name) and t.id == "CHILD" for t in n.targets))
        return [next(n for n in ast.parse(child).body if isinstance(n, ast.For))]
    main = next(n for n in tree.body if isinstance(n, ast.If) and isinstance(n.test, ast.Compare))
    return [next(n for n in main.body[0].body if isinstance(n, ast.For))]


class FakeResource:
    def __init__(self, inherited, noop=False):
        self.values = {k: inherited for k in TARGETS}
        self.noop = noop

    def __getattr__(self, name):
        return getattr(resource, name)

    def getrlimit(self, key):
        return self.values[key]

    def setrlimit(self, key, value):
        if not self.noop:
            self.values[key] = value


class Limits(unittest.TestCase):
    def test_every_leaf_policy_clamps_both_sides_and_checks_readback(self):
        paths = [WORKER, *[ROOT / base / name for base in
                 ("integrations/tau", "crates/zeno-fcis-cli/tau")
                 for name in ("runner.py", "importer.py")]]
        for path in paths:
            code = compile(ast.fix_missing_locations(ast.Module(body=policy_nodes(path), type_ignores=[])), str(path), "exec")
            for inherited in [(1, resource.RLIM_INFINITY), (1, 1), (1000000000, 1000000000)]:
                fake = FakeResource(inherited)
                env = {"resource": fake, "memory": 48 << 20, "cpu": 2, "output": 65536}
                exec(code, env)
                if path == WORKER:
                    for key, target in TARGETS.items():
                        env["tighten_limit"](key, target)
                for key, pair in fake.values.items():
                    target = TARGETS[key]
                    if path != WORKER:
                        if key == resource.RLIMIT_NOFILE or (path.name == "importer.py" and key == resource.RLIMIT_FSIZE):
                            continue
                        if path.name == "importer.py":
                            target = {resource.RLIMIT_AS: 256 << 20, resource.RLIMIT_CPU: 10, resource.RLIMIT_CORE: 0}[key]
                    expected = tuple(target if v == resource.RLIM_INFINITY else min(v, target) for v in inherited)
                    self.assertEqual(pair, expected, (path, key, inherited))
            fake = FakeResource((1000000000, 1000000000), noop=True)
            env = {"resource": fake, "memory": 48 << 20, "cpu": 2, "output": 65536}
            with self.assertRaises(RuntimeError, msg=str(path)):
                exec(code, env)
                if path == WORKER:
                    env["tighten_limit"](resource.RLIMIT_FSIZE, 65536)

    def worker(self, key, soft, hard, source):
        with tempfile.TemporaryDirectory() as directory:
            scratch = Path(directory) / "scratch.txt"
            scratch.touch()
            wrapper = ("import os,resource; resource.setrlimit(" + str(key) + ",(" + str(soft) + "," + str(hard) + ")); "
                       "os.execv('/usr/bin/python3',['python3','-I','-S','-B','-c'," + repr(WORKER.read_text()) + "])")
            result = subprocess.run(["/usr/bin/python3", "-I", "-S", "-B", "-c", wrapper],
                                    input=json.dumps({"source": source, "points": [[0]]}),
                                    text=True, capture_output=True, cwd=directory,
                                    env={"LC_ALL": "C"}, timeout=6)
            return result, scratch.stat().st_size

    def test_inherited_low_soft_or_hard_limits_run_without_exit_78(self):
        for key, value in [(resource.RLIMIT_FSIZE, 1024), (resource.RLIMIT_CPU, 1),
                           (resource.RLIMIT_AS, 40 << 20), (resource.RLIMIT_NOFILE, 16)]:
            for hard in [resource.RLIM_INFINITY, value]:
                # Linux caps NOFILE at nr_open; an unprivileged process cannot
                # install infinity there. Exercise the real inherited ceiling.
                if key == resource.RLIMIT_NOFILE and hard == resource.RLIM_INFINITY:
                    hard = resource.getrlimit(key)[1]
                with self.subTest(key=key, soft=value, hard=hard):
                    result, size = self.worker(key, value, hard, "def reference(p): return 7\n")
                    self.assertEqual(result.returncode, 0, (result.returncode, result.stdout, result.stderr))
                    self.assertEqual(json.loads(result.stdout), {"rows": [7]})
                    self.assertEqual(result.stderr, "zeno-capture-contained-v1\n")
                    self.assertEqual(size, 0)

    def test_fsize_1024_infinity_never_becomes_65536(self):
        source = "def reference(p):\n with open('scratch.txt','wb') as f: f.write(b'x'*4096)\n return 4096\n"
        result, size = self.worker(resource.RLIMIT_FSIZE, 1024, resource.RLIM_INFINITY, source)
        self.assertNotEqual(result.returncode, 0)
        self.assertNotEqual(result.returncode, 78)
        self.assertLessEqual(size, 1024)


if __name__ == "__main__":
    unittest.main()
