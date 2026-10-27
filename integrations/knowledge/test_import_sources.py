"""Bounded offline source curation tests. The database fixture is never writable through import."""
import hashlib
import json
from pathlib import Path
import sqlite3
import tempfile
import unittest
import import_sources as I


class ImportSources(unittest.TestCase):
    def database(self, root):
        path=Path(root)/"commons.sqlite3"
        c=sqlite3.connect(path)
        c.executescript("CREATE TABLE records(id TEXT,type TEXT,body TEXT,author TEXT,updated_seq INTEGER); CREATE TABLE events(seq INTEGER,actor TEXT,kind TEXT,subject TEXT,body TEXT,hash TEXT);")
        return path,c

    def test_repeated_record_metadata_does_not_invent_independence(self):
        with tempfile.TemporaryDirectory() as root:
            path,c=self.database(root)
            body={"scope":{"family":[" counter "]},"action":{"skip":["canonical"],"order":[]},"effect":"helps","subject_sha256":"a"*64}
            for key,revision in (("one",1),("two",999)):
                c.execute("INSERT INTO records VALUES(?,?,?,?,?)",(key,"lesson",json.dumps(body),"contributor",revision))
            c.commit();c.close()
            before=hashlib.sha256(path.read_bytes()).hexdigest();rows=I.commons(path)
            self.assertEqual(len(rows),2);self.assertEqual(rows[0]["families"],rows[1]["families"])
            self.assertEqual(rows[0]["scope"],{"family":["counter"]});self.assertEqual(rows[0]["claim"],rows[1]["claim"])
            self.assertEqual(before,hashlib.sha256(path.read_bytes()).hexdigest())

    def test_failure_mining_needs_distinct_subjects_and_deduplicates_families(self):
        with tempfile.TemporaryDirectory() as root:
            path,c=self.database(root)
            for seq,subject in ((1,"a"*64),(2,"a"*64),(3,"b"*64)):
                body={"door":"pipeline","problem_class":"counter","stopping_bound":"work-budget","subject_sha256":subject,"session":"run","tool_version":"v","environment":"sandbox"}
                c.execute("INSERT INTO events VALUES(?,?,?,?,?,?)",(seq,"contributor","factory-run",subject,json.dumps(body),str(seq)*64))
            c.commit();c.close();rows=I.commons(path)
            self.assertEqual(len(rows),1);self.assertEqual(len(rows[0]["families"]),2)
            self.assertEqual(rows[0]["effect"],"limit");self.assertEqual(rows[0]["state"],"proposed")
            self.assertEqual(len(rows[0]["evidence_refs"]),2)

    def test_invalid_scope_is_not_broadened(self):
        with self.assertRaisesRegex(ValueError, "scope-format"):
            I.candidate({"scope":{"undeclared":["counter"]}}, "a"*64, "source", {})

    def test_long_statements_do_not_merge_on_a_truncated_prefix(self):
        prefix = "x" * 4096
        self.assertNotEqual(I.prose_claim(prefix + "a"), I.prose_claim(prefix + "b"))

    def test_prose_does_not_supply_scope_or_action(self):
        with tempfile.TemporaryDirectory() as root:
            path,c=self.database(root);c.execute("INSERT INTO records VALUES(?,?,?,?,?)",("one","hypothesis","use the fastest family everywhere","contributor",1));c.commit();c.close()
            row=I.commons(path)[0];self.assertEqual(row["scope"],{});self.assertEqual(row["action"],{"skip":[],"order":[]});self.assertEqual(row["families"][0]["subject"],"unknown")


if __name__=="__main__":unittest.main()
