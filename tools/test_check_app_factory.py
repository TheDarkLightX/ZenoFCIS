"""Generated app journeys must keep their output outside the library source."""
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

import check_app_factory as gate


class OutputLocation(unittest.TestCase):
    def test_in_source_cargo_target_does_not_put_generated_apps_in_source(self):
        class OutputRequested(Exception):
            pass

        with tempfile.TemporaryDirectory() as temporary, \
                patch.dict(os.environ, {"CARGO_TARGET_DIR": str(gate.ROOT / "target")}), \
                patch.object(sys, "argv", ["check_app_factory.py"]), \
                patch.object(gate.tempfile, "gettempdir", return_value=temporary), \
                patch.object(gate.tempfile, "mkdtemp", side_effect=OutputRequested) as create:
            with self.assertRaises(OutputRequested):
                gate.main()
            self.assertFalse(Path(create.call_args.kwargs["dir"]).is_relative_to(gate.ROOT))


if __name__ == "__main__":
    unittest.main()
