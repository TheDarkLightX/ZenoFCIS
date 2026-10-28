#!/usr/bin/env python3
"""Qualify the Linux native-leaf supervisor with actual IPC and SQLite dispatch.

This development-profile host gate does not establish an installed release,
complete compiler/runtime provenance, aggregate build-worker limits or general
noninterference. Every required native test is selected by a fixed inventory.
"""
from __future__ import annotations
import argparse
import errno
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import resource
import shutil
import socket
import stat
import struct
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = ROOT / 'crates/zeno-fcis-shell-sqlite'
HELPER = PACKAGE / 'src/effects/native_worker.py'
CORE = ('python','bwrap','prlimit','loader','libc','libgcc','libm','seccomp')
TESTS = {
    'actual_requester_process_death_reaps_owned_native_tree',
    'native_peer_cannot_select_identity_slot_or_replay',
    'native_transport_unwind_stops_worker_and_retains_attempt',
    'native_truncation_stderr_pressure_cpu_and_wall_limits',
    'native_unknown_and_lost_reply_preserve_durable_attempt',
    'runtime_inventory_hash_and_image_refusals_precede_dispatch',
    'useful_native_dispatch_rechecks_current_grants',
}
REQUIRED = (
    'src/effects/native_worker.rs','src/effects/native_worker.py','src/effects/channel.rs',
    'src/effects/owner.rs','src/effects.rs','tests/effects_owner.rs',
    'tests/native_worker/tests.rs','tests/native_worker/probe.c',
)

class Refusal(ValueError):
    """Missing, contradictory or incomplete required qualification evidence."""


def sha(data):
    return hashlib.sha256(data).hexdigest()


def source_identity(info):
    """Detect replacement or mutation without treating atime reads as changes."""
    return (info.st_dev, info.st_ino, info.st_mode, info.st_size,
            info.st_mtime_ns, info.st_ctime_ns)


def source_stat(root, path, directory=False):
    """Check every component from the selected root without following symlinks."""
    try:
        relative = path.relative_to(root)
    except ValueError as error:
        raise Refusal('source input is outside the snapshot root') from error
    parts = relative.parts
    if any(part in ('.', '..') for part in parts):
        raise Refusal('source input path is not canonical')
    current = root
    for index in range(len(parts) + 1):
        if index:
            current = current / parts[index-1]
        info = current.lstat()
        if stat.S_ISLNK(info.st_mode):
            raise Refusal('source input or ancestor is a symlink')
        wants_directory = index < len(parts) or directory
        if wants_directory and not stat.S_ISDIR(info.st_mode):
            raise Refusal('source directory is not a directory')
        if not wants_directory and not stat.S_ISREG(info.st_mode):
            raise Refusal('source input is not a regular file')
    return info


def source_paths(root):
    """Enumerate the same selected inputs, refusing skipped or linked directories."""
    paths = [root/'Cargo.toml', root/'Cargo.lock', root/'rust-toolchain.toml',
             root/'tools/check_native_worker.py', root/'tools/check_native_startup.py']
    tools_directory = root/'tools'
    directories = {tools_directory: source_identity(source_stat(root, tools_directory, directory=True))}
    pending = [root/'crates']
    while pending:
        directory = pending.pop()
        before = source_stat(root, directory, directory=True)
        with os.scandir(directory) as entries:
            names = sorted(entry.name for entry in entries)
        after = source_stat(root, directory, directory=True)
        if source_identity(before) != source_identity(after):
            raise Refusal('source directory changed while scanning')
        directories[directory] = source_identity(after)
        for name in names:
            path = directory/name
            info = path.lstat()
            if stat.S_ISLNK(info.st_mode):
                raise Refusal('source input or ancestor is a symlink')
            if name == '__pycache__':
                continue
            if stat.S_ISDIR(info.st_mode):
                pending.append(path)
            elif stat.S_ISREG(info.st_mode):
                paths.append(path)
            else:
                raise Refusal('source entry is not a regular file or directory')
    return paths, directories


