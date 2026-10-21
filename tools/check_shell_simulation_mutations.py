#!/usr/bin/env python3
"""Execute shell-simulation behavioural controls in an isolated source copy.

The unmodified test passes first. Mutants must compile and then fail the named
assertion; compiler failures, signals and unrelated test failures are not kills.
No network, dependency changes, or edits to the original source tree are used.
"""
from __future__ import annotations
import argparse
import json
import os
from pathlib import Path
import resource
import shutil
import subprocess
import tempfile
import time

from cargo_source_scope import source_environment

ROOT = Path(__file__).resolve().parents[1]
V2 = 'crates/zeno-fcis-shell-sqlite/src/v2.rs'
DELIVERY = 'crates/zeno-fcis-shell-sqlite/src/v2/delivery.rs'
DATABASE = 'crates/zeno-fcis-shell-sqlite/tests/sim/database.rs'
FIXTURE = 'crates/zeno-fcis-shell-sqlite/tests/sim/fixture.rs'
SCHEDULE = 'crates/zeno-fcis-shell-sqlite/tests/sim/schedule.rs'
# Each entry is (name, test, assertion text, behavioural witness, textual edits).
MUTATIONS = [
    ('random-vector', 'random_and_schedule_vectors', 'published random vector',
     'Seed zero with a changed SplitMix64 increment disagrees with the fixed published vector.',
     [(SCHEDULE,'0x9e3779b97f4a7c15','0x9e3779b97f4a7c14')]),
    ('minimizer-noop', 'minimizer_self_test', 'minimizer must remove irrelevant steps',
     'A 64-step failure depending on only one step is left unreduced.',
     [(SCHEDULE,'while !steps.is_empty() {','while false && !steps.is_empty() {')]),
    ('fixture-destination', 'fixture_domain', 'left: [6, 0, 0, 0, 5, 119, 114, 111, 110, 103]',
     'The second notification has the wrong destination on a legal counter input.',
     [(FIXTURE,'                    ordinal: 1,','                    ordinal: 1,\n                    destination: c::Expr::Constant(c::Atom::Text(b"wrong")),')]),
    ('missing-second', 'outbox_exact', 'I1-I5 database mismatch',
     'Commit increment from genesis: two required notifications become one.',
     [(V2, 'for d in ds {\n            let (id, entry) = delivery_commitments',
       'for d in ds {\n            if lane == 1 && d.ordinal() == 1 { continue; }\n            let (id, entry) = delivery_commitments')]),
    ('extra-outbox', 'outbox_exact', 'I1-I5 database mismatch',
     'Commit increment from genesis: an undeclared third notification appears.',
     [(V2, '\nfn load_delivery(', '\nfn load_delivery(')]),  # Filled below with a bounded function edit.
    ('freshness-layers', 'freshness_interleaving', 'stale writer must return Concurrent',
     'Second handle commits inside the pre-BEGIN hook; removing transactional cached-tip and CAS guards changes the refusal (unique sequence still prevents double-apply).',
     [(V2, 'if data_version(connection)? != version || read_anchor(connection)? != *anchor {',
       'if false && (data_version(connection)? != version || read_anchor(connection)? != *anchor) {'),
      (V2, '    check_tip(connection, &segments, anchor)?;', '    // Mutant removes the layered cached-tip history check.'),
      (V2, 'AND sequence=?5 AND root=?6 AND chain=?7',
       'AND (?5 IS NOT NULL) AND (?6 IS NOT NULL) AND (?7 IS NOT NULL)')]),
    ('wrong-payload', 'acknowledgements_are_exact', 'I4 wrong payload',
     'Commit; acknowledge known pending ID with an incorrect hash and no intervening action.',
     [(DELIVERY, 'if super::relay::payload_sha256(&delivery.payload) != payload_sha256 {',
       'if false && super::relay::payload_sha256(&delivery.payload) != payload_sha256 {'),
      (DELIVERY, 'if super::relay::payload_sha256(&payload) != reported {',
       'if false && super::relay::payload_sha256(&payload) != reported {')]),
    ('changed-replay', 'changed_replay_is_refused', 'I2 changed replay',
     'Commit Increment under key 1; retry RecordFailure under key 1 with original prestate.',
     [(V2, 'if row.publication != compact\n', 'if false && (row.publication != compact\n'),
      (V2, '|| row.context != publication.evaluation().raw().context\n',
       '|| row.context != publication.evaluation().raw().context)\n')]),
    ('duplicate-ack', 'acknowledgements_are_exact', 'I4 duplicate acknowledgement',
     'Commit; acknowledge correct ID and hash; repeat exactly that acknowledgement.',
     [(DELIVERY, 'if acknowledged != 0 {', 'if false && acknowledged != 0 {')]),
    ('split-transaction', 'kill_and_reopen', 'I1-I5 database mismatch',
     'Child aborts AfterStateWrite: separately committed state survives without replay or outbox.',
     [(V2, '        interrupt(CrashPoint::AfterStateWrite)?;',
       '        tx.commit()?;\n        let tx = self.connection.transaction_with_behavior(TransactionBehavior::Immediate)?;\n        interrupt(CrashPoint::AfterStateWrite)?;')]),
    ('blind-comparator', 'comparator_self_test', 'comparator accepted unequal states',
     'Comparator receives states differing in their state-table cell.',
     []),
    ('receiver-split-dedup', 'lost_reply_receiver', 'I6 duplicate receiver effect',
     'Receive with lost reply; restart receiver; redeliver same ID: effect repeats without its dedup row.',
     []),
]


