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
    def test_getter_controls_and_selector_target_the_unconsumed_schema_getter(self):
        source=(gate.ROOT/gate.SUBJECT).read_text()
        rows={x[0]:x for x in gate.mutations()}
        for name in ("weaken_getter_contract","narrow_getter_domain"):
            _,path,old,new,expected=rows[name]
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
