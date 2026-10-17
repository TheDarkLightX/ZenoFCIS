"""Guard the actual law proof inventory and mutation anchors, not proof fixtures."""
import copy
import json
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest.mock import patch
import check_laws_v2 as gate
import check_verus as verifier
from verus_coverage import inventory

class LawCoverage(unittest.TestCase):
    def test_every_runtime_body_and_total_contract_is_covered(self):
        profile=json.loads((gate.ROOT/gate.PROFILE).read_text())
        runtime={name for name,data in profile['functions'].items() if data['mode']=='Exec'}
        self.assertEqual(len(profile['functions']),1586)
        self.assertEqual(len(runtime),641)
        self.assertEqual(set(profile['body_covered_functions']),runtime)
        self.assertTrue(all(v['requires']==0 and v['ensures']>0 for v in profile['functions'].values() if v['mode']=='Exec'))
        for suffix in ('laws::evaluate','laws::evaluate_into','laws::frame::observe','laws::predicate::node','laws::atoms::divide'):
            self.assertTrue(any(name.endswith(suffix) for name in runtime),suffix)
        self.assertTrue(any(name.endswith('spec::success_checks_every_applicable') for name in profile['functions']))

    def test_production_verus_command_suppresses_only_trigger_printing(self):
        class CommandCaptured(Exception):
            pass
        tools = Path("captured-pinned-tools")
        cache = Path("unused-laws-cache")
        evidence = Path("unused-laws-evidence")
        pin = json.loads((gate.ROOT / verifier.PIN).read_text())
        with patch.object(verifier, "prepare_tools", return_value=tools) as prepare, \
                patch.object(verifier, "run", side_effect=CommandCaptured) as run, \
                patch.object(gate, "native_checks") as native:
            with self.assertRaises(CommandCaptured):
                gate.check(cache, False, evidence)
        prepare.assert_called_once_with(cache, pin, False)
        run.assert_called_once()
        native.assert_not_called()
        command, cwd, environment = run.call_args.args
        self.assertEqual(command[0], str(tools / "verus"))
        self.assertEqual(cwd, gate.ROOT)
        self.assertEqual(environment["RUSTUP_TOOLCHAIN"], pin["rust_toolchain"])
        self.assertEqual(environment["VERUS_Z3_PATH"], str(tools / "z3"))
        self.assertEqual(command.count("--triggers-mode"), 1)
        self.assertEqual(command[command.index("--triggers-mode") + 1], "silent")
        self.assertEqual(command.count("silent"), 1)
        for flag in ("--crate-type=lib", "--edition=2024", "--no-cheating",
                     "--no-external-by-default", "--output-json"):
            self.assertEqual(command.count(flag), 1, flag)
        self.assertEqual(command[command.index("--num-threads") + 1], "2")
        self.assertEqual(command[command.index("-V") + 1], "spinoff-all")
        self.assertEqual(command[-3:], ["--log-dir", str(evidence / "positive"), str(gate.HARNESS)])
        self.assertEqual([command[i + 1] for i, arg in enumerate(command) if arg == "--log"],
                         ["vir", "vir-option=no_span+no_type+no_fn_details"])
        # The positive command retains the default budget and whole-crate scope.
        # The separate shrink_delivery_ordinals mutation keeps its own rlimit.
        self.assertNotIn("--rlimit", command)
        self.assertFalse(any(arg.startswith(("--verify-function", "--verify-module", "--no-verify"))
                             for arg in command))

    def test_meaningful_controls_remain_distinct_from_coverage_controls(self):
        mutants=gate.mutation_sources()
        self.assertEqual(len(mutants),28)
        self.assertEqual(sum(kind=='proof' for _,_,kind in mutants.values()),23)
        self.assertEqual(mutants['observe_before_read_permission'][2],'coverage')
        for path,changed,kind in mutants.values():
            self.assertIn(path,gate.UNIT_SOURCES)
            self.assertNotEqual(changed,(gate.ROOT/path).read_text())
            self.assertIn(kind,('proof','coverage'))

    def test_weakened_contract_keeps_runtime_body_and_limits_warning_suppression(self):
        source = (gate.ROOT / gate.SUBJECT).read_text()
        path, changed, kind = gate.mutation_sources()['weaken_entry_contract']
        self.assertEqual((path, kind), (gate.SUBJECT, 'coverage'))
        self.assertEqual(changed[changed.index('pub fn evaluate<'):],
                         source[source.index('pub fn evaluate<'):])
        self.assertEqual(changed.count('verifier::allow(dead_reveal)'), 1)
        self.assertNotIn('verifier::allow(dead_reveal)', source)
        self.assertIn('#[cfg_attr(verus_keep_ghost, verifier::allow(dead_reveal))]', changed)
        self.assertIn('verus_spec(result => ensures true,)', changed)

    def test_forged_report_cannot_omit_law_entry_or_completeness_theorem(self):
        profile=json.loads((gate.ROOT/gate.PROFILE).read_text())
        pin=json.loads((gate.ROOT/verifier.PIN).read_text())
        pin.update({key:profile[key] for key in ('expected_verified','target_functions')})
        report={'verus':{'commit':pin['commit'],'version':pin['version']},
            'verification-results':{'success':True,'errors':0,'verified':profile['expected_verified'],'encountered-error':False,
                'encountered-vir-error':False,'is-verifying-entire-crate':True},
            'func-details':dict.fromkeys(pin['target_functions'],{})}
        self.assertTrue(verifier.accepted(report,pin))
        for missing in pin['target_functions']:
            changed={**report,'func-details':{k:v for k,v in report['func-details'].items() if k!=missing}}
            self.assertFalse(verifier.accepted(changed,pin))
        changed={**report,'verification-results':{**report['verification-results'],'is-verifying-entire-crate':False}}
        self.assertFalse(verifier.accepted(changed,pin))

    def test_every_declared_source_is_hashed_and_symlinks_refuse(self):
        self.assertEqual(set(gate.snapshot()),set(map(str,gate.SOURCES)))
        with patch.object(Path,'is_symlink',return_value=True):
            with self.assertRaisesRegex(RuntimeError,'symbolic link'):gate.snapshot()

