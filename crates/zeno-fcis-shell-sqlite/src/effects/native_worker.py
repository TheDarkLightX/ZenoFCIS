"""Trusted Linux native-leaf bootstrap, embedded by native_worker.rs.

Dedicated streams 0/1 carry configuration, then worker frames. Stream 2 is a
separate supervisor channel. Worker stderr never reaches that control channel.
This program has no ordinary-execution fallback. Administrator-stable runtime
files and a responsive Linux kernel are premises, not established by hashes.
"""
import os
os.closerange(3, 2147483647)
import ctypes
import errno
import fcntl
import hashlib
import json
from pathlib import Path
import resource
import select
import struct
import subprocess
import sys
import time

CORE = ('python', 'bwrap', 'prlimit', 'loader', 'libc', 'libgcc', 'libm', 'seccomp')
DEFAULTS = ('/usr/bin/python3', '/usr/bin/bwrap', '/usr/bin/prlimit',
            '/usr/lib/x86_64-linux-gnu/ld-linux-x86-64.so.2',
            '/usr/lib/x86_64-linux-gnu/libc.so.6',
            '/usr/lib/x86_64-linux-gnu/libgcc_s.so.1',
            '/usr/lib/x86_64-linux-gnu/libm.so.6',
            '/usr/lib/x86_64-linux-gnu/libseccomp.so.2')
TARGETS = ('/lib64/ld-linux-x86-64.so.2', '/lib/x86_64-linux-gnu/libc.so.6',
           '/lib/x86_64-linux-gnu/libgcc_s.so.1', '/lib/x86_64-linux-gnu/libm.so.6')
SEC = ctypes.CDLL(DEFAULTS[-1], use_errno=True)
NAMES = ('read write close fstat newfstatat statx lseek openat getdents64 '
         'mmap mprotect munmap brk mremap madvise rt_sigaction rt_sigprocmask '
         'rt_sigreturn sigaltstack futex clock_gettime clock_nanosleep nanosleep '
         'getpid gettid getuid geteuid getgid getegid getrandom getcwd '
         'ftruncate fsync fdatasync exit exit_group arch_prctl set_tid_address '
         'set_robust_list rseq execve access readlink pread64 wait4').split()
MAX_FILE = 16 << 20
MAX_TOTAL = 64 << 20
MAX_IMAGE = 8 << 20
MAX_PINS = 256
MAX_WIRE = 1024 * 16
MAX_STDERR = 2048


def digest(data):
    return hashlib.sha256(data).digest()


def checked_path(path):
    if not path.is_absolute() or not path.is_file():
        raise RuntimeError('runtime-file')
    resolved = path.resolve(strict=True)
    for component in (resolved, *resolved.parents):
        status = component.stat()
        if status.st_uid != 0 or status.st_mode & 0o022:
            raise RuntimeError('runtime-permissions')
    return resolved


def read_file(path):
    with path.open('rb') as source:
        data = source.read(MAX_FILE + 1)
    if len(data) > MAX_FILE:
        raise RuntimeError('runtime-file-bound')
    return data


def loaded_files():
    paths = set()
    for module in tuple(sys.modules.values()):
        if module is sys.modules.get('__main__'):
            continue
        for attr in ('__file__', '__cached__'):
            value = getattr(module, attr, None)
            if value and Path(value).is_file():
                paths.add(str(checked_path(Path(value))))
    for line in Path('/proc/self/maps').read_text().splitlines():
        fields = line.split(None, 5)
        if len(fields) == 6 and fields[-1].startswith('/'):
            paths.add(str(checked_path(Path(fields[-1]))))
    return paths


def describe():
    """Installation/test input proposal only; never invoked as launch fallback."""
    primary = {role: str(checked_path(Path(path))) for role, path in zip(CORE, DEFAULTS)}
    files = loaded_files() | set(primary.values())
    if len(files) > MAX_PINS:
        raise RuntimeError('runtime-inventory-bound')
    for role, path in primary.items():
        print(role + '\t' + digest(read_file(Path(path))).hex() + '\t' + path)
    for path in sorted(files - set(primary.values())):
        print('dependency\t' + digest(read_file(Path(path))).hex() + '\t' + path)


