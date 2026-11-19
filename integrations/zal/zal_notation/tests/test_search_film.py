"""Finite oracles, evidence controls, and the author-facing compilation boundary."""
from itertools import product
from math import comb
from pathlib import Path as File
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from zal_notation import builtin, core, film_cli, search_film as film, semantics

EXAMPLES = File(__file__).resolve().parents[2] / "examples" / "search-film"
ESSAY = EXAMPLES / "Problem-Solving-as-Search.json"
SECOND = EXAMPLES / "Another-Goal.json"


class SearchFilm(unittest.TestCase):
    def test_every_visible_goal_and_path(self):
        for depth in range(2, 7):
            for up in range(1, depth):
                view = film.walk_view({"up": up, "right": depth-up})
                self.assertEqual(view["groups"], [comb(depth, i) for i in range(depth+1)])
                routes = ["".join(p) for p in product("RU", repeat=depth)]
                self.assertEqual(view["routes"], routes)
                for route in routes:
                    state = f"{route.count('R')},{route.count('U')}"
                    self.assertEqual(view["states"][route], state)
                    self.assertIn(route, view["members"][state])
                accepted = [p for p in routes if p.count("U") == up]
                self.assertEqual(view["solutions"], accepted)
                right_then_up = "R" * (depth-up) + "U" * up
                up_then_right = "U" * up + "R" * (depth-up)
                self.assertEqual(view["optima"], [right_then_up, up_then_right])
                replay = semantics.fsm_trace(builtin.fsm_grammar(), view["model"], list(view["fewestTurns"]))
                self.assertEqual(replay["final_state"], view["states"][view["fewestTurns"]])

    def test_four_operations_on_another_native_fsm(self):
        _, _, model = builtin.get_builtin("state-machine")
        start, transitions = film.transition_table(model)
        initial = film.Path((), start)
        one_step = film.branch_on_choice(initial, transitions)
        self.assertEqual({p.events for p in one_step}, {("release",), ("reserve",)})
        a = film.trace_path(("reserve", "release"), transitions, start)[-1]
        b = film.trace_path(("reserve", "reserve", "release"), transitions, start)[-1]
        self.assertEqual(a.state, b.state)
        self.assertEqual(film.merge_equivalent_states((a, b)), {a.state: (a, b)})
        self.assertEqual(film.select_by_cost((a, b)), (a, b))
        self.assertEqual(film.select_by_cost((a, b), cost=lambda p: len(p.events)), (a,))
        self.assertEqual(film.select_by_cost(()), ())
        with self.assertRaisesRegex(core.NotationError, "leaves"):
            film.trace_path(("unknown",), transitions, start)

    def test_film_and_independent_edit(self):
        essay = core.loads(film_cli.read_plan(ESSAY))
        self.assertEqual(essay["frames"], 2490)
        self.assertEqual(len(essay["shots"]), 9)
        byte = essay["bytes"]
        self.assertEqual([(c["l"], c["h"]) for c in byte["candidates"] if not c["misses"]], [(48, 160)])
        self.assertEqual(len({c["id"] for c in byte["candidates"]}) * byte["checkedInputs"], 2304)
        self.assertEqual(essay["walk"]["optima"], ["RRRUU", "UURRR"])
        self.assertEqual(essay["transfer"]["points"][-1], [5, 3])
        second = core.loads(film_cli.read_plan(SECOND))
        self.assertEqual([(s["view"], s["startFrame"], s["endFrame"]) for s in second["shots"]],
                         [("trace", 0, 120), ("merge", 120, 240)])
        self.assertIn("6 solutions", second["shots"][1]["caption"])
        self.assertNotEqual(essay["walk"]["modelDigest"], second["walk"]["modelDigest"])
        self.assertIsNone(second["bytes"])

    def test_authoring_cli_exports_the_same_plan(self):
        with tempfile.TemporaryDirectory() as folder:
            output = File(folder) / "checked.json"
            command = [sys.executable, str(EXAMPLES.parents[1] / "run.py"), "film-check",
                       str(ESSAY), "--output", str(output)]
            run = subprocess.run(command, check=True, capture_output=True, text=True)
            self.assertEqual(core.loads(run.stdout)["optimalRoutes"], ["RRRUU", "UURRR"])
            self.assertEqual(core.loads(output.read_text()), core.loads(film_cli.read_plan(ESSAY)))
            self.assertEqual(subprocess.run(command, capture_output=True).returncode, 2)

    def test_pilot_compatibility_and_immutable_plan(self):
        native = core.loads(ESSAY.read_text())
        pilot = {**native, "format": "essay-film/1"}
        del pilot["replay"]
        for shot in pilot["shots"]:
            shot["view"] = {"trace": "walk", "branch": "tree"}.get(shot["view"], shot["view"])
        with tempfile.TemporaryDirectory() as folder:
            dest = File(folder)
            shutil.copytree(EXAMPLES / "evidence", dest / "evidence")
            name = dest / "legacy.json"
            name.write_text(core.canonical(pilot))
            plan = core.loads(film_cli.read_plan(name))
            native_plan = core.loads(film_cli.read_plan(ESSAY))
            self.assertNotEqual(plan.pop("sourceDigest"), native_plan.pop("sourceDigest"))
            self.assertEqual(plan, native_plan)
            self.assertEqual(film_cli.read_plan(name), film_cli.read_plan(name))

    def test_changed_record_or_replay_is_refused_before_render(self):
        with tempfile.TemporaryDirectory() as folder:
            base = File(folder) / "input"
            shutil.copytree(EXAMPLES, base)
            for rel, mutate in [
                ("interval-synthesis.json", lambda d: d["records"][0]["result"].update(selected_index=1)),
                ("interval-synthesis.json", lambda d: d["records"][0]["result"]["candidates"][0]["result"].update(mismatch_count=0)),
                ("replay.json", lambda d: d["data"]["records"][0].update(status="stale")),
            ]:
                with self.subTest(rel=rel), patch.object(film_cli.subprocess, "run") as encoder:
                    target = base / "evidence" / rel
                    original = target.read_text()
                    document = core.loads(original)
                    mutate(document)
                    target.write_text(core.canonical(document))
                    out = File(folder) / "film"
                    with self.assertRaisesRegex(core.NotationError, "changed byte synthesis"):
                        film_cli.render(base / ESSAY.name, out)
                    encoder.assert_not_called()
                    self.assertFalse(out.exists())
                    target.write_text(original)

    def test_author_fields_and_byte_interval_changes(self):
        doc = core.loads(SECOND.read_text())
        doc["walk"] = {"right": 2, "up": 1}
        doc["budget"] = 3
        doc["shots"][0]["path"] = "RU"
        doc["shots"][0]["seconds"] = 2.5
        doc["shots"].append({"view": "budget", "seconds": 3, "title": "Trials", "caption": "{trialHits} goal hits"})
        doc["shots"].append({"view": "programs", "seconds": 4, "title": "Bytes", "caption": "Change both bounds."})
        doc["interval"] = {"lower": 0, "upper": 1}
        plan = core.loads(film.compile_film(core.canonical(doc)))
        self.assertEqual(plan["shots"][0]["endFrame"], 75)
        self.assertEqual(plan["shots"][0]["path"], "RU")
        self.assertEqual(plan["shots"][2]["caption"], "2 goal hits")
        self.assertEqual(len(plan["bytes"]["candidates"]), 3)
        doc["shots"][0]["path"] = "not a route"
        with self.assertRaisesRegex(core.NotationError, "legal prefix"):
            film.compile_film(core.canonical(doc))

    def test_read_limits_and_output_ownership(self):
        with tempfile.TemporaryDirectory() as folder:
            base = File(folder)
            name = base / "story.json"
            doc = core.loads(ESSAY.read_text())
            doc["evidence"] = "../record.json"
            name.write_text(core.canonical(doc))
            with self.assertRaisesRegex(core.NotationError, "Unsafe artifact path"):
                film_cli.read_plan(name)
            (base / "keep").write_text("owner")
            with self.assertRaisesRegex(core.NotationError, "new directory"):
                film_cli.render(SECOND, base)
            self.assertEqual((base / "keep").read_text(), "owner")
            with patch.object(film_cli.subprocess, "run") as run:
                run.return_value.returncode = 1
                with self.assertRaisesRegex(core.NotationError, "renderer failed"):
                    film_cli.render(SECOND, base / "failed")
            self.assertEqual(sorted(p.name for p in base.iterdir()), ["keep", "story.json"])
