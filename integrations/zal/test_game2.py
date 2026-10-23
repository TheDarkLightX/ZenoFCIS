import hashlib
import os
from pathlib import Path
import random
import re
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import behavior
import behavior2 as b
import effects
import typed2


HERE = Path(__file__).parent
EXAMPLES = HERE / "examples"
REPOSITORY = HERE.parents[1]
README = REPOSITORY / "docs" / "zal" / "README.md"
WITHDRAWAL_PROJECT = Path("/tmp/withdrawal-game/project.zeno")
PLAYER_ERROR = "Players need distinct names and disjoint declared states; at most two players"
GAME_COVERAGE_ERROR = "Game properties need two players whose states cover every state exactly once"
UNKNOWN_PLAYER = "Game property reaches names an undeclared player"
POST_STATE_FAIRNESS = "Game fairness reads only the state after a step (state' and primed variables)"

GAME_SOURCE = """zal 2;
machine mini_game;
states aa, bb;
events tick;
context none;
initial aa;
step atomic; frame control_and_variables;
default reject not_allowed unchanged;
rule move: aa + tick [true] -> accept bb;
player user moves in aa;
player operator moves in bb;
property reaches: user can force eventually (state == bb);
"""

WITHDRAWAL_SOURCE = """zal 2;
# Can the user keep withdrawing, whatever the operator does, if the operator reopens the window now and then?
machine withdrawal_game;
states open, paused, withdrawn;
events pause, reopen, withdraw;
context none;
initial open;
step atomic; frame control_and_variables;
default reject not_allowed unchanged;
rule w1: open + withdraw [true] -> accept withdrawn;
rule p1: withdrawn + pause [true] -> accept paused;
rule r1: withdrawn + reopen [true] -> accept open;
rule r2: paused + reopen [true] -> accept open;
player user moves in open;
player operator moves in paused, withdrawn;
fairness reopens: infinitely often (state' == open);
property keep_withdrawing: user can force infinitely often (state == withdrawn);
"""

# Canonical profile revisions captured before any game-language changes.
EXAMPLE_REVISIONS = {
    "context_free2.zal": "07466304cde3923ab1b1516cdc566e578c8f98c53946d69c8422ad93d3419c07",
    "keeper2.zal": "53dd1a9cd3e49a125795b16f8897e7d157f13753198939de015cffe23706de5c",
    "order.zal": "a2f11dc7c2519e58354c4f0da47a17d0577a75d04fedf57f23dd55127bab02f6",
    "order2.zal": "08be70625ac554ec6d831eefc2040e4ed6b05437dae7c2987fbbc1c83507e06d",
    "relay_typed.zal": "4c66c9a476f174eb3de844c6d7278f2999298a7d557d4558d33cd6321434c085",
    "revoked_mail2.zal": "49f300b4cdfed8bae570cb675126fe2a45a32ecb475f519a22d453cd4683c73f",
    "shell_effects.zal": "ea53f58838ddf927bb66080b7bf4d672e11ff63fad03a0396b747c3091a8e9c0",
    "withdrawal2.zal": "8057b15d9b23224d2e7907009f9cb47cd738f970ae3bdc7ff9487bfe1b24cab0",
}


def parse_example(path):
    source = path.read_text(encoding="ascii")
    first = next((line.strip() for line in source.splitlines()
                  if line.strip() and not line.lstrip().startswith("#")), "")
    parser = (effects if first in {"zal effects 1;", "ZAL effects version 1."}
              else typed2 if first in {"zal typed 1;", "ZAL typed version 1."}
              else b if first in {"zal 2;", "ZAL version 2."} else behavior)
    return parser.parse(source)


def reduced_for(model, game_index=0):
    transitions = b._Graph(model)
    rows = [[] for _ in transitions.configs]
    for ci, _, _, result in transitions.raw():
        if result[0] != "step":
            raise AssertionError(f"random game is not a valid transition system: {result[0]}")
        rows[ci].append(result[1])
    transitions.explore(rows)
    _, player, objective, formula = model.games[game_index]
    return b._reduce_game(b._game_graph(transitions, model, player), model, objective, formula)


