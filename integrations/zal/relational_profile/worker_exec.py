#!/usr/bin/env python3
"""Single-threaded resource setup before replacing this process with the worker."""
import os
import resource
import sys


def cap(which, maximum):
    soft, hard = resource.getrlimit(which)
    soft = maximum if soft == resource.RLIM_INFINITY else min(soft, maximum)
    hard = maximum if hard == resource.RLIM_INFINITY else min(hard, maximum)
    resource.setrlimit(which, (min(soft, hard), hard))


def main():
    if len(sys.argv) != 3:
        raise SystemExit('expected worker path and positive CPU seconds')
    worker, raw_seconds = sys.argv[1:]
    seconds = int(raw_seconds)
    if not 1 <= seconds <= 60:
        raise SystemExit('CPU seconds outside 1..60')
    cap(resource.RLIMIT_AS, 2 * 1024**3)
    cap(resource.RLIMIT_CPU, seconds)
    cap(resource.RLIMIT_FSIZE, 1024**2)
    cap(resource.RLIMIT_CORE, 0)
    os.nice(10)
    os.execve(worker, [worker, '+RTS', '-M1536m', '-K32m', '-RTS'],
              {'PATH': '/usr/bin:/bin', 'LANG': 'C.UTF-8', 'LC_ALL': 'C.UTF-8'})


if __name__ == '__main__':
    main()