def read_exact(count, deadline):
    out = bytearray()
    while len(out) < count:
        remaining = deadline - time.monotonic()
        if remaining <= 0 or not select.select([0], [], [], remaining)[0]:
            raise RuntimeError('configuration-timeout')
        data = os.read(0, count - len(out))
        if not data:
            raise RuntimeError('configuration-truncated')
        out.extend(data)
    return bytes(out)


def configuration():
    deadline = time.monotonic() + 5
    if read_exact(8, deadline) != b'ZFNAT001':
        raise RuntimeError('configuration-version')
    count = struct.unpack('<H', read_exact(2, deadline))[0]
    if not len(CORE) <= count <= MAX_PINS:
        raise RuntimeError('runtime-inventory-bound')
    files = []
    seen = set()
    total = 0
    snapshots = []
    for index in range(count):
        size = struct.unpack('<H', read_exact(2, deadline))[0]
        if not 1 <= size <= 512:
            raise RuntimeError('runtime-path-bound')
        path = checked_path(Path(read_exact(size, deadline).decode('utf-8')))
        expected = read_exact(32, deadline)
        if str(path) in seen:
            raise RuntimeError('runtime-duplicate')
        seen.add(str(path))
        data = read_file(path)
        total += len(data)
        if total > MAX_TOTAL or digest(data) != expected:
            raise RuntimeError('runtime-binding')
        files.append(str(path))
        if 3 <= index <= 6:
            snapshots.append(data)
    if set(files) != loaded_files() | set(files[:len(CORE)]):
        raise RuntimeError('runtime-closure')
    if files[0] != str(Path(sys.executable).resolve()) or files[7] != str(Path(DEFAULTS[7]).resolve()):
        raise RuntimeError('loaded-runtime-binding')
    argc = read_exact(1, deadline)[0]
    if argc > 8:
        raise RuntimeError('argument-count')
    args = []
    for _ in range(argc):
        size = struct.unpack('<H', read_exact(2, deadline))[0]
        if size > 512:
            raise RuntimeError('argument-bound')
        arg = read_exact(size, deadline).decode('utf-8')
        if '\0' in arg:
            raise RuntimeError('argument-nul')
        args.append(arg)
    size = struct.unpack('<I', read_exact(4, deadline))[0]
    expected = read_exact(32, deadline)
    if not 64 <= size <= MAX_IMAGE:
        raise RuntimeError('image-bound')
    image = read_exact(size, deadline)
    if image[:7] != b'\x7fELF\x02\x01\x01' or image[18:20] != b'\x3e\0' or digest(image) != expected:
        raise RuntimeError('image-binding')
    return files, snapshots, image, args


def seal(fd):
    os.lseek(fd, 0, os.SEEK_SET)
    fcntl.fcntl(fd, fcntl.F_ADD_SEALS, fcntl.F_SEAL_SEAL | fcntl.F_SEAL_SHRINK |
                fcntl.F_SEAL_GROW | fcntl.F_SEAL_WRITE)
    return fd


def snapshot(data):
    fd = os.memfd_create('zeno-native-input', os.MFD_CLOEXEC | os.MFD_ALLOW_SEALING)
    try:
        with os.fdopen(os.dup(fd), 'wb') as out:
            out.write(data)
        return seal(fd)
    except BaseException:
        os.close(fd)
        raise


