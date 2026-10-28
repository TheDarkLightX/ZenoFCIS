"""One trusted worker, single-threaded resource setup before exec; no imported code."""
import os
import resource
import sys


def main():
    worker = sys.argv[1]
    for key, cap in [(resource.RLIMIT_AS, 1024**3), (resource.RLIMIT_CPU, 10),
                     (resource.RLIMIT_FSIZE, 1024**2), (resource.RLIMIT_CORE, 0)]:
        resource.setrlimit(key, (cap, cap))
    os.nice(10)
    os.execve(worker, [worker, '+RTS', '-M512m', '-K16m', '-RTS'],
              {'PATH': '/usr/bin:/bin', 'LANG': 'C.UTF-8', 'LC_ALL': 'C.UTF-8'})


if __name__ == '__main__':
    main()
