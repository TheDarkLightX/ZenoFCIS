#!/usr/bin/env python3
"""Execute the shell-effects stages 1-3 and review controls in an isolated source copy.

Each named check passes on the unmodified tree first. Each mutant must then
build and fail its named check with the stated marker; a build failure, a
signal or an unrelated failure is not a kill. The purity control runs the
repository purity checker instead of a test. No network, dependency change
or edit to the original source tree is used.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

from cargo_source_scope import source_environment

ROOT = Path(__file__).resolve().parents[1]
REGISTRY = 'crates/zeno-fcis-shell/src/effects/registry.rs'
PROGRAM = 'crates/zeno-fcis-shell/src/effects/program.rs'
DRIVER = 'crates/zeno-fcis-shell/src/effects/driver.rs'
ADAPTER = 'crates/zeno-fcis-shell-sqlite/src/effects.rs'
SIMULATION = 'crates/zeno-fcis-shell/src/effects/simulation.rs'
RECORD = 'crates/zeno-fcis-shell/src/effects/record.rs'
HANDLER = 'crates/zeno-fcis-shell/src/effects/handler.rs'
SHELL =['-p', 'zeno-fcis-shell', '--lib']
REVIEW = ['-p', 'zeno-fcis-shell', '--test', 'review_regressions']
EVIDENCE = ['-p', 'zeno-fcis-shell', '--test', 'evidence_checks']
SQL_REVIEW = ['-p', 'zeno-fcis-shell-sqlite', '--features', 'crash-injection', '--test', 'review_regressions']
EXPLORER = 'crates/zeno-fcis-shell/src/effects/explore.rs'
PRODUCTION_TEST = 'crates/zeno-fcis-shell-sqlite/tests/effects_production.rs'
SQLITE = ['-p', 'zeno-fcis-shell-sqlite', '--test', 'effects']
PRODUCTION = ['-p', 'zeno-fcis-shell-sqlite', '--features', 'crash-injection', '--test', 'effects_production']
STORE = 'crates/zeno-fcis-shell-sqlite/src/effects.rs'
MOUNT = 'crates/zeno-fcis-shell-sqlite/src/effects/production.rs'


def signature_block(text: str, port: str) -> str:
    """The complete `Signature { .. },` entry of one port."""
    at = text.index(f'        port: Port::{port},\n')
    start = text.rindex('    Signature {\n', 0, at)
    end = text.index('\n    },\n', at) + len('\n    },\n')
    return text[start:end]


# (name, cargo target, named check, markers, behavioural witness, edits)
CONTROLS = [
    ('missing-port', SHELL, 'effects::tests::port_registry',
     ['left: Err(Missing(Acknowledge))'],
     'The registry loses the Acknowledge signature: the exhaustiveness check names the missing port.',
     [(REGISTRY, lambda text: signature_block(text, 'Acknowledge'), '')]),
    ('forged-subject', SQLITE, 'publication_binding',
     ['forged subject bound'],
     'The adapter stops comparing the rebuilt subject: data with one flipped subject byte binds to a publication.',
     [(ADAPTER, 'PublicationOutcome::Commit(publication) if publication.subject() == data.subject => {',
       'PublicationOutcome::Commit(publication) => {')]),
    ('wrong-store', SHELL, 'effects::tests::wrong_store_response',
     ['a refused response advanced the program: Ok(())'],
     "The driver stops comparing store identities: a lost-reply answer from another store resumes the commit program.",
     [(DRIVER, '(Some(got), Some(expected)) if got.identity != expected.identity => {',
       '(Some(got), Some(expected)) if false && got.identity != expected.identity => {')]),
    ('duplicate-response', SHELL, 'effects::tests::duplicate_response',
     ['a duplicated response was admitted'],
     'The driver stops binding ordinals: a repeated Busy answer to a commit resumes the retried commit and spends a retry.',
     [(DRIVER, 'if Some(got.id) == self.answered {\n            return Err(Refusal::Duplicate);',
       'if false && Some(got.id) == self.answered {\n            return Err(Refusal::Duplicate);'),
      (DRIVER, 'if got.id.ordinal < binding.id.ordinal {', 'if false && got.id.ordinal < binding.id.ordinal {'),
      (DRIVER, 'if got.id.ordinal != binding.id.ordinal {', 'if false && got.id.ordinal != binding.id.ordinal {')]),
    ('substituted-source', SHELL, 'effects::tests::step_conformance',
     ['program source bound: false', 'absent key keeps the original guard'],
     'A substituted step function retries unversioned whenever recovery reads history, even when the key is absent: '
     'the bound source digest no longer matches and the absent-key case requests Commit(Unversioned).',
     [(PROGRAM, '''                    let mode = if contains(&history, &continuation.invocation) {
                        CommitMode::Unversioned
                    } else {
                        continuation.invocation.mode
                    };''', '''                    let _ = (&history, contains);
                    let mode = CommitMode::Unversioned;''')]),
    ('partial-commit', SHELL, 'effects::explore_tests::simulation_laws',
     ['law commit-atomic failed'],
     'A commit writes its state, history entry and first outbox row but not the second: D is neither the '
     'pre-state nor the complete post-state.',
     [(SIMULATION, 'for delivery in deliveries(version, after.chain, &after.state) {',
       'for delivery in deliveries(version, after.chain, &after.state).into_iter().take(1) {')]),
    ('reset-key', SHELL, 'effects::explore_tests::simulation_laws',
     ['law reopen-audits failed'],
     'Reopening the store resets its replay keys, so a recovering commit after a crash would apply the same '
     'key a second time.',
     [(SIMULATION, 'let answer = Response::Reopen(Outcome::Ok(durable.snapshot()));\n',
       'let answer = Response::Reopen(Outcome::Ok(durable.snapshot()));\n'
       '                self.world.durable.replays.clear();\n')]),
    ('wrong-ack', SHELL, 'effects::explore_tests::simulation_laws',
     ['law acknowledge-exact failed'],
     "Acknowledging the second record sets the first record's bit instead.",
     [(SIMULATION, 'after.outbox[index].1 = true;', 'after.outbox[0].1 = true;')]),
    ('early-wake', SHELL, 'effects::explore_tests::simulation_laws',
     ['law time-monotone failed'],
     'A wait returns one tick before its deadline as an ordinary wake.',
     [(SIMULATION, 'let first = deadline.tick.max(self.world.tick);',
       'let first = deadline.tick.saturating_sub(1).max(self.world.tick);')]),
    ('out-of-range-draw', SHELL, 'effects::explore_tests::simulation_laws',
     ['law jitter-in-range failed'],
     'The jitter draw may equal its bound.',
     [(SIMULATION, '.unwrap_or(usize::MAX).min(256);', '.unwrap_or(usize::MAX).min(256) + 1;')]),
    ('stale-response', SHELL, 'effects::explore_tests::late_replies_refused',
     ['late reply admitted'],
     "The driver stops refusing older ordinals: the lost report of a delivery's first send, offered after "
     'the resend was requested, resumes the program.',
     [(DRIVER, 'if got.id.ordinal < binding.id.ordinal {', 'if false && got.id.ordinal < binding.id.ordinal {'),
      (DRIVER, 'if got.id.ordinal != binding.id.ordinal {', 'if false && got.id.ordinal != binding.id.ordinal {')]),
    ('replay-fallback', SHELL, 'effects::explore_tests::replay_is_exact',
     ['tampered transcript replayed: truncated'],
     'When the transcript runs out, replay answers from a live simulated store and accepts the live '
     'outcome: a truncated transcript replays to an outcome.',
     [(RECORD, '        let binding = binding.clone();\n',
       '        let binding = binding.clone();\n        let asked_live = operation.clone();\n'),
      (RECORD, '            None => return Err(ReplayError::Missing(at)),\n        }\n        at += 1;',
       '            None => {\n'
       '                let mut live = super::simulation::Simulation::new(\n'
       '                    super::simulation::World::new(super::simulation::Durable::genesis()),\n'
       '                    super::simulation::Profile::frozen(0),\n'
       '                    super::simulation::ALL_PORTS,\n'
       '                    super::simulation::Script::fault_free(),\n'
       '                );\n'
       '                match super::handler::Terminal::handle(&mut live, &binding, &asked_live) {\n'
       '                    super::handler::Handled::Answer(response) => {\n'
       '                        let _ = driver.reply(Reply { binding, response });\n'
       '                        at = links.len();\n'
       '                        continue;\n'
       '                    }\n'
       '                    _ => return Err(ReplayError::Missing(at)),\n'
       '                }\n'
       '            }\n        }\n        at += 1;'),
      (RECORD, '                None => return Err(ReplayError::Missing(at)),\n            }\n            if at + 1',
       '                None => return Ok(Ended::Done(done(outcome))),\n            }\n            if at + 1')]),
    ('broken-link', SHELL, 'effects::explore_tests::transcript_chain',
     ['broken link verified'],
     'Verification stops comparing each link with its predecessor and with its own digest.',
     [(RECORD, 'if link.previous != previous || link.digest != link_digest(previous, &link.entry) {',
       'if false && (link.previous != previous || link.digest != link_digest(previous, &link.entry)) {')]),
    ('uncovered-capability', SHELL, 'effects::explore_tests::stack_binding',
     ['uncovered port bound'],
     'Binding stops checking coverage: the delivery program binds to a store-only terminal without Send.',
     [(HANDLER, 'if !covered.contains(port) {', 'if false && !covered.contains(port) {')]),
    ('store-identity-authority', PRODUCTION, 'store_identity_binds_genesis_authority_and_interpreter',
     ['store identity ignores an input'],
     "The store identity leaves out the Authority's identity: stores of two contracts with one genesis "
     'would share it.',
     [(STORE, '    codec::bytes(&mut out, authority);\n', '    let _ = authority;\n')]),
    ('commit-mode-dropped', PRODUCTION, 'production_agrees_with_model',
     ['failed: commit'],
     'The production handler commits At(v) without its expected version: a stale handle gets PreState '
     'where V2 and the model refuse Concurrent.',
     [(MOUNT, 'shell.commit_with_crash_at(version, key, publication, interrupt)',
       'shell.commit_with_crash(key, publication, { let _ = version; interrupt })')]),
    ('busy-as-unknown', PRODUCTION, 'production_real_busy',
     ['busy is not applied'],
     'The production handler reports a real SQLite busy store as Unknown, though the transaction rolled back.',
     [(MOUNT, '''                    Response::not_applied(port, Fault::Busy)
                }
                rusqlite::ErrorCode::DiskFull''', '''                    Response::unknown(port)
                }
                rusqlite::ErrorCode::DiskFull''')]),
    ('project-away-time', PRODUCTION, 'production_fingerprint_retains_clock',
     ['clock state projected away'],
     'Removing retained adapter and connection state merges the two reviewed clock futures.',
     [(PRODUCTION_TEST, """        for handle in &self.handles {
            let state = ok(handle.test_execution_state());
            bytes.extend_from_slice(&(state.len() as u64).to_be_bytes());
            bytes.extend_from_slice(&state);
        }