def filter_fd():
    class Compare(ctypes.Structure):
        _fields_ = [('arg', ctypes.c_uint), ('op', ctypes.c_int),
                    ('datum_a', ctypes.c_uint64), ('datum_b', ctypes.c_uint64)]
    SEC.seccomp_init.argtypes = [ctypes.c_uint32]
    SEC.seccomp_init.restype = ctypes.c_void_p
    SEC.seccomp_syscall_resolve_name.argtypes = [ctypes.c_char_p]
    SEC.seccomp_syscall_resolve_name.restype = ctypes.c_int
    SEC.seccomp_rule_add_array.argtypes = [ctypes.c_void_p, ctypes.c_uint32, ctypes.c_int,
                                         ctypes.c_uint, ctypes.POINTER(Compare)]
    SEC.seccomp_rule_add_array.restype = ctypes.c_int
    SEC.seccomp_export_bpf.argtypes = [ctypes.c_void_p, ctypes.c_int]
    SEC.seccomp_export_bpf.restype = ctypes.c_int
    SEC.seccomp_release.argtypes = [ctypes.c_void_p]
    context = SEC.seccomp_init(0x00050000 | errno.EPERM)
    if not context:
        raise RuntimeError('filter-init')
    fd = os.memfd_create('zeno-native-filter', os.MFD_CLOEXEC | os.MFD_ALLOW_SEALING)
    try:
        def allow(name, predicates=()):
            number = SEC.seccomp_syscall_resolve_name(name.encode('ascii'))
            args = (Compare * len(predicates))(*(Compare(index, 4, value, 0) for index, value in predicates))
            if number < 0 or SEC.seccomp_rule_add_array(context, 0x7fff0000, number, len(args), args):
                raise RuntimeError('filter-rule')
        for name in NAMES:
            allow(name)
        allow('prlimit64', ((0, 0), (2, 0)))  # own process, query only
        allow('prctl', ((0, 39),))  # PR_GET_NO_NEW_PRIVS
        allow('prctl', ((0, 21),))  # PR_GET_SECCOMP
        if SEC.seccomp_export_bpf(context, fd):
            raise RuntimeError('filter-export')
        return seal(fd)
    except BaseException:
        os.close(fd)
        raise
    finally:
        SEC.seccomp_release(context)


def bind_parent(expected_parent):
    """Fail closed if the guardian died before or during this child-side link.

    This hook runs only in our fresh single-threaded bootstrap. Linux clears
    PDEATHSIG on fork, so bubblewrap's descendants still require their own
    lifetime qualification; this link covers the initial exec-chain child.
    """
    libc = ctypes.CDLL(None, use_errno=True)
    if (os.getppid() != expected_parent or
            libc.prctl(1, 9, 0, 0, 0) != 0 or  # PR_SET_PDEATHSIG, SIGKILL
            os.getppid() != expected_parent):
        os._exit(78)


def start(files, libraries, image, arguments):
    # Python preexec callbacks are supported only in this dedicated interpreter;
    # refuse a changed embedding/runtime rather than fork from multiple threads.
    if len(os.listdir('/proc/self/task')) != 1:
        raise RuntimeError('bootstrap-threads')
    expected_parent = os.getpid()
    held = []
    def hold(data):
        fd = snapshot(data)
        held.append(fd)
        return str(fd)
    try:
        command = [files[2], '--as=67108864:67108864', '--cpu=2:2', '--fsize=16777216:16777216',
                   '--nofile=32:32', '--core=0:0', '--', files[1], '--unshare-user', '--unshare-ipc',
                   '--unshare-pid', '--unshare-net', '--unshare-uts', '--unshare-cgroup', '--disable-userns',
                   '--assert-userns-disabled', '--die-with-parent', '--new-session', '--cap-drop', 'ALL',
                   '--hostname', 'zeno-worker', '--clearenv', '--size', str(32 << 20), '--tmpfs', '/',
                   '--perms', '500', '--file', hold(image), '/worker',
                   '--dir', '/lib64', '--dir', '/lib', '--dir', '/lib/x86_64-linux-gnu']
        for target, data in zip(TARGETS, libraries):
            command += ['--perms', '500', '--file', hold(data), target]
        command += ['--dev', '/dev', '--remount-ro', '/dev', '--perms', '600', '--bind-data', hold(b''),
                    '/scratch', '--remount-ro', '/', '--chdir', '/']
        fd = filter_fd()
        held.append(fd)
        command += ['--seccomp', str(fd)]
        info = os.memfd_create('zeno-native-info', os.MFD_CLOEXEC)
        held.append(info)
        command += ['--info-fd', str(info), '--', '/worker', *arguments]
        child = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                 stderr=subprocess.PIPE, close_fds=True, pass_fds=tuple(held), env={}, pipesize=4096,
                                 preexec_fn=lambda: bind_parent(expected_parent))
    except BaseException:
        for fd in held:
            os.close(fd)
        raise
    for fd in held:
        if fd != info:
            os.close(fd)
    return child, info