def random_game_source(rng, case):
    state_count = rng.randint(2, 4)
    event_count = rng.randint(1, 3)
    states = [f"s{index}" for index in range(state_count)]
    events = [f"e{index}" for index in range(event_count)]
    user_states = set(rng.sample(states, rng.randint(1, state_count - 1)))
    operator_states = set(states) - user_states
    has_fact, has_variable = rng.choice((False, True)), rng.choice((False, True))
    lines = ["zal 2;", f"machine random_{case};", f"states {', '.join(states)};",
             f"events {', '.join(events)};", f"context {'signal' if has_fact else 'none'};"]
    if has_variable:
        lines.append("var x in 0..=1 initially 0;")
    lines += ["initial s0;", "step atomic; frame control_and_variables;",
              "default reject not_allowed unchanged;"]
    rules = []
    for state_index, state in enumerate(states):
        for event_index, event in enumerate(events):
            if rng.random() < 0.82:
                guard = rng.choice(("true", "signal", "!signal")) if has_fact else "true"
                if rng.random() < 0.78:
                    target = rng.choice(states)
                    update = f", x = {rng.randint(0, 1)}" if has_variable and rng.random() < 0.4 else ""
                    outcome = f"accept {target}{update}"
                else:
                    outcome = "reject blocked unchanged"
                rules.append((f"r{state_index}_{event_index}",
                              f"rule r{state_index}_{event_index}: {state} + {event} [{guard}] -> {outcome};"))
    lines.extend(line for _, line in sorted(rules))
    lines += [f"player user moves in {', '.join(sorted(user_states))};",
              f"player operator moves in {', '.join(sorted(operator_states))};"]
    fairness_count = rng.randint(0, 2)
    for index in range(fairness_count):
        state_goal = rng.choice(states)
        formula = f"state' == {state_goal}"
        if has_variable and rng.random() < 0.5:
            formula = f"({formula} & x' == {rng.randint(0, 1)})"
        lines.append(f"fairness fair{index}: infinitely often ({formula});")
    objective = rng.choice(b.GAME_OBJECTIVES)
    goal = f"state == {rng.choice(states)}"
    if has_variable and rng.random() < 0.5:
        goal = f"({goal} & x == {rng.randint(0, 1)})"
    player = rng.choice(("user", "operator"))
    lines.append(f"property game_{case}: {player} can force {objective} ({goal});")
    return "\n".join(lines) + "\n"


