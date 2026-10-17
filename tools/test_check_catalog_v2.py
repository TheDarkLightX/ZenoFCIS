"""The control catalogue is executable and fails on stale/ambiguous anchors."""
import unittest
import copy
import json
from pathlib import Path
import check_catalog_v2 as gate
from verus_coverage import descriptor_inventory, require_coverage

class Controls(unittest.TestCase):
    def test_an_emitted_unreviewed_candidate_cannot_enter_normal_acceptance(self):
        gate.require_reviewed_profile({})
        gate.require_reviewed_profile({'reviewed': True})
        for flag in (False, None, 0, 1, 'true'):
            with self.assertRaisesRegex(ValueError, 'independent technical review'):
                gate.require_reviewed_profile({'reviewed': flag})

    def test_candidate_keeps_every_actual_target_and_never_claims_review(self):
        pin = json.loads((gate.ROOT / gate.verifier.PIN).read_text())
        report = {'verus': {'version': pin['version'], 'commit': pin['commit'],
            'toolchain': pin['rust_toolchain']},
            'verification-results': {'success': True, 'errors': 0, 'verified': 2,
                'encountered-error': False, 'encountered-vir-error': False,
                'is-verifying-entire-crate': True},
            'func-details': {'catalog_v2::a': {}, 'catalog_v2::b': {}, 'outside::c': {}}}
        vir = ''.join(f'(Function (Fun :path catalog_v2::{name}) :mode Exec :typ_bounds () '
            ':params () :ret () :require () :ensure (true) :d () :body (42))' for name in ('a', 'b'))
        candidate = gate.candidate_profile(vir, {'exit_code': 0, 'report': report}, pin)
        self.assertFalse(candidate['reviewed'])
        self.assertEqual(candidate['target_functions'], ['catalog_v2::a', 'catalog_v2::b'])
        self.assertEqual(candidate['body_covered_functions'], candidate['target_functions'])
        for section, key, value in (('verus', 'version', 'wrong'),
                ('verification-results', 'is-verifying-entire-crate', False),
                ('verification-results', 'errors', 1)):
            wrong = copy.deepcopy(report); wrong[section][key] = value
            with self.assertRaises(ValueError):
                gate.candidate_profile(vir, {'exit_code': 0, 'report': wrong}, pin)


    def test_control_anchors_and_failure_partition(self):
        controls=gate.mutations()
        self.assertEqual(len({x[0] for x in controls}),len(controls))
        self.assertEqual({x[4] for x in controls},{"proof","coverage"})
        for name,path,old,new,expected in controls:
            with self.subTest(name=name):
                source=(gate.ROOT/path).read_text()
                changed=gate.replace_once(source,old,new)
                self.assertNotEqual(changed,source)
                self.assertEqual(source.count(old),1)
    def test_getter_controls_and_selector_target_the_unconsumed_schema_getter(self):
        source=(gate.ROOT/gate.SUBJECT).read_text()
        rows={x[0]:x for x in gate.mutations()}
        expected_clauses = {
            "weaken_getter_contract": "ensures (result@ == self.view().0) || true,",
            "narrow_getter_domain": "requires false, ensures result@ == self.view().0,",
        }
        for name in expected_clauses:
            _,path,old,new,expected=rows[name]
            self.assertEqual(new,expected_clauses[name],name)
            self.assertEqual((path,expected),(gate.SUBJECT,"coverage"))
            following=source[source.index(old):].split("\n",2)[1]
            self.assertTrue(following.lstrip().startswith("pub fn original_schema(&self)"),name)
        prefix="catalog_v2::execution_v2::catalog::impl&%0::"
        vir="".join(f"(Function (Fun :path {prefix}{getter}) :mode Exec :typ_bounds () :params () :ret () "
                    ":require () :ensure (true) :d () :body (42))" for getter in ("original_contract","original_schema"))
        names=(prefix+"original_contract",prefix+"original_schema")
        profile={"namespace":"catalog_v2::","body_covered_functions":list(names),
                 "functions":gate.inventory(vir,"catalog_v2::",names)}
        schema_at=vir.index(prefix+"original_schema")
        def changed(old,new,at):
            return vir[:at]+vir[at:].replace(old,new,1)
        for name,old,new in (("weaken_getter_contract",":ensure (true)",":ensure (false)"),
                             ("narrow_getter_domain",":require ()",":require (true)")):
            intended,refusal,detail=gate.coverage_control(changed(old,new,schema_at),profile,name)
            self.assertEqual((intended,detail["function"]),(True,prefix+"original_schema"),name)
            # A change to the consumed contract getter is an unrelated refusal.
            intended,refusal,detail=gate.coverage_control(changed(old,new,0),profile,name)
            self.assertIsNotNone(refusal)
            self.assertFalse(intended,name)
    def cached_policy_fixture(self):
        namespace = "catalog_v2::"
        prefix = namespace + "execution_v2::catalog::"
        descriptor = namespace + "execution_v2::composition::descriptor::"
        def function(name, *, mode="Exec", requires="", ensures="true", body="(42)", params=""):
            return (f"(Function (Fun :path {name}) :mode {mode} :typ_bounds () "
                    f":params ({params}) :ret () :require ({requires}) :ensure ({ensures}) "
                    f":d () :body {body})")
        source = function(descriptor + "DescriptorData::model", mode="Spec", ensures="", body="None")
        declaration = function(descriptor + "DescriptorData::project", body="None")
        source += declaration
        source += "".join(function(descriptor + f"impl&%{index}::{method}", mode=mode,
                                   ensures="" if mode == "Spec" else "true")
                          for index in (2, 3)
                          for method, mode in (("model", "Spec"), ("project", "Exec")))
        target = prefix + "bind_original"
        other = prefix + "bind_checked"
        source += function(target) + function(other)
        raw, _ = descriptor_inventory(source, namespace, descriptor)
        bodies = [name for name, row in raw.items() if row["mode"] == "Exec"]
        functions, declarations = descriptor_inventory(source, namespace, descriptor, tuple(bodies))
        profile = {"namespace": namespace, "functions": functions,
                   "abstract_declarations": declarations, "body_covered_functions": bodies}
        require_coverage(source, profile)
        drift = source.replace(declaration, function(descriptor + "DescriptorData::project",
                                                    ensures="changed-read-id", body="None"))
        return source, drift, profile, target, other, function

    def test_cached_policy_body_control_is_independent_of_first_refusal_message(self):
        source, drift, profile, target, _, function = self.cached_policy_fixture()
        for base in (source, drift):
            with self.subTest(descriptor_drift=base == drift):
                changed = base.replace(function(target), function(target, body="(cached-policy 42)"))
                caught, refusal, detail = gate.coverage_control(changed, profile, "cached_policy_before_admission")
                self.assertTrue(caught)
                self.assertEqual(detail, {"function": target, "field": "body_sha256", "intended_refusal": True})
                if base == drift:
                    self.assertEqual(refusal, "translated descriptor declarations changed")

    def test_cached_policy_requires_exact_inventory_and_unchanged_contracts(self):
        source, drift, profile, target, other, function = self.cached_policy_fixture()
        original = function(target)
        changed = function(target, body="(cached-policy 42)")
        for base in (source, drift):
            for reason, refused in {
                    "no body delta": base,
                    "unrelated body delta": base.replace(function(other), function(other, body="(43)")),
                    "changed signature": base.replace(original, function(target, body="(43)", params="changed")),
                    "changed requires": base.replace(original, function(target, body="(43)", requires="false")),
                    "changed ensures": base.replace(original, function(target, body="(43)", ensures="false")),
                    "missing target": base.replace(original, ""),
                    "missing other": base.replace(original, changed).replace(function(other), ""),
                    "added other": base.replace(original, changed) + function(other + "_extra"),
            }.items():
                with self.subTest(descriptor_drift=base == drift, reason=reason):
                    self.assertFalse(gate.coverage_control(refused, profile, "cached_policy_before_admission")[0])

    def test_stale_or_ambiguous_control_is_not_accepted(self):
        for source in ("missing", "twice twice"):
            with self.assertRaises(ValueError):
                gate.replace_once(source,"twice","changed")
    def test_report_must_prove_entire_crate_with_pinned_tool(self):
        self.assertFalse(gate.qualified({"exit_code":0,"report":{}},{}))
        self.assertFalse(gate.qualified({"exit_code":1,"report":{}},{}))
    def test_outbox_control_requires_its_own_loop_obligation(self):
        result={"exit_code":1,"report":{"verification-results":{
            "success":False,"errors":1,"encountered-vir-error":False,
            "is-verifying-entire-crate":True}}}
        diagnostics=("invariant not satisfied at end of loop body\n"+str(gate.MATCHING)
                     +"\nspec::branch(defs@,bs@[j],ls@)")
        self.assertTrue(gate.proof_control(result,"omit_unused_outbox",gate.MATCHING,diagnostics))
        for unrelated in (diagnostics.replace("spec::branch", "spec::other"),
                          diagnostics+"\nResource limit exceeded",
                          diagnostics.replace(str(gate.MATCHING),"dependency.rs")):
            self.assertFalse(gate.proof_control(result,"omit_unused_outbox",gate.MATCHING,unrelated))
        self.assertFalse(gate.proof_control(result,"omit_policy_equality",gate.MATCHING,diagnostics))
        result["report"]["verification-results"]["encountered-vir-error"]=True
        self.assertFalse(gate.proof_control(result,"omit_unused_outbox",gate.MATCHING,diagnostics))

