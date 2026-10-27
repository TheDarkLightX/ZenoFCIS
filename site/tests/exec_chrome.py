#!/usr/bin/env python3
"""Exec the browser with only standard IO and DevTools descriptors 3 and 4."""
import os
import resource
import sys


def main():
    executable, *arguments = sys.argv[1:]
    for descriptor in range(5):
        os.fstat(descriptor)
        os.set_inheritable(descriptor, True)
    limit = resource.getrlimit(resource.RLIMIT_NOFILE)[1]
    if limit == resource.RLIM_INFINITY:
        limit = os.sysconf('SC_OPEN_MAX')
    os.closerange(5, limit)
    os.execvp(executable, [executable, *arguments])


if __name__ == '__main__':
    main()
