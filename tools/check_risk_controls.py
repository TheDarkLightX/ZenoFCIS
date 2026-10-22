#!/usr/bin/env python3
"""Build isolated quantitative-risk mutants; require named assertion failures.

Scratch is outside the source checkout. Each mutant restores pristine source
before the next one. Logs and source patches are retained; scratch is removed.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
FILE = 'crates/zeno-fcis-contract/src/risk.rs'
CARGO = ['nice', '-n', '19', 'ionice', '-c3', 'cargo', '+1.97.1', 'test',
         '--locked', '--offline', '-p', 'zeno-fcis-contract', '--test', 'risk']

REACHABLE = '''        let state_width = self.positions.iter().take_while(|p|p.source==Source::State).count();
        let genesis:Vec<i128> = self.positions[..state_width].iter().map(|p| {
            match p.field.and_then(|id|rules.genesis.get(&id)) {
                Some(Constant::Int(v))=>*v, Some(Constant::Bool(v))=>i128::from(*v), _=>0,
            }
        }).collect();
        let mut reachable = std::collections::BTreeSet::from([genesis.clone()]);
        let mut queue = std::collections::VecDeque::from([genesis]);
        while let Some(state) = queue.pop_front() {
            for ordinal in 0..cardinality(&self.lists[state_width..]) {
                let mut tuple=state.clone(); tuple.extend(element(&self.lists[state_width..],ordinal));
                let decision=self.decision(&tuple)?;
                let post=self.successor(&tuple,&decision)?;
                if reachable.insert(post.clone()) {queue.push_back(post);}
            }
        }
'''

def replace_once(text, before, after):
    assert text.count(before) == 1, ('mutation anchor', before)
    return text.replace(before, after)

def mutate(text, name):
    if name == 'reachable-for-inputs':
        text = replace_once(text, 'engine.inputs()', 'engine.inputs(&rules)')
        text = replace_once(text, 'fn inputs(&mut self) -> Answer {',
                            'fn inputs(&mut self, rules:&Rules) -> Answer {\n' + REACHABLE)
        return replace_once(text, 'let tuple = element(self.lists, ordinal);',
                            'let tuple = element(self.lists, ordinal);\n'
                            '            if !reachable.contains(&tuple[..state_width]) { continue; }')
    if name == 'max-uses-min':
        text = replace_once(text, 'candidates[1].cost > worst.cost', 'candidates[1].cost < worst.cost')
        return replace_once(text, 'candidates[1].cost > kept[1].cost', 'candidates[1].cost < kept[1].cost')
    return replace_once(text, 'for _depth in 0..steps {', 'for _depth in 0..steps.saturating_sub(1) {')

CONTROLS = [
    ('reachable-for-inputs', 'counts_include_unreachable_declared_inputs',
     'count=4,total=120', 'count=0', 'pre=(4,2,1), command=0, context=0; unreachable but declared'),
    ('max-uses-min', 'max_plus_uses_maximum_fee',
     'worst=6', 'worst=0', 'three amount-1 swaps have fee 2+2+2=6'),
    ('k-off-by-one', 'horizon_includes_length_k',
     'K=3 worst=6', 'K=3 worst=4', 'three amount-1 swaps; length 3 is omitted'),
]

def run(command, cwd, path, env):
    with path.open('w') as log:
        result = subprocess.run(command, cwd=cwd, env=env, stdout=log,
                                stderr=subprocess.STDOUT, timeout=600, check=False)
    return result.returncode

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--scratch', type=Path, required=True)
    args = p.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    args.scratch.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, CARGO_PROFILE_DEV_DEBUG='0', CARGO_PROFILE_TEST_DEBUG='0',
               CARGO_BUILD_JOBS='4', CARGO_INCREMENTAL='0')
    original = (ROOT / FILE).read_text()
    records = []
    with tempfile.TemporaryDirectory(prefix='risk-controls-', dir=args.scratch) as temporary:
        scratch = Path(temporary)
        source = scratch / 'source'
        shutil.copytree(ROOT, source, ignore=shutil.ignore_patterns('.git', 'target', '__pycache__', '.zeno', 'node_modules'))
        # Reuse this bounded build directory across controls. It is removed by
        # TemporaryDirectory after logs and mutation patches are retained.
        env['CARGO_TARGET_DIR'] = str(scratch / 'target')
        for name, test, normal, mutated, witness in CONTROLS:
            target = source / FILE
            target.write_text(original)
            normal_exit = run(CARGO + [test, '--', '--exact'], source,
                              args.out / (name + '-normal.log'), env)
            assert normal_exit == 0, (name, 'normal test must pass')
            changed = mutate(original, name)
            target.write_text(changed)
            import difflib
            (args.out / (name + '.patch')).write_text(''.join(difflib.unified_diff(
                original.splitlines(True), changed.splitlines(True), fromfile='a/'+FILE, tofile='b/'+FILE)))
            build = run(CARGO + ['--no-run'], source, args.out / (name + '-build.log'), env)
            assert build == 0, (name, 'mutant must build')
            failure = run(CARGO + [test, '--', '--exact'], source,
                          args.out / (name + '-test.log'), env)
            output = (args.out / (name + '-test.log')).read_text()
            assert failure == 101 and f'test {test} ... FAILED' in output
            assert 'assertion `left == right` failed' in output
            assert 'could not compile' not in output and 'signal:' not in output
            records.append(dict(control=name, test=test, normal_exit=normal_exit,
                                build_exit=build, mutated_exit=failure, normal_result=normal,
                                mutated_result=mutated, witness=witness,
                                source_sha256=hashlib.sha256(changed.encode()).hexdigest()))
            (args.out / 'results.json').write_text(json.dumps(records, indent=2)+'\n')
            print(json.dumps(records[-1]), flush=True)
        bytes_removed = sum(f.stat().st_size for f in scratch.rglob('*') if f.is_file())
    assert (ROOT / FILE).read_text() == original, 'controls changed the original source'
    print(json.dumps({'status':'PASS', 'controls':len(records), 'scratch_bytes_removed':bytes_removed}))

if __name__ == '__main__':
    main()
