#!/usr/bin/env python3
"""Planted-defect controls for the portable desk: its exchange, its build and its page; authority: none.

Each control mutates an isolated copy of integrations/zal, requires its named test or named page check to fail
there, and then requires the named tests and the page checks to pass on the unmutated copy. The live source
tree is never written. The page controls need node; --python-only leaves them out.

The worker that runs Python as WebAssembly, the offline worker and the extension's background script run only
in a browser. No control here reaches them; portable_desk_browser_smoke.cjs observes them.
"""
import argparse
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ZAL = Path(__file__).resolve().parent
TESTS = "test_portable_desk."
ENVIRONMENT = {**os.environ, "PYTHONDONTWRITEBYTECODE": "1"}
# (label, file, original text, defect text, named test)
DEFECTS = [
    ('table-built-although-each-input-is-asked', 'portable_desk.py',
     'STAGED = {"with_edits": False, "with_table": False}',
     'STAGED = {"with_edits": False, "with_table": True}',
     'Exchange.test_an_opened_statement_is_the_loopback_desks_bundle_without_the_table_and_the_edits'),
    ('edits-planted-before-the-statement-is-shown', 'portable_desk.py',
     'STAGED = {"with_edits": False, "with_table": False}',
     'STAGED = {"with_edits": True, "with_table": False}',
     'Exchange.test_an_opened_statement_is_the_loopback_desks_bundle_without_the_table_and_the_edits'),
    ('text-that-is-not-ascii-rewritten', 'portable_desk.py',
     'source.encode("utf-8", "surrogatepass")',
     'source.encode("ascii", "replace")',
     'Exchange.test_text_that_is_not_ascii_or_is_too_long_meets_zals_own_rule_for_sources'),
    ('stale-revision-answered', 'portable_desk.py',
     '        if data.get("revision") != self.model.revision:\n            raise Refusal("This page shows an older revision; reload")\n',
     '',
     'Exchange.test_a_request_is_bounded_well_formed_and_current_or_it_is_refused'),
    ('edits-planted-in-a-statement-that-fails', 'portable_desk.py',
     '        if not self.passing:\n            return None\n',
     '',
     'Exchange.test_planted_edits_on_request_are_the_loopback_desks'),
    ('a-step-another-rule-decides', 'portable_desk.py',
     'if step.rule == data["rule"]:',
     'if step.rule != data["rule"]:',
     'Exchange.test_the_first_input_a_rule_decides_is_one_zal_says_it_decides'),
    ('unknown-rule-searched-for', 'portable_desk.py',
     '        if data.get("rule") not in {rule.id for rule in self.model.rules}:\n            raise Refusal("The open statement has no rule of that name")\n',
     '',
     'Exchange.test_a_request_is_bounded_well_formed_and_current_or_it_is_refused'),
    ('request-bound-dropped', 'portable_desk.py',
     'if not isinstance(text, str) or not 0 < len(text) <= REQUEST_BOUND:',
     'if not isinstance(text, str) or not 0 < len(text):',
     'Exchange.test_a_request_is_bounded_well_formed_and_current_or_it_is_refused'),
    ('inexact-number-let-through', 'portable_desk.py',
     'data = json.loads(text, **json_numbers.hooks(json_numbers.raising(Refusal, "request: ")))',
     'data = json.loads(text)',
     'Exchange.test_a_request_is_bounded_well_formed_and_current_or_it_is_refused'),
    ('name-of-a-statement-unchecked', 'portable_desk.py',
     'if not isinstance(name, str) or not LABEL.match(name) or not isinstance(source, str):',
     'if not isinstance(name, str) or not isinstance(source, str):',
     'Exchange.test_a_request_is_bounded_well_formed_and_current_or_it_is_refused'),
    ('error-reported-as-a-refusal', 'portable_desk.py',
     'reply = {"ok": False, "error": f"{type(error).__name__}: {error}"[:2000], "unexpected": True}',
     'reply = {"ok": False, "error": f"{type(error).__name__}: {error}"[:2000]}',
     'Exchange.test_an_error_is_a_reply_that_says_it_is_no_refusal'),
    ('digest-blind-to-the-checker', 'portable_desk.py',
     'kept = {key: value for key, value in bundle.items() if key != "python"}',
     'kept = {key: value for key, value in bundle.items() if key not in ("python", "checker")}',
     'Exchange.test_the_digest_names_what_the_page_shows_and_not_the_python_that_computed_it'),
    ('refused-text-leaves-the-earlier-one-open', 'portable_desk.py',
     '        self.model = self.profile = self.count = None\n        self.passing = False\n        # Encoded as it is',
     '        # Encoded as it is',
     'Exchange.test_a_source_zal_refuses_gets_the_located_diagnostic_and_nothing_stays_open'),
    ('exchange-claims-authority', 'portable_desk.py',
     '"source_bound": review_desk.MAX_SOURCE, "authority": "none"}',
     '"source_bound": review_desk.MAX_SOURCE, "authority": "owner"}',
     'Exchange.test_about_names_the_checker_and_grants_nothing'),
    ('exchange-writes-a-file', 'portable_desk.py',
     '        self.current(data)\n        if not self.passing:',
     '        self.current(data)\n        open("edits.log", "w").close()\n        if not self.passing:',
     'Exchange.test_nothing_is_written_and_no_socket_is_opened'),
    ('runtime-hash-not-compared', 'portable_desk_build.py',
     '        if sha256(data) != expected:',
     '        if False:',
     'Build.test_a_runtime_that_is_not_the_pinned_one_is_refused_and_nothing_is_built'),
    ('packed-sources-not-compared-with-the-repository', 'portable_desk_build.py',
     '    if (alone["checker"], alone["examples"]) != (seen["checker"], seen["examples"]) or not set(alone["files"]) <= set(files):',
     '    if False:',
     'Build.test_the_sources_that_travel_are_zals_own_files_and_enough_to_answer_alone'),
    ('probe-that-failed-taken-as-an-answer', 'portable_desk_build.py',
     '    if done.returncode != 0:',
     '    if False:',
     'Build.test_the_sources_that_travel_are_zals_own_files_and_enough_to_answer_alone'),
    ('script-left-inside-the-page', 'portable_desk_build.py',
     '''    html = html[:opened] + '<script src="desk.js"></script>' + html[closed + len("</script>"):]\n''',
     '',
     'Build.test_no_script_stays_in_the_page_and_its_policy_admits_only_its_own_files'),
    ('policy-admits-evaluated-text', 'portable_desk_build.py',
     '''WEB_POLICY = ("default-src 'none'; script-src 'self' 'wasm-unsafe-eval';''',
     '''WEB_POLICY = ("default-src 'none'; script-src 'self' 'unsafe-eval';''',
     'Build.test_no_script_stays_in_the_page_and_its_policy_admits_only_its_own_files'),
    ('portable-script-placed-after-the-page', 'portable_desk_build.py',
     'script = (HERE / "portable_desk.js").read_text(encoding="utf-8") + "\\n" + html[opened + len("<script>"):closed]',
     'script = html[opened + len("<script>"):closed] + "\\n" + (HERE / "portable_desk.js").read_text(encoding="utf-8")',
     'Build.test_no_script_stays_in_the_page_and_its_policy_admits_only_its_own_files'),
    ('extension-asks-for-a-permission', 'portable_desk_build.py',
     '"manifest_version": 3, "name": NAME, "short_name": SHORT_NAME, "version": version,',
     '"manifest_version": 3, "permissions": ["tabs"], "name": NAME, "short_name": SHORT_NAME, "version": version,',
     'Build.test_the_extension_asks_for_no_permission_and_admits_only_its_own_scripts'),
    ('version-an-extension-cannot-carry', 'portable_desk_build.py',
     '    if not VERSION.match(version):\n        raise Refusal("The version needs one to four numbers separated by dots, as an extension manifest requires")\n',
     '',
     'Build.test_the_extension_asks_for_no_permission_and_admits_only_its_own_scripts'),
    ('offline-copy-misses-a-file', 'portable_desk_build.py',
     'json.dumps(["./", *kept])',
     'json.dumps(["./", *kept[1:]])',
     'Build.test_the_web_folder_keeps_every_one_of_its_files_for_starting_offline'),
    ('newer-build-takes-over-open-pages', 'portable_desk_service_worker.js',
     'self.addEventListener("install", (event) => event.waitUntil(',
     'self.addEventListener("install", (event) => self.skipWaiting() || event.waitUntil(',
     'Build.test_the_web_folder_keeps_every_one_of_its_files_for_starting_offline'),
    ('typed-and-effects-examples-carried', 'portable_desk_build.py',
     '        except Refusal:                                                # a typed or effects profile, which the desk does not read\n            continue\n',
     '        except Refusal:\n            pass\n',
     'Build.test_each_carried_example_has_the_digest_the_exchange_gives_for_it'),
    ('digest-of-another-example', 'portable_desk_build.py',
     '"status": told[example["file"]]["status"], "digest": told[example["file"]]["digest"]} for example in examples]',
     '"status": told[example["file"]]["status"], "digest": results[0]["digest"]} for example in examples]',
     'Build.test_each_carried_example_has_the_digest_the_exchange_gives_for_it'),
    ('mark-with-opaque-corners', 'portable_desk_build.py',
     'radius, inset, bar = (0 if bleed else 0.22 * edge),',
     'radius, inset, bar = 0,',
     'Build.test_the_mark_is_drawn_at_the_size_asked_with_no_file_kept_in_the_repository'),
    ('record-leaves-out-the-runtime', 'portable_desk_build.py',
     '"files": pin["files"]},\n              "sources": sources,',
     '"files": {}},\n              "sources": sources,',
     'Build.test_the_same_input_builds_the_same_bytes_and_the_record_names_every_file'),
]

