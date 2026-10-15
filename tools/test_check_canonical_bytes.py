"""Coverage and native byte-reader regressions; fixtures are not proof results."""
from contextlib import ExitStack, redirect_stdout
import copy
import io
import json
import os
from pathlib import Path
import tempfile
import subprocess
import unittest
from unittest.mock import patch

import check_canonical_bytes as gate
import check_verus as verifier
from verus_coverage import inventory


class CanonicalByteCoverage(unittest.TestCase):
    def test_runtime_domains_and_mutation_anchors_remain_complete(self):
        profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        runtime = {name for name, record in profile["functions"].items() if record["mode"] == "Exec"}
        self.assertEqual(len(profile["functions"]), 171)
        self.assertEqual(len(runtime), 61)
        self.assertIn("canonical_bytes::canonical_v2::read_big_endian", runtime)
        self.assertIn("canonical_bytes::canonical_v2::read_signed_128", runtime)
        self.assertEqual(set(profile["body_covered_functions"]), runtime)
        self.assertTrue(all(record["requires"] == 0 and record["ensures"] > 0
                            for record in profile["functions"].values() if record["mode"] == "Exec"))
        mutations = gate.mutation_sources((gate.ROOT / gate.SUBJECT).read_text(),
                                           (gate.ROOT / gate.SPEC).read_text())
        self.assertEqual(len(mutations), 17)
        self.assertEqual(sum(expected == "proof" for _, _, expected in mutations.values()), 9)
        self.assertEqual(sum(expected == "proof_contract" for _, _, expected in mutations.values()), 3)
        for path, changed, expected in mutations.values():
            self.assertNotEqual(changed, (gate.ROOT / path).read_text())
            self.assertIn(expected, ("proof", "proof_contract", "coverage"))

    def test_unused_getter_controls_change_only_the_envelope_original_contract(self):
        envelope = (gate.ROOT / gate.ENVELOPE).read_text()
        self.assertEqual(envelope.count("pub fn original(&self)"), 1)
        getter = envelope.index("    pub fn original(&self)")
        contract = ("    #[cfg_attr(verus_keep_ghost, verus_spec(result =>\n"
                    "        ensures result@ == self.view().0,\n    ))]\n")
        start = getter - len(contract)
        self.assertEqual(envelope[start:getter], contract)
        payload_getter = ("    #[cfg_attr(verus_keep_ghost, verus_spec(result =>\n"
                          "        ensures result@ == self.view().1,\n    ))]\n    pub fn bytes(&self)")
        self.assertEqual(envelope.count(payload_getter), 1)
        self.assertGreater(envelope.index(payload_getter), getter)
        replacements = {
            "omit_unused_getter_contract": "",
            "weaken_unused_getter_contract":
                "    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures true,))]\n",
            "narrow_unused_getter_domain": ("    #[cfg_attr(verus_keep_ghost, verus_spec(result =>\n"
                                            "        requires self.view().0.len() > 0,\n"
                                            "        ensures result@ == self.view().0,\n    ))]\n"),
        }
        mutations = gate.mutation_sources((gate.ROOT / gate.SUBJECT).read_text(),
                                           (gate.ROOT / gate.SPEC).read_text())
        for name, replacement in replacements.items():
            path, changed, expected = mutations[name]
            self.assertEqual((path, expected), (gate.ENVELOPE, "coverage"))
            # Only the original getter's contract differs; its body, the
            # payload getter and every other function keep their exact text.
            self.assertEqual(changed, envelope[:start] + replacement + envelope[getter:])
            self.assertEqual(changed.count(payload_getter), 1)
        # Catalog proofs call the schema getter, so no control may target it.
        self.assertNotIn(gate.SUBJECT.parent / "schema/admission.rs",
                         {path for path, _, _ in mutations.values()})

    def test_simulated_success_cannot_omit_the_actual_byte_functions(self):
        pin = json.loads((gate.ROOT / verifier.PIN).read_text())
        profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        pin.update({key: profile[key] for key in ("expected_verified", "target_functions")})
        report = {"verus": {"commit": pin["commit"], "version": pin["version"]},
                  "verification-results": {"success": True, "errors": 0, "verified": profile["expected_verified"],
                    "encountered-error": False, "encountered-vir-error": False,
                    "is-verifying-entire-crate": True},
                  "func-details": dict.fromkeys(pin["target_functions"], {})}
        self.assertTrue(verifier.accepted(report, pin))
        # Verus reports associated methods by type name; VIR inventories use
        # internal impl identifiers. Test omissions in the actual report names.
        for name in profile["target_functions"]:
            changed = {**report, "func-details": {key: value for key, value in report["func-details"].items()
                                                  if key != name}}
            self.assertFalse(verifier.accepted(changed, pin))

    def test_all_evidence_sources_are_hashed_and_symlinks_refuse(self):
        from unittest.mock import patch
        sources = gate.snapshot()
        self.assertEqual(set(sources), set(map(str, gate.SOURCES)))
        canonical_files = {str(path.relative_to(gate.ROOT))
                           for path in (gate.ROOT / gate.SUBJECT.parent).rglob("*.rs")}
        self.assertLessEqual(canonical_files, set(map(str, gate.UNIT_SOURCES)))
        self.assertIn("verification/verus/canonical_bytes_tests.rs", set(map(str, gate.UNIT_SOURCES)))
        with patch.object(Path, "is_symlink", return_value=True):
            with self.assertRaisesRegex(RuntimeError, "symbolic link"):
                gate.snapshot()


