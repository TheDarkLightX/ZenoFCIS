"""Tests for Zeno Commons. Run: python3 -m unittest integrations/commons/test_commons.py"""

from __future__ import annotations

import json
import multiprocessing
import os
import shlex
import sqlite3
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from commons import DB_NAME, Commons, CommonsError, normalize_resource  # noqa: E402
from archive_test_support import ARCHIVE_REFUSAL, is_archive_refusal  # noqa: E402


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
        self.clock.at += 601  # a stopped renewal, a paused process or a clock jump: the lease is stale, not free
        self.assertEqual([(c["owner"], c["state"]) for c in b.lease_check("/tmp/zf-x")], [("builder-a", "stale")])
        with self.assertRaisesRegex(CommonsError, "never released"):
            b.lease_take("/tmp/zf-x/sub")
        taken = b.lease_take("/tmp/zf-x", force_reason="builder-a posted finished, workspace released")
        self.assertEqual(taken["displaced"], [["/tmp/zf-x", "builder-a"]])
        [event] = [e for e in b.events(kind_prefix="lease.take") if e["actor"] == "builder-b"]
        self.assertIn("force_reason", event["body"])
        with self.assertRaises(CommonsError):
            a.lease_release("/tmp/zf-x")  # only the owner releases, unless forced with a reason
        a.lease_release("/tmp/zf-x", force_reason="owner confirmed it finished")
        self.assertEqual(b.leases(), [])
        a.lease_take("/tmp/zf-y", ttl_seconds=600)
        self.clock.at += 601
        self.assertEqual(a.lease_renew("/tmp/zf-y", ttl_seconds=600)["resource"], "/tmp/zf-y")  # the holder renews

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
        a.lease_take("branch:dev/2.6-x", ttl_seconds=60)
        with self.assertRaises(CommonsError):
            a.status("done")
        self.clock.at += 3600
        with self.assertRaises(CommonsError):
            a.status("done")  # a stale lease is still held
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
        self.assertEqual(s.verify(), {"ok": False, "broken_at": 3, "checked": 2, "reason": "hash chain mismatch"})

    def test_archive_keeps_the_newest_events_and_the_whole_history_still_verifies(self):
        s = self.open("lead")
        for i in range(30):
            s.emit("atdd", "result", f"scenario-{i}", {"i": i})
        top = s.events(limit=1000)[-1]["seq"]
        out = s.archive(keep_events=10)
        self.assertEqual((out["first"], out["last"], out["archived"]), (1, top - 10, top - 10))
        self.assertEqual(s.events(limit=1000)[0]["seq"], top - 9)  # the newest ten stay live
        live = s.verify()
        self.assertTrue(live["ok"])
        self.assertEqual(live["from_seq"], top - 9)
        for i in range(30):
            s.emit("atdd", "result", f"later-{i}", {"i": i})
        second = s.archive(keep_events=5)
        self.assertEqual(second["first"], top - 9)
        full = s.verify(archives=True)
        self.assertTrue(full["ok"], full)
        self.assertEqual((full["from_seq"], full["archives"]), (1, 2))
        self.assertEqual(full["checked"], s.events(limit=1000)[-1]["seq"])
        self.assertEqual(s.archive(keep_events=5)["archived"], 1)  # only the previous archive event was left over
        self.assertEqual(s.archive(keep_events=5)["archived"], 1)

    def test_archive_keeps_unread_messages_and_drops_read_ones(self):
        lead, worker, idle = self.open("lead"), self.open("worker"), self.open("idle")
        read = lead.send("worker", "read this")["seq"]
        unread = lead.send("idle", "still waiting")["seq"]
        lead.send("*", "everyone")
        worker.ack(read)
        for i in range(5):
            lead.emit("test", "tick", f"t-{i}", {})
        out = lead.archive(keep_events=0)
        self.assertEqual(out["messages_removed"], 2)
        self.assertEqual([m["seq"] for m in idle.inbox()["messages"]], [unread])
        self.assertEqual(worker.inbox(include_read=True)["messages"], [])

    def test_archive_refuses_a_broken_chain_and_writes_nothing(self):
        s = self.open("lead")
        for i in range(6):
            s.emit("atdd", "result", f"s-{i}", {"exit": 0})
        raw = sqlite3.connect(self.dir / DB_NAME)
        raw.execute("UPDATE events SET body=? WHERE seq=4", (json.dumps({"exit": 1}),))
        raw.commit()
        raw.close()
        with self.assertRaisesRegex(CommonsError, "chain is broken at 4"):
            s.archive(keep_events=1)
        self.assertEqual(list((self.dir / "archive").glob("*.xz")) if (self.dir / "archive").exists() else [], [])
        self.assertEqual(s.storage()["archived_through"], 0)

    def test_a_changed_segment_or_checkpoint_is_detected(self):
        s = self.open("lead")
        for i in range(12):
            s.emit("atdd", "result", f"s-{i}", {"exit": 0})
        out = s.archive(keep_events=3)
        segment = self.dir / "archive" / out["segment"]
        self.assertTrue(s.verify(archives=True)["ok"])
        original = segment.read_bytes()
        segment.write_bytes(original[:-1] + bytes([original[-1] ^ 1]))
        self.assertFalse(s.verify(archives=True)["ok"])
        segment.write_bytes(original)
        raw = sqlite3.connect(self.dir / DB_NAME)
        raw.execute("UPDATE checkpoints SET head=?", ("f" * 64,))
        raw.commit()
        raw.close()
        self.assertFalse(s.verify()["ok"])  # the live chain no longer starts where the checkpoint says

    def test_archive_if_over_does_nothing_within_budget(self):
        s = self.open("lead")
        for i in range(8):
            s.emit("test", "tick", f"t-{i}", {})
        self.assertEqual(s.archive(keep_events=1, if_over=True)["reason"], "within budget")
        out = s.archive(keep_events=1, if_over=True, max_live_events=5)
        self.assertGreater(out["archived"], 0)
        self.assertFalse(out["storage"]["over_budget"])

    def test_record_title_correction_and_size_cap(self):
        s = self.open("lead")
        rid = s.record_add("lesson", "wrong cause named", "first body")["id"]
        s.record_update(rid, title="right cause named", note="the lead deleted it, not the agent")
        record = s.record(rid)
        self.assertEqual(record["title"], "right cause named")
        self.assertIn("title was: wrong cause named", record["body"])
        with self.assertRaises(CommonsError):
            s.record_update(rid, note="x" * (65536 - len(record["body"])))
        self.assertEqual(s.record(rid)["body"], record["body"])

    def test_anchors_detect_a_rewritten_history_and_removed_events(self):
        s = self.open("lead")
        for i in range(6):
            s.emit("atdd", "result", f"s-{i}", {"exit": 0})
        anchors = [s.head()]
        self.assertTrue(s.verify(anchors=anchors)["ok"])
        raw = sqlite3.connect(self.dir / DB_NAME)
        rows = raw.execute("SELECT seq, at, actor, kind, subject, body FROM events ORDER BY seq").fetchall()
        prev = "0" * 64
        import hashlib
        for seq, at, actor, kind, subject, body in rows:  # rewrite every event consistently from the start
            body = json.loads(body)
            if seq == 3:
                body = {"exit": 1}
            text = json.dumps(body, sort_keys=True, separators=(",", ":"))
            digest = hashlib.sha256((prev + json.dumps([at, actor, kind, subject, body], sort_keys=True,
                                                       separators=(",", ":"), ensure_ascii=False)).encode()).hexdigest()
            raw.execute("UPDATE events SET body=?, prev=?, hash=? WHERE seq=?", (text, prev, digest, seq))
            prev = digest
        raw.commit()
        self.assertTrue(s.verify()["ok"])  # the chain alone cannot see a full rewrite
        rewritten = s.verify(anchors=anchors)
        self.assertFalse(rewritten["ok"])
        self.assertIn("rewritten", rewritten.get("reason", ""))
        raw.execute("DELETE FROM events WHERE seq=(SELECT MAX(seq) FROM events)")
        raw.commit()
        raw.close()
        removed = s.verify(anchors=anchors)
        self.assertFalse(removed["ok"])
        self.assertIn("removed", removed.get("reason", ""))

    def test_the_store_refuses_links_and_foreign_schemas_before_writing(self):
        other = Path(self.tmp.name) / "other.sqlite3"
        raw = sqlite3.connect(other)
        raw.execute("CREATE TABLE notes(x)")
        raw.commit()
        raw.close()
        linked = Path(self.tmp.name) / "linked"
        linked.mkdir()
        (linked / DB_NAME).symlink_to(other)
        with self.assertRaisesRegex(CommonsError, "symbolic link"):
            Commons(linked, "lead")
        foreign = Path(self.tmp.name) / "foreign"
        foreign.mkdir()
        raw = sqlite3.connect(foreign / DB_NAME)
        raw.execute("CREATE TABLE notes(x)")
        raw.commit()
        raw.close()
        with self.assertRaisesRegex(CommonsError, "not Commons tables"):
            Commons(foreign, "lead")
        raw = sqlite3.connect(foreign / DB_NAME)
        self.assertEqual(raw.execute("SELECT name FROM sqlite_master").fetchall(), [("notes",)])  # untouched
        raw.close()
        s = self.open("lead")
        s.close()
        raw = sqlite3.connect(self.dir / DB_NAME)
        raw.execute("CREATE TRIGGER t AFTER INSERT ON events BEGIN DELETE FROM records; END")
        raw.commit()
        raw.close()
        with self.assertRaisesRegex(CommonsError, "trigger t"):
            Commons(self.dir, "lead")
        raw = sqlite3.connect(self.dir / DB_NAME)
        raw.execute("DROP TRIGGER t")
        raw.commit()
        raw.close()
        (self.dir / "archive").symlink_to(Path(self.tmp.name))
        with self.assertRaisesRegex(CommonsError, "symbolic link"):
            Commons(self.dir, "lead")

    def test_a_duplicate_gap_folds_into_its_canonical_gap(self):
        first, second = self.open("sec-review"), self.open("zenomail")
        canonical = first.record_add(
            "gap", "Judges cannot see deliveries", "lemmas see state only",
            tags=["judges", "severity-medium"], refs=["log-a"],
        )["id"]
        duplicate = second.record_add(
            "gap", "Lemmas cannot observe deliveries a decision makes", "paid at most once",
            tags=["security", "severity-high"], refs=["log-b"],
        )["id"]
        second.link(duplicate, "duplicates", canonical)
        second.link(duplicate, "duplicates", canonical)
        merged = first.record(canonical)
        self.assertEqual(first.record(duplicate)["status"], "superseded")
        self.assertEqual(merged["tags"], ["judges", "security", "severity-high"])
        self.assertEqual(merged["refs"], sorted(["log-a", "log-b", duplicate]))
        self.assertEqual(merged["body"].count("folded in duplicate"), 1)
        [gap] = first.gaps()
        self.assertEqual(
            (gap["id"], gap["severity"], gap["reporters"]),
            (canonical, "high", ["sec-review", "zenomail"]),
        )
        with self.assertRaises(CommonsError):
            first.link(canonical, "duplicates", duplicate)

    def test_an_imported_gap_counts_its_original_reporter_not_the_importer(self):
        s = self.open("lead")
        log = Path(self.tmp.name) / "gaps.jsonl"
        log.write_text(json.dumps({"agent": "sec-review", "severity": "high", "need": "Lemmas cannot see deliveries"}) + "\n")
        s.import_gaps(log)
        [gap] = s.gaps()
        self.assertEqual(gap["reporters"], ["sec-review"])
        events = len(s.events(limit=1000))
        s.import_gaps(log)
        self.assertEqual(len(s.events(limit=1000)), events)

    def test_gaps_rank_by_severity_then_reporters_then_age(self):
        a, b = self.open("agent-a"), self.open("agent-b")
        low = a.record_add("gap", "Low one", tags=["severity-low"])["id"]
        unrated = a.record_add("gap", "Unrated one")["id"]
        high_old = a.record_add("gap", "High old", tags=["severity-high"])["id"]
        high_new = a.record_add("gap", "High new", tags=["severity-high"])["id"]
        twice = a.record_add("gap", "High twice", tags=["severity-high"])["id"]
        echo = b.record_add("gap", "High twice, again", tags=["severity-medium"])["id"]
        blocker = b.record_add("gap", "Blocker", tags=["severity-blocker"])["id"]
        b.link(echo, "duplicates", twice)
        a.record_update(unrated, severity="medium")
        done = a.record_add("gap", "Already fixed", tags=["severity-blocker"])["id"]
        a.record_update(done, status="resolved")
        order = [gap["id"] for gap in a.gaps()]
        self.assertEqual(order, [blocker, twice, high_old, high_new, unrated, low])
        self.assertIn(done, [gap["id"] for gap in a.gaps(include_closed=True)])

    def test_dedup_proposes_similar_gaps_and_leaves_distinct_ones(self):
        s = self.open("lead")
        older = s.record_add(
            "gap", "Per-request work grows with the store's history", "every request replays history",
        )["id"]
        newer = s.record_add(
            "gap", "Serve state and command in time that does not grow with the store's history",
            "replays every decision on each request",
        )["id"]
        s.record_add("gap", "Show token amounts with decimals", "1.5 tokens at scale e18")
        pairs = s.similar("gap", threshold=0.2)
        self.assertEqual([(pair["older"], pair["newer"]) for pair in pairs], [(older, newer)])

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
        base = shlex.split(command) if command else [sys.executable, "-I", str(HERE / "cli.py")]
        is_rust = bool(command) and ("commons" in base or Path(base[0]).name == "zeno-fcis")
        if is_rust and "commons" not in base:
            base.append("commons")

        def run(*arguments):
            return subprocess.run(
                ["nice", "-n", "19", "ionice", "-c3", *base, "--store", self.store,
                 "--actor", actor, *arguments],
                env=self.env, capture_output=True, text=True, timeout=60)

        if args[0] != "register" and run("whoami").returncode != 0:
            run("register", "--role", "test")
        return run(*args)

    def mcp_command(self):
        command = os.environ.get("ZENO_FCIS_COMMONS_CLI")
        base = shlex.split(command) if command else [sys.executable, "-I", str(HERE / "mcp.py")]
        is_rust = bool(command) and ("commons" in base or Path(base[0]).name == "zeno-fcis")
        if is_rust:
            if "commons" not in base:
                base.append("commons")
            base.append("mcp")
        elif command:
            base = [str(HERE / "mcp.py") if item.endswith("cli.py") else item for item in base]
        return ["nice", "-n", "19", "ionice", "-c3", *base,
                "--store", self.store, "--actor", "agent-1"]

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

    def test_cli_export_events_with_archive_recomputes_the_chain(self):
        for i in range(12):
            self.cli("lead", "emit", "test", "tick", f"t-{i}")
        archived = self.cli("lead", "archive", "--keep", "4")
        if is_archive_refusal(archived):
            self.assertEqual(archived.returncode, 2)
            self.assertEqual(json.loads(archived.stderr)["refused"], ARCHIVE_REFUSAL)
            verify = self.cli("lead", "verify", "--archives")
            export = self.cli("lead", "export", "events", "--with-archive")
            self.assertEqual((verify.returncode, export.returncode), (2, 2))
            self.assertTrue(is_archive_refusal(verify))
            self.assertTrue(is_archive_refusal(export))
            self.skipTest(ARCHIVE_REFUSAL)
        self.assertEqual(archived.returncode, 0, archived.stderr)
        out = Path(self.tmp.name) / "events.jsonl"
        self.assertEqual(self.cli("lead", "export", "events", "--with-archive", "--out", str(out)).returncode, 0)
        prev, seqs = "0" * 64, []
        import hashlib
        for line in out.read_text().splitlines():
            e = json.loads(line)
            body = json.loads(e["body"])
            digest = hashlib.sha256((prev + json.dumps([e["at"], e["actor"], e["kind"], e["subject"], body],
                                                       sort_keys=True, separators=(",", ":"),
                                                       ensure_ascii=False)).encode()).hexdigest()
            self.assertEqual((e["prev"], e["hash"]), (prev, digest))
            prev = digest
            seqs.append(e["seq"])
        self.assertEqual(seqs, list(range(1, len(seqs) + 1)))
        md = self.cli("lead", "export", "records", "--format", "md")
        self.assertEqual(md.returncode, 0)
        self.assertIn("# Zeno Commons records", md.stdout)

    def test_cli_update_takes_status_note_ref_title_and_severity(self):
        added = self.cli("lead", "add", "gap", "A gap", "--tag", "severity-low")
        self.assertEqual(added.returncode, 0, added.stderr)
        record_id = json.loads(added.stdout)["id"]
        updated = self.cli(
            "lead", "update", record_id, "--status", "resolved", "--note", "fixed", "--ref", "abc123",
            "--title", "A gap, retitled", "--severity", "high", "--tag", "triaged",
        )
        self.assertEqual(updated.returncode, 0, updated.stderr)
        shown = json.loads(self.cli("lead", "show", record_id).stdout)
        self.assertEqual(
            (shown["status"], shown["title"], shown["refs"]),
            ("resolved", "A gap, retitled", ["abc123"]),
        )
        self.assertIn("severity-high", shown["tags"])
        self.assertIn("triaged", shown["tags"])
        self.assertEqual(self.cli("lead", "merge", record_id, "--into", record_id).returncode, 2)

    def test_mcp_round_trip(self):
        proc = subprocess.Popen(self.mcp_command(),
                                env=self.env,
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