# (label, file, original text, defect text, named page check of test_portable_desk.cjs)
PAGE_DEFECTS = [
    ('engine-from-another-address', 'portable_desk.js',
     'P.engine = new Worker("portable_desk_engine.js", { type: "module" });',
     'P.engine = new Worker("https://example.invalid/engine.js", { type: "module" });',
     'the page starts one module worker from its own file and reaches nothing else'),
    ('text-altered-on-its-way-to-zal', 'portable_desk.js',
     'told = await question({ route: "open", name: label(), source: text });',
     'told = await question({ route: "open", name: label(), source: text.trim() });',
     'the first example is sent to ZAL as it is carried, under its own name'),
    ('message-carries-more-than-the-request', 'portable_desk.js',
     'P.engine.postMessage({ id, request: JSON.stringify(request) });',
     'P.engine.postMessage({ id, request: JSON.stringify(request), kept: P.shelf });',
     'every message to the engine is one request of the desk and nothing else'),
    ('status-supplied-by-the-page', 'portable_desk.js',
     'DESK.bundles = [{ ...told.bundle, pending: told.edits_follow ? "edits" : null }];',
     'DESK.bundles = [{ ...told.bundle, check: { ...told.bundle.check, status: "pass-with-scope" }, pending: told.edits_follow ? "edits" : null }];',
     "a text ZAL accepts replaces what the desk shows, with ZAL's own status"),
    ('planted-edits-never-asked-for', 'portable_desk.js',
     'if (told.edits_follow) plant(at, told.bundle.revision);',
     'if (false) plant(at, told.bundle.revision);',
     'the planted edits are asked for after the statement is on the screen'),
    ('edits-of-an-earlier-text-shown', 'portable_desk.js',
     'if (P.at !== at || !b || b.revision !== revision || b.pending !== "edits") return;',
     'if (P.at !== at || !b || b.pending !== "edits") return;',
     'planted edits that arrive for an earlier text are not shown with the current one'),
    ('reply-for-a-closed-statement-shown', 'portable_desk.js',
     '    if (P.at !== at) return;                                     /* another statement was opened meanwhile */\n',
     '',
     'a reply for a statement that is no longer open is not shown'),
    ('typing-asks-zal', 'portable_desk.js',
     '    later();\n    if (changed() !== P.flagged)',
     '    later(); check();\n    if (changed() !== P.flagged)',
     'nothing is sent to ZAL by typing'),
    ('example-edited-in-place', 'portable_desk.js',
     '    if (example()) adopt();\n',
     '    if (example()) example().source = P.draft;\n',
     'typing in an example makes a copy kept on this device and leaves the example alone'),
    ('text-never-written-to-the-browser', 'portable_desk.js',
     '    later();\n    if (changed() !== P.flagged)',
     '    if (changed() !== P.flagged)',
     'the text is written to this browser shortly after typing, not on every key'),
    ('text-written-on-every-key', 'portable_desk.js',
     '    later();\n    if (changed() !== P.flagged)',
     '    keep();\n    if (changed() !== P.flagged)',
     'the text is written to this browser shortly after typing, not on every key'),
    ('text-not-written-when-the-page-is-left', 'portable_desk.js',
     '    window.addEventListener("pagehide", flush);\n',
     '',
     'the text is written when the page is left, without waiting'),
    ('lost-text-not-mentioned', 'portable_desk.js',
     '    catch (e) { P.kept = false; }\n  }\n  function recall() {',
     '    catch (e) { P.kept = true; }\n  }\n  function recall() {',
     'a browser that will not keep the text is named, and the page goes on'),
    ('kept-statement-with-a-name-zal-refuses', 'portable_desk.js',
     'typeof s.name === "string" && LABEL.test(s.name) && typeof s.source === "string"',
     'typeof s.name === "string" && typeof s.source === "string"',
     'kept statements that cannot be read are left out, not shown'),
    ('error-shown-as-a-refusal-by-zal', 'portable_desk.js',
     '    else if (P.error && !changed()) out.push(P.error.unexpected\n',
     '    else if (P.error && !changed()) out.push(!P.error\n',
     'an error is not presented as a refusal by ZAL'),
    ('diagnostic-without-its-line', 'portable_desk.js',
     '      refusal(P.error, P.error.reason),\n',
     '      h("p", {}, P.error.reason),\n',
     "a refused text shows ZAL's located diagnostic under Write"),
    ('older-text-on-the-desk-not-mentioned', 'portable_desk.js',
     '    else if (P.error && !changed()) v.append(h("div", { class: "banner" }, `ZAL does not accept your newer text.',
     '    else if (false) v.append(h("div", { class: "banner" }, `ZAL does not accept your newer text.',
     'the desk keeps showing the text ZAL last accepted and says which'),
    ('change-since-the-check-not-mentioned', 'portable_desk.js',
     '    else if (changed()) v.append(h("div", { class: "banner" }, `You have changed the text since ZAL read it.',
     '    else if (false) v.append(h("div", { class: "banner" }, `You have changed the text since ZAL read it.',
     'the verdict says the text has changed since ZAL read it'),
    ('question-about-a-text-the-engine-no-longer-holds', 'portable_desk.js',
     'if (body.revision && body.revision !== P.loaded && P.accepted !== null) {',
     'if (false) {',
     'a question about the text on the desk has ZAL read that text again first'),
    ('review-replaces-the-statement', 'portable_desk.js',
     'onclick: () => { S.draft = P.draft; checkCandidate(); } }',
     'onclick: () => { S.draft = P.draft; check(); } }',
     'reviewing an edit asks ZAL to compare the text under Write with the revision shown, and replaces nothing'),
    ('undo-keeps-the-edit', 'portable_desk.js',
     'onclick: () => replaceText(P.checked) }, "Undo my edits")',
     'onclick: () => replaceText(P.draft) }, "Undo my edits")',
     'Undo my edits brings back the text ZAL read'),
    ('symbol-not-put-into-the-text', 'portable_desk.js',
     '    area.value = text.slice(0, from) + token + text.slice(to);\n',
     '',
     'a symbol button puts the symbol into the text'),
    ('blank-text-can-be-checked', 'portable_desk.js',
     'disabled: blank || P.busy === "check" || P.state === "failed", onclick: check }',
     'disabled: P.busy === "check" || P.state === "failed", onclick: check }',
     'New opens a blank statement under Write and asks ZAL nothing'),
    ('name-zal-refuses-taken', 'portable_desk.js',
     'if (wanted !== own.name && LABEL.test(wanted)) own.name = named(wanted, own);',
     'if (wanted !== own.name) own.name = named(wanted, own);',
     'a name ZAL would not accept is not taken'),
    ('file-name-not-made-plain', 'portable_desk.js',
     'let base = String(wanted || "").replace(/[^A-Za-z0-9 ._-]/g, "-").replace(',
     'let base = String(wanted || "").replace(',
     'a chosen file becomes a statement on this device, under a name ZAL accepts, and ZAL reads it'),
    ('deleted-at-the-first-press', 'portable_desk.js',
     'if (P.doomed !== own.id) { P.doomed = own.id; said(); return; }',
     'if (false) { P.doomed = own.id; said(); return; }',
     'deleting asks to be pressed again'),
    ('another-checker-used', 'portable_desk.js',
     'if (said.about.checker !== DESK.portable.checker) { fail(',
     'if (false) { fail(',
     'a checker other than the one the desk was built with is not used'),
    ('result-not-compared-with-the-build', 'portable_desk.js',
     "P.agrees = carried && carried.source === text ? told.digest === carried.digest : null;",
     "P.agrees = carried && carried.source === text ? true : null;",
     "an example whose result differs from the build's is called out"),
    ('own-text-passed-off-as-a-carried-example', 'portable_desk.js',
     '    const carried = example();\n    P.error = null;',
     '    const carried = examples().find((e) => e.source === text);\n    P.error = null;',
     "a text of the reader's own is not compared with a carried example"),
    ('stop-leaves-the-worker-running', 'portable_desk.js',
     '    if (P.engine) P.engine.terminate();\n',
     '',
     'Stop ends the worker, starts another and says the edits were not completed'),
    ('engine-failure-not-reported', 'portable_desk.js',
     'if (said.type === "failed") { fail(said.reason); return; }',
     'if (said.type === "failed") { return; }',
     'an engine that does not start is reported with its reason, and nothing can be checked'),
    ('rule-that-never-fires-left-unsaid', 'portable_desk.js',
     'if (given) tryInput(given); else { P.unfired.add(rule); render(); }',
     'if (given) tryInput(given);',
     "a rule that no declared input fires says so, by ZAL's own answer"),
    ('offline-copy-registered-by-the-extension', 'portable_desk.js',
     'if (DESK.portable.kind === "web" && typeof navigator',
     'if (typeof navigator',
     'the extension page registers no offline copy and behaves the same'),
    ('portable-page-described-as-reading-a-file', 'review_desk.html',
     'DEVICE ? "ZAL runs on this device. Nothing is sent anywhere." : LIVE ?',
     'DEVICE ? "Live on this machine." : LIVE ?',
     'the page names itself, grants nothing and offers no agreement'),
    ('portable-page-asks-a-server', 'review_desk.html',
     '  if (DEVICE) return PORTABLE.ask(route, body);\n',
     '',
     'an input is explained by asking ZAL, with the revision the desk shows'),
    ('edits-still-coming-called-absent', 'review_desk.html',
     '  return b.pending ? "ZAL is re-checking each planted edit on this device. This part fills in when it has finished."',
     '  return false ? "ZAL is re-checking each planted edit on this device. This part fills in when it has finished."',
     'until they arrive the verdict and the Evidence tab say the edits are being re-checked'),
    ('write-tab-missing', 'review_desk.html',
     '...(DEVICE ? [["write", "Write", null]] : []),',
     '',
     "the statement is shown in ZAL's English and in symbols beside the instruments"),
    ('loopback-candidate-box-on-the-portable-page', 'review_desk.html',
     '  if (DEVICE) PORTABLE.candidate(panel);\n  else if (LIVE) {',
     '  if (ASKS) {',
     'with the text unchanged there is no edit to review'),
    ('step-where-a-rule-fires-not-asked-of-zal', 'review_desk.html',
     'onclick: () => PORTABLE.fires(clause.id) }',
     'onclick: () => {} }',
     'a step where a rule fires is asked of ZAL and opened'),
]