class RefusalAdmission(unittest.TestCase):
    def setUp(self):
        self.pin=json.loads((gate.ROOT/verifier.PIN).read_text())
        self.profile={'target_functions':['laws::intended']}
        self.report={'verus':{'version':self.pin['version'],'commit':self.pin['commit'],
            'toolchain':self.pin['rust_toolchain']},'func-details':{'laws::intended':{}},
            'verification-results':{'success':False,'errors':1,'verified':1,
                'encountered-error':True,'encountered-vir-error':False,
                'is-verifying-entire-crate':True}}
        self.name='reset_shared_meter'
        self.path,self.changed,_=gate.mutation_sources()[self.name]
        self.first,self.last=verifier.function_lines(self.changed,'evaluate_into')
        self.error=f'error: postcondition not satisfied\n --> {self.path}:{self.first}:1\n'
        self.terminal='error: aborting due to 1 previous error\n'

    def accepted(self,stderr,report=None,code=1):
        return gate.semantic_refusal(SimpleNamespace(returncode=code,stderr=stderr),
            report or self.report,self.name,self.path,self.changed,gate.ROOT,
            self.pin,self.profile)

    def test_every_retained_mutant_has_one_precise_target(self):
        mutations=gate.mutation_sources()
        self.assertEqual(set(gate.SEMANTIC_FUNCTIONS),{n for n,(_,_,k) in mutations.items() if k=='proof'})
        self.assertEqual(set(gate.COVERAGE_FIELDS),{n for n,(_,_,k) in mutations.items() if k=='coverage'})
        for name,(_,changed,kind) in mutations.items():
            if kind=='proof':
                first,last=verifier.function_lines(changed,gate.SEMANTIC_FUNCTIONS[name])
                self.assertGreater(last,first,name)

    def test_unknown_and_operational_failures_are_never_kills(self):
        self.assertTrue(self.accepted(self.error+self.terminal))
        for text in ['SMT result UNKNOWN','solver returned unknown','Resource limit (rlimit) exceeded',
                'timed out','out of memory','transport error','internal compiler error',
                'error[E0308]: mismatched types']:
            self.assertFalse(self.accepted(self.error+self.terminal+text),text)
        for code in (0,2,101,124,-9):
            self.assertFalse(self.accepted(self.error+self.terminal,code=code),code)

    def test_wrong_source_function_pin_or_partial_report_is_refused(self):
        for text in [self.error.replace(str(self.path),'other/'+self.path.name)+self.terminal,
                self.error.replace(f':{self.first}:1',f':{self.last+1}:1')+self.terminal,
                self.error,self.error+self.terminal.replace('1 previous','2 previous'),
                self.error+'error: assertion failed\n --> unrelated.rs:1:1\n'
                    +self.terminal.replace('1 previous error','2 previous errors')]:
            self.assertFalse(self.accepted(text),text)
        for section,key,value in [('verus','version','wrong'),('verus','commit','wrong'),
                ('verus','toolchain','wrong'),('verification-results','is-verifying-entire-crate',False),
                ('verification-results','encountered-error',False),
                ('verification-results','encountered-vir-error',True),
                ('verification-results','errors',True),('verification-results','verified',0)]:
            report=copy.deepcopy(self.report);report[section][key]=value
            self.assertFalse(self.accepted(self.error+self.terminal,report),(section,key))
        report={**self.report,'func-details':{}}
        self.assertFalse(self.accepted(self.error+self.terminal,report))

    def test_historical_resource_failures_remain_inconclusive(self):
        # Preserve the legacy declarations; do not silently convert either control.
        for name in ('change_declared_law_order','shrink_delivery_ordinals'):
            path,changed,kind=gate.mutation_sources()[name]
            self.assertEqual(kind,'proof')
            first,_=verifier.function_lines(changed,gate.SEMANTIC_FUNCTIONS[name])
            proc=SimpleNamespace(returncode=1,stderr=f'error: while loop: Resource limit (rlimit) exceeded\n --> {path}:{first}:1\n'+self.terminal)
            self.assertFalse(gate.semantic_refusal(proc,self.report,name,path,changed,
                gate.ROOT,self.pin,self.profile))

    def vir(self,overrides=None,extra=''):
        overrides=overrides or {}
        names=['laws::predicate::node','laws::evaluate','laws::spec::magnitude','unrelated']
        parts=[]
        for suffix in names:
            name='laws::execution_v2::'+suffix
            values=dict(mode='Spec' if suffix.endswith('magnitude') else 'Exec',
                requires='()',ensures='()' if suffix.endswith('magnitude') else '(true)',
                body='(body)',params='()')
            values.update(overrides.get(suffix,{}))
            parts.append(f'(Function (Fun :path {name}) :mode {values["mode"]} :typ_bounds () '
                f':params {values["params"]} :ret None :require {values["requires"]} '
                f':ensure {values["ensures"]} :d () :body {values["body"]})')
        return '\n'.join(parts)+extra

    def coverage_profile(self):
        bodies=['laws::execution_v2::'+n for n in ('laws::predicate::node','laws::evaluate','unrelated')]
        return dict(namespace='laws::',body_covered_functions=bodies,
            functions=inventory(self.vir(),'laws::',tuple(bodies)))

    def test_coverage_refusal_requires_the_intended_field_and_inventory(self):
        profile=self.coverage_profile()
        changes={'observe_before_read_permission':('laws::predicate::node',{'body':'(other)'}),
            'weaken_entry_contract':('laws::evaluate',{'ensures':'(false)'}),
            'narrow_entry_domain':('laws::evaluate',{'requires':'(true)'}),
            'changed_specification':('laws::spec::magnitude',{'body':'(other)'})}
        for name,(suffix,values) in changes.items():
            self.assertTrue(gate.coverage_refusal(self.vir({suffix:values}),profile,name)[0],name)
            self.assertFalse(gate.coverage_refusal(self.vir({'unrelated':{'body':'(other)'}}),profile,name)[0],name)
            self.assertFalse(gate.coverage_refusal(self.vir(),profile,name)[0],name)
            extra='\n(Function (Fun :path laws::unexpected) :mode Exec :typ_bounds () :params () :ret None :require () :ensure (true) :d () :body (body))'
            self.assertFalse(gate.coverage_refusal(self.vir({suffix:values},extra),profile,name)[0],name)
        self.assertFalse(gate.coverage_refusal(self.vir({'laws::predicate::node':
            {'body':'(other)','ensures':'(false)'}}),profile,'observe_before_read_permission')[0])

    def test_uncontracted_control_requires_exact_added_executable(self):
        profile=self.coverage_profile()
        probe='\n(Function (Fun :path laws::execution_v2::laws::uncontracted_law_probe) :mode Exec :typ_bounds () :params () :ret None :require () :ensure () :d () :body (body))'
        self.assertTrue(gate.coverage_refusal(self.vir(extra=probe),profile,'uncontracted_executable')[0])
        for bad in [probe.replace(':mode Exec',':mode Spec'),probe.replace(':ensure ()',':ensure (true)'),
                probe.replace(':require ()',':require (true)'),probe.replace('uncontracted_law_probe','other_probe')]:
            self.assertFalse(gate.coverage_refusal(self.vir(extra=bad),profile,'uncontracted_executable')[0])

if __name__=='__main__':unittest.main()