def source_manifest(root, paths, identities=None):
    """Hash regular source files and require every named native input in the map."""
    if not hasattr(os, 'O_NOFOLLOW'):
        raise Refusal('source no-follow open is unavailable')
    result = {}
    for path in sorted(paths):
        relative = str(path.relative_to(root))
        if relative in result:
            raise Refusal('duplicate source input')
        before = source_stat(root, path)
        fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | getattr(os, 'O_CLOEXEC', 0))
        with os.fdopen(fd, 'rb') as source:
            opened = os.fstat(source.fileno())
            if source_identity(before) != source_identity(opened):
                raise Refusal('source input changed before reading')
            data = source.read()
            if source_identity(opened) != source_identity(os.fstat(source.fileno())):
                raise Refusal('source input changed while reading')
        if source_identity(opened) != source_identity(source_stat(root, path)):
            raise Refusal('source input changed after reading')
        result[relative] = sha(data)
        if identities is not None:
            identities['file:'+relative] = source_identity(opened)
    required = {str(Path('crates/zeno-fcis-shell-sqlite')/name) for name in REQUIRED}
    if not required.issubset(result):
        raise Refusal('required native supervisor input omitted from snapshot')
    return result


def snapshot(root=None, *, identities=None):
    """Snapshot the actual checkout, or an explicitly supplied self-test fixture."""
    normal = root is None
    root = ROOT if normal else Path(root)
    try:
        if not root.is_absolute() or root.resolve(strict=True) != root:
            raise Refusal('snapshot root path changed')
        if normal and Path(os.path.abspath(__file__)) != root/'tools/check_native_worker.py':
            raise Refusal('running checker source path differs from snapshot')
        root_identity = source_identity(source_stat(root, root, directory=True))[:3]
        paths, directories = source_paths(root)
        observed = {}
        result = source_manifest(root, paths, observed)
        for directory, identity in directories.items():
            if source_identity(source_stat(root, directory, directory=True)) != identity:
                raise Refusal('source directory changed during snapshot')
            observed['directory:'+str(directory.relative_to(root))] = identity
        if source_identity(source_stat(root, root, directory=True))[:3] != root_identity:
            raise Refusal('snapshot root changed during snapshot')
        observed['ancestor:.'] = root_identity
        if identities is not None:
            identities.update(observed)
        return result
    except OSError as error:
        raise Refusal('source input missing or unreadable') from error


def require_same_snapshot(before, before_identities, after, after_identities):
    """Keep the hashed inventory and each selected path identity fixed during the gate."""
    if before != after or before_identities != after_identities:
        raise Refusal('qualification source changed during execution')


def runtime_rows(text):
    rows = []
    for line in text.splitlines():
        fields = line.split('\t')
        if len(fields)!=3 or not re.fullmatch('[0-9a-f]{64}',fields[1]):
            raise Refusal('malformed runtime pin')
        role, digest, path = fields
        if role not in (*CORE, 'dependency') or not Path(path).is_absolute():
            raise Refusal('unknown runtime role or relative runtime path')
        rows.append((role,digest,path))
    if not 8 < len(rows) <= 256 or tuple(row[0] for row in rows[:8])!=CORE:
        raise Refusal('incomplete runtime core inventory')
    if any(row[0]!='dependency' for row in rows[8:]) or len({row[2] for row in rows})!=len(rows):
        raise Refusal('duplicate or reordered runtime inventory')
    return rows


def configuration(rows, image, arguments, mutation=None):
    rows = list(rows)
    if mutation=='missing-pin':rows.pop(8)
    if mutation=='duplicate-pin':rows.append(rows[0])
    if mutation=='empty-pins':rows=[]
    out=bytearray(b'ZFNAT001' if mutation!='version' else b'INVALID!')
    out.extend(struct.pack('<H',len(rows)))
    for _,digest,path in rows:
        encoded=path.encode();out.extend(struct.pack('<H',len(encoded)));out.extend(encoded);out.extend(bytes.fromhex(digest))
    out.append(len(arguments))
    for arg in arguments:
        encoded=arg.encode();out.extend(struct.pack('<H',len(encoded)));out.extend(encoded)
    out.extend(struct.pack('<I',(8<<20)+1 if mutation=='oversize-image' else len(image)))
    expected=hashlib.sha256(image).digest()
    out.extend(bytes(32) if mutation=='image-hash' else expected)
    out.extend(image)
    return bytes(out)


