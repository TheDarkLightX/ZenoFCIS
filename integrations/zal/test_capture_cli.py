"""Heavy real-CLI capture grammar regression; invoked by the Rust integration gate."""
from contextlib import redirect_stdout
import io
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import capture_workspace as W
import loop_language as Z


class CaptureCliGrammarTests(unittest.TestCase):
    def setUp(self):
        run = W.D.run

        def selected_run(cli, arguments, cwd=None):
            # Only acquisition needs systemd; frozen-table operations keep
            # their finite-data POSIX profile even in a systemd CI environment.
            backend = "systemd" if arguments[:3] in (["loop", "capture", "open"],
                                                     ["loop", "capture", "whole"]) else "auto"
            return run(cli, ["--containment-backend", backend, *arguments], cwd)

        selection = patch.object(W.D, "run", selected_run)
        selection.start()
        self.addCleanup(selection.stop)

    def test_terminal_refusal_keeps_native_reason_and_next_step(self):
        native = {"schema": "zeno-fcis/black-box-capture/1", "status": "refused", "backend": "systemd",
                  "detail": {"reason": "capture-containment-unavailable",
                             "prerequisite": "systemd-user-manager"}}
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            output = io.StringIO()
            argv = ["init", "--workspace", str(folder / "capture.json"),
                    "--spec", str(folder / "spec.json"), "--reference", str(folder / "reference.py"),
                    "--session", str(folder / "session"), "--cli", str(folder / "zeno-fcis")]
            with patch.object(W.D, "resolve_cli", return_value=folder / "zeno-fcis"), \
                    patch.object(W.D, "run", return_value=(1, native)), \
                    redirect_stdout(output):
                code = W.terminal(argv)
        self.assertEqual(code, 1)
        result = json.loads(output.getvalue())
        self.assertEqual(result["reason"], "capture-workspace-operation")
        self.assertEqual(result["detail_reason"], "capture-containment-unavailable")
        self.assertIn("systemd user manager", result["message"])
        self.assertIn("then retry", result["message"])

    def test_real_cli_accepts_paired_int_bool_equality_and_conditionals(self):
        cli = os.environ.get('ZENO_CAPTURE_TEST_CLI')
        if not cli:
            self.skipTest('real CLI required; the Rust capture integration gate supplies it')
        for english in (False, True):
            with self.subTest(english=english), tempfile.TemporaryDirectory() as directory:
                folder = Path(directory)
                spec = folder / 'spec.json'
                spec.write_text('{"inputs":[{"kind":"Int","min":"0","max":"3"},{"kind":"Bool"}],'
                                '"outputs":[{"kind":"Int","min":"0","max":"3"},{"kind":"Bool"},'
                                '{"kind":"Int","min":"0","max":"3"}]}')
                source = folder / 'reference.py'
                source.write_text('def reference(p): return [p[0],int(p[1]==1),3-p[0] if p[1]==1 else p[0]]\n')
                workspace = W.CaptureWorkspace(folder / 'capture.json', cli=cli)
                workspace.init(spec, source, folder / 'session')
                ctx = workspace.ctx(workspace.load())
                expressions = ('x0', 'x1 == true', 'x1 ? 3 - x0 : x0')
                document = Z.Document(ctx.subject_ref, 'model', tuple(
                    Z.act('capture', 'p1', output=f'out{i}', expr=Z.read(text, ctx, False))
                    for i, text in enumerate(expressions)))
                proposal = Z.render(document, ctx, english)
                self.assertEqual(Z.parse(proposal, ctx), document)
                events = workspace.submit(proposal)
                acts = tuple(a for event in events[1:] for a in Z.parse(event['symbolic'], ctx).acts)
                self.assertTrue(any(a.kind == 'captured-exact' for a in acts), events)
                self.assertFalse(any(a.kind == 'capture-result' for a in acts), events)
                for event in events:
                    self.assertEqual(Z.parse(event['symbolic'], ctx), Z.parse(event['english'], ctx))


if __name__ == '__main__':
    unittest.main()