def run(work, command):
    result = subprocess.run(command, cwd=work, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=900, env=ENVIRONMENT)
    return result.returncode, result.stdout


def digest():
    return {path.name: hashlib.sha256(path.read_bytes()).hexdigest() for path in ZAL.iterdir() if path.is_file()}


def planted(work, filename, original, defect, command):
    """Run one command on the copy with one defect in place, then put the file back."""
    path = work / filename
    source = path.read_text(encoding="utf-8")
    if source.count(original) != 1:
        raise SystemExit(f"STOP: mutation anchor drift in {filename}: {original[:60]!r}")
    path.write_text(source.replace(original, defect), encoding="utf-8")
    try:
        return run(work, command)
    finally:
        path.write_text(source, encoding="utf-8")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--python-only", action="store_true", help="leave out the page controls, which need node")
    parser.add_argument("--only", nargs="*", help="run only the controls with these labels, and no baseline")
    options = parser.parse_args(argv)
    if not options.python_only and shutil.which("node", path=ENVIRONMENT["PATH"]) is None:
        raise SystemExit("STOP: the page controls need node; pass --python-only to run the Python controls alone")
    wanted = lambda label: options.only is None or label in options.only
    before, killed = digest(), 0
    with tempfile.TemporaryDirectory(prefix="zal-portable-desk-defects-") as directory:
        work = Path(directory) / "zal"
        shutil.copytree(ZAL, work, ignore=shutil.ignore_patterns("__pycache__", "*.lock"))
        for label, filename, original, defect, test in DEFECTS:
            if not wanted(label):
                continue
            code, output = planted(work, filename, original, defect, [sys.executable, "-B", "-m", "unittest", TESTS + test])
            name = test.rsplit(".", 1)[-1]
            if code == 0 or not (f"FAIL: {name}" in output or f"ERROR: {name}" in output):
                print(output[-3000:])
                raise SystemExit(f"STOP: planted defect survived or failed outside its named test: {label}")
            killed += 1
            print(f"KILLED {label} by {test}", flush=True)
        for test in sorted({control[4] for control in DEFECTS} if options.only is None else ()):
            code, output = run(work, [sys.executable, "-B", "-m", "unittest", TESTS + test])
            if code != 0:
                print(output[-3000:])
                raise SystemExit(f"STOP: baseline fails {test}")
            print(f"BASELINE PASS {test}", flush=True)
        if not options.python_only:
            for label, filename, original, defect, check in PAGE_DEFECTS:
                if not wanted(label):
                    continue
                code, output = planted(work, filename, original, defect, ["node", "test_portable_desk.cjs"])
                failed = next((line for line in output.splitlines() if "AssertionError [ERR_ASSERTION]: " in line), "")
                if code == 0 or check not in failed:
                    print(output[-3000:])
                    raise SystemExit(f"STOP: planted page defect survived or failed outside its named check: {label}")
                killed += 1
                print(f"KILLED {label} by page check: {check}", flush=True)
            if options.only is None:
                code, output = run(work, ["node", "test_portable_desk.cjs"])
                if code != 0:
                    print(output[-3000:])
                    raise SystemExit("STOP: baseline fails test_portable_desk.cjs")
                print("BASELINE PASS test_portable_desk.cjs", flush=True)
    if digest() != before:
        raise SystemExit("STOP: live source changed during isolated defect controls")
    if options.only is not None:
        print(f"{killed} of the named controls killed; no baseline was run, so this is not a pass")
        return 0
    print(f"PASS: {killed} named planted controls killed; baseline passes; live source unchanged")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
