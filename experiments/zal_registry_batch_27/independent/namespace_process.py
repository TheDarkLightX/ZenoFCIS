"""Pidfd ownership tracking when procfs and the caller use different PID namespaces.

Pidfd fdinfo identifies the target in the mounted procfs namespace. Signals use
only the fd, never a translated numeric PID. This tracks observed descendants;
it is not a cgroup or a general defense against adversarial process creation.
"""
from __future__ import annotations

import os
from pathlib import Path
import signal
import time
from subprocess import TimeoutExpired


def identity(proc_pid):
    try:
        text = Path(f"/proc/{proc_pid}/stat").read_text()
        fields = text[text.rfind(")") + 2:].split()
        return {"state": fields[0], "ppid": int(fields[1]), "start_ticks": int(fields[19])}
    except (FileNotFoundError, ProcessLookupError, PermissionError, ValueError, IndexError):
        return None


def fd_target(fd):
    fields = dict(line.split(":", 1) for line in Path(f"/proc/self/fdinfo/{fd}").read_text().splitlines())
    return int(fields["Pid"])


def namespace_pid(proc_pid, depth):
    fields = dict(line.split(":", 1) for line in Path(f"/proc/{proc_pid}/status").read_text().splitlines())
    return int(fields["NSpid"].split()[depth - 1])


class Owned:
    def __init__(self, process):
        self.process = process
        self.entries = {}
        self.actions = []
        status = dict(line.split(":", 1) for line in Path("/proc/self/status").read_text().splitlines())
        self.depth = len(status["NSpid"].split())
        parent = int(status["Pid"])
        fd = os.pidfd_open(process.pid, 0)
        try:
            proc_pid = fd_target(fd)
            current = identity(proc_pid)
            if current is None or current["ppid"] != parent or fd_target(fd) != proc_pid:
                raise RuntimeError("cannot bind owned child to procfs")
            self.root = proc_pid
            self.entries[proc_pid] = {"fd": fd, **current, "depth": 0, "namespace_pid": process.pid}
        except BaseException:
            try:
                signal.pidfd_send_signal(fd, signal.SIGKILL)
                process.wait(timeout=2)
            finally:
                os.close(fd)
            raise

    def active(self, proc_pid):
        entry = self.entries[proc_pid]
        current = identity(proc_pid)
        return (fd_target(entry["fd"]) == proc_pid and current is not None
                and current["state"] != "Z" and current["start_ticks"] == entry["start_ticks"])

    def add(self, proc_pid, parent):
        if proc_pid in self.entries or not self.active(parent):
            return
        before = identity(proc_pid)
        if before is None or before["ppid"] != parent:
            return
        try:
            local = namespace_pid(proc_pid, self.depth)
            fd = os.pidfd_open(local, 0)
        except (FileNotFoundError, ProcessLookupError):
            return
        try:
            after = identity(proc_pid)
            if (fd_target(fd) != proc_pid or after is None or after["ppid"] != parent
                    or before["start_ticks"] != after["start_ticks"] or not self.active(parent)):
                os.close(fd)
                return
            self.entries[proc_pid] = {"fd": fd, **after, "namespace_pid": local,
                                      "depth": self.entries[parent]["depth"] + 1}
        except BaseException:
            os.close(fd)
            raise

    def scan(self):
        # Some kernels omit task/*/children. Observe parent links from procfs;
        # add() still requires a live retained parent and an exact pidfd match.
        children = {}
        for path in Path("/proc").iterdir():
            if path.name.isdecimal():
                proc_pid = int(path.name)
                current = identity(proc_pid)
                if current is not None:
                    children.setdefault(current["ppid"], []).append(proc_pid)
        queue = [self.root]
        seen = set()
        while queue:
            parent = queue.pop(0)
            if parent in seen or not self.active(parent):
                continue
            seen.add(parent)
            for child in children.get(parent, []):
                self.add(child, parent)
                if child in self.entries:
                    queue.append(child)

    def send(self, proc_pid, sig):
        if self.active(proc_pid):
            try:
                signal.pidfd_send_signal(self.entries[proc_pid]["fd"], sig)
                self.actions.append({"proc_pid": proc_pid, "signal": sig.name})
            except ProcessLookupError:
                pass

    def finish(self):
        self.scan()
        ordered = sorted(self.entries, key=lambda pid: self.entries[pid]["depth"], reverse=True)
        for sig in (signal.SIGTERM, signal.SIGKILL):
            for pid in ordered:
                self.send(pid, sig)
            deadline = time.monotonic() + 2
            while time.monotonic() < deadline:
                self.process.poll()
                if not any(self.active(pid) for pid in ordered):
                    break
                time.sleep(0.02)
        try:
            self.process.wait(timeout=2)
        except TimeoutExpired:
            pass
        result = {"identities": [{"proc_pid": pid, **{k: v for k, v in entry.items() if k != "fd"}}
                                  for pid, entry in self.entries.items()],
                  "signals": self.actions, "survivors": [pid for pid in ordered if self.active(pid)]}
        for entry in self.entries.values():
            os.close(entry["fd"])
        return result


def wait_owned(process, tracker, timeout):
    deadline = time.monotonic() + timeout
    while process.poll() is None:
        tracker.scan()
        if time.monotonic() >= deadline:
            raise TimeoutError("owned process deadline")
        time.sleep(0.1)
    return process.returncode
