"""The production source guard refuses native-only execution in proof bodies."""
import unittest
from check_collections_runtime import guard_text, masked

class RuntimeSourceGuardTests(unittest.TestCase):
    def test_verified_loop_and_ghost_proofs(self):
        text='''fn frame() {
            #[cfg_attr(verus_keep_ghost, verus_spec(invariant true, decreases 1,))]
            while false { #[cfg(verus_keep_ghost)] proof! { assert(true); } }
            #[cfg(verus_keep_ghost)] proof_decl! { let ghost value = 1; }
        }'''
        self.assertEqual(guard_text(text),['frame'])
    def test_native_only_branches_are_refused(self):
        for attribute in ['#[cfg(not(verus_keep_ghost))]', '#[cfg(any())]', '#[cfg(all(unix, not(verus_keep_ghost)))]]',
                          '#[cfg_attr(not(verus_keep_ghost), allow(dead_code))]']:
            with self.subTest(attribute=attribute),self.assertRaises(ValueError):
                guard_text('fn writes() { '+attribute+' { unverified(); } }')
    def test_cfg_expression_and_ghost_exec_are_refused(self):
        for body in ['if cfg!(not(verus_keep_ghost)) { write(); }', '#[cfg(verus_keep_ghost)] write();']:
            with self.subTest(body=body),self.assertRaises(ValueError): guard_text('fn writes() { '+body+' }')
    def test_owned_trust_and_nontermination_exemptions_are_refused(self):
        for body in ['assume(false);','admit();','#[verifier::external_body] fn bypass() {}',
                     '#[verifier::exec_allows_no_decreases_clause] loop {}','unsafe { access(); }']:
            with self.subTest(body=body),self.assertRaises(ValueError): guard_text('fn frame() { '+body+' }')
    def test_unbounded_loop_is_refused(self):
        for body in ['loop {}','while true {}','for item in values {}']:
            with self.subTest(body=body),self.assertRaises(ValueError): guard_text('fn frame() { '+body+' }')
    def test_strings_comments_and_lifetimes_preserve_function_boundaries(self):
        text='''fn frame<'a>(value: &'a str) { let text = r#"} cfg!(false) {"#;
            /* nested /* cfg(not(verus_keep_ghost)) */ comment */
            let other="{ unsafe }"; // assume(false)
        }'''
        self.assertEqual(guard_text(text),['frame'])
        self.assertEqual(masked(text).count('\n'),text.count('\n'))

if __name__ == '__main__': unittest.main()
