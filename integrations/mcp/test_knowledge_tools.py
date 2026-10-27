"""Actual Python MCP handlers with a stubbed CLI; never load private knowledge."""
import json
import os
import subprocess
import unittest
from types import SimpleNamespace
from unittest.mock import patch
import zeno_fcis_knowledge as K


class KnowledgeTools(unittest.TestCase):
    def call(self,fn,*args,**kwargs):
        with patch.dict(os.environ,{'ZENO_FCIS_CLI':'/tmp/zeno-fcis-test-cli','ZENO_FCIS_KB':'hidden-private-path'}),patch.object(K.subprocess,'run',return_value=SimpleNamespace(returncode=0,stdout=b'{"authority":"none","status":"advised"}')) as run:
            result=fn(*args,**kwargs)
            self.assertEqual(result['authority'],'none')
            actual=run.call_args
            self.assertEqual(actual.args[0][1],'kb')
            self.assertNotIn('ZENO_FCIS_KB',actual.kwargs['env'])
            self.assertNotIn('shell',actual.kwargs)
            self.assertNotIn('loop',actual.args[0])
            self.assertNotIn('--out',actual.args[0])
            return actual.args[0]

    def test_only_readonly_tools_are_exported(self):
        self.assertEqual({f.__name__ for f in K.TOOLS},{'kb_advise','kb_plan','kb_explain','kb_assessment_view'})
        self.assertTrue(K.ANNOTATIONS['readOnlyHint'])
        self.assertFalse(K.ANNOTATIONS['destructiveHint'])
        self.assertEqual(self.call(K.kb_advise,[], 'task(subject,sample).')[2],'advise')
        self.assertEqual(self.call(K.kb_plan,['/tmp/zeno-fcis-test-a','/tmp/zeno-fcis-test-b'],'task(subject,sample).','original','examples').count('--kb'),2)
        self.assertIn('usage_applies(L,S)',self.call(K.kb_explain,[],'task(subject,sample).','usage_applies(L,S)'))

    def test_assessment_view_has_no_write_path(self):
        for view, operation in (("assessments", "assessment-view"), ("queue", "assessment-queue"), ("leaderboard", "leaderboard"), ("telemetry", "telemetry")):
            command = self.call(K.kb_assessment_view, [], view)
            self.assertEqual(command[2], operation)
            self.assertNotIn("--task", command)
        for view in ("assess", "curate", "reuse", "grow", "refresh"):
            with self.assertRaises(K.KnowledgeToolError): K.kb_assessment_view([], view)

    def test_refuses_write_operation_and_invalid_argument_shapes(self):
        with self.assertRaisesRegex(K.KnowledgeToolError,'operation-refused'):K._invoke('replay',[],'')
        for paths in ([1],[''],['x']*8,'x'):
            with self.assertRaises(K.KnowledgeToolError):K.kb_advise(paths,'task(subject,sample).')

    def test_errors_never_echo_private_stdout_stderr_or_exception_text(self):
        secret='synthetic-canary-never-log'
        for response in (SimpleNamespace(returncode=1,stdout=secret.encode(),stderr=secret.encode()),SimpleNamespace(returncode=0,stdout=b'{"authority":"program"}',stderr=b'')):
            with patch.dict(os.environ,{'ZENO_FCIS_CLI':'/tmp/zeno-fcis-test-cli'}),patch.object(K.subprocess,'run',return_value=response):
                with self.assertRaises(K.KnowledgeToolError) as e:K.kb_advise([], 'task(subject,sample).')
                self.assertNotIn(secret,str(e.exception));self.assertEqual(str(e.exception),'kb-call-unavailable')
        with patch.dict(os.environ,{'ZENO_FCIS_CLI':'/tmp/zeno-fcis-test-cli'}),patch.object(K.subprocess,'run',side_effect=subprocess.TimeoutExpired(secret,10)):
            with self.assertRaisesRegex(K.KnowledgeToolError,'^kb-call-unavailable$'):K.kb_advise([], 'task(subject,sample).')

    def test_ordinary_mcp_loop_disables_environment_and_saved_session_kb(self):
        import zeno_fcis_transform as T
        with patch.dict(os.environ,{'ZENO_FCIS_KB':'private-canary'}),patch.object(T,'_cli',return_value='/tmp/zeno-fcis-test-cli'),patch.object(T.subprocess,'run',return_value=SimpleNamespace(returncode=0,stdout=json.dumps({'schema':T.LOOP_SCHEMA,'authority':'none'}),stderr='')) as run:
            T.transform_replay('/tmp/zeno-fcis-test-session')
            self.assertIn('--no-kb',run.call_args.args[0])
            self.assertNotIn('ZENO_FCIS_KB',run.call_args.kwargs['env'])

if __name__=='__main__':unittest.main()