def control(data):
    deadline = time.monotonic() + 1
    while data:
        remaining = deadline - time.monotonic()
        if remaining <= 0 or not select.select([], [2], [], remaining)[1]:
            raise RuntimeError('control-backpressure')
        data = data[os.write(2, data):]


def final_record(reason, code, errors, observed_errors, outgoing, incoming):
    record = bytearray(64)
    record[:8] = b'ZFNEND01'
    record[8] = reason
    struct.pack_into('<iI', record, 12, code, observed_errors)
    record[20:52] = digest(errors)
    struct.pack_into('<II', record, 52, outgoing, incoming)
    return record


def supervise(child, info):
    streams = [child.stdin, child.stdout, child.stderr]
    if any(stream is None for stream in streams):
        child.kill()
        child.wait(timeout=2)
        raise RuntimeError('missing-pipe')
    incoming, outgoing, diagnostics = (stream.fileno() for stream in streams)
    for fd in (0, 1, 2, incoming, outgoing, diagnostics):
        os.set_blocking(fd, False)
    pending_input, pending_output, errors = bytearray(), bytearray(), bytearray()
    input_total = output_total = error_total = 0
    stdout_open = stderr_open = True
    ready = False
    deadline = time.monotonic() + 5
    reason = 7
    try:
        while True:
            if not ready:
                status = os.pread(info, 4096, 0)
                if status.endswith(b'}\n'):
                    init = json.loads(status)['child-pid']
                    control(b'ZFNOK001' + struct.pack('<II', child.pid, init))
                    ready = True
            if time.monotonic() >= deadline:
                reason = 2
                break
            reads = ([0] if len(pending_input) < 16 else [])
            if stdout_open and len(pending_output) < 16:
                reads.append(outgoing)
            if stderr_open:
                reads.append(diagnostics)
            writes = ([incoming] if pending_input else []) + ([1] if pending_output else [])
            readable, writable, _ = select.select(reads, writes, [], 0.02)
            for fd in readable:
                cap = 16 - len(pending_input if fd == 0 else pending_output) if fd in (0, outgoing) else 256
                data = os.read(fd, cap)
                if fd == 0:
                    if not data:
                        reason = 3
                        break
                    input_total += len(data)
                    pending_input.extend(data)
                elif fd == outgoing:
                    stdout_open = bool(data)
                    output_total += len(data)
                    pending_output.extend(data)
                else:
                    stderr_open = bool(data)
                    error_total += len(data)
                    errors.extend(data[:MAX_STDERR - len(errors)])
            if reason != 7:
                break
            if input_total > MAX_WIRE or output_total > MAX_WIRE or error_total > MAX_STDERR:
                reason = 4
                break
            for fd in writable:
                buffer = pending_input if fd == incoming else pending_output
                try:
                    count = os.write(fd, buffer)
                except BrokenPipeError:
                    reason = 5
                    break
                del buffer[:count]
            if reason != 7:
                break
            if child.poll() is not None and not stdout_open and not stderr_open and not pending_output:
                reason = 1
                break
    finally:
        if child.poll() is None:
            child.kill()
        child.wait(timeout=2)
        for stream in streams:
            stream.close()
        os.close(info)
    control(final_record(reason, child.returncode, errors, error_total, output_total, input_total))
    return 0 if reason in (1, 3) else 124 if reason == 2 else 70


def main():
    if sys.platform != 'linux' or os.uname().machine != 'x86_64' or sys.version_info < (3, 11) or os.geteuid() == 0:
        return 78
    if sys.argv[1:] == ['--describe-runtime']:
        describe()
        sys.stdout.flush()
        return 0
    if len(sys.argv) != 1:
        return 78
    for key, cap in ((resource.RLIMIT_AS, 96 << 20), (resource.RLIMIT_CPU, 2),
                     (resource.RLIMIT_CORE, 0), (resource.RLIMIT_NOFILE, 64)):
        resource.setrlimit(key, (cap, cap))
    files, libraries, image, arguments = configuration()
    child, info = start(files, libraries, image, arguments)
    return supervise(child, info)


if __name__ == '__main__':
    try:
        code = main()
    except BaseException:
        try:
            os.set_blocking(2, False)
            control(final_record(6, 78, b'', 0, 0, 0))
        except BaseException:
            pass
        code = 78
    os._exit(code)
