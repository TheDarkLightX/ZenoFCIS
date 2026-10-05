"""The control catalogue is executable and fails on stale/ambiguous anchors."""
import unittest
import check_catalog_v2 as gate

class Controls(unittest.TestCase):
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

if __name__=="__main__":
    unittest.main()