def exact(stream, count):
    out=bytearray()
    while len(out)<count:
        data=stream.recv(count-len(out))
        if not data:raise Refusal('truncated bootstrap status or request')
        out.extend(data)
    return bytes(out)


def bootstrap_case(work, rows, image, optimized, label, mutation=None, secret=b'private-A'):
    canary=work/'canary.txt';canary.write_bytes(secret)
    try:
        os.fstat(200)
    except OSError as error:
        if error.errno!=errno.EBADF:raise
    else:
        raise Refusal('FD 200 is occupied; run in a dedicated test controller')
    source=os.open(canary,os.O_RDONLY)
    os.dup2(source,200,inheritable=True);os.close(source)
    parent,endpoint=socket.socketpair();status,status_endpoint=socket.socketpair()
    parent.settimeout(8);status.settimeout(8)
    child=None
    try:
        command=['/usr/bin/python3','-I','-S','-B',*(['-O'] if optimized else []),'-c',HELPER.read_text()]
        child=subprocess.Popen(command,stdin=endpoint,stdout=endpoint,stderr=status_endpoint,
                               close_fds=True,pass_fds=(200,),env={'ZENO_OWNER_CREDENTIAL':'harmless-canary'},cwd='/')
        endpoint.close();status_endpoint.close()
        parent.sendall(configuration(rows,image,['once',str(canary)],mutation))
        request=b''
        if mutation is None:
            ready=exact(status,16)
            if ready[:8]!=b'ZFNOK001':raise Refusal('positive bootstrap refused')
            request=exact(parent,16)
            if request!=b'ZFW1'+bytes(12):raise Refusal('independent C worker request differs')
            parent.sendall(b'ZFR1'+bytes(12))
        record=exact(status,64)
        code=child.wait(timeout=2)
        if record[:8]!=b'ZFNEND01':raise Refusal('completion channel differs')
        worker_exit,errors=struct.unpack_from('<iI',record,12)
        outgoing,incoming=struct.unpack_from('<II',record,52)
        if mutation is None:
            if code!=0 or record[8]!=1 or worker_exit!=0 or errors or (outgoing,incoming)!=(16,16):
                raise Refusal('positive bootstrap did not complete useful protected work')
        elif code!=78 or record[8]!=6 or request or outgoing or incoming:
            raise Refusal('negative bootstrap did not refuse before native work')
        reset=False
        try:
            tail=parent.recv(1)
        except ConnectionResetError:
            # Early refusal can leave configuration bytes unread. Only the
            # already validated separate status plus exit 78 admits this reset.
            if mutation is None:raise
            reset=True;tail=b''
        if tail:raise Refusal('unexpected worker bytes after required exchange')
        if canary.read_bytes()!=secret:raise Refusal('native worker changed host canary')
        row={'case':label,'optimized':optimized,'mutation':mutation,'exit':code,'worker_exit':worker_exit,
             'termination':record[8],'request_hex':request.hex(),'stderr_bytes':errors,
             'stderr_prefix_sha256':record[20:52].hex(),'output_bytes':outgoing,'input_bytes':incoming,'early_refusal_reset':reset}
        (work/(label+'.json')).write_text(json.dumps(row,indent=2)+'\n')
        return row
    except BaseException as error:
        (work/(label+'.json')).write_text(json.dumps({'case':label,'status':'incomplete','error':str(error)},indent=2)+'\n')
        raise
    finally:
        if child is not None and child.poll() is None:child.kill();child.wait(timeout=2)
        for stream in (parent,endpoint,status,status_endpoint):stream.close()
        os.close(200)


def build_bounds():
    resource.setrlimit(resource.RLIMIT_AS,(6*1024**3,6*1024**3))


