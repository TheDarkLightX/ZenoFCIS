"""Pure source/control admission tests; they never invoke Cargo or Verus."""
import unittest
from pathlib import Path
import scalar_arithmetic_controls as controls

class SourceControls(unittest.TestCase):
    def test_each_fault_changes_only_a_unique_source_body_anchor(self):
        root=Path(__file__).resolve().parents[1]
        rows=controls.specimens(root)
        self.assertEqual(len(rows),8)
        for name,path,before,after,function,witness in controls.CONTROLS:
            source=(root/path).read_text()
            changed=rows[name][1]
            self.assertNotEqual(source,changed)
            self.assertIn("fn "+function,source)
            self.assertIn("fn "+function,changed)
            self.assertTrue(witness)
            # Changing a body does not delete its proof postcondition.
            self.assertEqual(source.count("ensures"),changed.count("ensures"))

    def test_mutation_anchor_failures_are_not_controls(self):
        with self.assertRaises(ValueError):
            controls.replace_once("a a","a","b")
        with self.assertRaises(ValueError):
            controls.replace_once("a","missing","b")

if __name__=="__main__":unittest.main()