class CanonicalByteEvidence(unittest.TestCase):
    """Synthetic classifier/retention fixtures, never proof or native receipts."""
    def setUp(self):
        self.pin = json.loads((gate.ROOT / verifier.PIN).read_text())
        self.controls = gate.mutation_sources((gate.ROOT / gate.SUBJECT).read_text(),
                                             (gate.ROOT / gate.SPEC).read_text())
        self.profile = {'target_functions': ['canonical_bytes::intended']}
        self.report = {'verus': {key: self.pin[key] for key in ('version', 'commit')},
            'func-details': dict.fromkeys(self.profile['target_functions'], {}),
            'verification-results': {'success': False, 'errors': 1, 'verified': 1,
                'encountered-error': True, 'encountered-vir-error': False,
                'is-verifying-entire-crate': True}}
        self.report['verus']['toolchain'] = self.pin['rust_toolchain']

    def classify(self, name='reverse_byte_order', *, report=None, stderr=None, code=1, **kwargs):
        changed = self.controls[name][1]
        if stderr is None and name in gate.CONTRACT_REJECTIONS:
            expected = sorted(gate.CONTRACT_REJECTIONS[name])
            stderr = "".join(f"error: {header}\n --> {path}:{line}:1\n"
                             for header, path, line in expected)
            stderr += f"error: aborting due to {len(expected)} previous errors\n"
        if stderr is None:
            path, function = gate.SEMANTIC_TARGETS[name]
            source = changed if path == gate.SUBJECT else (gate.ROOT / path).read_text()
            line = verifier.function_lines(source, function)[0]
            stderr = (f'error: postcondition not satisfied\n --> {path}:{line}:1\n'
                      'error: aborting due to 1 previous error\n')
        proc = subprocess.CompletedProcess([], code, json.dumps(self.report if report is None else report), stderr)
        return gate.classify_mutation(proc, self.report if report is None else report,
            name, changed, gate.ROOT, self.pin, self.profile, **kwargs)

    def test_every_original_control_has_one_precise_kind_and_function(self):
        self.assertEqual(len(self.controls), 17)
        self.assertEqual(set(gate.SEMANTIC_TARGETS) | set(gate.CONTRACT_REJECTIONS),
                         {n for n, row in self.controls.items() if row[2] in ('proof', 'proof_contract')})
        self.assertEqual(set(gate.COVERAGE_TARGETS),
                         {n for n, row in self.controls.items() if row[2] == 'coverage'})
        for name in set(gate.SEMANTIC_TARGETS) | set(gate.CONTRACT_REJECTIONS):
            self.assertEqual(self.classify(name)[:2], (True, 'killed'), name)

    def test_unknown_timeout_and_resources_are_inconclusive_even_beside_targeted_errors(self):
        for extra in ('SMT result UNKNOWN', 'solver returned unknown', 'resource limit exceeded',
                      'rlimit exceeded', 'timed out', 'out of memory', 'transport error', 'panic'):
            line = verifier.function_lines(self.controls['reverse_byte_order'][1], 'read_big_endian')[0]
            stderr = (f'error: postcondition not satisfied\n --> {gate.SUBJECT}:{line}:1\n'
                      'error: aborting due to 1 previous error\n' + extra)
            self.assertEqual(self.classify(stderr=stderr)[:2], (False, 'inconclusive'), extra)
        self.assertEqual(self.classify(timed_out=True)[:2], (False, 'inconclusive'))
        for code in (-9, 2, 137):
            self.assertEqual(self.classify(code=code)[:2], (False, 'inconclusive'))

    def test_semantic_kill_requires_exact_pin_whole_run_and_intended_error_region(self):
        for field, value in [('success', True), ('errors', True), ('verified', True),
                             ('encountered-vir-error', True), ('is-verifying-entire-crate', False)]:
            report = copy.deepcopy(self.report); report['verification-results'][field] = value
            self.assertFalse(self.classify(report=report)[0], field)
        for field in ('version', 'commit', 'toolchain'):
            report = copy.deepcopy(self.report); report['verus'][field] = 'unrelated'
            self.assertFalse(self.classify(report=report)[0], field)
        report = copy.deepcopy(self.report); report['func-details'] = {}
        self.assertFalse(self.classify(report=report)[0])
        for stderr in ('error: postcondition not satisfied\n --> unrelated.rs:1:1\n',
                       f'error: postcondition not satisfied\n --> {gate.SUBJECT}:9999:1\n',
                       'error[E0308]: mismatched types\n'):
            self.assertFalse(self.classify(stderr=stderr)[0])

    def test_omitted_signed_contract_targets_its_actual_schema_caller(self):
        for name in ('omit_signed_contract', 'weaken_signed_contract'):
            expected = sorted(gate.CONTRACT_REJECTIONS[name])
            self.assertEqual(len(expected), 3)
            errors = ''.join(f'error: {header}\n --> {path}:{line}:1\n' * 2
                            for header, path, line in expected)
            summary = 'error: aborting due to 6 previous errors\n'
            self.assertTrue(self.classify(name, stderr=errors + summary)[0])
            for header, path, line in expected:
                missing = errors.replace(f'error: {header}\n --> {path}:{line}:1\n', '')
                self.assertFalse(self.classify(name, stderr=missing + summary)[0])
            self.assertFalse(self.classify(name,
                stderr=errors.replace(str(gate.SCHEMA), 'unrelated.rs') + summary)[0])
            self.assertFalse(self.classify(name, stderr=errors + 'note: timeout\n' + summary)[0])
        self.assertEqual(len(gate.CONTRACT_REJECTIONS['narrow_signed_input']), 4)
        stderr = f'error: postcondition not satisfied\n --> {gate.SUBJECT}:60:1\nerror: aborting due to 1 previous error\n'
        self.assertFalse(self.classify('omit_signed_contract', stderr=stderr)[0])


    def test_exact_loop_entry_spelling_is_local_to_excessive_width(self):
        raw = (f"error: invariant not satisfied before loop\n --> {gate.SUBJECT}:32:19\n"
               "error: aborting due to 1 previous error\n")
        self.assertTrue(self.classify("permit_excessive_width", stderr=raw)[0])
        self.assertFalse(self.classify("wrong_radix", stderr=raw)[0])
        for wrong in (raw.replace("before loop", "before loop "),
                      raw.replace(str(gate.SUBJECT), "unrelated.rs"),
                      raw.replace(":32:19", ":9999:19"),
                      raw + "note: Resource limit (rlimit) exceeded\n"):
            self.assertFalse(self.classify("permit_excessive_width", stderr=wrong)[0])


    def test_wrong_byte_offset_exact_headers_preserve_strict_semantic_guards(self):
        raw = (f"error: invariant not satisfied at end of loop body\n --> {gate.SUBJECT}:35:13\n"
               f"error: precondition not met: index in bounds for this access\n --> {gate.SUBJECT}:47:40\n"
               "error: aborting due to 2 previous errors\n")
        self.assertEqual(self.classify("wrong_byte_offset", stderr=raw)[:2], (True, "killed"))
        for name in set(gate.SEMANTIC_TARGETS) - {"wrong_byte_offset"}:
            self.assertFalse(self.classify(name, stderr=raw)[0], name)
        for wrong in (raw.replace("loop body\n", "loop body \n"),
                      raw.replace("this access\n", "this access \n"),
                      raw.replace(str(gate.SUBJECT), "unrelated.rs", 1),
                      raw.replace(":47:40", ":9999:40"),
                      raw.replace("2 previous errors", "1 previous error"),
                      raw.replace("error: aborting due to 2 previous errors\n", ""),
                      raw + "error[E0308]: mismatched types\n",
                      raw + "note: UNKNOWN\n",
                      raw + "note: Resource limit (rlimit) exceeded\n"):
            self.assertFalse(self.classify("wrong_byte_offset", stderr=wrong)[0], wrong)
        for field, value in (("is-verifying-entire-crate", False), ("errors", 0),
                             ("verified", 0), ("encountered-vir-error", True)):
            report = copy.deepcopy(self.report)
            report["verification-results"][field] = value
            self.assertFalse(self.classify("wrong_byte_offset", stderr=raw, report=report)[0], field)
        report = copy.deepcopy(self.report); report["verus"]["commit"] = "wrong-pin"
        self.assertFalse(self.classify("wrong_byte_offset", stderr=raw, report=report)[0])
        report = copy.deepcopy(self.report); report["func-details"] = {}
        self.assertFalse(self.classify("wrong_byte_offset", stderr=raw, report=report)[0])
        self.assertFalse(self.classify("wrong_byte_offset", stderr=raw, timed_out=True)[0])
        for code in (0, 2, -9):
            self.assertFalse(self.classify("wrong_byte_offset", stderr=raw, code=code)[0])
        proc = subprocess.CompletedProcess([], 1, json.dumps(self.report), raw)
        self.assertTrue(gate.classify_mutation(proc, self.report, "wrong_byte_offset",
            self.controls["wrong_byte_offset"][1], gate.ROOT, self.pin, self.profile)[0])
        self.assertEqual(proc.stderr, raw)


    def test_narrow_domain_requires_all_demonstrated_callers_and_complete_evidence(self):
        name = "narrow_signed_input"
        self.assertTrue(self.classify(name)[0])
        expected = sorted(gate.CONTRACT_REJECTIONS[name])
        full = "".join(f"error: {h}\n --> {p}:{n}:1\n" for h, p, n in expected)
        summary = "error: aborting due to 4 previous errors\n"
        for extra in ("note: UNKNOWN\n", "note: Resource limit (rlimit) exceeded\n",
                      "error[E0308]: mismatched types\n",
                      "error: precondition not satisfied\n --> unrelated.rs:1:1\n"):
            self.assertFalse(self.classify(name, stderr=full + extra + summary)[0])
        for index in range(4):
            fewer = "".join(f"error: {h}\n --> {p}:{n}:1\n"
                            for j, (h, p, n) in enumerate(expected) if j != index)
            self.assertFalse(self.classify(name, stderr=fewer + summary)[0])
        self.assertFalse(self.classify(name, stderr=full)[0])
        self.assertFalse(self.classify(name, stderr=full + "error: aborting due to 3 previous errors\n")[0])
        for group, key, value in (("verus", "toolchain", "other"),
                ("verification-results", "encountered-error", False),
                ("verification-results", "errors", 5),
                ("verification-results", "is-verifying-entire-crate", False),
                ("verification-results", "verified", True)):
            report = copy.deepcopy(self.report); report[group][key] = value
            self.assertFalse(self.classify(name, report=report)[0])
        report = copy.deepcopy(self.report); report["func-details"] = {}
        self.assertFalse(self.classify(name, report=report)[0])


    def test_coverage_controls_need_exact_target_inventory_and_declared_field(self):
        getter = 'canonical_bytes::canonical_v2::envelope::impl&%0::original'
        signed = 'canonical_bytes::canonical_v2::spec::signed'
        probe = 'canonical_bytes::canonical_v2::uncontracted_width'
        def function(name, *, mode='Exec', requires='()', ensures='((Bool true))', body='(Int 1)'):
            return (f'(Function (Fun :path {name}) :mode {mode} :typ_bounds () :params () '
                    f':ret () :require {requires} :ensure {ensures} :d () :body {body})')
        original = function(getter) + function(signed, mode='Spec', ensures='()')
        installed = json.loads((gate.ROOT / gate.PROFILE).read_text())
        namespace = installed['namespace']
        self.assertEqual(namespace, 'canonical_bytes::canonical_v2::')
        self.assertIn(getter, installed['functions'])
        self.assertIn(signed, installed['functions'])
        self.assertNotIn(probe, installed['functions'])
        profile = {'namespace': namespace, 'body_covered_functions': [getter],
                   'functions': inventory(original, namespace, (getter,))}
        cases = {
            'omit_unused_getter_contract': original.replace(':ensure ((Bool true))', ':ensure ()'),
            'weaken_unused_getter_contract': original.replace('Bool true', 'Bool false'),
            'narrow_unused_getter_domain': original.replace(':require ()', ':require ((Bool true))', 1),
            'change_signed_specification': function(getter) + function(signed, mode='Spec', ensures='()', body='(Int 2)'),
            'add_uncontracted_function': original + function(probe, ensures='()'),
        }
        for name, vir in cases.items():
            self.assertTrue(gate.coverage_control(vir, profile, name)[0], name)
            self.assertFalse(gate.coverage_control(vir + function(namespace + 'unexpected'), profile, name)[0])
            self.assertFalse(gate.coverage_control(original, profile, name)[0])
        self.assertFalse(gate.coverage_control(cases['weaken_unused_getter_contract'], profile,
                                               'change_signed_specification')[0])
        changed_body = cases['weaken_unused_getter_contract'].replace('(Int 1)', '(Int 2)', 1)
        self.assertFalse(gate.coverage_control(changed_body, profile, 'weaken_unused_getter_contract')[0])
        self.assertFalse(gate.coverage_control(original + function(probe), profile, 'add_uncontracted_function')[0])
        self.assertFalse(gate.coverage_control('(Function', profile, 'add_uncontracted_function')[0])
        report = copy.deepcopy(self.report)
        report['verification-results'].update({'success': True, 'errors': 0, 'verified': 1163,
                                               'encountered-error': False})
        proc = subprocess.CompletedProcess([], 0, '', '')
        name = 'add_uncontracted_function'
        args = (proc, report, name, self.controls[name][1], gate.ROOT, self.pin, profile)
        self.assertTrue(gate.classify_mutation(*args, vir=cases[name])[0])
        proc.stderr = 'solver result UNKNOWN'
        self.assertEqual(gate.classify_mutation(*args, vir=cases[name])[:2], (False, 'inconclusive'))
        proc.stderr = ''; report['verification-results']['is-verifying-entire-crate'] = False
        self.assertFalse(gate.classify_mutation(*args, vir=cases[name])[0])

    def test_read_identity_renaming_preserves_all_semantic_structure(self):
        target = "canonical_bytes::canonical_v2::envelope::impl&%0::original"
        neighbor = "canonical_bytes::neighbor"
        def read(identity, kind="Copy"):
            return f"(UnfinalizedReadKind :preliminary_kind (ReadKind {kind}) :id {identity})"
        def function(name, ids, *, omitted=False):
            binder = ('(VarIdent "%return" (VarIdentDisambiguate AirLocal))' if omitted
                      else '(VarIdent "result" (VarIdentDisambiguate RustcId 12))')
            ensure = "()" if omitted else f"((Eq {read(ids[0])} (Const Int 1)))"
            return (f"(Function (Fun :path {name}) :mode Exec :typ_bounds () :params () "
                    f":ret (Param :name {binder} :typ (Typ Bool) :mode Exec "
                    ":user_mut false :unwrapped_info None) :require () "
                    f":ensure {ensure} :d () :body (Pair {read(ids[1])} "
                    f"{read(ids[2])} {read(ids[1])} (Const Int 17)))")
        original = function(target, (10, 11, 12)) + function(neighbor, (20, 21, 22))
        changed = function(target, (30, 31, 32), omitted=True) + function(neighbor, (40, 41, 42))
        profile = {"namespace": "canonical_bytes::", "body_covered_functions": [target, neighbor],
                   "functions": inventory(original, "canonical_bytes::", (target, neighbor))}
        name = "omit_unused_getter_contract"
        self.assertFalse(gate.coverage_control(changed, profile, name)[0])
        self.assertTrue(gate.coverage_control(changed, profile, name, original)[0])
        for wrong in (changed.replace("(Const Int 17)", "(Const Int 18)", 1),
                changed.replace(":id 32", ":id 31"),
                changed.replace(":id 31", ":id 33", 1),
                changed.replace("(ReadKind Copy)", "(ReadKind Move)", 1),
                changed.replace("(Typ Bool)", "(Typ Int)", 1),
                changed.replace('"%return"', '"other_return"'),
                changed.replace(":id 31", ":id ３１"),
                changed.replace("(Const Int 1)", "(Const Int 0)")):
            self.assertFalse(gate.coverage_control(wrong, profile, name, original)[0], wrong)
        self.assertFalse(gate.coverage_control(changed, profile, name,
            original.replace("(Const Int 17)", "(Const Int 18)", 1))[0])
        self.assertFalse(gate.same_read_identity_structure("(Const Int 7)",
            "(Const Int 8)", {}, {}))

    def test_narrow_getter_return_identity_is_consistent_and_function_local(self):
        target = "canonical_bytes::canonical_v2::envelope::impl&%0::original"
        neighbor = "canonical_bytes::neighbor"
        def function(name, result_id=12, read_id=10, narrowed=False):
            binder = f'(VarIdent "result" (VarIdentDisambiguate RustcId {result_id}))'
            require = "((Const Bool true))" if narrowed else "()"
            return (f"(Function (Fun :path {name}) :mode Exec :typ_bounds () :params () "
                    f":ret (Param :name {binder} :typ (Typ Bool) :mode Exec "
                    f":user_mut false :unwrapped_info None) :require {require} "
                    f":ensure ((Eq {binder} (Const Bool true))) :d () "
                    f":body (Read (UnfinalizedReadKind :preliminary_kind (ReadKind Copy) "
                    f":id {read_id}) (Const Int 17)))")
        original = function(target) + function(neighbor, read_id=20)
        changed = function(target, 36, 30, True) + function(neighbor, read_id=40)
        profile = {"namespace": "canonical_bytes::", "body_covered_functions": [target, neighbor],
                   "functions": inventory(original, "canonical_bytes::", (target, neighbor))}
        name = "narrow_unused_getter_domain"
        self.assertFalse(gate.coverage_control(changed, profile, name)[0])
        self.assertTrue(gate.coverage_control(changed, profile, name, original)[0])
        for wrong in (changed.replace("RustcId 36", "RustcId 37", 1),
                changed.replace('(Eq (VarIdent "result"', '(Eq (VarIdent "self"', 1),
                changed.replace("(Typ Bool)", "(Typ Int)", 1),
                changed.replace("(Const Int 17)", "(Const Int 18)", 1),
                changed.replace("(Const Bool true))) :d", "(Const Bool false))) :d", 1),
                changed.replace("RustcId 36", "RustcId ３６"),
                changed.replace("RustcId 12", "RustcId 48")):
            self.assertFalse(gate.coverage_control(wrong, profile, name, original)[0], wrong)

    def test_timeout_retains_raw_outputs_command_and_inconclusive_partial_receipt(self):
        profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        report = copy.deepcopy(self.report)
        report['func-details'] = dict.fromkeys(profile['target_functions'], {})
        report['verification-results'].update({'success': True, 'errors': 0,
            'verified': profile['expected_verified'], 'encountered-error': False})
        control = {'reverse_byte_order': self.controls['reverse_byte_order']}
        def fake_run(command, cwd, environment, **kwargs):
            logs = Path(command[command.index('--log-dir') + 1])
            if logs.name == 'positive':
                logs.mkdir(parents=True); (logs / 'crate.vir').write_text('synthetic positive VIR')
                return subprocess.CompletedProcess(command, 0, json.dumps(report), '')
            raise subprocess.TimeoutExpired(command, kwargs['timeout'],
                output=b'raw incomplete proof stdout\n', stderr=b'raw solver timeout stderr\n')
        with tempfile.TemporaryDirectory() as temporary, ExitStack() as stack:
            directory = Path(temporary)
            stack.enter_context(patch.object(gate.verifier, 'prepare_tools', return_value=directory / 'tools'))
            stack.enter_context(patch.object(gate.verifier, 'run', side_effect=fake_run))
            stack.enter_context(patch.object(gate, 'native_checks', return_value={}))
            stack.enter_context(patch.object(gate, 'require_coverage', return_value=profile['functions']))
            stack.enter_context(patch.object(gate, 'mutation_sources', return_value=control))
            stack.enter_context(patch.object(gate, 'UNIT_SOURCES', (gate.HARNESS, gate.SUBJECT)))
            stack.enter_context(patch.object(gate, 'adjust_specimen_source_lengths'))
            stack.enter_context(redirect_stdout(io.StringIO()))
            with self.assertRaisesRegex(RuntimeError, 'inconclusive'):
                gate.check(directory / 'cache', False, directory)
            specimen = directory / 'reverse_byte_order'
            self.assertEqual((specimen / 'verus.json').read_text(), 'raw incomplete proof stdout\n')
            self.assertEqual((specimen / 'verus.stderr').read_text(), 'raw solver timeout stderr\n')
            process = json.loads((specimen / 'verus-process.json').read_text())
            self.assertTrue(process['timed_out']); self.assertIsNone(process['returncode'])
            self.assertEqual((process['cwd'], process['timeout_seconds']), (str(gate.ROOT), 900))
            entries = json.loads((directory / 'mutations.json').read_text())
            self.assertEqual(len(entries), 1)
            self.assertFalse(entries[0]['killed'])
            self.assertEqual(entries[0]['classification'], 'inconclusive')


class NativeCanonicalBytes(unittest.TestCase):
    def test_actual_source_matches_independent_integer_conversions(self):
        pin = json.loads((gate.ROOT / verifier.PIN).read_text())
        with tempfile.TemporaryDirectory(prefix="zeno-byte-native-") as temporary:
            evidence = gate.native_checks(Path(temporary), pin, dict(os.environ))
        self.assertEqual(evidence["exit_code"], 0)
        for name in ("every_two_byte_integer_preserves_byte_order",
                     "widths_offsets_and_every_truncation_match_std",
                     "every_sign_bit_boundary_and_both_signed_extremes_match_std",
                     "invalid_offsets_and_widths_never_wrap_or_read_outside_input"):
            self.assertIn(f"tests::{name} ... ok", evidence["test_output"])


if __name__ == "__main__":
    unittest.main()