class GameGrammarTests(unittest.TestCase):
    def test_both_surfaces_round_trip_and_keep_legacy_data_shape(self):
        model = b.parse(GAME_SOURCE)
        self.assertEqual(b.parse(model.render()), model)
        self.assertEqual(b.parse(model.render(True), "english"), model)
        self.assertEqual(model.players, (("operator", ("bb",)), ("user", ("aa",))))
        self.assertEqual(model.games, (("reaches", "user", "eventually", b.Expr("state", ("bb",))),))
        self.assertIn("players", model.data())
        self.assertIn("games", model.data())
        self.assertEqual(b.help_topic(model, "player:user")["entries"][0]["concept"], "player")
        self.assertEqual(b.help_topic(model, "property:reaches")["entries"][0]["concept"], "game")
        self.assertIn("player user moves in aa;", model.render())
        self.assertIn("property reaches: user can force eventually (state == bb);", model.render())
        self.assertIn("Property reaches: user can force (state is bb) to happen eventually.", model.render(True))
        valued = GAME_SOURCE.replace("initial aa;", "var x in 0..=1 initially 0;\ninitial aa;")
        valued = valued.replace("state == bb", "(state == bb & x == 1)")
        valued_model = b.parse(valued)
        self.assertEqual(b.parse(valued_model.render(True), "english"), valued_model)
        players_only = b.parse(GAME_SOURCE.replace(
            "property reaches: user can force eventually (state == bb);\n", ""))
        self.assertNotIn("games", players_only.data())
        self.assertIn("players", players_only.data())
        concepts = [item["concept"] for item in b.legend(model)["entries"]]
        self.assertIn("player", concepts)
        self.assertIn("game", concepts)
        legacy = b.parse((EXAMPLES / "order2.zal").read_text(encoding="ascii"))
        self.assertNotIn("players", legacy.data())
        self.assertNotIn("games", legacy.data())

    def test_players_refuse_invalid_names_ownership_and_limits_exactly(self):
        invalid_sources = [
            GAME_SOURCE.replace("player operator moves in bb;", "player user moves in bb;"),
            GAME_SOURCE.replace("player operator moves in bb;", "player operator moves in aa, bb;"),
            GAME_SOURCE.replace("player operator moves in bb;", "player operator moves in missing;"),
            GAME_SOURCE.replace("player user moves in aa;", "player state moves in aa;"),
            GAME_SOURCE.replace("machine mini_game;", "machine user;"),
            GAME_SOURCE.replace("player operator moves in bb;", "player operator moves in bb;\nplayer reviewer moves in aa;"),
            GAME_SOURCE.replace("player operator moves in bb;", ""),
            GAME_SOURCE.replace("states aa, bb;", "states aa, bb, cc;"),
        ]
        for source in invalid_sources:
            with self.subTest(source=source), self.assertRaises(b.Refusal) as raised:
                b.parse(source)
            expected = GAME_COVERAGE_ERROR if source in invalid_sources[-2:] else PLAYER_ERROR
            self.assertEqual(str(raised.exception), expected)

    def test_game_property_refusals_are_exact(self):
        with self.assertRaises(b.Refusal) as raised:
            b.parse(GAME_SOURCE.replace("user can force", "missing can force"))
        self.assertEqual(str(raised.exception), UNKNOWN_PLAYER)
        fairness = "fairness reopen: infinitely often (state == aa);\n"
        with self.assertRaises(b.Refusal) as raised:
            b.parse(GAME_SOURCE.replace("initial aa;", "initial aa;\n" + fairness))
        self.assertEqual(str(raised.exception), POST_STATE_FAIRNESS)
        variable_fairness = GAME_SOURCE.replace("initial aa;", "var x in 0..=1 initially 0;\ninitial aa;\n")
        variable_fairness = variable_fairness.replace(
            "property reaches:", "fairness current: infinitely often (x == 0);\nproperty reaches:")
        with self.assertRaises(b.Refusal) as raised:
            b.parse(variable_fairness)
        self.assertEqual(str(raised.exception), POST_STATE_FAIRNESS)
        always = GAME_SOURCE.replace("eventually", "always").replace("state == bb", "state == aa")
        self.assertEqual(b.parse(always.replace("initial aa;", "initial aa;\n" + fairness)).games[0][2], "always")

    def test_game_ids_participate_in_property_identifier_uniqueness(self):
        duplicate = GAME_SOURCE + "property reaches: user can force always (state == aa);\n"
        with self.assertRaisesRegex(b.Refusal, "IDs must be distinct"):
            b.parse(duplicate)
        player_collision = GAME_SOURCE + "property user: always(state == aa);\n"
        with self.assertRaises(b.Refusal) as raised:
            b.parse(player_collision)
        self.assertEqual(str(raised.exception), PLAYER_ERROR)

    def test_game_properties_obey_both_property_count_bounds(self):
        too_many_games = GAME_SOURCE + """property g1: user can force always (state == aa);
property g2: user can force always (state == aa);
property g3: user can force always (state == aa);
property g4: user can force always (state == aa);
"""
        with self.assertRaisesRegex(b.Refusal, "profile refuses|Bounds:"):
            b.parse(too_many_games)
        too_many_total = GAME_SOURCE + """property g1: user can force always (state == aa);
property g2: user can force always (state == aa);
property g3: user can force always (state == aa);
property t0: always(state == aa);
property t1: always(state == aa);
property t2: always(state == aa);
property t3: always(state == aa);
property t4: always(state == aa);
"""
        with self.assertRaisesRegex(b.Refusal, "profile refuses|Bounds:"):
            b.parse(too_many_total)

    def test_semantic_diff_reports_player_and_game_changes(self):
        original = b.parse(GAME_SOURCE)
        ownership = GAME_SOURCE.replace("player user moves in aa;", "player user moves in bb;")
        ownership = ownership.replace("player operator moves in bb;", "player operator moves in aa;")
        player_change = b.semantic_diff(original, b.parse(ownership))
        self.assertIn("players", player_change["changed_objects"])
        game_change = b.semantic_diff(original, b.parse(GAME_SOURCE.replace("state == bb", "state == aa")))
        self.assertIn("games", game_change["changed_objects"])

    def test_existing_example_revisions_are_unchanged(self):
        paths = sorted(path for path in EXAMPLES.rglob("*.zal") if path.name != "withdrawal_game2.zal")
        self.assertEqual({path.name: parse_example(path).revision for path in paths}, EXAMPLE_REVISIONS)

    def test_withdrawal_example_and_its_variants(self):
        path = EXAMPLES / "withdrawal_game2.zal"
        self.assertEqual(path.read_text(encoding="ascii"), WITHDRAWAL_SOURCE)
        model = b.parse(WITHDRAWAL_SOURCE)
        self.assertEqual(b.parse(model.render(True), "english"), model)
        report = b.check(model)
        self.assertEqual(report["properties"]["keep_withdrawing"]["initial_wins"], True)
        without_fairness = WITHDRAWAL_SOURCE.replace(
            "fairness reopens: infinitely often (state' == open);\n", "")
        failed = b.check(b.parse(without_fairness))
        self.assertEqual((failed["status"], failed["obligation"], failed["property"]),
                         ("counterexample", "game", "keep_withdrawing"))
        eventual = without_fairness.replace("can force infinitely often", "can force eventually")
        reached = b.check(b.parse(eventual))
        self.assertTrue(reached["properties"]["keep_withdrawing"]["initial_wins"])


