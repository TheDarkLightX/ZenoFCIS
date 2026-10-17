"""Independent type-flow, trace, diagnosis, reading and region controls."""
from copy import deepcopy
import unittest
from unittest.mock import patch
import loop_strategy_reference as R
import loop_strategy_controls as Controls


def boolp(nodes,root): return {"inputs":[{"kind":"Bool"}],"outputs":[{"kind":"Bool"}],"nodes":nodes,"roots":[root]}
YES = boolp([["Bool",True]],0)
X = boolp([["Input",0]],0)
NOT_X = boolp([["Input",0],["Not",0]],1)

class StrategyReference(unittest.TestCase):
    def test_control_classification_rejects_refusal_stop_and_wrong_success(self):
        self.assertEqual(Controls.self_test(),[])
        stopped=Controls.self_test(True)
        self.assertIn("stop-not-refutation",stopped)
        self.assertIn("types-negative",stopped)
        self.assertIn("cases-negative",stopped)

    def test_type_and_trace_comparisons_are_individually_load_bearing(self):
        self.assertEqual(R.self_test(),[])
        with patch.object(R,"compare",return_value=True):
            self.assertEqual(R.self_test(),["types-negative","trace-negative","diagnosis-probe-negative","readings-negative","cover-negative","regions-negative","cases-negative"])

    def test_blocked_loop_produces_nothing_on_any_branch(self):
        p={"needs":["Proposal"],"goal":["Qualified"],"node":{"seq":[{"step":"check-program"}]}}
        self.assertEqual(R.check_plan(p),["step-input-missing","goal-unreachable"])
        p["node"]={"branch":{"on":"Proposal","cases":[]}}
        self.assertEqual(R.check_plan(p),["empty-branch","goal-unreachable"])

    def test_feedback_bound_and_early_trace_prefix(self):
        p={"needs":["Original","Proposal"],"goal":["Qualified"],"node":{"repeat":{"body":{"step":"check-program"},"until":"Qualified","bound":2}}}
        self.assertEqual(R.check_plan(p),[])
        self.assertTrue(R.conforms(p,[]))
        self.assertTrue(R.conforms(p,["check-program"]*2))
        self.assertFalse(R.conforms(p,["check-program"]*3))
        self.assertFalse(R.conforms(p,["observe"]))

    def test_branch_does_not_supply_an_undeclared_empty_path(self):
        p={"needs":["Model","Original","Proposal"],"goal":["Qualified"],"node":{"seq":[{"branch":{"on":"Model","cases":[{"step":"observe"}]}},{"step":"check-program"}]}}
        self.assertFalse(R.conforms(p,["check-program"]))
        self.assertTrue(R.conforms(p,["observe","check-program"]))

    def test_case_tree_committee_and_region_replay(self):
        domain={"kind":"Int","min":"0","max":"3"};out={"kind":"Int","min":str(R.C.R.I64_MIN),"max":str(R.C.R.I64_MAX)}
        original={"inputs":[domain],"outputs":[out],"nodes":[["Input",0],["Int","2"],["Lt",0,1],["Int","7"],["Select",2,3,0]],"roots":[4]}
        constant={**original,"nodes":[["Int","7"]],"roots":[0]};identity={**original,"nodes":[["Input",0]],"roots":[0]}
        pred={**original,"outputs":[{"kind":"Bool"}],"nodes":original["nodes"][:3],"roots":[2]}
        problem={"learn":{"integers":[constant,identity],"predicates":[pred]},"config":{"initial":2,"spot_checks":1}}
        run=R.case_search(problem,original)
        self.assertEqual(run["meaning"],(7,7,2,3))
        self.assertEqual(run["spot_queries"],[1])
        problem["config"]["initial"]=1
        sampled=R.case_search(problem,original)
        self.assertEqual(sampled["meaning"],(7,7,7,7))  # The full checker must refute it.
        problem["config"]["queries"]=0
        with self.assertRaisesRegex(ValueError,"query-budget"):R.case_search(problem,original)

    def test_region_guidance_probes_a_rare_exception_inside_a_learned_case(self):
        domain={"kind":"Int","min":"0","max":"7"};out={"kind":"Int","min":str(R.C.R.I64_MIN),"max":str(R.C.R.I64_MAX)}
        original={"inputs":[domain],"outputs":[out],"nodes":[["Input",0],["Int","4"],["Lt",0,1],["Int","2"],["Eq",0,3],["Int","9"],["Int","5"],["Select",4,5,6],["Int","7"],["Select",2,7,8]],"roots":[9]}
        terms=[{**original,"nodes":[["Int",str(v)]],"roots":[0]} for v in (5,7,9)]
        predicates=[{**original,"outputs":[{"kind":"Bool"}],"nodes":[["Input",0],["Int",str(v)],[op,0,1]],"roots":[2]} for v,op in ((4,"Lt"),(2,"Eq"))]
        problem={"learn":{"integers":terms,"predicates":predicates},"config":{"initial":2,"spot_checks":1}}
        run=R.case_search(problem,original)
        self.assertEqual(run["spot_queries"][0],2)
        self.assertEqual(run["meaning"],(5,5,9,5,7,7,7,7))

    def test_minimal_weak_fault_worlds_and_best_probe(self):
        problem={"circuit":{"inputs":1,"gates":[{"kind":"not","wires":[0]},{"kind":"not","wires":[1]}],"outputs":[2]},"vector":[0],"observations":[[2,1]],"max_faults":2}
        diagnoses, probe=R.diagnostic(problem)
        self.assertEqual([d["gates"] for d in diagnoses],[[0],[1]])
        self.assertEqual(probe,{"wire":1,"zero":1,"one":1})
        self.assertTrue(all(all(w[2]==1 for w in d["worlds"])for d in diagnoses))
        problem["observations"].append([1,0])
        diagnoses, probe=R.diagnostic(problem)
        self.assertEqual([d["gates"] for d in diagnoses],[[0]])
        self.assertIsNone(probe)

    def test_readings_collapse_spelling_and_ask_only_for_answer_dependence(self):
        equal=boolp([["Input",0],["Eq",0,0]],1)
        p={"background":YES,"query":"count","readings":[{"id":"s1","alternatives":[YES,equal,X]}]}
        distinct, answers, q=R.reading_answers(p)
        self.assertEqual(distinct,[[0,2]])
        self.assertEqual([a for _,a in answers],[2,1])
        self.assertEqual(q["ordinal"],0)
        self.assertEqual((q["holds"],q["does_not_hold"]),([0],[2]))
        p["readings"][0]["alternatives"]=[X,NOT_X]
        _,answers,q=R.reading_answers(p)
        self.assertEqual([a for _,a in answers],[1,1]);self.assertIsNone(q)

    def test_missing_intended_reading_and_empty_models_are_not_owner_intent(self):
        p={"background":YES,"query":"count","readings":[{"id":"s1","alternatives":[X]}]}
        _,answers,q=R.reading_answers(p)
        self.assertEqual(answers[0][1],1);self.assertIsNone(q)
        p["background"]=boolp([["Bool",False]],0)
        p["query"]="must";p["readings"].append({"id":"question","alternatives":[YES]})
        self.assertEqual(R.reading_answers(p)[1][0][1],0)

    def test_greedy_cover_tie_and_region_minority_side(self):
        self.assertEqual(R.greedy_cover([[True,True,False],[False,False,True]],[0,1,2]),[0,1])
        self.assertIsNone(R.greedy_cover([[True,False]],[0,1]))
        regions=[[True,True,False,False],[False,False,True,True]]
        predicates=[[True,False,False,False],[False,False,True,False]]
        self.assertEqual(R.minority_cells(regions,predicates),[[True,False,False,False],[False,False,True,False]])

    def test_diagnostic_and_reading_negative_reports_fail_their_comparisons(self):
        problem={"circuit":{"inputs":1,"gates":[{"kind":"not","wires":[0]},{"kind":"not","wires":[1]}],"outputs":[2]},"vector":[0],"observations":[[2,1]],"max_faults":2}
        plan={"needs":["Model"],"goal":["Answer"],"node":{"seq":[{"step":"observe"},{"step":"explain"},{"step":"isolate"}]}}
        diagnoses,q=R.diagnostic(problem)
        report={"plan":plan,"trace":["observe","explain"],"kind":"diagnose","diagnoses":diagnoses,"question":q}
        self.assertEqual(R.validate(problem,report),[])
        report["diagnoses"]=[];report["question"]=None
        self.assertEqual(R.validate(problem,report),["diagnoses","probe"])
        with patch.object(R,"compare",return_value=True):self.assertEqual(R.validate(problem,report),[])
        problem={"background":YES,"query":"count","readings":[{"id":"s1","alternatives":[YES,X]}]}
        report={"plan":{"needs":["Proposal","Model"],"goal":["Answers"],"node":{"seq":[{"step":"formalize-alternatives"},{"step":"distinct-readings"},{"step":"solve-every-reading"}]}},"trace":["formalize-alternatives"],"kind":"ask-when-it-matters","distinct":[],"answers":[],"question":None}
        self.assertEqual(R.validate(problem,report),["reading-distinct","reading-answers","reading-question"])
        with patch.object(R,"compare",return_value=True):self.assertEqual(R.validate(problem,report),[])

if __name__=="__main__":unittest.main()