def run(command, work, label, env, timeout=600, bounded_build=False, inherited=()):
    start=time.monotonic()
    with (work/(label+'.log')).open('w') as log:
        result=subprocess.run(command,cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT,
                              timeout=timeout,preexec_fn=build_bounds if bounded_build else None,pass_fds=inherited)
    output=(work/(label+'.log')).read_text()
    (work/(label+'.json')).write_text(json.dumps({'command':command,'exit':result.returncode,
        'elapsed_seconds':time.monotonic()-start,'log_sha256':sha(output.encode())},indent=2)+'\n')
    if result.returncode:raise Refusal(f'{label} failed; see {work/(label+".log")}')
    return output


def check_inventory(output):
    rows=[line.removesuffix(': test') for line in output.splitlines() if line.endswith(': test')]
    expected={'native_worker_process::'+name for name in TESTS}
    if len(rows)!=len(set(rows)) or set(rows)!=expected:
        raise Refusal('required native test inventory differs')
    return expected


def check_summary(output):
    summaries=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',output)
    if summaries!=[(str(len(TESTS)),'0','0')]:
        raise Refusal('required native tests were not all executed')


def snapshot_self_test():
    """Test source coverage and failures using disposable, isolated fixtures."""
    def refused(label, action, message):
        try:
            action()
        except Refusal as error:
            if message not in str(error):
                raise Refusal(f'{label} refused for the wrong reason: {error}') from error
            return
        raise Refusal('snapshot self-test accepted '+label)

    with tempfile.TemporaryDirectory(prefix='zeno-native-snapshot-') as scratch:
        scratch = Path(scratch).resolve()
        root = scratch/'repo'
        root.mkdir()
        files = ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml',
                 'tools/check_native_worker.py', 'tools/check_native_startup.py',
                 'crates/other/src/lib.rs']
        files += [str(Path('crates/zeno-fcis-shell-sqlite')/name) for name in REQUIRED]
        for name in files:
            path = root/name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(('fixture:'+name).encode())
        ignored = root/'crates/other/__pycache__/cache.pyc'
        ignored.parent.mkdir(parents=True)
        ignored.write_bytes(b'ignored cache')
        before_identities = {}
        before = snapshot(root, identities=before_identities)
        probe_name = 'crates/zeno-fcis-shell-sqlite/tests/native_worker/probe.c'
        probe = root/probe_name
        original = probe.read_bytes()
        if set(before) != set(files) or before.get(probe_name) != sha(original):
            raise Refusal('regular snapshot fixture has incomplete or extra source coverage')
        probe.write_bytes(original+b' changed')
        changed = snapshot(root)
        if before == changed or before[probe_name] == changed[probe_name]:
            raise Refusal('snapshot did not expose source content drift')
        probe.write_bytes(original)

        # Identical bytes do not excuse replacement of a source path during the gate.
        before_identities = {}
        if snapshot(root, identities=before_identities) != before:
            raise Refusal('restored content differs before path-replacement control')
        replacement = scratch/'same-byte-original.c'
        probe.rename(replacement)
        probe.write_bytes(original)
        try:
            after_identities = {}
            same_bytes = snapshot(root, identities=after_identities)
            if same_bytes != before:
                raise Refusal('same-byte replacement changed the hash-map control')
            if before_identities['file:'+probe_name][:2] == after_identities['file:'+probe_name][:2]:
                raise Refusal('same-byte fixture did not replace the source inode')
            refused('same-byte path replacement',
                    lambda: require_same_snapshot(before, before_identities,
                                                  same_bytes, after_identities),
                    'qualification source changed during execution')
        finally:
            probe.unlink()
            replacement.rename(probe)

        probe.unlink()
        refused('missing required input', lambda: snapshot(root), 'omitted from snapshot')
        probe.write_bytes(original)
        paths, _ = source_paths(root)
        refused('required input omitted from map',
                lambda: source_manifest(root, [path for path in paths if path != probe]),
                'omitted from snapshot')
        refused('noncanonical snapshot root', lambda: snapshot(root/'..'/'repo'),
                'snapshot root path changed')

        def linked_directory(name, label):
            path = root/name
            external = scratch/('linked-'+label)
            path.rename(external)
            path.symlink_to(external, target_is_directory=True)
            try:
                refused(label, lambda: snapshot(root), 'symlink')
            finally:
                path.unlink()
                external.rename(path)

        linked_directory('crates/zeno-fcis-shell-sqlite/tests/native_worker', 'native-test-directory')
        linked_directory('tools', 'tools-directory')
        linked_directory('crates', 'crates-directory')
        moved_probe = scratch/'linked-probe.c'
        probe.rename(moved_probe)
        probe.symlink_to(moved_probe)
        try:
            refused('source file symlink', lambda: snapshot(root), 'symlink')
        finally:
            probe.unlink()
            moved_probe.rename(probe)

        cargo = root/'Cargo.lock'
        cargo_bytes = cargo.read_bytes()
        cargo.unlink()
        refused('missing selected source', lambda: snapshot(root), 'missing or unreadable')
        cargo.write_bytes(cargo_bytes)

        original_open = os.open
        def denied_open(path, flags, *args, **kwargs):
            if Path(path) == probe:
                raise PermissionError(errno.EACCES, 'self-test unreadable source')
            return original_open(path, flags, *args, **kwargs)
        os.open = denied_open
        try:
            refused('unreadable source', lambda: snapshot(root), 'missing or unreadable')
        finally:
            os.open = original_open

        other = root/'crates/other'
        original_scandir = os.scandir
        def denied_scandir(path):
            if Path(path) == other:
                raise PermissionError(errno.EACCES, 'self-test unreadable directory')
            return original_scandir(path)
        os.scandir = denied_scandir
        try:
            refused('unreadable source directory', lambda: snapshot(root), 'missing or unreadable')
        finally:
            os.scandir = original_scandir

        injected = other/'new-during-scan.rs'
        def drift_scandir(path):
            entries = original_scandir(path)
            if Path(path) != other:
                return entries
            class ChangingDirectory:
                def __enter__(self):
                    return entries.__enter__()
                def __exit__(self, kind, value, traceback):
                    result = entries.__exit__(kind, value, traceback)
                    injected.write_bytes(b'new source')
                    return result
            return ChangingDirectory()
        os.scandir = drift_scandir
        try:
            refused('directory changed during snapshot', lambda: snapshot(root),
                    'source directory changed while scanning')
        finally:
            os.scandir = original_scandir
            injected.unlink(missing_ok=True)

        def drift_open(path, flags, *args, **kwargs):
            if Path(path) == probe:
                probe.write_bytes(original+b' changed during read')
            return original_open(path, flags, *args, **kwargs)
        os.open = drift_open
        try:
            refused('source changed during snapshot', lambda: snapshot(root),
                    'source input changed before reading')
        finally:
            os.open = original_open
            probe.write_bytes(original)
        if snapshot(root) != before:
            raise Refusal('restored snapshot fixture differs')
    return 13