class GameSolverTests(unittest.TestCase):
    CONTEXT_GAME = """zal 2;
machine context_game;
states open, paused, withdrawn;
events idle, try;
context authorized;
initial open;
step atomic; frame control_and_variables;
default reject not_allowed unchanged;
rule take: open + try [authorized] -> accept withdrawn;
player user moves in open;
player operator moves in paused, withdrawn;
property reaches: user can force eventually (state == withdrawn);
"""

    def test_eventually_goal_is_absorbing_and_makes_fairness_true(self):
        model = b.parse(GAME_SOURCE + "rule leave_goal: bb + tick [true] -> accept aa;\n")
        game = reduced_for(model)
        goals = [node for node in game.configuration_nodes if game.goal[node]]
        self.assertTrue(goals)
        for node in goals:
            self.assertEqual(game.successors[node], (node,))
            self.assertTrue(all(row[node] for row in game.fairness))

    def test_always_reduction_marks_every_safe_node_and_ignores_fairness(self):
        source = self.CONTEXT_GAME.replace("eventually", "always").replace("state == withdrawn", "state == open")
        model = b.parse(source + "fairness current: infinitely often (state == open);\n")
        game = reduced_for(model)
        self.assertEqual(game.fairness, (tuple(True for _ in game.successors),))
        intermediate = [node for node, descriptor in enumerate(game.configurations) if descriptor[0] == "context"]
        self.assertTrue(intermediate)
        self.assertTrue(all(game.goal[node] for node in intermediate))
        bad = [node for node in game.configuration_nodes if not game.goal[node]]
        self.assertTrue(bad)
        self.assertTrue(all(game.successors[node] == (node,) for node in bad))

    def test_success_record_and_game_scope_are_exact(self):
        result = b.check(b.parse(GAME_SOURCE))
        self.assertEqual(result["checks"]["game_properties"], "pass-with-scope")
        self.assertEqual(result["properties"]["reaches"], {
            "status": "pass-with-scope", "kind": "game", "player": "user", "objective": "eventually",
            "english": "Property reaches: user can force (state is bb) to happen eventually.",
            "configurations": 2, "winning_configurations": 2, "initial_wins": True,
            "solvers": ["fixed-point", "parity"]})
        self.assertIn("game properties range over every play from the initial configuration in which each player "
                      "chooses the event in its own states and the other player chooses the context", result["scope"])

    def test_context_choice_belongs_to_opponent(self):
        result = b.check(b.parse(self.CONTEXT_GAME))
        self.assertEqual(result["status"], "counterexample")
        self.assertEqual(result["obligation"], "game")
        self.assertEqual((result["property"], result["player"], result["objective"], result["initial"]["state"]),
                         ("reaches", "user", "eventually", "open"))
        self.assertLessEqual(len(result["winning_configurations"]), 16)
        self.assertFalse(result["initial"].get("variables"))

    def test_game_runs_after_existing_temporal_properties(self):
        source = GAME_SOURCE + "property prior_safety: never(state == bb);\n"
        result = b.check(b.parse(source))
        self.assertEqual((result["status"], result["obligation"], result["property"]),
                         ("counterexample", "property", "prior_safety"))

    def test_solver_disagreement_is_unknown_and_not_a_pass(self):
        with patch.object(b, "_parity_winners", return_value=frozenset()):
            result = b.check(b.parse(GAME_SOURCE))
        self.assertEqual((result["status"], result["diagnostic"]), ("unknown", "solvers disagree"))
        self.assertNotEqual(result["status"], "pass-with-scope")

    def test_solver_agreement_2000_random_machines_on_every_node(self):
        rng = random.Random(20261022)
        for case in range(2000):
            model = b.parse(random_game_source(rng, case))
            game = reduced_for(model)
            fixed = b._fixed_point_winners(game)
            parity = b._parity_winners(game)
            self.assertEqual(fixed, parity, f"solver disagreement in random machine {case}")