class FailureOrder(unittest.TestCase):
    def test_first_survivor_precedes_later_timeout_and_cli_retains_all_rows(self):
        from contextlib import ExitStack
        import os
        import subprocess
        import sys
        import tempfile
        from unittest.mock import patch
        pin = (gate.ROOT/gate.verifier.PIN).read_text()
        for jobs in (1, 2):
            for proof_refusal in (False, True):
                with self.subTest(jobs=jobs, proof_refusal=proof_refusal), \
                        tempfile.TemporaryDirectory() as tmp, ExitStack() as stack:
                    root = Path(tmp)
                    subject = Path('subject.rs')
                    (root/subject).write_text('old')
                    for path, content in ((gate.verifier.PIN, pin), (gate.PROFILE, '{"expected_verified":1}')):
                        (root/path).parent.mkdir(parents=True, exist_ok=True)
                        (root/path).write_text(content)
                    seen = []
                    prepared = object()
                    def run(source, directory, command, environment):
                        directory.mkdir(parents=True)
                        (directory/'crate.vir').write_text('fixture')
                        (directory/'stderr').write_text('')
                        return {'report':{'verification-results':{'verified':1}}, 'stderr':str(directory/'stderr')}
                    def native(source, directory, environment, **kwargs):
                        self.assertIs(kwargs['prepared'], prepared)
                        directory.mkdir(parents=True)
                        positive = source == root
                        if not positive:
                            seen.append(source.name)
                            if source.name == 'second-timeout':
                                raise subprocess.TimeoutExpired('native', 1)
                        killed = not positive and (proof_refusal or source.name == 'last-killed')
                        (directory/'test.stdout').write_text('test result: FAILED.' if killed else
                            'test result: ok. 8 passed; 0 failed; 0 ignored')
                        return {'compile_exit':0, 'test_exit':101 if killed else 0, 'stdout':str(directory/'test.stdout')}
                    controls = [(name, subject, 'old', 'new', 'proof') for name in
                                ('first-survivor', 'second-timeout', 'last-killed')]
                    replacements = {'ROOT':root, 'snapshot':lambda *_:{str(subject):'fixture'},
                        'run':run, 'native_tests':native, 'prepare_native_dependencies':lambda *a:prepared,
                        'qualified':lambda *a:True, 'observed_inventory':lambda *a:({'catalog_v2::x':{'mode':'Exec'}}, None),
                        'require_coverage':lambda *a:None, 'require_reviewed_profile':lambda *a:None,
                        'adjust_specimen_source_lengths':lambda *a:None, 'mutations':lambda:controls,
                        'proof_control':lambda result, name, *a:name != 'first-survivor'}
                    stack.enter_context(patch.dict(os.environ, {'ZENO_FCIS_CONTROL_JOBS':str(jobs)}))
                    for name, value in replacements.items():
                        stack.enter_context(patch.object(gate, name, value))
                    stack.enter_context(patch.object(gate.verifier, 'prepare_tools', return_value=root/'tools'))
                    stack.enter_context(patch.object(gate.verifier, 'verify_tool_files'))
                    stack.enter_context(patch.object(gate.verifier, 'evidence_bindings', return_value={}))
                    output = root/'receipt.json'
                    stack.enter_context(patch.object(sys, 'argv', ['catalog', '--out', str(output)]))
                    self.assertEqual(gate.main(), 1)
                    receipt = json.loads(output.read_text())
                    self.assertIn('first-survivor', receipt['failure'])
                    self.assertEqual(receipt['status'], 'unfinished')
                    self.assertEqual([r['name'] for r in receipt['mutations']], [c[0] for c in controls])
                    self.assertEqual([r['killed'] for r in receipt['mutations']], [False, False, True])
                    self.assertEqual(receipt['mutations'][1]['error_type'], 'TimeoutExpired')
                    self.assertCountEqual(seen, [c[0] for c in controls])


if __name__=="__main__":
    unittest.main()
