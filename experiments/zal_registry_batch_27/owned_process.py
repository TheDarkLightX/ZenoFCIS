"""Track only descendants of an owned subprocess using PID start times and pidfds."""
import os
from pathlib import Path
import signal
from subprocess import TimeoutExpired
import time


def identity(pid):
    try:
        text = Path(f"/proc/{pid}/stat").read_text()
        fields = text[text.rfind(")") + 2:].split()
        return {"state": fields[0], "ppid": int(fields[1]), "start_ticks": int(fields[19])}
    except (FileNotFoundError, ProcessLookupError, PermissionError, ValueError, IndexError):
        return None


class Owned:
    def __init__(self, process):
        self.process = process
        self.entries = {}
        self.actions = []
        self.add(process.pid, None, 0)
        if process.pid not in self.entries and process.poll() is None:
            raise RuntimeError("could not pin owned process identity")

    def add(self, pid, parent, depth):
        if pid in self.entries:
            return
        before = identity(pid)
        if before is None or (parent is not None and before["ppid"] != parent):
            return
        try:
            fd = os.pidfd_open(pid, 0)
        except ProcessLookupError:
            return
        after = identity(pid)
        if after is None or before["ppid"] != after["ppid"] or before["start_ticks"] != after["start_ticks"]:
            os.close(fd)
            return
        self.entries[pid] = {"fd": fd, "start_ticks": before["start_ticks"], "ppid": before["ppid"], "depth": depth}

    def scan(self):
        queue = [self.process.pid]
        seen = set()
        while queue:
            pid = queue.pop(0)
            if pid in seen or pid not in self.entries:
                continue
            seen.add(pid)
            current = identity(pid)
            if not current or current["start_ticks"] != self.entries[pid]["start_ticks"]:
                continue
            for child_file in Path(f"/proc/{pid}/task").glob("*/children"):
                try:
                    children = [int(x) for x in child_file.read_text().split()]
                except (FileNotFoundError, ProcessLookupError, PermissionError, ValueError):
                    continue
                for child in children:
                    self.add(child, pid, self.entries[pid]["depth"] + 1)
                    if child in self.entries:
                        queue.append(child)

    def send(self, pid, sig):
        entry = self.entries[pid]
        current = identity(pid)
        if current is None or current["start_ticks"] != entry["start_ticks"] or current["state"] == "Z":
            return
        try:
            signal.pidfd_send_signal(entry["fd"], sig)
            self.actions.append({"pid": pid, "start_ticks": entry["start_ticks"], "signal": sig.name})
        except ProcessLookupError:
            pass

    def finish(self):
        self.scan()
        ordered = sorted(self.entries, key=lambda pid: self.entries[pid]["depth"], reverse=True)
        for pid in ordered:
            self.send(pid, signal.SIGTERM)
        deadline = time.monotonic() + 2
        while time.monotonic() < deadline:
            self.process.poll()
            if not any((current := identity(pid)) is not None and current["state"] != "Z" and current["start_ticks"] == self.entries[pid]["start_ticks"] for pid in ordered):
                break
            time.sleep(0.05)
        for pid in ordered:
            self.send(pid, signal.SIGKILL)
        try:
            self.process.wait(timeout=2)
        except TimeoutExpired:
            pass
        deadline = time.monotonic() + 2
        while time.monotonic() < deadline:
            if not any((current := identity(pid)) is not None and current["state"] != "Z" and current["start_ticks"] == self.entries[pid]["start_ticks"] for pid in ordered):
                break
            time.sleep(0.05)
        survivors = [pid for pid in ordered if (current := identity(pid)) is not None and current["state"] != "Z" and current["start_ticks"] == self.entries[pid]["start_ticks"]]
        result = {"identities": [{"pid": pid, "ppid": entry["ppid"], "start_ticks": entry["start_ticks"], "depth": entry["depth"]} for pid, entry in self.entries.items()], "signals": self.actions, "survivors": survivors}
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
