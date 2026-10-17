"""Public-example knowledge acts; native process mocked explicitly."""
from copy import deepcopy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from behavior import Refusal
import loop_composable_language as C
import loop_language as Z
import loop_strategy_knowledge as K
import loop_workspace as W
from test_loop_language import COUNTER_SUBJECT


class KnowledgeActs(unittest.TestCase):
    def test_both_surfaces_round_trip_and_claim_only_owned_prefixes(self):
        ctx = Z.Context(COUNTER_SUBJECT)
        for value in ({"operation": "check"}, {"operation": "query", "query": "strategy(S)"},
                      {"operation": "plan", "have": ["original", "vocabulary"], "want": ["qualified"],
                       "max_steps": 8, "max_states": 4096, "max_work": 2000000}):
            act = Z.act("strategy-kb", "k", request=json.dumps(value, sort_keys=True, separators=(",", ":")))
            for english in (False, True):
                self.assertEqual(Z.parse_statement(Z.render_act(act, ctx, english), ctx, english), act)
        self.assertFalse(C.claims("version-space v;", False))
        self.assertFalse(C.claims("Capture c uses x.", True))
        self.assertFalse(C.claims("StrategyKBElse k.", True))

    def test_private_paths_owner_fields_and_malformed_requests_are_refused(self):
        for value in ({"operation": "check", "kb": "external.kb"},
                      {"operation": "check", "path": "external.kb"},
                      {"operation": "query"}, {"operation": "check", "query": "p(X)"},
                      {"operation": "plan", "have": ["original"], "want": []},
                      {"operation": "query", "query": "strategy(S)", "answers": []},
                      {"operation": "check", "max_work": True},
                      {"operation": "check", "max_steps": 17}):
            with self.assertRaises(Refusal):
                K.request(json.dumps(value))
        with self.assertRaises(Refusal):
            K.request('{"operation":"check","operation":"plan"}')

    def test_public_example_suppresses_ambient_private_kb_and_mcp_disables_sessions(self):
        import os
        from types import SimpleNamespace
        with patch.dict(os.environ,{'ZENO_FCIS_KB':'synthetic-private-canary'}),patch.object(K.D.subprocess,'run',return_value=SimpleNamespace(returncode=0,stdout=b'{}')) as run:
            K.D.run(Path('test-cli'),['loop','strategy','--kb-op','check'])
            self.assertNotIn('ZENO_FCIS_KB',run.call_args.kwargs['env'])
            self.assertNotIn('--kb',run.call_args.args[0])
        with K.D.disable_automatic_knowledge(),patch.object(K.D.subprocess,'run',return_value=SimpleNamespace(returncode=0,stdout=b'{}')) as run:
            K.D.run(Path('test-cli'),['loop','resume','--session','session'])
            self.assertIn('--no-kb',run.call_args.args[0])

    def test_mcp_imported_bridge_scopes_automatic_kb_disable_to_one_call(self):
        import importlib.util
        spec=importlib.util.spec_from_file_location('local_zal_bridge',Path(K.D.__file__).with_name('mcp.py'))
        bridge=importlib.util.module_from_spec(spec);spec.loader.exec_module(bridge)
        def call(*args):
            self.assertTrue(K.D._automatic_kb_disabled.get())
            return {'authority':'none'}
        with patch.object(W,'mcp_call',side_effect=call):bridge.Bridge(object()).call('zal_loop_read',{})
        self.assertFalse(K.D._automatic_kb_disabled.get())

    def fixture(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        workspace = W.LoopWorkspace(Path(temp.name) / "state.json")
        state = {"subject": deepcopy(COUNTER_SUBJECT), "incumbent": {"keep": True},
                 "events": [], "questions": {}, "next_question": 1}
        return workspace, state, Z.Context(state["subject"])

    def test_route_uses_example_only_emits_fact_and_spends_no_attempt(self):
        workspace, state, ctx = self.fixture()
        initial = deepcopy(state)
        a = Z.act("strategy-kb", "k", request='{"operation":"plan","have":["model"],"want":["answer"]}')
        report = {"door": "strategy", "status": "proposed-plan", "detail": {"selected_strategy": "diagnose", "attempts_spent": 0}}
        with patch.object(workspace, "_contract"), patch.object(workspace, "cli", return_value=Path("test-cli")), \
                patch.object(K.D, "run", return_value=(0, report)) as run, patch.object(workspace, "_fact") as fact:
            K.route(workspace, state, ctx, [a], 1)
            self.assertNotIn("--kb", run.call_args.args[1])
            self.assertEqual(run.call_args.args[1], ["loop", "strategy", "--kb-op", "plan", "--have", "model", "--want", "answer"])
            fact.assert_called_once()
        self.assertEqual(state, initial)

    def test_stop_or_spurious_acceptance_cannot_mutate_contract_or_incumbent(self):
        workspace, state, ctx = self.fixture()
        initial = deepcopy(state)
        a = Z.act("strategy-kb", "k", request='{"operation":"check","max_work":0}')
        for code, status in ((2, "inconclusive"), (0, "qualified")):
            with patch.object(workspace, "_contract"), patch.object(workspace, "cli", return_value=Path("test-cli")), \
                    patch.object(K.D, "run", return_value=(code, {"status": status})), patch.object(workspace, "_fact") as fact:
                if status == "qualified":
                    with self.assertRaises(Refusal):
                        K.route(workspace, state, ctx, [a], 1)
                    fact.assert_not_called()
                else:
                    K.route(workspace, state, ctx, [a], 1)
                    fact.assert_called_once()
            self.assertEqual(state, initial)


if __name__ == "__main__":
    unittest.main()
