#!/usr/bin/env python3
"""Independently observe controlled native-worker startup death cuts.

Each invocation is a fresh, single-purpose subreaper. Pauses are injected into
an isolated bootstrap copy; product code has no fault-injection switch. Every
signal uses a pidfd pinned to an observed owned descendant. Passing these cuts
does not qualify bubblewrap's internal forks or an installed account.
"""
import argparse
import ctypes
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import time

from check_native_worker import configuration, runtime_rows, sha


def identity(pid):
    try:
        fields = Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()
        command = Path(f'/proc/{pid}/cmdline').read_bytes().split(b'\0')[:2]
        return {'pid': pid, 'start': int(fields[19]), 'state': fields[0],
                'ppid': int(fields[1]), 'command': [s.decode(errors='replace') for s in command]}
    except (FileNotFoundError, ProcessLookupError):
        return None


def children(pid):
    found = set()
    try:
        tasks = list(Path(f'/proc/{pid}/task').iterdir())
    except (FileNotFoundError, ProcessLookupError):
        return found
    for task in tasks:
        try:
            found.update(map(int, (task / 'children').read_text().split()))
        except (FileNotFoundError, ProcessLookupError):
            pass
    return found


def check(args):
    libc = ctypes.CDLL(None, use_errno=True)
    if libc.prctl(36, 1, 0, 0, 0):  # PR_SET_CHILD_SUBREAPER
        raise RuntimeError('startup controller cannot become a subreaper')
    rows = runtime_rows(args.pins.read_text())
    source = args.source.read_text()
    anchor = 'preexec_fn=lambda: bind_parent(expected_parent)'
    if source.count(anchor) != 1:
        raise RuntimeError('startup binding anchor changed')
    pause = 'os.kill(os.getpid(), 19)'
    if args.cut == 'during-link':
        needle = 'libc.prctl(1, 9, 0, 0, 0)'
        if source.count(needle) != 1:
            raise RuntimeError('parent-death syscall anchor changed')
        injected = source.replace(needle, f'({pause}, {needle})[1]')
    else:
        steps = {'before-link': f'{pause}, bind_parent(expected_parent)',
                 'after-link': f'bind_parent(expected_parent), {pause}',
                 'unlinked-control': pause}[args.cut]
        injected = source.replace(anchor, 'preexec_fn=lambda: (' + steps + ')')
    args.work.mkdir(parents=True, exist_ok=False)
    (args.work / 'bootstrap.py').write_text(injected)
    image = args.image.read_bytes()
    peer, endpoint = socket.socketpair()
    status, control = socket.socketpair()
    peer.settimeout(3)
    report = {'cut': args.cut, 'optimized': args.optimized,
              'source_sha256': sha(source.encode()), 'injected_sha256': sha(injected.encode()),
              'pins_sha256': sha(args.pins.read_bytes()), 'image_sha256': sha(image)}
    pinned = {}

    def collect():
        todo, seen, live = list(children(os.getpid())), set(), []
        while todo:
            pid = todo.pop()
            if pid in seen:
                continue
            seen.add(pid)
            item = identity(pid)
            if item is None:
                continue
            key = (pid, item['start'])
            if key not in pinned:
                try:
                    fd = os.pidfd_open(pid)
                except ProcessLookupError:
                    continue
                after = identity(pid)
                if after is None or after['start'] != item['start']:
                    os.close(fd)
                    continue
                pinned[key] = fd
            live.append(item)
            todo.extend(children(pid))
        return live

    def send(item, sig):
        signal.pidfd_send_signal(pinned[(item['pid'], item['start'])], sig)

    try:
        command = ['/usr/bin/python3', '-I', '-S', '-B', *(['-O'] if args.optimized else []), '-c', injected]
        guardian = subprocess.Popen(command, stdin=endpoint, stdout=endpoint, stderr=control,
                                    close_fds=True, env={}, cwd='/')
        endpoint.close()
        control.close()
        peer.sendall(configuration(rows, image, ['sleep', '/unavailable-canary']))
        deadline = time.monotonic() + 4
        stopped = None
        while time.monotonic() < deadline:
            items = collect()
            stopped = next((p for p in items if p['ppid'] == guardian.pid and p['state'] in ('T', 't')), None)
            if stopped:
                break
            if guardian.poll() is not None:
                raise RuntimeError('guardian exited before controlled startup cut')
            time.sleep(0.01)
        if stopped is None:
            raise RuntimeError('owned pre-exec child did not stop')
        report['before'] = items
        send(next(p for p in items if p['pid'] == guardian.pid), signal.SIGKILL)
        report['guardian_exit'] = guardian.wait(timeout=2)
        try:
            send(stopped, signal.SIGCONT)
        except ProcessLookupError:
            pass
        started = time.monotonic()
        time.sleep(6)  # Observe beyond the worker's five-second supervision budget.
        live = collect()
        report['observed_seconds'] = time.monotonic() - started
        report['after'] = live
        report['living'] = [p for p in live if p['state'] != 'Z']
        escaped = any(p['command'] and p['command'][0] == '/worker' for p in report['living'])
        if args.cut == 'unlinked-control':
            if not escaped:
                raise RuntimeError('unlinked control did not reach the semantic orphan assertion')
        elif report['living']:
            raise RuntimeError('owned startup descendant survived guardian death')
        report['verdict'] = 'control-caught' if args.cut == 'unlinked-control' else 'passed'
    except BaseException as error:
        report['error'] = f'{type(error).__name__}: {error}'
        raise
    finally:
        for _ in range(3):
            for item in reversed(collect()):
                if item['state'] != 'Z':
                    try:
                        send(item, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
            time.sleep(0.04)
            while True:
                try:
                    pid, _ = os.waitpid(-1, os.WNOHANG)
                    if pid == 0:
                        break
                except ChildProcessError:
                    break
        report['remaining_owned'] = collect()
        for fd in pinned.values():
            os.close(fd)
        for stream in (peer, endpoint, status, control):
            stream.close()
        (args.work / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
    if report['remaining_owned']:
        raise RuntimeError('owned descendants remain after startup probe cleanup')
    print(json.dumps({'cut': args.cut, 'optimized': args.optimized, 'verdict': report['verdict'],
                      'source_sha256': report['source_sha256'], 'remaining_owned': 0}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for option in ('source', 'pins', 'image', 'work'):
        parser.add_argument('--' + option, type=Path, required=True)
    parser.add_argument('--cut', choices=('before-link', 'during-link', 'after-link', 'unlinked-control'), required=True)
    parser.add_argument('--optimized', action='store_true')
    check(parser.parse_args())
