"""The core owns immutable export status and disposition; the shell performs I/O."""
from contextlib import redirect_stdout, redirect_stderr
from dataclasses import FrozenInstanceError, fields, replace
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

import behavior
import behavior2 as b
import effects
import run
import typed2

EXAMPLES = Path(__file__).parent / "examples"


class GameExportResultTests(unittest.TestCase):
    def test_export_decision_does_not_load_modules_or_open_files(self):
        game = b.parse((EXAMPLES / "withdrawal_game2.zal").read_text())
        models = [game, behavior.parse((EXAMPLES / "order.zal").read_text()),
                  effects.parse((EXAMPLES / "shell_effects.zal").read_text()),
                  typed2.parse((EXAMPLES / "relay_typed.zal").read_text())]
        # Parsing and module preparation finish before the authoritative call.
        with patch("builtins.__import__", side_effect=AssertionError("module loading inside export decision")), patch("builtins.open", side_effect=AssertionError("file I/O inside export decision")):
            for model in models:
                result = b.export_game_decision(model, game.games[0][0], "game.zeno")
                self.assertEqual(result.exit_code, 0 if model is game else 2)
            self.assertEqual(b.export_game_decision(game, "missing", "game.zeno").exit_code, 2)

    def test_result_and_all_retained_values_are_immutable(self):
        model = b.parse((EXAMPLES / "withdrawal_game2.zal").read_text())
        before = model.data()
        result = b.export_game_decision(model, model.games[0][0], "game.zeno")
        self.assertEqual(result.exit_code, 0)
        for field in fields(result):
            self.assertIsInstance(getattr(result, field.name), (str, int, bool, type(None)))
            with self.assertRaises((FrozenInstanceError, AttributeError)):
                setattr(result, field.name, None)
        self.assertEqual(model.data(), before)
        self.assertEqual(result, b.export_game_decision(model, model.games[0][0], "game.zeno"))
        self.assertEqual(json.loads(result.status_json), {
            "status": "exported", "format": "contract-tables", "property": model.games[0][0],
            "nodes": result.node_count, "out": "game.zeno", "authority": "none"})

    def test_profile_and_property_refusals_are_complete_core_results(self):
        for model, name, reason in [
                (behavior.parse((EXAMPLES / "order.zal").read_text()), "missing",
                 "Game export needs a profile-2 game property model"),
                (b.parse((EXAMPLES / "withdrawal_game2.zal").read_text()), "missing",
                 "Game property missing is not declared"),
                (effects.parse((EXAMPLES / "shell_effects.zal").read_text()), "missing",
                 "The export-game command does not support zal/effects/1 sources"),
                (typed2.parse((EXAMPLES / "relay_typed.zal").read_text()), "missing",
                 "The export-game command does not support zal/typed-fsm/1 sources")]:
            result = b.export_game_decision(model, name, "game.zeno", "source text", "input.zal")
            self.assertEqual((result.status, result.refusal, result.exit_code, result.to_stderr),
                             ("refused", reason, 2, True))
            self.assertIsNone(result.text)
            self.assertIsNone(result.node_count)
            packet = json.loads(result.status_json)
            self.assertEqual(packet["diagnostic"], reason)
            self.assertEqual(packet["status"], "refused")
            self.assertEqual(packet["authority"], "none")
            self.assertEqual(packet["source_diagnostic"]["location"]["file"], "input.zal")

    def test_shell_obeys_core_status_stream_and_unknown_exit(self):
        source = EXAMPLES / "withdrawal_game2.zal"
        model = b.parse(source.read_text())
        accepted = b.export_game_decision(model, model.games[0][0], "game.zeno")
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "nested/game.zeno"
            for result in [accepted, replace(accepted, text=None, status_json='{"status": "unknown"}',
                                              exit_code=1, to_stderr=True)]:
                output.unlink(missing_ok=True)
                stdout, stderr = io.StringIO(), io.StringIO()
                with patch.object(sys, "argv", ["run.py", "export-game", str(source), "--property", model.games[0][0], "--out", str(output)]), patch.object(b, "export_game_decision", return_value=result), redirect_stdout(stdout), redirect_stderr(stderr):
                    code = run.main()
                self.assertEqual(code, result.exit_code)
                self.assertEqual((stderr if result.to_stderr else stdout).getvalue(), result.status_json + "\n")
                self.assertEqual((stdout if result.to_stderr else stderr).getvalue(), "")
                self.assertEqual(output.read_text() if output.exists() else None, result.text)


if __name__ == "__main__":
    unittest.main()
