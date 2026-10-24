"""Fail-closed checks for native divergence and proof-evidence acceptance."""
import copy
from pathlib import Path
import unittest
import check_arithmetic_proof as gate


class BindingTests(unittest.TestCase):
    def test_current_source_configuration_is_admitted(self):
        for p in gate.CLOSURE:
            gate.audit_native_binding((gate.ROOT/p).read_text())

    def test_native_only_escapes_refuse(self):
        snippets = (
            '#[cfg(not(verus_keep_ghost))] return Some(0);',
            '#[cfg_attr(not(verus_keep_ghost), inline)] fn f() {}',
            '#[cfg_attr(verus_keep_ghost, cfg(any()))] fn f() {}',
            '#[cfg(target_os="linux")] fn f() {}',
            'if cfg!(not(verus_keep_ghost)) { return None; }',
            'macro_rules! hidden { () => { 0 } } hidden!();',
            '#[verifier::external_body] fn f() {}',
            'proof! { assume(false); }',
            'include!("unreviewed.rs");',
        )
        for source in snippets:
            with self.subTest(source=source), self.assertRaises(ValueError):
                gate.audit_native_binding(source)

    def test_unterminated_attribute_refuses(self):
        with self.assertRaises(ValueError):
            gate.audit_native_binding('#[cfg(verus_keep_ghost)')

    def test_exact_count_and_inventory(self):
        pin = dict(version='v',commit='c',rust_toolchain='r')
        report = {'verus':dict(version='v',commit='c',toolchain='r'),
            'func-details':{'a':{},'b':{}},
            'verification-results':dict(success=True,errors=0,verified=3,
                **{'encountered-error':False,'encountered-vir-error':False,
                   'is-verifying-entire-crate':True})}
        profile = dict(expected_verified=3,functions=['a','b'])
        self.assertTrue(gate.accepted(report,pin,profile))
        for field,value in [('verified',True),('verified',2),('errors',1),
                            ('success',False),('is-verifying-entire-crate',False)]:
            changed=copy.deepcopy(report)
            changed['verification-results'][field]=value
            self.assertFalse(gate.accepted(changed,pin,profile))
        changed=copy.deepcopy(report);changed['func-details']['c']={}
        self.assertFalse(gate.accepted(changed,pin,profile))
        changed=copy.deepcopy(report);changed['verus']['commit']='other'
        self.assertFalse(gate.accepted(changed,pin,profile))


if __name__ == '__main__':
    unittest.main()
