"""Portable desk controls: a page that runs ZAL itself gets the loopback desk's answers through one text exchange,
and the exchange writes nothing and reaches nothing. No browser, network or provider is used."""

import hashlib
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest import mock

import io
import re
import shutil
import struct
import zipfile
import zlib

import portable_desk as portable
import portable_desk_build as build
import review_desk as desk
from behavior import Refusal
from test_review_desk import LIMIT, OVERFLOWS, REFUND, TOUR

HERE = Path(__file__).resolve().parent
EXAMPLES = ("order.zal", "order2.zal", "keeper2.zal", "context_free2.zal", "withdrawal_game2.zal", "ladder_symmetry2.zal")
BROKEN = LIMIT.replace("rule take:", "rule take now:")
UNFIRED = LIMIT.replace("rule lock_it:", "rule unfired: locked + withdraw [used > 3] -> accept locked;\nrule lock_it:")
SOURCES = {**{name: (HERE / "examples" / name).read_text(encoding="ascii") for name in EXAMPLES},
           "limit.zal": LIMIT, "refund.zal": REFUND, "tour.zal": TOUR}
plain = lambda value: json.loads(json.dumps(value))      # as the page receives it


class Exchange(unittest.TestCase):
    def setUp(self):
        self.desk = portable.Desk()

    def ask(self, **request):
        return json.loads(self.desk.exchange(json.dumps(request)))

    def opened(self, text, name="limit.zal"):
        told = self.ask(route="open", name=name, source=text)
        self.assertTrue(told["ok"], told)
        return told["reply"]

    def loopback(self, text, name="limit.zal"):
        """The loopback desk's own object for the same text, read from a file as `serve` reads it."""
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        path = Path(folder.name) / name
        path.write_bytes(text.encode("utf-8", "surrogatepass"))
        return desk.OpenStatement(path)

    def test_an_opened_statement_is_the_loopback_desks_bundle_without_the_table_and_the_edits(self):
        for name, text in SOURCES.items():
            with self.subTest(source=name):
                full = desk.statement_bundle(text, name)[0]
                self.assertEqual(full["decisions"]["status"], "complete")
                expected = {**full, "edits": None, "cross_checks": {**full["cross_checks"], "tuples": None},
                            "decisions": {"status": "not-precomputed", "reason": "this page asks ZAL about each input as it is tried"}}
                told = self.opened(text, name)
                self.assertEqual(told["bundle"], plain(expected))
                self.assertEqual(told["bundle"]["check"]["authority"], "none")
                self.assertEqual((told["digest"], told["edits_follow"]), (portable.digest_of(told["bundle"]), full["check"]["status"] in desk.PASSING))

    def test_planted_edits_on_request_are_the_loopback_desks(self):
        for name in ("limit.zal", "keeper2.zal", "order.zal"):
            with self.subTest(source=name):
                full = desk.statement_bundle(SOURCES[name], name)[0]
                revision = self.opened(SOURCES[name], name)["bundle"]["revision"]
                told = self.ask(route="edits", revision=revision)
                self.assertEqual((told["ok"], told["reply"]), (True, plain(full["edits"])))
                self.assertGreater(len(told["reply"]["rows"]), 0)
        failing = self.opened(OVERFLOWS)
        self.assertEqual((failing["bundle"]["check"]["status"], failing["edits_follow"]), ("counterexample", False))
        self.assertEqual(self.ask(route="edits", revision=failing["bundle"]["revision"]), {"ok": True, "reply": None})

    def test_an_input_and_a_candidate_are_answered_as_the_loopback_desk_answers_them(self):
        revision = self.opened(LIMIT)["bundle"]["revision"]
        served = self.loopback(LIMIT)
        for given in desk.input_tuples(served.model, served.profile):
            expected = served.act("explain", {"revision": revision, "input": given})
            self.assertEqual(self.ask(route="explain", revision=revision, input=given), {"ok": True, "reply": plain(expected)})
        self.assertTrue(expected["sentence"].startswith("From ") and expected["nearest"]["want"] in ("Accept", "Reject"))
        for candidate in (LIMIT, OVERFLOWS, BROKEN, SOURCES["order.zal"]):
            expected = served.act("candidate", {"revision": revision, "source": candidate})
            self.assertEqual(self.ask(route="candidate", revision=revision, source=candidate), {"ok": True, "reply": plain(expected)})
        self.assertEqual(self.opened(LIMIT)["bundle"]["revision"], revision)       # a candidate replaced nothing

    def test_a_source_zal_refuses_gets_the_located_diagnostic_and_nothing_stays_open(self):
        revision = self.opened(LIMIT)["bundle"]["revision"]
        told = self.ask(route="open", name="limit.zal", source=BROKEN)
        expected = self.loopback(BROKEN).error
        self.assertEqual((told["ok"], told["reply"]), (True, {"error": plain(expected)}))
        self.assertTrue(expected["location"]["line"] and "unexpected" not in expected)
        later = self.ask(route="explain", revision=revision, input={})
        self.assertEqual(later, {"ok": False, "error": "ZAL does not accept the open text; correct it and check again"})

    def test_the_desks_own_disagreement_is_an_error_and_not_a_refusal_by_zal(self):
        with mock.patch.object(desk, "check_formal", side_effect=desk.Disagreement("planted disagreement")):
            told = self.ask(route="open", name="limit.zal", source=LIMIT)
        self.assertEqual(told["reply"], {"error": {"reason": "planted disagreement", "unexpected": True}})
        with mock.patch.object(desk, "check_sentences", side_effect=TypeError("planted fault")):
            told = self.ask(route="open", name="limit.zal", source=LIMIT)
        self.assertEqual(told["reply"], {"error": {"reason": "TypeError: planted fault", "unexpected": True}})

    def test_text_that_is_not_ascii_or_is_too_long_meets_zals_own_rule_for_sources(self):
        for text in (LIMIT + "# café\n", LIMIT + "# \ud800\n", LIMIT + "#" * desk.MAX_SOURCE):
            with self.subTest(length=len(text)):
                told = self.ask(route="open", name="limit.zal", source=text)
                self.assertEqual((told["ok"], told["reply"]["error"]["reason"]), (True, self.loopback(text).error["reason"]))
                self.assertIn("ASCII text at most 64 KiB", told["reply"]["error"]["reason"])

    def test_a_request_is_bounded_well_formed_and_current_or_it_is_refused(self):
        revision = self.opened(LIMIT)["bundle"]["revision"]
        given = {"state": "unlocked", "variables": {"used": 1}, "event": "withdraw", "context": {}}
        refused = [None, b"{}", "", "{", "[]", '"open"', json.dumps({"route": "about", "pad": "x" * portable.REQUEST_BOUND}),
                   '{"route": "about", "note": 1e400}', '{"route": "about", "note": NaN}', '{"route": "about", "note": 0.30000000000000001}',
                   '{"route": "about", "note": -0}',
                   json.dumps({"route": "edits", "revision": "0" * 64}), json.dumps({"route": "fires", "revision": "0" * 64, "rule": "take"}),
                   json.dumps({"route": "explain", "revision": revision, "input": {**given, "variables": {"used": 1.0}}}),
                   json.dumps({"route": "explain", "revision": "0" * 64, "input": given}),
                   json.dumps({"route": "explain", "revision": revision}),
                   json.dumps({"route": "edits"}), json.dumps({"route": "accept", "revision": revision}),
                   json.dumps({"route": "export", "revision": revision}), json.dumps({"revision": revision}),
                   json.dumps({"route": "fires", "revision": revision, "rule": "absent"}),
                   json.dumps({"route": "open", "name": "limit.zal"}), json.dumps({"route": "open", "source": LIMIT}),
                   json.dumps({"route": "open", "name": "../limit.zal", "source": LIMIT}),
                   json.dumps({"route": "open", "name": "limit.zal", "source": 7})]
        for text in refused:
            with self.subTest(request=str(text)[:60]):
                told = json.loads(self.desk.exchange(text))
                self.assertEqual((told["ok"], set(told)), (False, {"ok", "error"}))
                self.assertTrue(told["error"])
        self.assertEqual(self.ask(route="explain", revision=revision, input=given)["ok"], True)   # the refusals changed nothing
        self.opened(REFUND, "refund.zal")
        self.assertIn("older revision", self.ask(route="explain", revision=revision, input=given)["error"])

    def test_an_error_is_a_reply_that_says_it_is_no_refusal(self):
        with mock.patch.object(portable.Desk, "answer", side_effect=RecursionError("planted depth")):
            told = self.ask(route="about")
        self.assertEqual(told, {"ok": False, "error": "RecursionError: planted depth", "unexpected": True})

    def test_the_first_input_a_rule_decides_is_one_zal_says_it_decides(self):
        bundle = self.opened(UNFIRED)["bundle"]
        rules = [entry["id"] for entry in bundle["declared"] if entry["concept"] == "rule"]
        self.assertIn("unfired", rules)
        for rule in rules:
            told = self.ask(route="fires", revision=bundle["revision"], rule=rule)
            self.assertTrue(told["ok"], told)
            if rule == "unfired":
                self.assertIsNone(told["reply"])
                continue
            decided = self.ask(route="explain", revision=bundle["revision"], input=told["reply"])["reply"]
            self.assertEqual(decided["outcome"]["rule"], rule)

    def test_the_digest_names_what_the_page_shows_and_not_the_python_that_computed_it(self):
        told = self.opened(LIMIT)
        bundle = told["bundle"]
        self.assertEqual(portable.digest_of({**bundle, "python": "another"}), told["digest"])
        self.assertNotEqual(portable.digest_of({**bundle, "check": {**bundle["check"], "status": "counterexample"}}), told["digest"])
        self.assertNotEqual(portable.digest_of({**bundle, "checker": "0" * 64}), told["digest"])
        self.assertEqual(self.opened(LIMIT)["digest"], told["digest"])

    def test_about_names_the_checker_and_grants_nothing(self):
        told = self.ask(route="about")["reply"]
        self.assertEqual((told["checker"], told["authority"], told["source_bound"]),
                         (portable.checker_identity(), "none", desk.MAX_SOURCE))
        self.assertEqual(self.opened(LIMIT)["bundle"]["checker"], told["checker"])

    def test_nothing_is_written_and_no_socket_is_opened(self):
        digest = lambda folder: {path.name: hashlib.sha256(path.read_bytes()).hexdigest() for path in Path(folder).iterdir() if path.is_file()}
        before, here = digest(HERE), os.getcwd()
        with tempfile.TemporaryDirectory() as folder:
            os.chdir(folder)
            try:
                with mock.patch("socket.socket", side_effect=AssertionError("No network is permitted")):
                    revision = self.opened(LIMIT)["bundle"]["revision"]
                    given = {"state": "unlocked", "variables": {"used": 1}, "event": "withdraw", "context": {}}
                    for request in ({"route": "edits"}, {"route": "explain", "input": given}, {"route": "candidate", "source": OVERFLOWS},
                                    {"route": "fires", "rule": "take"}):
                        self.assertTrue(self.ask(revision=revision, **request)["ok"])
                    self.ask(route="open", name="limit.zal", source=BROKEN)
                self.assertEqual(list(Path(folder).iterdir()), [])
            finally:
                os.chdir(here)
        self.assertEqual(digest(HERE), before)


