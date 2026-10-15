"""Run G10 native mutation controls in one isolated, source-bound checkout."""
from pathlib import Path
import json
import re
import shutil
import subprocess
import tempfile

from check_verus import digest
from checker_controls import NATIVE_TARGETS

EXPECTED_PANICS = {
    'P3_reverse_receivers': ('Coverage',),
    'P5_ignore_worker_failure': ('slot 0',),
}


def native_result(run, name, *, failure, expected_panics=()):
    """A compile error, timeout, zero-test filter or unrelated panic is not a kill."""
    state = 'FAILED' if failure else 'ok'
    if not re.search(r'^test ' + re.escape(name) + r' \.\.\. ' + state + r'$', run.stdout, re.M):
        return False
    summary = ('test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured;'
               if failure else 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured;')
    if summary not in run.stdout or run.returncode != (101 if failure else 0):
        return False
    if re.search(r'^error(?:\[|: could not compile)|resource limit|out of memory|SIGKILL|signal: 9',
                 run.stderr, re.M | re.I):
        return False
    if failure:
        panic = re.search(r"^thread '" + re.escape(name) + r"'(?: \([0-9]+\))? panicked at ",
                          run.stdout, re.M)
        if panic is None:
            return False
        detail = run.stdout[panic.end():].split('\n', 1)[-1]
        return (re.search(r'assertion[^\n]*failed', detail) is not None
                or any(detail.startswith(message + '\n') for message in expected_panics))
    return True


def source_files(root):
    # Copy all tracked sources/fixtures, plus interrupted G10's new source files.
    names = subprocess.check_output(['git', 'ls-files', '-z', '--cached', '--others',
                                     '--exclude-standard'], cwd=root).decode().split('\0')
    return tuple(sorted({Path(name) for name in names if name and
                         (root / name).is_file()}))


def hashes(root, paths):
    result = {}
    for path in paths:
        full = root / path
        if full.is_symlink() or not full.is_file():
            raise ValueError(f'missing or symlink native control input: {path}')
        result[str(path)] = digest(full)
    return result


def run_controls(root, out, environment, controls, run_logged):
    out.mkdir(parents=True, exist_ok=False)
    specimen = Path(tempfile.mkdtemp(prefix='zfc-g10-controls-', dir='/tmp'))
    (out / 'specimen.json').write_text(json.dumps({'path': str(specimen)}) + '\n')
    paths = source_files(root)
    original = hashes(root, paths)
    for path in paths:
        destination = specimen / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(root / path, destination)
    if hashes(specimen, paths) != original:
        raise ValueError('native control copy differs from source')
    (out / 'source.json').write_text(json.dumps(original, indent=2) + '\n')
    results, baselines = [], set()
    for name, (selector, target, test) in NATIVE_TARGETS.items():
        path, changed, kind = controls[name]
        if kind != 'native':
            raise ValueError(f'wrong native control kind: {name}')
        command = ['cargo', '+1.97.1', 'test', '--locked', '--offline', '-p',
                   'zeno-fcis-cli', selector, target, test, '--', '--exact']
        key = (selector, target, test)
        if key not in baselines:
            positive, _ = run_logged(command, specimen, environment, out / (name + '-positive'))
            if not native_result(positive, test, failure=False):
                raise ValueError(f'native control {name} baseline did not pass exactly one test')
            baselines.add(key)
        if hashes(specimen, paths) != original:
            raise ValueError(f'native baseline {name} source changed')
        (specimen / path).write_text(changed)
        before = hashes(specimen, paths)
        if [p for p in original if before[p] != original[p]] != [str(path)]:
            raise ValueError(f'native control {name} changed more than its owned source')
        directory = out / name
        directory.mkdir()
        (directory / 'source-before.json').write_text(json.dumps(before, indent=2) + '\n')
        run, _ = run_logged(command, specimen, environment, directory / 'run')
        after = hashes(specimen, paths)
        (directory / 'source-after.json').write_text(json.dumps(after, indent=2) + '\n')
        result = {'name': name, 'expected': 'native',
                  'killed': native_result(run, test, failure=True,
                      expected_panics=EXPECTED_PANICS.get(name, ())),
                  'source_stable': before == after,
                  'attribution': {'file': str(path), 'test': test, 'exit_code': run.returncode}}
        (directory / 'classification.json').write_text(json.dumps(result, indent=2) + '\n')
        results.append(result)
        shutil.copyfile(root / path, specimen / path)
        if not result['killed'] or not result['source_stable']:
            raise ValueError(f'native control {name} survived or failed for another reason')
    if hashes(root, paths) != original or hashes(specimen, paths) != original:
        raise ValueError('native control source changed during qualification')
    return results
