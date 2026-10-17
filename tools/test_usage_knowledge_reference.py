"""Scoped usage finite requirements matrix and controls. Model evidence, not a Rust run."""
import itertools
import unittest
from unittest.mock import patch
import strategy_knowledge_reference as R
import usage_knowledge_reference as U


class UsageTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.public=R.EXAMPLE.read_text()
        cls.validations=U.validation_inputs(cls.public)

    def usage(self,task,extra='',validations=None):
        return U.Usage([self.public+extra],task,self.validations if validations is None else validations)

    def assert_exact(self,u,expected):
        self.assertEqual(u.check(),[])
        self.assertEqual(u.advise(),expected)
        self.assertEqual(sorted({r[0] for r in u.rows('usage_applies')}),expected)
        for id in expected:
            for trace in u.explain(id):u.verify_trace(trace)

    def test_three_exact_public_tasks_and_every_answer_has_trace(self):
        cases=[('task(subject,check-program).',['check-inputs']),
               ('task(subject,observe).task(tool,zeno-fcis).task(version,v241).task(environment,sandbox).',['containment','diagnostic-order']),
               ('task(subject,learn-terms).task(tool,zeno-fcis).task(version,v241).task(environment,capacity-demo).task(family,finite-learner).task(capability,finite-domain).task(prerequisite,vocabulary).task(tuples-count,4097).',['learner-bound'])]
        for task,expected in cases:
            with self.subTest(task=task):self.assert_exact(self.usage(task),expected)

    def test_scope_matrix_missing_changed_version_units_and_boundary(self):
        for tool,version,env,size in itertools.product(('zeno-fcis','other','unknown',None),('v241','v240','unknown',None),('capacity-demo','sandbox','unknown',None),(4095,4096,4097,None)):
            task='task(subject,learn-split).task(family,finite-learner).task(capability,finite-domain).task(prerequisite,vocabulary).'
            for key,value in [('tool',tool),('version',version),('environment',env),('tuples-count',size)]:
                if value is not None:task+=f'task({key},{value}).'
            u=self.usage(task)
            applicable=tool=='zeno-fcis' and version=='v241' and env=='capacity-demo'
            self.assertEqual(u.advise(),['learner-bound'] if applicable else [])
            self.assertEqual(sorted({r[0] for r in u.rows('usage_applies')}),u.advise())
            if applicable:
                reasons=[r[2] for r in u.exclusions()]
                self.assertEqual(reasons,['size-unknown'] if size is None else ['size-limit'] if size>4096 else [])
        u=self.usage('task(subject,learn-terms).task(tool,zeno-fcis).task(version,v241).task(environment,capacity-demo).task(family,finite-learner).task(capability,finite-domain).task(prerequisite,vocabulary).task(tuples-bytes,4097).')
        self.assertEqual(u.exclusions()[0][2],'size-unknown')

    def test_family_capability_and_prerequisite_unknown_matrix(self):
        base='task(subject,learn-terms).task(tool,zeno-fcis).task(version,v241).task(environment,capacity-demo).'
        keys=('family','capability','prerequisite');values=('finite-learner','finite-domain','vocabulary')
        for choices in itertools.product(('correct','other','unknown','missing'),repeat=3):
            task=base+''.join(f'task({key},{value if choice=="correct" else choice}).' for key,value,choice in zip(keys,values,choices) if choice!='missing')
            self.assertEqual(self.usage(task).advise(),['learner-bound'] if all(c=='correct' for c in choices) else [])

    def test_numeric_comparator_is_bounded_and_rejects_text_ordering(self):
        kb=R.KB('n(-9223372036854775808).n(1).n(2).n(9223372036854775807).over(N):-n(N),N>1.under(N):-n(N),N<=1.')
        self.assertEqual(kb.query('over(N)'),[{'N':2},{'N':9223372036854775807}])
        self.assertEqual(len(kb.query('under(N)')),2)
        with self.assertRaisesRegex(ValueError,'comparison-type'):R.KB('n(a).over(N):-n(N),N>1.')
        with self.assertRaises(ValueError):R.KB('n(9223372036854775808).')

    def test_inheritance_is_directional_and_scope_is_retained(self):
        task='task(subject,learn-terms).task(tool,zeno-fcis).task(version,v241).task(environment,capacity-demo).task(family,finite-learner).task(capability,finite-domain).task(prerequisite,vocabulary).'
        self.assert_exact(self.usage(task),['learner-bound'])
        self.assertEqual(self.usage(task.replace('v241','v240')).advise(),[])
        missing_environment=self.usage("task(subject,observe).task(tool,zeno-fcis).task(version,v241).")
        self.assertEqual(missing_environment.rows("usage_unknown_environment"),[("containment",)])
        self.assertIn(("usage_unknown_environment",("containment",)),missing_environment.kb.proofs)
        self.assertNotIn(('learner','learn-terms'),self.usage(task).rows('isa'))
        cycle=self.usage(task,'isa(learner,learn-terms).')
        self.assertIn('kb-isa-cycle',cycle.check())
        self.assertEqual(cycle.advise(),[])

    def test_evidence_removal_stale_dependency_and_unpromoted_never_enable(self):
        task='task(subject,check-program).'
        self.assert_exact(self.usage(task),['check-inputs'])
        self.assertEqual(self.usage(task,validations=[]).advise(),[])
        from dataclasses import replace
        stale=[replace(v,status='stale') for v in self.validations]
        self.assertEqual(self.usage(task,validations=stale).advise(),[])
        wrong=[replace(v,dependency_sha256='f'*64) for v in self.validations]
        self.assertEqual(self.usage(task,validations=wrong).advise(),[])
        source=self.public.replace('record(check-inputs,promoted','record(check-inputs,proposed')
        u=U.Usage([source],task,self.validations)
        self.assertEqual(u.advise(),['check-inputs-old'])
        # An unpromoted or stale new record cannot erase an accepted broad record.
        source=self.public.replace('record(check-inputs,promoted','record(check-inputs,proposed')+'supersedes(missing,check-inputs-old).'
        self.assertIn('kb-unknown-lesson',U.Usage([source],task,self.validations).check())

    def test_conflict_and_unevidenced_lesson_fail_exact_checks(self):
        bad='lesson(bad,check-program).strength(bad,observed).effect(bad,hurts).'
        u=self.usage('task(subject,check-program).',bad)
        self.assertIn('kb-unresolved-conflict',u.check())
        self.assertIn('kb-lesson-without-evidence',u.check())
        self.assertEqual(u.advise(),[])
        v=self.usage('task(subject,check-program).','supersedes(check-inputs-old,check-inputs).')
        self.assertIn('kb-supersedes-cycle',v.check())

    def test_disjoint_scopes_and_narrow_supersession_preserve_broad_record(self):
        dep='a'*64
        def lesson(id,version,effect='helps'):
            return f'lesson({id},sample).scope({id},version,{version}).strength({id},declared).effect({id},{effect}).evidence({id},r,"{dep}").record({id},promoted,usage-v1,"{dep}",observation,f).'
        source=lesson('broad','v240')+lesson('other','v241','hurts')+'conflict(broad,other).'
        self.assertNotIn('kb-unresolved-conflict',U.Usage([source]).check())
        broad=lesson('broad','v240').replace('scope(broad,version,v240).','')
        narrow=lesson('narrow','v241')+'supersedes(narrow,broad).'
        source=broad+narrow
        vals=U.validation_inputs(source)
        self.assertEqual(U.Usage([source],'task(subject,sample).task(version,v241).',vals).advise(),['narrow'])
        self.assertEqual(U.Usage([source],'task(subject,sample).task(version,v240).',vals).advise(),['broad'])
        self.assertEqual(U.Usage([source],'task(subject,sample).',vals).advise(),['broad'])

    def test_layer_override_retains_both_sides_and_reports_conflict(self):
        first=self.public
        _,rules=R.Parser(self.public).program()
        facts,_=R.Parser(self.public).program()
        later=U.serialize([(p,a) for p,a in facts if p in U.METADATA and a[0]=='learner-bound'],[]).replace('4096','3')
        u=U.Usage([first,later],'task(subject,learn-terms).',self.validations)
        self.assertIn('kb-layer-conflict',u.check())
        self.assertEqual(u.rows('limit','learner-bound')[0][2],3)
        self.assertEqual(len(u.overrides),1)
        self.assertIn(('limit',('learner-bound','tuples',4096,'count')),u.overrides[0][2])
        self.assertIn(('limit',('learner-bound','tuples',3,'count')),u.overrides[0][3])

    def test_planner_excludes_sandbox_step_then_replans_from_observed_outputs(self):
        task='task(subject,observe).task(tool,zeno-fcis).task(version,v241).task(environment,sandbox).'
        p=U.plan([self.public],task,self.validations,{'model','observations'},{'answer'})
        self.assertEqual(p['steps'],['explain','isolate'])
        self.assertIn(('observe','containment','environment-blocked'),p['exclusions'])
        # A failed/unknown first step produces nothing. No diagnosis is invented.
        again=U.plan([self.public],task,self.validations,{'model','observations'},{'answer'},produced=())
        self.assertEqual(again['next_step'],'explain')
        observed=U.plan([self.public],task,self.validations,{'model','observations'},{'answer'},produced={'diagnoses'})
        self.assertEqual(observed['next_step'],'isolate')
        no_path=U.plan([self.public],task,self.validations,{'model'},{'answer'})
        self.assertIsNone(no_path['steps'])

    def test_wrong_units_and_unvalidated_blocker_do_not_prune(self):
        task='task(subject,observe).task(tool,zeno-fcis).task(version,v241).task(environment,sandbox).'
        p=U.plan([self.public],task,[],{'model'},{'answer'})
        self.assertEqual(p['steps'],['observe','explain','isolate'])
        self.assertEqual(p['exclusions'],[])

    def test_schema_typos_cardinality_reserved_names_and_provenance_faults(self):
        for extra,code in [('scope(learner-bound,versoin,v241).','kb-scope-key'),('strength(learner-bound,certain).','kb-strength'),('effect(learner-bound,awesome).','kb-effect')]:
            self.assertIn(code,self.usage('task(subject,learn-terms).',extra).check())
        for extra in ('task(subject,sample).','usage_validated(learner-bound).','limit(x,1,2).'):
            with self.assertRaises(ValueError):self.usage('task(subject,sample).',extra)
        u=self.usage('task(subject,check-program).')
        trace=u.explain('check-inputs')[0]
        with self.assertRaisesRegex(AssertionError,'trace-empty'):u.verify_trace([])
        changed=[dict(d) for d in trace];changed[-1]['rule']=999
        with self.assertRaisesRegex(AssertionError,'trace-provenance'):u.verify_trace(changed)

    def test_statement_edits_invalidate_replay_and_scope_diamonds_conflict(self):
        task='task(subject,learn-terms).task(tool,zeno-fcis).task(version,v241).task(environment,capacity-demo).task(family,finite-learner).task(capability,finite-domain).task(prerequisite,vocabulary).'
        edited=self.public.replace('limit(learner-bound,tuples,4096,count)','limit(learner-bound,tuples,3,count)')
        self.assertEqual(U.Usage([edited],task,self.validations).advise(),[])
        self.assertIn('kb-lesson-unvalidated',U.Usage([edited],task,self.validations).check())
        from dataclasses import replace
        expired=[replace(v,last_validation=0) for v in self.validations]
        self.assertEqual(U.Usage([self.public],'task(subject,check-program).',expired).advise(),[])
        facts='isa(joint,left).isa(joint,right).'
        for id,family in [('a','left'),('b','right')]:
            facts+=f'lesson({id},sample).scope({id},family,{family}).strength({id},declared).effect({id},neutral).evidence({id},r,"'+('a'*64)+f'").record({id},proposed,usage-v1,"'+('a'*64)+'",observation,f).'
        self.assertIn('kb-unresolved-conflict',U.Usage([facts+'conflict(a,b).']).check())
        for task in ('task(tool,a).task(tool,b).','task(tuples-count,-1).','task(typo,a).'):
            with self.assertRaises(ValueError):self.usage(task)

    def test_native_wire_trace_checker_and_forged_ordinals(self):
        # Synthetic wire fixture, not native execution. Native integration tests
        # feed actual compiled reports to the same verifier after build approval.
        from copy import deepcopy
        task='task(subject,check-program).'
        u=self.usage(task)
        native=(R.EXAMPLE.parents[1]/'src/strategy_knowledge/reasoning.rs').read_text().split('r#"',1)[1].split('"#;',1)[0]
        facts,user_rules=R.Parser(self.public).program()
        rules=user_rules+R.Parser(native).program()[1]
        bases=set(facts)
        ground=lambda f:{'predicate':f[0],'arguments':list(f[1])}
        traces=[]
        for id in u.advise():
            for trace in u.explain(id):
                nodes=[]
                for d in trace:
                    index=d['rule'];binding={}
                    if index is not None:
                        rule=u.kb.rules[index];index=rules.index(rule)
                        positive=[a for kind,a in rule[1] if kind=='yes']
                        for (p,terms),(q,row) in zip(positive,d['premises']):
                            self.assertEqual(p,q);self.assertTrue(u.kb.match(terms,row,binding))
                    nodes.append({'fact':ground(d['fact']),'rule':index,'bindings':binding,'premises':[ground(p) for p in d['premises']],
                                  'absent':[{'predicate':p,'arguments':[None if t==R.Var('_') else t for t in args]} for p,args in d['absent']],
                                  'comparisons':[{'operator':op,'left':a,'right':b} for op,(a,b) in d['comparisons']],
                                  'source_layer':0 if index is not None and index<len(user_rules) or index is None and d['fact'] in bases else None})
                traces.append({'conclusion':ground(trace[-1]['fact']),'trace':nodes})
        report={'status':'advised','lessons':u.advise(),'explanations':traces}
        self.assertGreater(U.verify_native_explanations(report,task),0)
        forged=deepcopy(report);forged['explanations'][0]['trace'][-1]['rule']=999
        with self.assertRaisesRegex(AssertionError,'trace-rule'):U.verify_native_explanations(forged,task)
        forged=deepcopy(report);forged['explanations'][0]['trace'][0]['source_layer']=123
        with self.assertRaisesRegex(AssertionError,'trace-layer'):U.verify_native_explanations(forged,task)
        forged=deepcopy(report);forged['explanations'][0]['trace'][-1]['premises']=[]
        with self.assertRaisesRegex(AssertionError,'trace-premise-count'):U.verify_native_explanations(forged,task)

    def test_stubbed_rules_advice_conflict_and_provenance_controls_fail(self):
        U.self_test()
        for stub,diagnostic in [('advice','advice-unknown-control'),('conflict','conflict-control'),('evidence','evidence-control'),('provenance','provenance-control'),('inheritance','inheritance-control')]:
            with self.subTest(stub=stub),self.assertRaisesRegex(AssertionError,diagnostic):U.self_test(stub)


if __name__=='__main__':unittest.main()
