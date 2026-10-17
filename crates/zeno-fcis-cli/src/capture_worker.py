"""Interpreter execution adapter, LLM-generated; no candidate judging here.

Run only from the Rust capture shell inside its existing contained unit.
Owner source is compiled only after the kernel policy is installed. Source and
input points arrive on stdin; stderr is never relayed to a proposer.
"""
import ctypes
import errno
import json
import math
import os
import resource
import sys


def contain():
    if sys.version_info < (3, 11) or sys.platform != "linux" or os.uname().machine != "x86_64":
        raise RuntimeError("unsupported-runtime")
    # Close ALL inherited non-protocol descriptors before submitted code, even
    # descriptors above the new NOFILE cap. Do not inherit a control socket.
    os.closerange(3, 2147483647)
    for key, limit in ((resource.RLIMIT_CPU, 2), (resource.RLIMIT_AS, 48 << 20),
                       (resource.RLIMIT_FSIZE, 65536), (resource.RLIMIT_CORE, 0),
                       (resource.RLIMIT_NOFILE, 32)):
        resource.setrlimit(key, (limit, limit))
    libc = ctypes.CDLL(None, use_errno=True)
    libc.prctl.argtypes = [ctypes.c_int] + [ctypes.c_ulong] * 4
    libc.prctl.restype = ctypes.c_int
    libc.syscall.restype = ctypes.c_long
    sec = ctypes.CDLL("libseccomp.so.2", use_errno=True)
    sec.seccomp_init.argtypes = [ctypes.c_uint32]
    sec.seccomp_init.restype = ctypes.c_void_p
    sec.seccomp_syscall_resolve_name.argtypes = [ctypes.c_char_p]
    sec.seccomp_syscall_resolve_name.restype = ctypes.c_int
    sec.seccomp_rule_add.argtypes = [ctypes.c_void_p, ctypes.c_uint32, ctypes.c_int, ctypes.c_uint]
    sec.seccomp_rule_add.restype = ctypes.c_int
    sec.seccomp_load.argtypes = [ctypes.c_void_p]
    sec.seccomp_load.restype = ctypes.c_int
    sec.seccomp_release.argtypes = [ctypes.c_void_p]
    if libc.prctl(38, 1, 0, 0, 0) or libc.prctl(4, 0, 0, 0, 0):
        raise RuntimeError("containment-prctl")
    if libc.syscall(444, 0, 0, 1) < 3:
        raise RuntimeError("containment-landlock")

    class Ruleset(ctypes.Structure):
        _fields_ = [("handled", ctypes.c_uint64)]

    class PathRule(ctypes.Structure):
        _pack_ = 1
        _fields_ = [("allowed", ctypes.c_uint64), ("fd", ctypes.c_int32)]

    # Filesystem rights through ABI 3 TRUNCATE. Newer device IOCTL rights
    # remain denied by seccomp. No new files, links or directories are allowed.
    attrs = Ruleset((1 << 15) - 1)
    ruleset = libc.syscall(444, ctypes.byref(attrs), ctypes.sizeof(attrs), 0)
    if ruleset < 0:
        raise RuntimeError("containment-landlock-create")
    try:
        for path, access in ((".", (1 << 2) | (1 << 3)),
                             ("scratch.txt", (1 << 1) | (1 << 2) | (1 << 14))):
            fd = os.open(path, os.O_PATH | os.O_CLOEXEC)
            try:
                rule = PathRule(access, fd)
                if libc.syscall(445, ruleset, 1, ctypes.byref(rule), 0):
                    raise RuntimeError("containment-landlock-rule")
            finally:
                os.close(fd)
        if libc.syscall(446, ruleset, 0):
            raise RuntimeError("containment-landlock-restrict")
    finally:
        os.close(ruleset)
    ctx = sec.seccomp_init(0x00050000 | errno.EPERM)
    if not ctx:
        raise RuntimeError("containment-seccomp-init")
    try:
        # Default deny: all sockets, clone/fork, exec, ptrace/process_vm,
        # signaling other processes, fcntl, ioctl, mounts, bpf and io_uring.
        names = ("read write close fstat newfstatat statx lseek openat getdents64 "
                 "mmap mprotect munmap brk mremap madvise rt_sigaction rt_sigprocmask "
                 "rt_sigreturn sigaltstack futex clock_gettime clock_nanosleep nanosleep "
                 "getpid gettid getuid geteuid getgid getegid getrandom getcwd "
                 "ftruncate fsync fdatasync exit exit_group").split()
        for name in names:
            number = sec.seccomp_syscall_resolve_name(name.encode("ascii"))
            if number < 0 or sec.seccomp_rule_add(ctx, 0x7fff0000, number, 0):
                raise RuntimeError("containment-seccomp-rule")
        if sec.seccomp_load(ctx):
            raise RuntimeError("containment-seccomp-load")
    finally:
        sec.seccomp_release(ctx)


def main():
    request = json.loads(sys.stdin.buffer.read((2 << 20) + 1))
    try:
        contain()
    except BaseException:
        os._exit(78)
    # fd 2 is a bootstrap-only marker channel. Close it before compiling owner
    # code. Seccomp denies descriptor duplication; opening scratch cannot reach
    # this closed socket. Exit codes alone are never containment evidence.
    os.write(2, b"zeno-capture-contained-v1\n")
    os.close(2)
    namespace = {"__name__": "capture_submission", "math": math}
    exec(compile(request["source"], "<submission>", "exec"), namespace)
    result = [namespace["reference"](tuple(point)) for point in request["points"]]
    print(json.dumps({"rows": result}, ensure_ascii=True, allow_nan=False))


if __name__ == "__main__":
    try:
        main()
    except BaseException:
        os._exit(70)