def replace(text, before, after, *, count=1):
    found = text.count(before)
    if found != count:
        raise RuntimeError(f'mutation anchor count {found}, wanted {count}: {before!r}')
    return text.replace(before, after)


def edits(name, source):
    """Construct function-bounded edits without depending on formatting widths."""
    if name == 'extra-outbox':
        text = source[V2]
        start = text.index('fn insert_deliveries(')
        end = text.index('\nfn load_delivery(', start)
        before = text[start:end]
        sql = '''    connection.execute("INSERT INTO v2_deliveries SELECT sequence,lane,999,channel,destination_root,payload_root,destination,payload,marker,?1,entry_hash,0 FROM v2_deliveries WHERE sequence=?2 AND lane=1 AND ordinal=0", params![[238_u8;32].as_slice(), sequence])?;
'''
        after = replace(before, '    Ok(())\n}', sql+'    Ok(())\n}')
        return [(V2,before,after)]
    if name == 'blind-comparator':
        text=source[DATABASE]
        start=text.index('pub(crate) fn compare(')
        end=text.index('pub(crate) struct Receiver',start)
        before=text[start:end]
        header=before[:before.index('{')+1]
        return [(DATABASE,before,header+'\n    let _=(expected,actual);\n    Ok(())\n}\n')]
    if name == 'receiver-split-dedup':
        text = source[DATABASE]
        start = text.index('    pub(crate) fn receive(')
        end = text.index('    pub(crate) fn unprotected_effect(',start)
        before = text[start:end]
        # Find the dedup INSERT statement independently of rustfmt line breaks.
        insert_start = before.index('            ok(tx.execute("INSERT INTO dedup') if '            ok(tx.execute("INSERT INTO dedup' in before else before.index('            ok(tx.execute(\n                "INSERT INTO dedup')
        insert_end = before.index('));',insert_start)+3
        after = before[:insert_start]+before[insert_end:]
        dedup = '''        if !lose_reply { ok(self.connection.execute("INSERT OR IGNORE INTO dedup VALUES(?1,?2)", params![d.id.as_bytes().as_slice(),&d.payload])); }
'''
        after = replace(after,'        let _ = lose_reply;',dedup)
        return [(DATABASE,before,after)]
    return next(m[4] for m in MUTATIONS if m[0]==name)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out',type=Path)
    parser.add_argument('--check-cas-only',action='store_true',help='record the CAS-only control masked by earlier guards')
    parser.add_argument('--check-hash-recheck-only',action='store_true',help='record the hash recheck masked by the earlier hash check')
    parser.add_argument('--only',choices=[m[0] for m in MUTATIONS])
    args=parser.parse_args()
    env=dict(os.environ, CARGO_PROFILE_DEV_DEBUG='0', CARGO_NET_OFFLINE='true')
    env.setdefault('CARGO_BUILD_JOBS', '2')
    env.setdefault('CARGO_TARGET_DIR',str(ROOT/'target'/'shell-simulation-mutations'))
    resource.setrlimit(resource.RLIMIT_CORE,(0,0))
    command=['cargo','+1.97.1','test','-p','zeno-fcis-shell-sqlite','--features','crash-injection','--locked','--offline','--test','simulation']
    results=[]
    def run(root, arguments):
        disk=subprocess.run(['df','-B1G','/'],capture_output=True,text=True,check=True)
        if shutil.disk_usage('/').free<6*1024**3: raise RuntimeError('root free space below 6 GiB')
        process=subprocess.run(command+arguments,cwd=root,env=source_environment(root,env),stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True,timeout=300)
        return process
    selected=[m for m in MUTATIONS if args.only is None or args.only==m[0]]
    if args.check_cas_only:
        selected=[('cas-only','freshness_interleaving','', 'Second-handle hook commit is still refused by cached-tip guards.',[(V2,'AND sequence=?5 AND root=?6 AND chain=?7','AND (?5 IS NOT NULL) AND (?6 IS NOT NULL) AND (?7 IS NOT NULL)')])]

    if args.check_hash_recheck_only:
        selected=[('hash-recheck-only','acknowledgements_are_exact','', 'No intervening action: relayed() refuses the wrong hash before the removed transactional recheck.',[(DELIVERY,'if super::relay::payload_sha256(&payload) != reported {','if false && super::relay::payload_sha256(&payload) != reported {')])]
    for test in sorted({m[1] for m in selected}):
        baseline=run(ROOT,[test,'--','--exact'])
        if baseline.returncode or '1 passed' not in baseline.stdout:
            raise SystemExit(f'BASELINE FAILED {test}:\n{baseline.stdout}')
    with tempfile.TemporaryDirectory(prefix='shell-simulation-mutations-') as temp:
        copy=Path(temp)/'source'
        shutil.copytree(ROOT,copy,ignore=shutil.ignore_patterns('.git','target','node_modules','__pycache__'))
        paths={p for m in selected for p,_,_ in m[4]}|{DATABASE,V2}
        original={p:(copy/p).read_text() for p in paths}
        for name,test,marker,witness,_ in selected:
            changed={}
            for path,before,after in (_ if name in {'cas-only','hash-recheck-only'} else edits(name,original)):
                count=2 if name=='duplicate-ack' else 1
                changed[path]=replace(changed.get(path,original[path]),before,after,count=count)
            for path,text in changed.items():(copy/path).write_text(text)
            start=time.monotonic()
            build=run(copy,['--no-run'])
            if build.returncode: raise SystemExit(f'BUILD FAILURE (not a kill) {name}:\n{build.stdout}')
            result=run(copy,[test,'--','--exact','--nocapture'])
            if name in {'cas-only','hash-recheck-only'}:
                valid=result.returncode==0 and '1 passed' in result.stdout
            else:
                valid=result.returncode==101 and 'test result: FAILED.' in result.stdout and marker in result.stdout
            if not valid:
                raise SystemExit(f'NOT KILLED for named reason {name}:\n{result.stdout}')
            row=dict(mutation=name,test=test,witness=witness,build_exit=build.returncode,test_exit=result.returncode,result='masked on this schedule' if name in {'cas-only','hash-recheck-only'} else 'killed',seconds=round(time.monotonic()-start,3))
            results.append(row);print(json.dumps(row),flush=True)
            if args.out:
                args.out.parent.mkdir(parents=True,exist_ok=True)
                args.out.with_name(args.out.stem+'-'+name+'.log').write_text(build.stdout+'\n'+result.stdout)
            for path in changed:(copy/path).write_text(original[path])
    if args.out:args.out.write_text(json.dumps(results,indent=2)+'\n')
    print(f'PASS: baseline and {len(results)} executed source controls; see per-control classifications')


if __name__=='__main__':main()
