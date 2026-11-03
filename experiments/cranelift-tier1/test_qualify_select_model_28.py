"""Negative controls for source-bound provisional scalar-select model qualification.

These tests do not run the pinned compiler or the VeriISLE verifier; the
isolated workflow executes the separate source-bound audit against Wasmtime.
"""
import unittest

from qualify_select_model_28 import (
    BOOL_CONVERSION, OPERAND_C, OPERAND_FALSE, OPERAND_TRUE,
    SELECT_INTERPRETER, SELECT_SPEC, check_defined_i8_model,
    check_semantic_anchors,
)

META = "cranelift/codegen/meta/src/shared/instructions.rs"
STEP = "cranelift/interpreter/src/step.rs"
VALUE = "cranelift/interpreter/src/value.rs"


def sources():
    return {
        META: "\n".join([OPERAND_C, OPERAND_TRUE, OPERAND_FALSE]),
        STEP: SELECT_INTERPRETER,
        VALUE: BOOL_CONVERSION,
    }


class SelectModelAuditTests(unittest.TestCase):
    def test_defined_i8_domain_is_exhausted(self):
        result = check_defined_i8_model()
        self.assertEqual(result["condition_values_exhaustively_checked"], 256)
        self.assertEqual(result["finite_edge_value_pairs"], 256 * 25)

    def test_positive_independent_source_anchors(self):
        check_semantic_anchors(sources(), SELECT_SPEC)

    def test_rejects_changed_source_semantics_or_type(self):
        changes = (
            (META, OPERAND_C, OPERAND_C.replace("ScalarTruthy", "Any")),
            (META, OPERAND_TRUE, OPERAND_TRUE.replace("true", "false")),
            (META, OPERAND_FALSE, OPERAND_FALSE.replace("false", "true")),
            (STEP, SELECT_INTERPRETER, SELECT_INTERPRETER.replace(
                "arg(1), arg(2)", "arg(2), arg(1)")),
            (VALUE, BOOL_CONVERSION, BOOL_CONVERSION.replace("b != 0", "b == 0")),
        )
        for path, before, after in changes:
            with self.subTest(path=path, after=after):
                altered = sources()
                altered[path] = altered[path].replace(before, after)
                with self.assertRaisesRegex(ValueError, "missing or ambiguous"):
                    check_semantic_anchors(altered, SELECT_SPEC)

    def test_rejects_changed_or_duplicated_provisional_model(self):
        variants = (
            SELECT_SPEC.replace("(= c #x00) y x", "(= c #x00) x y"),
            SELECT_SPEC.replace("(bv 8)", "(bv 16)"),
            SELECT_SPEC + "\n" + SELECT_SPEC,
        )
        for spec in variants:
            with self.subTest(spec=spec[:60]):
                with self.assertRaises(ValueError):
                    check_semantic_anchors(sources(), spec)

    def test_rejects_missing_upstream_anchor(self):
        altered = sources()
        altered[STEP] = ""
        with self.assertRaises(ValueError):
            check_semantic_anchors(altered, SELECT_SPEC)


if __name__ == "__main__":
    unittest.main()
