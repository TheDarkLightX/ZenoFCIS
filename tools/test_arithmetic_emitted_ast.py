import json
from pathlib import Path
import unittest
import arithmetic_emitted_ast as ast
import check_arithmetic_emission as gate

ROOT=Path(__file__).resolve().parents[1]


class ExpressionBinding(unittest.TestCase):
    def test_emitter_and_proof_include_the_same_helpers(self):
        emitter=(ROOT/gate.EMITTER).read_text()
        harness=(ROOT/gate.primitive.HARNESS).read_text()
        bindings=gate.helper_path_binding(emitter,harness)
        self.assertEqual({row['verified_source'] for row in bindings},
                         {str(gate.primitive.SCALAR),str(gate.primitive.WIDE)})
        for changed_emitter,changed_harness in (
            (emitter.replace('include_str!("value/wide.rs")','include_str!("other/wide.rs")'),harness),
            (emitter,harness.replace('/value/scalar.rs','/value/other.rs')),
            (emitter+'\ninclude_str!("unexpected.rs");',harness)):
            with self.assertRaisesRegex(ValueError,'E2_helper_paths'):
                gate.helper_path_binding(changed_emitter,changed_harness)

    def test_every_expression_family(self):
        values={
            'input[0]':['input',0],'-1_i128':['int','-1'],
            'v0.checked_add(v1)?':['add',0,1], 'v0.checked_sub(v1)?':['sub',0,1],
            'arithmetic::scalar::multiply(v0,v1)?':['mul',0,1],
            'arithmetic::scalar::divide(true,v0,v1)?':['div_ceil',0,1],
            'arithmetic::scalar::divide(false,v0,v1)?':['div_floor',0,1],
            'arithmetic::scalar::mul_div(true,v0,v1,v2)?':['mul_div_ceil',0,1,2],
            'arithmetic::scalar::mul_div(false,v0,v1,v2)?':['mul_div_floor',0,1,2],
            'arithmetic::scalar::signed_mul_div_floor(v0,v1,v2)?':['signed_mul_div_floor',0,1,2],
            'i128::from(arithmetic::scalar::products_ge(v0,v1,v2,v3,v4,v5)?)':['products_ge',0,1,2,3,4,5],
            'i128::from(v0 == v1)':['eq',0,1], 'i128::from(v0 < v1)':['lt',0,1],
            'i128::from(v0 == 1 && v1 == 1)':['and',0,1],
            'i128::from(v0 == 0)':['not',0],
            'if v0 == 1 { v1 } else { v2 }':['select',0,1,2],
        }
        for text,wanted in values.items():self.assertEqual(ast.expression(text),wanted)

    def test_well_typed_refusal_and_laziness_faults_refuse_at_named_obligations(self):
        for text,obligation in (
            ('arithmetic::scalar::multiply(v0,v1).unwrap_or(0)','E1_partial_refusal_propagation'),
            ('if v2 == 1 { arithmetic::scalar::divide(false,v0,v1)? } else { 0_i128 }','E1_eager_evaluation')):
            with self.assertRaises(ast.BindingMismatch) as e:ast.expression(text)
            self.assertEqual(e.exception.obligation,obligation)

    def test_unknown_or_injected_expression_refuses(self):
        for text in ('0_i128; return Some([0]);','v0.wrapping_add(v1)','{ unsafe { 0 } }'):
            with self.assertRaises(ast.BindingMismatch):ast.expression(text)

    def test_helper_identity_includes_line_endings_and_native_only_code(self):
        d=dict(min=str(-(1<<127)),max=str((1<<127)-1),boolean=False)
        program=dict(inputs=[d,d],outputs=[d],nodes=[['input',0],['input',1],['mul',0,1]],roots=[2])
        guard=lambda target:f'    if !({d["min"]}_i128..={d["max"]}_i128).contains(&{target}) {{ return None; }}\n'
        prefix=('// Generated from zeno-fcis/finite-i128/3. Pure code; no commit authority.\n'
            '/// Evaluates the generated finite program; no commit authority.\n#[rustfmt::skip]\n'
            'pub fn transition(input: &[i128]) -> Option<[i128; 1]> {\n'
            '    if input.len() != 2 { return None; }\n'+guard('input[0]')+guard('input[1]')+
            '    let v0: i128 = input[0];\n    let _ = v0;\n'
            '    let v1: i128 = input[1];\n    let _ = v1;\n'
            '    let v2: i128 = arithmetic::scalar::multiply(v0,v1)?;\n    let _ = v2;\n'+
            guard('v2')+'    Some([v2,])\n}\n')
        suffix=ast.helper_suffix(ROOT)
        self.assertEqual(ast.decode(prefix+suffix,program,ROOT)[0],program)
        with self.assertRaises(ast.BindingMismatch):
            ast.decode(prefix.replace('\n','\u2028')+suffix,program,ROOT)
        changed=suffix.replace('a.checked_mul(b)','{ #[cfg(not(verus_keep_ghost))] return Some(0); a.checked_mul(b) }')
        for bad in (suffix.replace('\n','\r\n'),changed):
            with self.assertRaises(ast.BindingMismatch) as e:ast.decode(prefix+bad,program,ROOT)
            self.assertEqual(e.exception.obligation,'E2_helper_identity')


if __name__=='__main__':unittest.main()