def self_test():
    rows=[f'{role}\t'+('1'*64)+f'\t/synthetic/{role}' for role in CORE]
    rows.append('dependency\t'+('2'*64)+'\t/synthetic/dependency')
    runtime='\n'.join(rows)
    runtime_rows(runtime)
    inventory='\n'.join('native_worker_process::'+name+': test' for name in sorted(TESTS))
    check_inventory(inventory)
    summary=f'test result: ok. {len(TESTS)} passed; 0 failed; 0 ignored;'
    check_summary(summary)
    cases=[
        ('empty runtime',runtime_rows,''),('missing runtime role',runtime_rows,'\n'.join(rows[1:])),
        ('duplicate runtime',runtime_rows,runtime+'\n'+rows[-1]),
        ('relative runtime path',runtime_rows,runtime.replace('/synthetic/python','relative')),
        ('invalid digest',runtime_rows,runtime.replace('1'*64,'z'*64,1)),
        ('unknown role',runtime_rows,runtime.replace('dependency\t','unknown\t')),
        ('empty native inventory',check_inventory,''),
        ('missing native test',check_inventory,'\n'.join(inventory.splitlines()[1:])),
        ('extra native test',check_inventory,inventory+'\nforeign: test'),
        ('duplicate native test',check_inventory,inventory+'\n'+inventory.splitlines()[0]),
        ('empty execution',check_summary,''),
        ('zero execution',check_summary,'test result: ok. 0 passed; 0 failed; 0 ignored;'),
        ('partial execution',check_summary,'test result: ok. 6 passed; 0 failed; 0 ignored;'),
        ('ignored execution',check_summary,'test result: ok. 7 passed; 0 failed; 1 ignored;'),
        ('duplicate execution',check_summary,summary+'\n'+summary),
    ]
    for name,check,value in cases:
        try:
            check(value)
        except Refusal:
            continue
        raise Refusal('self-test accepted '+name)
    snapshot_refusals = snapshot_self_test()
    print(f'native-worker-host self-test: PASS ({len(cases)} refusals; 3 positive inputs; '
          f'{snapshot_refusals} snapshot refusals; source drift and restoration)')
    return 0


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--self-test',action='store_true')
    parser.add_argument('--work-dir',type=Path)
    parser.add_argument('--target-dir',type=Path)
    args=parser.parse_args()
    if args.self_test:return self_test()
    if sys.platform!='linux' or platform.machine()!='x86_64' or os.geteuid()==0:
        raise Refusal('required native-worker-host platform/account is unavailable')
    if args.work_dir is None:
        work=Path(tempfile.mkdtemp(prefix='zeno-native-worker-'))
    else:
        work=args.work_dir.resolve();work.mkdir(parents=True,exist_ok=False)
    target=(args.target_dir or Path(os.environ.get('CARGO_TARGET_DIR',ROOT/'target'))).resolve()
    env=dict(os.environ,CARGO_PROFILE_DEV_DEBUG='0',CARGO_PROFILE_TEST_DEBUG='0',CARGO_INCREMENTAL='0',
             ZENO_NATIVE_RUNTIME_PINS=str(work/'runtime-pins.tsv'),ZENO_NATIVE_WORKER_IMAGE=str(work/'probe'),
             ZENO_NATIVE_EVIDENCE_DIR=str(work),TMPDIR=str(work))
    for key in ('RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS','RUSTC_WRAPPER','RUSTC_WORKSPACE_WRAPPER'):
        env.pop(key,None)
    before_identities={}
    before=snapshot(identities=before_identities);(work/'inputs.json').write_text(json.dumps(before,sort_keys=True,indent=2)+'\n')
    compiler=shutil.which('cc')
    if compiler is None:raise Refusal('native probe compiler is unavailable')
    run([compiler,'--version'],work,'c-compiler',env)
    run([compiler,'-std=c11','-O2','-Wall','-Wextra','-Werror',str(PACKAGE/'tests/native_worker/probe.c'),'-o',str(work/'probe')],work,'compile-probe',env)
    image=(work/'probe').read_bytes()
    bootstrap=[]
    startup=[]
    failures=[]
    for optimized in (False,True):
        prefix='optimized' if optimized else 'normal'
        command=['/usr/bin/python3','-I','-S','-B',*(['-O'] if optimized else []),str(HELPER),'--describe-runtime']
        output=run(command,work,prefix+'-runtime',env)
        rows=runtime_rows(output)
        (work/(prefix+'-runtime-pins.tsv')).write_text(output)
        if not optimized:(work/'runtime-pins.tsv').write_text(output)
        paired=[]
        cases=[(prefix+'-positive-A',None,b'private-A'),(prefix+'-positive-B',None,b'private-B')]
        cases += [(prefix+'-'+kind,kind,b'private-A') for kind in
                  ('missing-pin','duplicate-pin','empty-pins','version','image-hash','oversize-image')]
        for label,mutation,secret in cases:
            try:
                row=bootstrap_case(work,rows,image,optimized,label,mutation,secret)
            except (OSError,ValueError,subprocess.SubprocessError) as error:
                failures.append({'case':label,'error':str(error)})
                continue
            bootstrap.append(row)
            if mutation is None:paired.append(row)
        for cut in ('before-link','during-link','after-link','unlinked-control'):
            label=prefix+'-startup-'+cut
            try:
                output=run(['/usr/bin/python3','-B',str(ROOT/'tools/check_native_startup.py'),
                            '--source',str(HELPER),'--pins',str(work/(prefix+'-runtime-pins.tsv')),
                            '--image',str(work/'probe'),'--work',str(work/label),'--cut',cut,
                            *(['--optimized'] if optimized else [])],work,label,env,timeout=20)
                row=json.loads(output)
                expected='control-caught' if cut=='unlinked-control' else 'passed'
                if row.get('verdict')!=expected or row.get('remaining_owned')!=0:
                    raise Refusal('startup cut verdict or cleanup differs')
                startup.append(row)
            except (OSError,ValueError,subprocess.SubprocessError) as error:
                failures.append({'case':label,'error':str(error)})
        if len(paired)==2 and paired[0]|{'case':''} != paired[1]|{'case':''}:
            failures.append({'case':prefix+'-canaries','error':'declared observations differ'})
    if failures:
        (work/'bootstrap-failures.json').write_text(json.dumps(failures,indent=2)+'\n')
        raise Refusal(f'bootstrap corpus failed; inspect {work/"bootstrap-failures.json"}')
    base=['nice','-n','19','ionice','-c','3','cargo','+1.97.1','test','--profile','dev','--locked','--offline',
          '--jobs','1','--target','x86_64-unknown-linux-gnu','--target-dir',str(target),'-p','zeno-fcis-shell-sqlite',
          '--features','crash-injection','--test','effects_owner','native_worker_process::']
    flags=['--','--ignored','--skip','native_requester_role']
    inventory=run([*base,*flags,'--list'],work,'native-inventory',env,bounded_build=True)
    expected=check_inventory(inventory)
    try:
        os.fstat(200)
    except OSError as error:
        if error.errno!=errno.EBADF:raise
    else:
        raise Refusal('FD 200 is occupied before native test inheritance')
    descriptor=os.open(work/'canary.txt',os.O_RDONLY)
    os.dup2(descriptor,200,inheritable=True);os.close(descriptor)
    env['ZENO_NATIVE_REQUIRE_INHERITED_FD']='1'
    try:
        output=run([*base,*flags,'--test-threads=1','--nocapture'],work,'native-tests',env,bounded_build=True,inherited=(200,))
    finally:
        os.close(200)
    check_summary(output)
    after_identities={}
    after=snapshot(identities=after_identities)
    require_same_snapshot(before,before_identities,after,after_identities)
    receipt={'schema':'zeno-fcis/native-worker-host/1','status':'passed','native_tests':sorted(expected),
             'native_test_count':len(TESTS),'bootstrap_cases':bootstrap,'source_manifest_sha256':sha((work/'inputs.json').read_bytes()),
             'startup_cases':startup,
             'worker_sha256':sha(image),'runtime_pins_sha256':sha((work/'runtime-pins.tsv').read_bytes()),
             'target':str(target),'profile':'dev, crash-injection, x86_64-unknown-linux-gnu, Rust 1.97.1, locked offline',
             'limits':['local host qualification; installed release/account qualification remains required',
                       'runtime inventory is not complete compiler/OS provenance or attestation',
                       'paired finite canaries do not prove general or timing noninterference',
                       'single leaf per-process limits do not qualify aggregate multiprocess build workers']}
    (work/'qualification.json').write_text(json.dumps(receipt,sort_keys=True,indent=2)+'\n')
    print(f'native-worker-host: PASS ({len(TESTS)} integrated tests; {len(bootstrap)} bootstrap cases; {len(startup)} startup cuts/controls); evidence: {work}')
    return 0

if __name__=='__main__':
    try:
        raise SystemExit(main())
    except (OSError,ValueError,subprocess.SubprocessError) as error:
        print(f'native-worker-host: REFUSED: {error}',file=sys.stderr)
        raise SystemExit(1)