""", '')]),
    ('wrong-recipient', SHELL, 'effects::explore_tests::simulation_laws',
     ['law commit-atomic failed'],
     'The reviewed surviving mutant reroutes both deliveries while leaving the specification helper unchanged.',
     [(SIMULATION, 'for delivery in deliveries(version, after.chain, &after.state) {',
       'for mut delivery in deliveries(version, after.chain, &after.state) {\n'
       '            delivery.destination = b"wrong-recipient".to_vec();')]),
    ('forget-send-uncertainty', REVIEW, 'lost_send_remains_unknown_after_clock_failure',
     ['send uncertainty was erased'],
     'A later clock failure falsely reports non-execution after an unknown send.',
     [(PROGRAM, 'let outcome = if continuation.may_have_sent {',
       'let outcome = if false && continuation.may_have_sent {')]),
    ('unbound-reopen', SQL_REVIEW, 'production_rebind_rejects_other_genesis',
     ['substituted genesis accepted'],
     'Reopening a replaced path accepts a different valid genesis under the retained identity.',
     [(MOUNT, 'if shell.effects_identity()? != self.store.identity {',
       'if false && shell.effects_identity()? != self.store.identity {')]),
    ('omit-full', REVIEW, 'frozen_alphabet_contains_required_failures',
     ['Full missing from frozen alphabet'],
     'The read fault alphabet loses Full again.',
     [(SIMULATION, 'Some(3) => Handled::Answer(Response::not_applied(port, Fault::Full)),',
       'Some(3) => Handled::Answer(Response::not_applied(port, Fault::Io)),')]),
    ('discard-checkpoint-frontier', EVIDENCE, 'resumed_search_matches_uninterrupted_closure',
     ['resumed closure differs'],
     'A budget stop returns a checkpoint without the retained prefixes, so resume disagrees with uninterrupted closure.',
     [(EXPLORER, '                policies: policy_data.clone(),\n                frontier,',
       '                policies: policy_data.clone(),\n                frontier: VecDeque::new(),')]),
    ('omit-handler-provenance', EVIDENCE, 'manifest_sources_configuration_and_seed_are_bound',
     ['handler implementation missing'],
     'A named stack without its implementation sources cannot be qualified.',
     [(RECORD, 'implementations: library_implementations(),', 'implementations: Vec::new(),')]),
    ('ambient-clock', None, 'purity',
     ['error[clock]'],
     'A step function reads std::time::Instant: the purity checker reports a clock violation in program.rs.',
     [(PROGRAM, '\nfn ordered(', '\n#[allow(dead_code)]\nfn elapsed() -> bool {\n    std::time::Instant::now().elapsed().as_nanos() > 0\n}\n\nfn ordered(')]),
]


def replace(text: str, before, after: str) -> str:
    anchor = before(text) if callable(before) else before
    found = text.count(anchor)
    if found != 1:
        raise RuntimeError(f'mutation anchor count {found}, wanted 1: {anchor[:80]!r}')
    return text.replace(anchor, after)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, help='write the control rows here, logs beside it')
    parser.add_argument('--cli', type=Path, required=True, help='built zeno-fcis binary for the purity control')
    parser.add_argument('--only', choices=[control[0] for control in CONTROLS])
    parser.add_argument('--release', action='store_true', help='use optimized checks for the finite explorers')
    args = parser.parse_args()
    env = dict(os.environ, CARGO_NET_OFFLINE='true')
    env.setdefault('CARGO_TARGET_DIR', str(ROOT / 'target' / 'shell-effects-controls'))
    cargo = ['cargo', '+1.97.1', 'test', '--locked', '--offline'] + (['--release'] if args.release else [])

    def run(root: Path, argv: list[str]) -> subprocess.CompletedProcess[str]:
        if shutil.disk_usage('/').free < 8 * 1024**3:
            raise RuntimeError('root free space below 8 GiB')
        return subprocess.run(argv, cwd=root, env=source_environment(root, env) if argv[0] == "cargo" else env, stdout=subprocess.PIPE,
                              stderr=subprocess.STDOUT, text=True, timeout=900)

    def check(root: Path, target, name: str) -> subprocess.CompletedProcess[str]:
        if target is None:
            return run(root, [str(args.cli), 'purity', '--no-kb', 'crates/zeno-fcis-shell'])
        return run(root, cargo + target + [name, '--', '--exact'])

    selected = [control for control in CONTROLS if args.only in (None, control[0])]
    baseline_exits = {}
    for name, target, test, *_ in selected:
        baseline = check(ROOT, target, test)
        passed = 'purity: clean' in baseline.stdout if target is None else ' 1 passed' in baseline.stdout
        if baseline.returncode or not passed:
            raise SystemExit(f'BASELINE FAILED {name} ({test}):\n{baseline.stdout}')
        baseline_exits[name] = baseline.returncode
    rows = []
    with tempfile.TemporaryDirectory(prefix='shell-effects-controls-') as temp:
        copy = Path(temp) / 'source'
        shutil.copytree(ROOT, copy, ignore=shutil.ignore_patterns('.git', 'target', 'node_modules', '__pycache__'))
        for name, target, test, markers, witness, edits in selected:
            original = {path: (copy / path).read_text() for path, _, _ in edits}
            changed = dict(original)
            for path, before, after in edits:
                changed[path] = replace(changed[path], before, after)
            for path, text in changed.items():
                (copy / path).write_text(text)
            start = time.monotonic()
            package = target[:2] if target else ['-p', 'zeno-fcis-shell']
            build = run(copy, cargo + ['--no-run'] + package
                        + (target[2:] if target else ['--lib']))
            if build.returncode:
                raise SystemExit(f'BUILD FAILURE (not a kill) {name}:\n{build.stdout}')
            result = check(copy, target, test)
            if target is None:
                killed = 'purity: violations' in result.stdout
            else:
                killed = result.returncode == 101 and 'test result: FAILED.' in result.stdout
            missing = [marker for marker in markers if marker not in result.stdout]
            if not killed or missing:
                raise SystemExit(f'NOT KILLED for the named reason {name} (missing {missing}):\n{result.stdout}')
            for path, text in original.items():
                (copy / path).write_text(text)
            restored = check(copy, target, test)
            passed = 'purity: clean' in restored.stdout if target is None else ' 1 passed' in restored.stdout
            if restored.returncode or not passed:
                raise SystemExit(f'RESTORED BASELINE FAILED {name}:\n{restored.stdout}')
            row = dict(control=name, check=test, witness=witness, build_exit=build.returncode,
                       normal_exit=baseline_exits[name], check_exit=result.returncode,
                       restored_exit=restored.returncode, result='killed',
                       seconds=round(time.monotonic() - start, 3))
            rows.append(row)
            print(json.dumps(row), flush=True)
            if args.out:
                args.out.parent.mkdir(parents=True, exist_ok=True)
                args.out.with_name(f'{args.out.stem}-{name}.log').write_text(build.stdout + '\n' + result.stdout)
                args.out.with_name(f'{args.out.stem}-{name}-restored.log').write_text(restored.stdout)
    if args.out:
        args.out.write_text(json.dumps(rows, indent=2) + '\n')
    print(f'PASS: baselines and {len(rows)} source controls killed for their named reasons')


if __name__ == "__main__":
    try:
        from telemetry import run_cli
    except ImportError:
        # Reporting is optional, including for a standalone copy of this tool.
        run_cli = lambda function: function()
    def _entrypoint():
        main()
    run_cli(_entrypoint)
