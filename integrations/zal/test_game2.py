import hashlib
import json
import os
from pathlib import Path
import random
import re
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
from types import SimpleNamespace

import behavior
import behavior2 as b
import effects
import typed2


HERE = Path(__file__).parent
EXAMPLES = HERE / "examples"
REPOSITORY = HERE.parents[1]
README = REPOSITORY / "docs" / "zal" / "README.md"
WITHDRAWAL_PROJECT = Path("/tmp/withdrawal-game/project.zeno")
DEADLINE_PROJECT = Path("/tmp/withdrawal-deadline/project.zeno")
PLAYER_ERROR = "Players need distinct names and disjoint declared states; at most two players"
GAME_COVERAGE_ERROR = "Game properties need two players whose states cover every state exactly once"
UNKNOWN_PLAYER = "Game property reaches names an undeclared player"
POST_STATE_FAIRNESS = "Game fairness reads only the state after a step (state' and primed variables)"
DEADLINE_ERROR = "Game deadline K must be a literal from 0 to 64"
MIXED_OBJECTIVE_ERROR = "Game property quick_withdrawal cannot mix objectives"

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

DEADLINE_SOURCE = """zal 2;
# Can the user reach withdrawn within one rule application, whatever the operator chooses as context?
machine deadline_game;
states open, withdrawn;
events withdraw, wait;
context signal;
initial open;
step atomic; frame control_and_variables;
default reject not_allowed unchanged;
rule w1: open + withdraw [true] -> accept withdrawn;
rule r1: withdrawn + wait [true] -> accept open;
player user moves in open;
player operator moves in withdrawn;
property quick_withdrawal: user can force within 1 (state == withdrawn);
"""

