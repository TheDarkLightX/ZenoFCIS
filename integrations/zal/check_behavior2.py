#!/usr/bin/env python3
"""Challenge the zal/finite-fsm/2 checker with planted defects; authority: none.

Each defect is a textual mutation of a temporary copy of behavior2.py. It must
import, fail its named test, and leave the live source unchanged; the
unmutated copy must then pass the whole profile 2 suite.
"""
import argparse
import hashlib
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ZAL = Path(__file__).resolve().parent
TESTS = 'test_behavior2.Profile2Tests.'
# (label, original text, defect text, named test)
DEFECTS = [
    ('wraparound-instead-of-trap',
     '            if not low <= after <= high:\n                return ("trap", rule, update.variable, after)',
     '            if not low <= after <= high:\n                after = low + (after - low) % (high - low + 1)',
     'test_update_leaving_range_traps'),
    ('requirement-ignores-post-state',
     '    if op == "state_next":\n        return step.post_state == node.args[0]',
     '    if op == "state_next":\n        return step.state == node.args[0]',
     'test_requirement_reads_post_state'),
    ('requirement-ignores-post-variables',
     '    if op == "var_next":\n        return step.post_values[term.args[0]]',
     '    if op == "var_next":\n        return step.values[term.args[0]]',
     'test_requirement_reads_post_state'),
    ('weak-next-at-the-end',
     '        return position + 1 < size and evaluate_finite(node.args[0], word, position + 1)',
     '        return position + 1 >= size or evaluate_finite(node.args[0], word, position + 1)',
     'test_finite_trace_strong_next_and_inclusive_within'),
    ('eventually-accepted-without-fairness',
     '        if property_shape(formula)["kind"] == "liveness" and not model.fairness:',
     '        if property_shape(formula)["kind"] == "liveness" and not model.fairness and False:',
     'test_liveness_needs_fairness'),
    ('lasso-missing-its-cycle',
     '    cycle += _shortest_path(graph, [current], {entry}, inside)\n    return cycle, positions',
     '    cycle += _shortest_path(graph, [current], {entry}, inside)\n    return [], positions',
     'test_liveness_counterexample_is_a_fair_lasso'),
    ('within-k-exclusive-bound',
     '"bound": expr.args[0] if expr.op == "within" else None',
     '"bound": expr.args[0] - 1 if expr.op == "within" else None',
     'test_within_is_inclusive_and_exact'),
    ('within-k-finite-off-by-one',
     '        last = min(position + node.args[0], size - 1)',
     '        last = min(position + node.args[0] - 1, size - 1)',
     'test_finite_trace_strong_next_and_inclusive_within'),
    ('implication-rendered-as-and',
     'BINARY_TEMPLATES = {"implies": ("({a} => {b})", "(if {a}, then {b})"),',
     'BINARY_TEMPLATES = {"implies": ("({a} => {b})", "({a} and {b})"),',
     'test_connective_renderings_and_truth'),
]


def run(work, target, log):
    result = subprocess.run([sys.executable, '-B', '-m', 'unittest', target], cwd=work, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=600)
    log.write_text(result.stdout)
    return result


def planted(logs):
    logs.mkdir(parents=True, exist_ok=True)
    live = ZAL / 'behavior2.py'
    before = hashlib.sha256(live.read_bytes()).hexdigest()
    with tempfile.TemporaryDirectory(prefix='zal2-defects-') as directory:
        work = Path(directory) / 'zal'
        shutil.copytree(ZAL, work, ignore=shutil.ignore_patterns('__pycache__', 'node_modules'))
        original = (work / 'behavior2.py').read_text()
        for label, anchor, defect, test in DEFECTS:
            if original.count(anchor) != 1:
                raise RuntimeError('mutation anchor drift: ' + label)
            (work / 'behavior2.py').write_text(original.replace(anchor, defect))
            built = subprocess.run([sys.executable, '-B', '-c', 'import behavior2'], cwd=work, capture_output=True, text=True)
            if built.returncode:
                raise RuntimeError('planted defect did not import: ' + label + '\n' + built.stderr[-2000:])
            result = run(work, TESTS + test, logs / (label + '.log'))
            if result.returncode == 0 or ('FAIL: ' + test) not in result.stdout:
                raise RuntimeError('planted defect survived or failed outside its named check: ' + label)
            print('KILLED ' + label + ' by ' + test, flush=True)
        (work / 'behavior2.py').write_text(original)
        result = run(work, 'test_behavior2', logs / 'baseline.log')
        if result.returncode:
            raise RuntimeError('baseline profile 2 suite failed after restoring the copy')
    if hashlib.sha256(live.read_bytes()).hexdigest() != before:
        raise RuntimeError('live behavior2.py changed during the planted run')
    print(f'PASS: {len(DEFECTS)} named planted controls; baseline passes; live source unchanged')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=('planted',))
    parser.add_argument('--logs', type=Path)
    args = parser.parse_args()
    if args.logs is not None:
        planted(args.logs)
    else:
        with tempfile.TemporaryDirectory(prefix='zal2-defect-logs-') as logs:
            planted(Path(logs))


if __name__ == '__main__':
    main()
