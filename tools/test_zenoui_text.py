"""ui.json text may not hide characters that change how a label reads."""

import copy
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

import zenoui

BASE = json.loads((zenoui.ROOT / "site/zenoui/test-projects/wide-counter/ui.json").read_text())
# One character of each refused kind, with its code point as the error names it.
REFUSED = {
    "right-to-left override": "‮",
    "left-to-right embedding": "‪",
    "first strong isolate": "⁨",
    "pop directional isolate": "⁩",
    "right-to-left mark": "‏",
    "Arabic letter mark": "؜",
    "zero width space": "​",
    "zero width joiner": "‍",
    "word joiner": "⁠",
    "invisible separator": "⁣",
    "byte order mark": "﻿",
    "soft hyphen": "­",
    "combining grapheme joiner": "͏",
    "Hangul filler": "ㅤ",
    "variation selector": "️",
    "tag letter": "\U000e0041",
    "interlinear annotation": "￹",
    "C0 control": "\u0007",
    "tab": "\t",
    "line feed": "\n",
    "delete": "\u007f",
    "C1 next line": "\u0085",
    "C1 control": "\u009b",
    "line separator": " ",
    "paragraph separator": " ",
    "private use": "",
    "lone surrogate": "\ud800",
}
# Each place a string can appear, as a change to the metadata and the path the
# error names.
PLACES = {
    "title": (lambda m, c: m.__setitem__("title", "Exact" + c + " counter"), "ui.json.title"),
    "field label": (lambda m, c: m["fields"]["pre.100.110"].__setitem__("label", c + "Balance"), "ui.json.fields.pre.100.110.label"),
    "unit": (lambda m, c: m["fields"]["pre.100.110"].__setitem__("unit", "cred" + c + "its"), "ui.json.fields.pre.100.110.unit"),
    "value label": (lambda m, c: m["fields"]["pre.100.110"].__setitem__("values", {"0": "empty" + c}), "ui.json.fields.pre.100.110.values.0"),
    "variant label": (lambda m, c: m["variants"].__setitem__("120", "Incr" + c + "ease"), "ui.json.variants.120"),
    "command label": (lambda m, c: m["commands"]["121"].__setitem__("label", "Decrease" + c), "ui.json.commands.121.label"),
    "reason message": (lambda m, c: m["reasons"].__setitem__("200", "Only " + c + "the operator."), "ui.json.reasons.200"),
}


class InvisibleCharacterTests(unittest.TestCase):
    def test_every_refused_kind_is_refused_in_every_place(self):
        for kind, character in REFUSED.items():
            for place, (change, path) in PLACES.items():
                metadata = copy.deepcopy(BASE)
                change(metadata, character)
                expected = f"UiInvisibleCharacter:{path}:U+{ord(character):04X}"
                with self.subTest(kind=kind, place=place):
                    with self.assertRaises(ValueError, msg="invisible character accepted") as refused:
                        zenoui.visible(metadata)
                    self.assertEqual(str(refused.exception), expected)

    def test_a_key_is_named_without_its_hidden_character(self):
        metadata = copy.deepcopy(BASE)
        metadata["reasons"]["20‮0"] = "Mirrored"
        with self.assertRaisesRegex(ValueError, r"^UiInvisibleCharacter:ui\.json\.reasons\.20\\u202e0:U\+202E$"):
            zenoui.visible(metadata)

    def test_visible_text_in_any_script_is_kept(self):
        metadata = copy.deepcopy(BASE)
        metadata["title"] = "Crédit — 信用 — Кредит — קרדיט — رصيد — 💳 €5"
        metadata["fields"]["pre.100.110"]["values"] = {"0": "épuisé", "1": "一"}
        self.assertEqual(zenoui.visible(metadata), metadata)
        self.assertEqual(zenoui.visible(BASE), BASE)

    def test_the_build_refuses_before_it_writes_anything(self):
        with tempfile.TemporaryDirectory(prefix="zenoui-text-") as directory:
            project = Path(directory) / "project"
            project.mkdir()
            metadata = copy.deepcopy(BASE)
            metadata["commands"]["120"]["label"] = "Increase‮"
            (project / "ui.json").write_text(json.dumps(metadata, ensure_ascii=False), encoding="utf-8")
            out = Path(directory) / "out"
            arguments = ["zenoui.py", str(project), "--out", str(out), "--oracle", "/nonexistent"]
            refusal = None
            with mock.patch.object(sys, "argv", arguments):
                try:
                    zenoui.main()
                except ValueError as error:
                    refusal = str(error)
                except OSError:
                    pass  # It went on to run the missing oracle.
            self.assertEqual(refusal, "UiInvisibleCharacter:ui.json.commands.120.label:U+202E",
                             "the build did not check ui.json text")
            self.assertFalse(out.exists(), "output written before the refusal")


if __name__ == "__main__":
    unittest.main()