class Build(unittest.TestCase):
    """The build with a stand-in runtime of a few bytes: no browser and no WebAssembly runs here."""

    @classmethod
    def setUpClass(cls):
        cls.folder = tempfile.TemporaryDirectory()
        cls.addClassCleanup(cls.folder.cleanup)
        cls.runtime, cls.out = Path(cls.folder.name) / "runtime", Path(cls.folder.name) / "out"
        cls.runtime.mkdir()
        stand_in = {"pyodide.mjs": b"export const loadPyodide = null;\n", "pyodide.asm.wasm": b"\0asm stand-in"}
        for name, data in stand_in.items():
            (cls.runtime / name).write_bytes(data)
        cls.pin = {"package": "stand-in", "version": "1", "license": "none", "source": "nowhere",
                   "files": {name: hashlib.sha256(data).hexdigest() for name, data in stand_in.items()}}
        cls.examples = [example for example in build.carried_examples() if example["file"] in ("order.zal", "ladder_symmetry2.zal")]
        cls.record = build.build(cls.runtime, cls.out, "1.2.3", cls.pin, cls.examples)
        cls.web = {str(path.relative_to(cls.out / "web")): path.read_bytes() for path in (cls.out / "web").rglob("*") if path.is_file()}
        cls.extension = {str(path.relative_to(cls.out / "extension")): path.read_bytes() for path in (cls.out / "extension").rglob("*") if path.is_file()}

    def rebuilt(self, **changes):
        with tempfile.TemporaryDirectory() as out:
            options = {"runtime": self.runtime, "out": out, "version": "1.2.3", "pin": self.pin, "examples": self.examples, **changes}
            record = build.build(**options)
            return record, {str(path.relative_to(out)): path.read_bytes() for path in Path(out).rglob("*") if path.is_file()}

    def test_a_runtime_that_is_not_the_pinned_one_is_refused_and_nothing_is_built(self):
        with tempfile.TemporaryDirectory() as folder:
            other, out = Path(folder) / "runtime", Path(folder) / "out"
            shutil.copytree(self.runtime, other)
            (other / "pyodide.asm.wasm").write_bytes(b"\0asm stand-in, changed")
            with self.assertRaisesRegex(Refusal, "pyodide.asm.wasm is not the pinned one"):
                build.build(other, out, "1.2.3", self.pin, self.examples)
            (other / "pyodide.asm.wasm").unlink()
            with self.assertRaisesRegex(Refusal, "has no pyodide.asm.wasm"):
                build.build(other, out, "1.2.3", self.pin, self.examples)
            self.assertFalse(out.exists())

    def test_the_pin_in_the_repository_names_one_published_runtime_by_hash(self):
        pin = build.read_pin()
        self.assertEqual(sorted(pin["files"]), ["pyodide-lock.json", "pyodide.asm.mjs", "pyodide.asm.wasm", "pyodide.mjs", "python_stdlib.zip"])
        self.assertTrue(all(re.fullmatch(r"[0-9a-f]{64}", digest) for digest in pin["files"].values()))
        self.assertTrue(pin["archive"].endswith(f"/{pin['package']}-{pin['version']}.tgz") and pin["archive_integrity"].startswith("sha512-") and pin["license"])
        with tempfile.TemporaryDirectory() as folder:
            for text in ('{"files": {}}', '{"files": {"../x": "' + "0" * 64 + '"}}', '{"files": {"x": "short"}}', "[]"):
                (Path(folder) / "pin.json").write_text(text)
                with self.assertRaises(Refusal):
                    build.read_pin(Path(folder) / "pin.json")

    def test_no_script_stays_in_the_page_and_its_policy_admits_only_its_own_files(self):
        for files, kind in ((self.web, "web"), (self.extension, "extension")):
            page, script = files["index.html"].decode(), files["desk.js"].decode()
            with self.subTest(kind=kind):
                self.assertEqual(re.findall(r"<script[^>]*>", page), ['<script src="desk.js">'])
                self.assertNotRegex(page, r"\son[a-z]+=")
                self.assertTrue(script.startswith('"use strict";') and script.index("const PORTABLE = ") < script.index("const DESK = "))
                self.assertIn(f'const DESK = {{"mode":"device","bundles":[],"portable":{{"version":"1.2.3","checker":"{self.record["checker"]}"', script)
                self.assertIn(f'"kind":"{kind}"', script)
                self.assertNotIn("__ZAL_", script + page)
                self.assertEqual(script.count((HERE / "portable_desk.js").read_text()), 1)
                self.assertIn("<title>ZAL portable desk</title>", page)
        policy = re.search(r'http-equiv="Content-Security-Policy" content="([^"]+)"', self.web["index.html"].decode()).group(1)
        self.assertEqual(policy, build.WEB_POLICY)
        for part in ("default-src 'none'", "script-src 'self' 'wasm-unsafe-eval'", "connect-src 'self'", "worker-src 'self'", "base-uri 'none'", "form-action 'none'"):
            self.assertIn(part, policy)
        self.assertNotRegex(policy, r"https?:|\*|'unsafe-eval'|script-src[^;]*'unsafe-inline'")
        self.assertNotIn("Content-Security-Policy", self.extension["index.html"].decode())

    def test_the_sources_that_travel_are_zals_own_files_and_enough_to_answer_alone(self):
        with zipfile.ZipFile(io.BytesIO(self.web["zal_sources.zip"])) as archive:
            names = archive.namelist()
            self.assertEqual(names, sorted(self.record["sources"]))
            for name in names:
                self.assertEqual(archive.read(name), (HERE / name).read_bytes(), name)
                self.assertEqual(hashlib.sha256(archive.read(name)).hexdigest(), self.record["sources"][name])
        self.assertTrue({"behavior.py", "behavior2.py", "symmetry2.py", "explain2.py", "ladder_en.json", "review_desk.py", "portable_desk.py", "workflow.py"} <= set(names))
        self.assertFalse({"portable_desk_build.py", "codex_transport.py", "test_review_desk.py"} & set(names))
        self.assertEqual(self.record["checker"], portable.checker_identity())
        self.assertEqual(self.web["zal_sources.zip"], self.extension["zal_sources.zip"])
        real = build.probe

        def forgetful(folder, examples):                               # as if a file ZAL needs had not been noticed
            seen = real(folder, examples)
            return {**seen, "files": [name for name in seen["files"] if name != "symmetry2.py"]} if Path(folder) == HERE else seen

        def altered(folder, examples):                                 # as if the packed sources were not the repository's
            seen = real(folder, examples)
            return seen if Path(folder) == HERE else {**seen, "checker": "0" * 64}

        for stand_in, said in ((forgetful, "ZAL did not answer the desk"), (altered, "do not answer as the repository does")):
            with mock.patch.object(build, "probe", side_effect=stand_in), tempfile.TemporaryDirectory() as out:
                with self.assertRaisesRegex(Refusal, said):
                    build.build(self.runtime, out, "1.2.3", self.pin, self.examples)
                self.assertEqual(list(Path(out).iterdir()), [])

    def test_each_carried_example_has_the_digest_the_exchange_gives_for_it(self):
        shown = json.loads(re.search(r"const DESK = (\{.*?\});\n", self.web["desk.js"].decode(), re.S).group(1))["portable"]["examples"]
        self.assertEqual([example["file"] for example in shown], ["ladder_symmetry2.zal", "order.zal"])
        for example in shown:
            told = json.loads(portable.Desk().exchange(json.dumps({"route": "open", "name": example["file"], "source": example["source"]})))["reply"]
            self.assertEqual((example["digest"], example["status"], example["name"]), (told["digest"], told["bundle"]["check"]["status"], told["bundle"]["name"]))
            self.assertEqual(example["source"], (HERE / "examples" / example["file"]).read_text(encoding="ascii"))
        self.assertEqual([entry["digest"] for entry in self.record["examples"]], [example["digest"] for example in shown])
        carried = {example["file"] for example in build.carried_examples()}
        self.assertTrue({"order.zal", "withdrawal2.zal", "prepared_batch3.zal"} <= carried and not {"relay_typed.zal", "shell_effects.zal"} & carried)

    def test_the_same_input_builds_the_same_bytes_and_the_record_names_every_file(self):
        record, files = self.rebuilt()
        self.assertEqual(record, self.record)
        self.assertEqual(files["portable-desk-extension.zip"], (self.out / "portable-desk-extension.zip").read_bytes())
        self.assertEqual(json.loads(files["portable-desk-build.json"]), record)
        for folder, built in (("web", self.web), ("extension", self.extension)):
            self.assertEqual(record[folder], {name: hashlib.sha256(data).hexdigest() for name, data in sorted(built.items())})
        with zipfile.ZipFile(io.BytesIO(files["portable-desk-extension.zip"])) as archive:
            self.assertEqual({name: archive.read(name) for name in archive.namelist()}, self.extension)
        self.assertEqual((record["authority"], record["version"], record["runtime"]["files"]), ("none", "1.2.3", self.pin["files"]))

    def test_the_web_folder_keeps_every_one_of_its_files_for_starting_offline(self):
        worker = self.web["service_worker.js"].decode()
        kept = json.loads(re.search(r"const FILES = (\[.*?\]);", worker).group(1))
        self.assertEqual(sorted(kept), sorted(["./", *(name for name in self.web if name != "service_worker.js")]))
        stamp = re.search(r'const STAMP = "([0-9a-f]{16})";', worker).group(1)
        changed = self.rebuilt(version="1.2.4")[1]["web/service_worker.js"].decode()
        self.assertNotIn(stamp, changed)                               # a different build keeps its files under another name
        self.assertNotIn("skipWaiting", worker)                        # a newer build waits for open pages to close
        manifest = json.loads(self.web["manifest.webmanifest"])
        self.assertEqual((manifest["display"], manifest["start_url"], manifest["scope"]), ("standalone", "./index.html", "./"))
        self.assertTrue(all(icon["src"] in self.web for icon in manifest["icons"]) and {"192x192", "512x512"} <= {icon["sizes"] for icon in manifest["icons"]})
        self.assertNotIn("service_worker.js", self.extension)

    def test_the_extension_asks_for_no_permission_and_admits_only_its_own_scripts(self):
        manifest = json.loads(self.extension["manifest.json"])
        self.assertEqual((manifest["manifest_version"], manifest["version"], manifest["content_security_policy"]), (3, "1.2.3", {"extension_pages": build.EXTENSION_POLICY}))
        self.assertFalse({"permissions", "host_permissions", "optional_permissions", "optional_host_permissions", "content_scripts",
                          "web_accessible_resources", "externally_connectable"} & set(manifest))
        self.assertEqual(manifest["background"], {"service_worker": "extension_background.js", "scripts": ["extension_background.js"]})
        self.assertEqual(manifest["browser_specific_settings"]["gecko"]["data_collection_permissions"], {"required": ["none"]})
        self.assertLessEqual(len(manifest["description"]), 132)
        self.assertTrue(all(path in self.extension for path in [*manifest["icons"].values(), "extension_background.js", "index.html", "desk.js"]))
        background = self.extension["extension_background.js"].decode()
        self.assertNotRegex(background, r"fetch|XMLHttpRequest|https?://|storage|cookies|scripting")
        for version in ("1.0-beta", "01.2", "1.2.3.4.5", ""):
            with self.assertRaises(Refusal):
                build.build(self.runtime, self.out, version, self.pin, self.examples)

    def test_the_mark_is_drawn_at_the_size_asked_with_no_file_kept_in_the_repository(self):
        def decoded(data):
            self.assertEqual(data[:8], b"\x89PNG\r\n\x1a\n")
            width, height, depth, kind = struct.unpack(">IIBB", data[16:26])
            at, pixels = 8, b""
            while at < len(data):
                length, name = struct.unpack(">I", data[at:at + 4])[0], data[at + 4:at + 8]
                if name == b"IDAT":
                    pixels += data[at + 8:at + 8 + length]
                at += 12 + length
            rows = zlib.decompress(pixels)
            return (width, height, depth, kind), lambda x, y: rows[y * (1 + 4 * width) + 1 + 4 * x:][:4]
        for size in (16, 48, 192):
            shape, pixel = decoded(self.web[f"icons/icon-{size}.png"] if size != 16 else self.extension["icons/icon-16.png"])
            self.assertEqual(shape, (size, size, 8, 6))
            self.assertEqual(pixel(0, 0)[3], 0)                        # a rounded corner is clear
            self.assertEqual(pixel(size // 2, size // 2)[3], 255)
        shape, pixel = decoded(self.web["icons/maskable-512.png"])
        self.assertEqual((shape, pixel(0, 0)), ((512, 512, 8, 6), bytes([0x13, 0x1a, 0x24, 255])))
        self.assertEqual(pixel(256, 256)[:3], b"\xff\xff\xff")       # the letter crosses the middle
        self.assertEqual(build.mark(48), build.mark(48))
        self.assertEqual(list(HERE.glob("*.png")) + list(HERE.glob("*.ico")), [])


if __name__ == "__main__":
    unittest.main()
