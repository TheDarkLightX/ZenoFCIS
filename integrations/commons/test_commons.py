"""Tests for Zeno Commons. Run: python3 -m unittest integrations/commons/test_commons.py"""

from __future__ import annotations

import json
import multiprocessing
import os
import sqlite3
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from commons import DB_NAME, Commons, CommonsError, normalize_resource  # noqa: E402


class Clock:
    def __init__(self, at=1_800_000_000.0):
        self.at = at

    def __call__(self):
        return self.at


def _writer(directory, actor, count):
    store = Commons(directory, actor)
    for i in range(count):
        store.emit("test", "tick", f"{actor}-{i}", {"i": i})
    store.close()


class CommonsTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self.tmp.name) / "store"
        self.clock = Clock()

    def tearDown(self):
        self.tmp.cleanup()

    def open(self, actor, register=True):
        store = Commons(self.dir, actor, clock=self.clock)
        self.addCleanup(store.close)
        if register:
            try:
                store.identity()
            except CommonsError:
                store.register(kind="agent", role="test")
        return store

    def test_identities_have_unique_names_and_hash_ids_and_are_addressable(self):
        a, b = self.open("builder-a"), self.open("builder-b")
        ida, idb = a.identity()["id"], b.identity()["id"]
        self.assertRegex(ida, r"^ag-[0-9a-f]{16}$")
        self.assertNotEqual(ida, idb)
        with self.assertRaises(CommonsError):
            a.register("builder-b")  # names are unique
        sent = a.send(idb, "by hash id")  # address by ID ...
        self.assertEqual(sent["to"], "builder-b")
        a.send("builder-b", "by name")    # ... or by name
        self.assertEqual(len(b.inbox()["messages"]), 2)
        with self.assertRaises(CommonsError):
            a.send("nobody", "unknown recipient")
        listed = {row["agent"]: row["id"] for row in a.agents()}
        self.assertEqual(listed, {"builder-a": ida, "builder-b": idb})

    def test_unregistered_actors_cannot_coordinate_but_tools_can_emit(self):
        ghost = self.open("ghost", register=False)
        for act in (lambda: ghost.lease_take("/tmp/work-ghost"), lambda: ghost.status("working"),
                    lambda: ghost.record_add("lesson", "x"), lambda: ghost.ack(0)):
            with self.assertRaises(CommonsError):
                act()
        self.open("lead")
        with self.assertRaises(CommonsError):
            ghost.send("lead", "hello")
        self.assertGreater(ghost.emit("atdd", "result", "scenario-x", {"exit": 0}), 0)

    def test_concurrent_writers_lose_nothing_and_sequence_only_grows(self):
        Commons(self.dir, "setup").close()
        procs = [multiprocessing.Process(target=_writer, args=(self.dir, f"w{n}", 60)) for n in range(8)]
        for p in procs:
            p.start()
        for p in procs:
            p.join(120)
            self.assertEqual(p.exitcode, 0)
        store = self.open("reader", register=False)
        events = store.events(limit=10000)
        self.assertEqual(len(events), 480)
        self.assertEqual([e["seq"] for e in events], list(range(1, 481)))
        per_actor = {}
        for e in events:
            per_actor.setdefault(e["actor"], []).append(e["body"]["i"])
        self.assertEqual(sorted(per_actor), [f"w{n}" for n in range(8)])
        for values in per_actor.values():
            self.assertEqual(values, list(range(60)))  # each writer's own events keep their order
        self.assertTrue(store.verify()["ok"])

    def test_lease_conflict_expiry_renew_release(self):
        a, b = self.open("builder-a"), self.open("builder-b")
        a.lease_take("/tmp/zf-x", ttl_seconds=600)
        with self.assertRaises(CommonsError):
            b.lease_take("/tmp/zf-x")
        a.lease_renew("/tmp/zf-x", ttl_seconds=600)
        self.clock.at += 601
        self.assertEqual(b.lease_check("/tmp/zf-x"), [])  # expired leases free the resource
        b.lease_take("/tmp/zf-x")
        with self.assertRaises(CommonsError):
            a.lease_release("/tmp/zf-x")  # only the owner releases, unless forced with a reason
        a.lease_release("/tmp/zf-x", force_reason="owner confirmed it finished")
        self.assertEqual(b.leases(), [])

    def test_lease_on_a_folder_protects_its_parent_and_children(self):
        a, lead = self.open("reviewer"), self.open("lead")
        a.lease_take("/tmp/work-sec/source")
        self.assertTrue(lead.lease_check("/tmp/work-sec"))           # deleting the parent touches the lease
        self.assertTrue(lead.lease_check("/tmp/work-sec/source/x"))  # so does a path inside it
        self.assertEqual(lead.lease_check("/tmp/work-secondary"), [])  # a sibling with a shared prefix does not
        with self.assertRaises(CommonsError):
            lead.lease_take("/tmp/work-sec")

    def test_done_requires_released_leases(self):
        a = self.open("builder")
        a.lease_take("branch:dev/2.6-x")
        with self.assertRaises(CommonsError):
            a.status("done")
        a.lease_release("branch:dev/2.6-x")
        self.assertEqual(a.status("done")["state"], "done")
        with self.assertRaises(CommonsError):
            a.status("blocked")  # blocked needs its question

    def test_messages_and_read_receipts_only_move_forward(self):
        lead, builder = self.open("lead"), self.open("builder")
        first = lead.send("builder", "use option A")["seq"]
        second = lead.send("*", "freeze moved to Friday")["seq"]
        self.assertEqual([m["seq"] for m in builder.inbox()["messages"]], [first, second])
        builder.ack(second)
        builder.ack(first)  # an older acknowledgement never moves the receipt back
        self.assertEqual(builder.inbox()["read_through"], second)
        self.assertEqual(builder.inbox()["messages"], [])
        self.assertEqual(lead.inbox()["messages"], [])  # senders do not receive their own broadcasts
        with self.assertRaises(CommonsError):
            builder.ack(10_000)

    def test_records_links_and_supersession(self):
        s = self.open("builder")
        h = s.record_add("hypothesis", "Adaptive indexing halves search work", tags=["synthesis"])["id"]
        r = s.record_add("refutation", "No gain below 300k comparisons", refs=["evidence/REPORT.md"])["id"]
        s.link(r, "refutes", h)
        s.record_update(h, status="refuted", note="see refutation", refs=[r])
        shown = s.record(h)
        self.assertEqual(shown["status"], "refuted")
        self.assertIn({"src": r, "rel": "refutes", "dst": h}, shown["links"])
        new = s.record_add("lesson", "Create RAM-disk roots before launch", tags=["codex-launch"])["id"]
        old = s.record_add("lesson", "Launch builders directly", tags=["codex-launch"])["id"]
        s.link(new, "supersedes", old)
        self.assertEqual(s.record(old)["status"], "superseded")

    def test_advise_returns_negative_memory_for_the_action(self):
        s = self.open("lead")
        s.record_add("lesson", "Create the RAM-disk root before launching a sandboxed builder",
                      body="Otherwise each command gets a private /dev/shm and rebuilds.", tags=["codex-launch", "ramdisk"])
        s.record_add("lesson", "Never run cargo fmt --all in a lane worktree", tags=["cargo"])
        s.record_add("hypothesis", "Unrelated claim about launch timing")  # hypotheses are not advice
        builder = self.open("builder")
        builder.lease_take("/tmp/work-ui")
        advice = s.advise("launch a sandboxed builder", tags=["codex-launch"], text="/tmp/work-ui cleanup")
        titles = [a["title"] for a in advice["advice"]]
        self.assertEqual(titles[0], "Create the RAM-disk root before launching a sandboxed builder")
        self.assertNotIn("Unrelated claim about launch timing", titles)
        self.assertEqual([l["owner"] for l in advice["leases"]], ["builder"])

    def test_superseded_lessons_are_not_advice(self):
        s = self.open("lead")
        old = s.record_add("lesson", "Run the sweep on the RAM disk", tags=["sweep"])["id"]
        s.record_update(old, status="superseded")
        self.assertEqual(s.advise("run the sweep", tags=["sweep"])["advice"], [])

    def test_import_is_idempotent_and_writes_no_events_the_second_time(self):
        log = Path(self.tmp.name) / "gaps.jsonl"
        log.write_text("\n".join([
            json.dumps({"schema": "zenofcis-feature-gap/1"}),
            json.dumps({"agent": "pilot-a", "severity": "blocker", "need": "A mailbox holding many messages",
                        "what_zenofcis_lacks": "collections"}),
            json.dumps({"agent": "pilot-b", "severity": "high", "need": "u128 amounts in the UI"})]) + "\n")
        s = self.open("lead")
        self.assertEqual(s.import_gaps(log), {"added": 2, "existing": 0})
        before = len(s.events(limit=10000))
        self.assertEqual(s.import_gaps(log), {"added": 0, "existing": 2})
        self.assertEqual(len(s.events(limit=10000)), before)
        self.assertEqual(len(s.records(type="gap", tag="severity-blocker")), 1)

    def test_tampering_with_history_is_detected(self):
        s = self.open("lead")
        for i in range(5):
            s.emit("atdd", "result", f"scenario-{i}", {"exit": 0})
        self.assertTrue(s.verify()["ok"])
        raw = sqlite3.connect(self.dir / DB_NAME)
        raw.execute("UPDATE events SET body=? WHERE seq=3", (json.dumps({"exit": 1}),))
        raw.commit()
        raw.close()
        self.assertEqual(s.verify(), {"ok": False, "broken_at": 3, "checked": 2})

    def test_refusals(self):
        s = self.open("lead")
        for bad in (lambda: s.record_add("opinion", "x"), lambda: s.record_add("lesson", "  "),
                    lambda: s.status("sleeping"), lambda: s.link("a", "likes", "b"),
                    lambda: s.lease_take("/tmp/x", ttl_seconds=1), lambda: s.send("bad name!", "hi")):
            with self.assertRaises(CommonsError):
                bad()
        with self.assertRaises(CommonsError):
            Commons(self.dir, "bad actor name")

    def test_paths_normalize(self):
        self.assertEqual(normalize_resource("/tmp/a/../b/"), "/tmp/b")
        self.assertEqual(normalize_resource("branch:dev/2.6"), "branch:dev/2.6")


class InterfaceTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.store = str(Path(self.tmp.name) / "store")
        self.env = {**os.environ, "ZENO_FCIS_COMMONS": self.store}

    def tearDown(self):
        self.tmp.cleanup()

    def cli(self, actor, *args):
        command = os.environ.get("ZENO_FCIS_COMMONS_CLI")
        base = command.split() if command else [sys.executable, "-I", str(HERE / "cli.py")]
        run = lambda *a: subprocess.run([*base, "--actor", actor, *a], env=self.env, capture_output=True, text=True, timeout=60)
        if args[0] != "register" and run("whoami").returncode != 0:
            run("register", "--role", "test")
        return run(*args)

    def test_cli_brief_advice_is_one_line_per_item(self):
        self.cli("lead", "add", "lesson", "Lease folders before cleanup", "--tag", "cleanup")
        out = json.loads(self.cli("builder", "advise", "cleanup of old folders", "--tag", "cleanup", "--brief").stdout)
        self.assertEqual(len(out["advice"]), 1)
        self.assertRegex(out["advice"][0], r"^les-[0-9a-f]{12} lesson score=\d+ Lease folders before cleanup$")
        self.assertIsInstance(out["unread"], int)

    def test_cli_register_and_whoami(self):
        first = self.cli("builder", "register", "--kind", "builder", "--role", "collections")
        self.assertEqual(first.returncode, 0)
        ident = json.loads(first.stdout)
        self.assertEqual(json.loads(self.cli("lead", "whoami", ident["id"]).stdout)["name"], "builder")
        self.assertEqual(self.cli("builder", "register").returncode, 2)

    def test_cli_lease_check_exit_code_and_refusal(self):
        self.assertEqual(self.cli("builder", "lease", "take", "/tmp/work-x").returncode, 0)
        self.assertEqual(self.cli("lead", "lease", "check", "/tmp").returncode, 3)
        self.assertEqual(self.cli("lead", "lease", "check", "/tmp/work-y").returncode, 0)
        refused = self.cli("lead", "lease", "take", "/tmp/work-x/source")
        self.assertEqual(refused.returncode, 2)
        self.assertIn("leased by builder", refused.stderr)

    def test_mcp_round_trip(self):
        proc = subprocess.Popen([sys.executable, "-I", str(HERE / "mcp.py"), "--store", self.store, "--actor", "agent-1"],
                                stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        requests = [
            {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}},
            {"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}},
            {"jsonrpc": "2.0", "id": 20, "method": "tools/call", "params": {"name": "commons_register", "arguments": {"role": "test"}}},
            {"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "commons_record_add", "arguments": {
                "type": "gap", "title": "No keyed collections", "tags": ["collections"]}}},
            {"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "commons_advise", "arguments": {
                "action": "build a mailbox", "tags": ["collections"]}}},
            {"jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": {"name": "commons_status", "arguments": {"state": "napping"}}},
        ]
        out, _ = proc.communicate("".join(json.dumps(r) + "\n" for r in requests), timeout=60)
        replies = [json.loads(line) for line in out.splitlines()]
        self.assertEqual(replies[0]["result"]["serverInfo"]["name"], "zenofcis-commons")
        self.assertIn("commons_advise", [t["name"] for t in replies[1]["result"]["tools"]])
        self.assertRegex(json.loads(replies[2]["result"]["content"][0]["text"])["id"], r"^ag-")
        advice = json.loads(replies[4]["result"]["content"][0]["text"])["advice"]
        self.assertEqual(advice[0]["title"], "No keyed collections")
        self.assertTrue(replies[5]["result"]["isError"])

    def test_emit_hook_is_a_no_op_without_a_store_and_never_raises(self):
        code = ("import sys; sys.path.insert(0, %r); from emit import emit; "
                "print(emit('zenoui', 'law-refused', 'L7', {'app': 'swap'}))") % str(HERE)
        env = {k: v for k, v in os.environ.items() if k != "ZENO_FCIS_COMMONS"}
        self.assertEqual(subprocess.run([sys.executable, "-I", "-c", code], env=env, capture_output=True,
                                        text=True).stdout.strip(), "None")
        broken = subprocess.run([sys.executable, "-I", "-c", code], env={**env, "ZENO_FCIS_COMMONS": "/proc/no-such"},
                                capture_output=True, text=True)
        self.assertEqual(broken.returncode, 0)
        self.assertIn("event not recorded", broken.stderr)
        ok = subprocess.run([sys.executable, "-I", "-c", code], env=self.env, capture_output=True, text=True)
        self.assertEqual(ok.stdout.strip(), "1")


if __name__ == "__main__":
    unittest.main()