CONTEXT_DEADLINE_SOURCE = """zal 2;
machine context_deadline;
states open, withdrawn;
events tick;
context authorized;
initial open;
step atomic; frame control_and_variables;
default reject blocked unchanged;
rule take: open + tick [authorized] -> accept withdrawn;
rule stay: withdrawn + tick [true] -> accept open;
player user moves in open;
player operator moves in withdrawn;
property quick_withdrawal: user can force within 1 (state == withdrawn);
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
GAME_EXAMPLE_REVISIONS = {
    "deadline_game2.zal": "f9d5a4db71761f0ef6b9a5dff04bfec3189c617f56ff8cfe18b77facd49e9aef",
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
    transitions, arena = game_arena_for(model, game_index)
    _, player, objective, formula = model.games[game_index]
    return b._reduce_game(arena, model, objective, formula)


def game_arena_for(model, game_index=0):
    transitions = b._Graph(model)
    rows = [[] for _ in transitions.configs]
    for ci, _, _, result in transitions.raw():
        if result[0] != "step":
            raise AssertionError(f"random game is not a valid transition system: {result[0]}")
        rows[ci].append(result[1])
    transitions.explore(rows)
    _, player, objective, formula = model.games[game_index]
    return transitions, b._game_graph(transitions, model, player)


def deadline_parts(model, game_index=0):
    _, player, objective, formula = model.games[game_index]
    if objective != "within":
        raise AssertionError("expected a bounded game property")
    transitions, arena = game_arena_for(model, game_index)
    bound, goal_formula = formula.args
    goal = tuple(b._configuration_holds(goal_formula, node) for node in arena.configurations)
    return transitions, arena, goal, bound


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


def random_deadline_source(rng, case):
    lines = random_game_source(rng, case).splitlines()
    match = re.fullmatch(r"property game_\d+: ([a-z][a-z0-9_]*) can force "
                         r"(?:eventually|always|infinitely often) \((.+)\);", lines[-1])
    if not match:
        raise AssertionError(lines[-1])
    lines[-1] = (f"property game_{case}: {match[1]} can force within {rng.randint(0, 5)} "
                 f"({match[2]});")
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

    def test_within_deadline_round_trips_in_both_surfaces(self):
        model = b.parse(DEADLINE_SOURCE)
        self.assertEqual(b.parse(model.render()), model)
        self.assertEqual(b.parse(model.render(True), "english"), model)
        self.assertEqual(
            model.games[0][2:],
            ("within", b.Expr("within", (1, b.Expr("state", ("withdrawn",))))),
        )
        self.assertIn("property quick_withdrawal: user can force within 1 (state == withdrawn);", model.render())
        self.assertIn("Property quick_withdrawal: user can force (state is withdrawn) to happen within 1 steps.",
                      model.render(True))
        maximum = b.parse(DEADLINE_SOURCE.replace("within 1", "within 64"))
        self.assertEqual(maximum.games[0][3].args[0], 64)
        self.assertEqual(b.parse(maximum.render(True), "english"), maximum)

    def test_within_deadline_and_mixed_objective_refusals_are_exact(self):
        for bound in ("-1", "65", "1+1", "999999999999999999999999999999"):
            with self.subTest(bound=bound), self.assertRaises(b.Refusal) as raised:
                b.parse(DEADLINE_SOURCE.replace("within 1", f"within {bound}"))
            self.assertEqual(str(raised.exception), DEADLINE_ERROR)
        mixed = DEADLINE_SOURCE.replace(
            "within 1 (state == withdrawn)",
            "within 1 (state == withdrawn) and eventually (state == open)")
        with self.assertRaises(b.Refusal) as raised:
            b.parse(mixed)
        self.assertEqual(str(raised.exception), MIXED_OBJECTIVE_ERROR)
        english = b.parse(DEADLINE_SOURCE).render(True).replace(
            "to happen within 1 steps.", "and (state is open) to happen eventually.")
        with self.assertRaises(b.Refusal) as raised:
            b.parse(english, "english")
        self.assertEqual(str(raised.exception), MIXED_OBJECTIVE_ERROR)

    def test_within_offsets_include_zero_ignore_fairness_and_keep_context_inside_step(self):
        model = b.parse(DEADLINE_SOURCE)
        self.assertEqual(b.check(model)["properties"]["quick_withdrawal"]["initial_wins"], True)
        zero_steps = b.check(b.parse(DEADLINE_SOURCE.replace(
            "initial open;", "initial withdrawn;").replace("within 1", "within 0")))
        self.assertTrue(zero_steps["properties"]["quick_withdrawal"]["initial_wins"])
        at_zero = b.parse(DEADLINE_SOURCE.replace("within 1", "within 0"))
        self.assertEqual((b.check(at_zero)["status"], b.check(at_zero)["obligation"]),
                         ("counterexample", "game"))
        with_fairness = DEADLINE_SOURCE.replace(
            "property quick_withdrawal:", "fairness current: infinitely often (state == open);\n"
            "property quick_withdrawal:")
        self.assertEqual(b.check(b.parse(with_fairness))["status"], "pass-with-scope")

    def test_within_k_includes_exact_bound(self):
        model = b.parse(DEADLINE_SOURCE)
        result = b.check(model)
        self.assertEqual(result["status"], "pass-with-scope")
        self.assertTrue(result["properties"]["quick_withdrawal"]["initial_wins"])


    def test_context_nodes_do_not_consume_deadline_steps(self):
        model = b.parse(DEADLINE_SOURCE)
        _, arena, goal, bound = deadline_parts(model)
        self.assertTrue(any(node[0] == "context" for node in arena.configurations))
        fixed = b._deadline_fixed_winners(arena, goal, bound)
        parity = b._deadline_parity_winners(arena, goal, bound)
        self.assertEqual(fixed, parity)
        self.assertIn(arena.initial, fixed)

    def test_context_choice_is_opponent_owned_and_can_block_deadline(self):
        model = b.parse(CONTEXT_DEADLINE_SOURCE)
        transitions, arena, goal, bound = deadline_parts(model)
        fixed = b._deadline_fixed_winners(arena, goal, bound)
        parity = b._deadline_parity_winners(arena, goal, bound)
        self.assertEqual(fixed, parity)
        self.assertNotIn(arena.initial, fixed)
        result = b.check(model)
        self.assertEqual((result["status"], result["obligation"], result["objective"]),
                         ("counterexample", "game", "within 1"))

    def test_solver_agreement_2000_random_deadline_machines_on_every_node(self):
        rng = random.Random(20261023)
        for case in range(2000):
            model = b.parse(random_deadline_source(rng, case))
            _, arena, goal, bound = deadline_parts(model)
            fixed = b._deadline_fixed_winners(arena, goal, bound)
            parity = b._deadline_parity_winners(arena, goal, bound)
            self.assertEqual(fixed, parity, f"deadline solver disagreement in random machine {case}")

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
        new_examples = {"withdrawal_game2.zal", "deadline_game2.zal"}
        paths = sorted(path for path in EXAMPLES.rglob("*.zal") if path.name not in new_examples)
        self.assertEqual({path.name: parse_example(path).revision for path in paths}, EXAMPLE_REVISIONS)
        self.assertEqual({path.name: b.parse(path.read_text(encoding="ascii")).revision
                          for path in sorted(EXAMPLES.glob("deadline_game2.zal"))},
                         GAME_EXAMPLE_REVISIONS)

    def test_game_readings_have_formula_and_plain_rungs(self):
        model = b.parse(WITHDRAWAL_SOURCE)
        reading, = b.game_readings(model)
        self.assertEqual(reading["formula_rung"], "⟨⟨user⟩⟩ □◇ (state = withdrawn)")
        self.assertEqual(reading["plain_rung"],
                         "The user can make sure the state is withdrawn again and again, whatever the operator does, "
                         "provided every declared fairness condition holds infinitely often.")
        self.assertEqual(b.legend(model)["readings"], [reading])
        explanation = b.explain(model, "open", {}, "pause", {})
        self.assertEqual(explanation["readings"], [reading])

        objectives = b.parse(GAME_SOURCE + "property stays: user can force always (state == aa);\n"
                             "property returns: user can force infinitely often (state == bb);\n")
        by_id = {item["property"]: item for item in b.game_readings(objectives)}
        self.assertEqual(by_id["reaches"]["formula_rung"], "⟨⟨user⟩⟩ ◇ (state = bb)")
        self.assertEqual(by_id["stays"]["formula_rung"], "⟨⟨user⟩⟩ □ (state = aa)")
        self.assertEqual(by_id["returns"]["formula_rung"], "⟨⟨user⟩⟩ □◇ (state = bb)")
        self.assertNotIn("provided", by_id["stays"]["plain_rung"])

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


class GameTableTests(unittest.TestCase):
    def test_cli_explain_and_legend_readings(self):
        source = EXAMPLES / "withdrawal_game2.zal"
        given = '{"state":"open","variables":{},"event":"pause","context":{}}'
        explained = subprocess.run([sys.executable, "-B", str(REPOSITORY / "integrations/zal/run.py"),
                                    "explain", str(source), "--input", given],
                                   cwd=REPOSITORY, text=True, stdout=subprocess.PIPE,
                                   stderr=subprocess.PIPE, timeout=120)
        self.assertEqual(explained.returncode, 0, explained.stderr)
        result = json.loads(explained.stdout)
        self.assertEqual(result["readings"][0]["formula_rung"], "⟨⟨user⟩⟩ □◇ (state = withdrawn)")
        self.assertEqual(result["authority"], "none")
        read = subprocess.run([sys.executable, "-B", str(REPOSITORY / "integrations/zal/run.py"),
                               "legend", str(source)], cwd=REPOSITORY, text=True,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=120)
        self.assertEqual(read.returncode, 0, read.stderr)
        self.assertEqual(json.loads(read.stdout)["readings"], result["readings"])

    def test_readme_commands_and_exact_output(self):
        readme = README.read_text(encoding="utf-8")

        def documented_output(marker):
            match = re.search(rf"<!-- {marker} -->\s*```json\n(.*?)\n```", readme, re.DOTALL)
            self.assertIsNotNone(match, f"missing documented output {marker}")
            return match.group(1) + "\n"

        WITHDRAWAL_PROJECT.parent.mkdir(parents=True, exist_ok=True)
        commands = [
            ["python3", "integrations/zal/run.py", "check",
             "integrations/zal/examples/withdrawal_game2.zal"],
            ["python3", "integrations/zal/run.py", "export-game",
             "integrations/zal/examples/withdrawal_game2.zal", "--property", "keep_withdrawing",
             "--out", str(WITHDRAWAL_PROJECT)],
        ]
        markers = ["withdrawal-game-check-output", "withdrawal-game-export-output"]
        for command, marker in zip(commands, markers):
            with self.subTest(command=command):
                completed = subprocess.run(command, cwd=REPOSITORY, text=True, encoding="utf-8",
                                           stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=120)
                self.assertEqual(completed.returncode, 0, completed.stderr)
                self.assertEqual(completed.stdout, documented_output(marker))

        binary = os.environ.get("ZENO_FCIS_BIN")
        if binary:
            checked = subprocess.run([binary, "check", "--expand", str(WITHDRAWAL_PROJECT)],
                                     cwd=REPOSITORY, text=True, stdout=subprocess.PIPE,
                                     stderr=subprocess.STDOUT, timeout=120)
            self.assertEqual(checked.returncode, 0, checked.stdout[-2000:])
            stable = re.findall(r"^law 510 stable on commit = (true|false);$",
                                checked.stdout, flags=re.MULTILINE)
            self.assertEqual(stable, ["true"], checked.stdout[-2000:])
            rows = re.findall(r"^law (6\d\d) from_[a-z0-9_]+ on commit = (true|false);$",
                              checked.stdout, flags=re.MULTILINE)
            self.assertEqual(len(rows), 3, checked.stdout[-2000:])
            actual = {int(law) - 600: value == "true" for law, value in rows}
            model = b.parse((EXAMPLES / "withdrawal_game2.zal").read_text(encoding="ascii"))
            game = reduced_for(model)
            expected_winners = b._fixed_point_winners(game)
            self.assertEqual(actual, {node: node in expected_winners for node in range(3)})

        deadline_project = DEADLINE_PROJECT
        deadline_project.parent.mkdir(parents=True, exist_ok=True)
        deadline_commands = [
            ["python3", "integrations/zal/run.py", "check",
             "integrations/zal/examples/deadline_game2.zal"],
            ["python3", "integrations/zal/run.py", "export-game",
             "integrations/zal/examples/deadline_game2.zal", "--property", "quick_withdrawal",
             "--out", str(deadline_project)],
        ]
        deadline_markers = ["deadline-game-check-output", "deadline-game-export-output"]
        for command, marker in zip(deadline_commands, deadline_markers):
            with self.subTest(command=command):
                completed = subprocess.run(command, cwd=REPOSITORY, text=True, encoding="utf-8",
                                           stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=120)
                self.assertEqual(completed.returncode, 0, completed.stderr)
                self.assertEqual(completed.stdout, documented_output(marker))
        if binary:
            checked = subprocess.run([binary, "check", "--expand", str(deadline_project)],
                                     cwd=REPOSITORY, text=True, stdout=subprocess.PIPE,
                                     stderr=subprocess.STDOUT, timeout=120)
            self.assertEqual(checked.returncode, 0, checked.stdout[-2000:])
            native = re.search(r"<!-- deadline-game-native-output -->\s*```text\n(.*?)\n```",
                               readme, re.DOTALL)
            self.assertIsNotNone(native, "missing documented native deadline expansion")
            self.assertEqual(checked.stdout, native.group(1) + "\n")
            self.assertNotRegex(checked.stdout, r"law 510 stable on commit")
            rows = re.findall(r"^law (6\d\d) from_n(\d+) on commit = (true|false);$",
                              checked.stdout, flags=re.MULTILINE)
            model = b.parse((EXAMPLES / "deadline_game2.zal").read_text(encoding="ascii"))
            _, arena, goal, bound = deadline_parts(model)
            expected_winners = b._deadline_fixed_winners(arena, goal, bound)
            self.assertEqual(len(rows), len(arena.configuration_nodes), checked.stdout[-2000:])
            actual = {int(node): value == "true" for _, node, value in rows}
            self.assertEqual(actual, {node: node in expected_winners
                                      for node in arena.configuration_nodes})

    def test_300_random_reduced_games_export_tables(self):
        rng = random.Random(20261023)
        binary = os.environ.get("ZENO_FCIS_BIN")
        with tempfile.TemporaryDirectory(prefix="zal-games-") as temporary:
            for case in range(300):
                count = rng.randint(1, 8)
                successors = tuple(tuple(sorted(rng.sample(range(count), rng.randint(1, min(3, count)))))
                                   for _ in range(count))
                owners = tuple(rng.randint(0, 1) for _ in range(count))
                goal = tuple(bool(rng.getrandbits(1)) for _ in range(count))
                fairness = tuple(tuple(bool(rng.getrandbits(1)) for _ in range(count))
                                 for _ in range(rng.randint(1, min(3, max(1, 12 // count)))))
                configurations = tuple(("configuration", f"s{node}", ()) for node in range(count))
                game = b._GameGraph(successors, owners, goal, fairness, configurations,
                                    tuple(range(count)), 0, False)
                project = b._game_table_project(game)
                self.assertIn("const table 110 mover 107", project)
                self.assertIn("const table 840 goal 107", project)
                self.assertIn("derived table", project)
                self.assertIn("law 510 stable on commit = stable[0];", project)
                path = Path(temporary) / f"game-{case:03}.zeno"
                path.write_text(project, encoding="ascii")
                if binary:
                    checked = subprocess.run([binary, "check", "--expand", str(path)], text=True,
                                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=120)
                    self.assertEqual(checked.returncode, 0, checked.stdout[-2000:])
                    stable = re.findall(r"^law 510 stable on commit = (true|false);$",
                                        checked.stdout, flags=re.MULTILINE)
                    self.assertEqual(stable, ["true"], checked.stdout[-2000:])
                    rows = re.findall(r"^law (6\d\d) from_[a-z0-9_]+ on commit = (true|false);$",
                                      checked.stdout, flags=re.MULTILINE)
                    self.assertEqual(len(rows), count, checked.stdout[-2000:])
                    actual = {int(law) - 600: value == "true" for law, value in rows}
                    self.assertEqual(set(actual), set(range(count)))
                    expected = b._fixed_point_winners(game)
                    self.assertEqual(actual, {node: node in expected for node in range(count)},
                                     f"table result mismatch in reduced game {case}")
        if not binary:
            print("ZENO_FCIS_BIN is unset; skipped zeno-fcis check --expand and stable-law replay for the "
                  "300 exported games.")

    def test_300_random_bounded_games_expand_natively_and_match_solver_a(self):
        binary = os.environ.get("ZENO_FCIS_BIN")
        if not binary:
            self.skipTest("ZENO_FCIS_BIN is unset; native bounded-game expansion was not run")
        rng = random.Random(20261024)
        model = SimpleNamespace(facts=(), inputs=(), events=("tick",),
                                contexts=lambda: iter(({},)))
        with tempfile.TemporaryDirectory(prefix="zal-deadlines-") as temporary:
            for case in range(300):
                count = rng.randint(1, 8)
                successors = tuple(tuple(sorted(rng.sample(range(count), rng.randint(1, min(3, count)))))
                                   for _ in range(count))
                owners = tuple(rng.randint(0, 1) for _ in range(count))
                goal = tuple(bool(rng.getrandbits(1)) for _ in range(count))
                bound = rng.randint(0, 5)
                configurations = tuple(("configuration", f"s{node}", ()) for node in range(count))
                arena = b._GameGraph(successors, owners, goal, (), configurations,
                                     tuple(range(count)), 0, False)
                project = b._deadline_table_project(arena, model, bound)
                path = Path(temporary) / f"deadline-{case:03}.zeno"
                path.write_text(project, encoding="ascii")
                checked = subprocess.run([binary, "check", "--expand", str(path)], text=True,
                                         stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=120)
                self.assertEqual(checked.returncode, 0, checked.stdout[-2000:])
                self.assertNotRegex(checked.stdout, r"law 510 stable on commit")
                rows = re.findall(r"^law (6\d\d) from_s(\d+) on commit = (true|false);$",
                                  checked.stdout, flags=re.MULTILINE)
                expected = b._deadline_fixed_winners(arena, goal, bound)
                self.assertEqual(len(rows), count, checked.stdout[-2000:])
                actual = {int(node): value == "true" for _, node, value in rows}
                self.assertEqual(actual, {node: node in expected for node in range(count)},
                                 f"bounded table mismatch in random reduced game {case}")

    def test_one_fewer_outer_stratum_makes_stable_false(self):
        rng = random.Random(20261024)
        for case in range(500):
            model = b.parse(random_game_source(rng, case))
            game = reduced_for(model)
            outer, levels, z_chain, middle_chains = b._fixed_point_strata(game)
            if outer <= 1:
                continue
            outer_stable = z_chain[outer] == z_chain[outer - 1]
            middle_stable = all(middle_chains[index][levels[index]]
                                == middle_chains[index][levels[index] - 1]
                                for index in range(1, outer + 1))
            self.assertTrue(outer_stable and middle_stable,
                            f"the proposed {outer} outer strata must make stable true")
            self.assertNotEqual(z_chain[outer - 1], z_chain[outer - 2],
                                "one fewer outer stratum must make stable false")
            return
        self.fail("random campaign found no game requiring more than one outer stratum")

    def test_export_names_and_comments_follow_model_shape(self):
        simple, count = b.export_game_project(b.parse(GAME_SOURCE), "reaches")
        self.assertEqual(count, 2)
        self.assertIn("law 600 from_aa on commit = win[0];", simple)
        self.assertIn("law 601 from_bb on commit = win[1];", simple)
        contextual, _ = b.export_game_project(b.parse(GameSolverTests.CONTEXT_GAME), "reaches")
        self.assertIn("// n0: configuration state=open variables=", contextual)
        self.assertIn("// n2: context choice state=open variables= event=idle", contextual)
        self.assertIn("law 600 from_n0 on commit = win[0];", contextual)

    def test_table_export_refuses_more_than_sixty_four_nodes_exactly(self):
        model = b.parse(GAME_SOURCE)
        count = 65
        configurations = tuple(("configuration", "aa", ()) for _ in range(count))
        too_wide = b._GameGraph(tuple((node,) for node in range(count)), tuple(0 for _ in range(count)),
                                tuple(False for _ in range(count)), (tuple(True for _ in range(count)),),
                                configurations, tuple(range(count)), 0, False)
        with self.assertRaises(b.Refusal) as raised:
            with patch.object(b, "_reduce_game", return_value=too_wide):
                b.export_game_project(model, "reaches")
        self.assertEqual(str(raised.exception), "The table form of this game has 65 nodes; a table holds at most 64")

    def test_bounded_export_refuses_more_than_sixty_four_configurations(self):
        model = b.parse(DEADLINE_SOURCE)
        count = 65
        configurations = tuple(("configuration", "open", ()) for _ in range(count))
        too_wide = b._GameGraph(tuple((node,) for node in range(count)), tuple(0 for _ in range(count)),
                                tuple(False for _ in range(count)), (), configurations,
                                tuple(range(count)), 0, False)
        with self.assertRaises(b.Refusal) as raised:
            with patch.object(b, "_deadline_config_graph", return_value=too_wide):
                b.export_game_project(model, "quick_withdrawal")
        self.assertEqual(str(raised.exception),
                         "The table form of this game has 65 nodes; a table holds at most 64")


if __name__ == "__main__":
    unittest.main()
