import hashlib
from pathlib import Path
import tempfile
import unittest
import check_arithmetic_typed_proof as gate


class TypedBinding(unittest.TestCase):
    def test_exact_source_pin_rejects_native_divergence(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);p=root/'node.rs';p.write_text('fn node() {}\n')
            profile={'source_sha256':{'node.rs':hashlib.sha256(p.read_bytes()).hexdigest()}}
            gate.require_sources(root,profile)
            p.write_text('fn node() { #[cfg(not(verus_keep_ghost))] return; }\n')
            with self.assertRaises(ValueError):gate.require_sources(root,profile)

    def test_decision_boundaries_have_only_reviewed_configuration(self):
        for path in gate.BOUNDARIES:gate.audit_boundary((gate.ROOT/path).read_text())

    def test_native_and_ghost_executable_overrides_refuse(self):
        for text in ('#[cfg(not(verus_keep_ghost))] return None;',
                     '#[cfg(verus_keep_ghost)] return Some(0);',
                     '#[cfg_attr(not(verus_keep_ghost), inline)] fn f() {}'):
            with self.assertRaises(ValueError):gate.audit_boundary(text)


if __name__=='__main__':unittest.main()
